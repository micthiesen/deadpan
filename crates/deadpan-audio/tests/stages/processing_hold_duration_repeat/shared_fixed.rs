//! Shared semantic duration traverses a fixed stage without changing its extent.
use super::*;
use std::num::NonZeroU32;

fn fixture_with_outer(pitch: PitchPolicy, partial: bool) -> ProjectDocument {
    wrap_outer(repeated_fixture(pitch, false), partial)
}

fn wrap_outer(base: ProjectDocument, partial: bool) -> ProjectDocument {
    let mut wire = serde_json::to_value(&base).unwrap();
    let names = base
        .nodes()
        .keys()
        .filter(|name| **name != id("root"))
        .map(|name| (name.to_string(), format!("copy-{name}")))
        .collect::<BTreeMap<_, _>>();
    for (name, node) in base.nodes().iter().filter(|(name, _)| **name != id("root")) {
        let mut copied = node.clone();
        match &mut copied.kind {
            NodeKind::Sequence { children } => {
                for child in children {
                    *child = id(&names[child.as_str()]);
                }
            }
            NodeKind::Repeat { child, .. } | NodeKind::Retime { child, .. } => {
                *child = id(&names[child.as_str()]);
            }
            NodeKind::Source { .. } | NodeKind::Hold { .. } => {}
        }
        wire["nodes"][&names[name.as_str()]] = serde_json::to_value(copied).unwrap();
    }
    for (name, entries) in base.overrides() {
        let copied = PlayOverrides::try_from(
            entries
                .iter()
                .map(|(iteration, root)| PlayOverride {
                    iteration: iteration.clone(),
                    root: id(&names[root.as_str()]),
                })
                .collect::<Vec<_>>(),
        )
        .unwrap();
        wire["overrides"][&names[name.as_str()]] = serde_json::to_value(copied).unwrap();
    }
    let order =
        IterationOrder::new(RevisionId::new("shared-fixed-outer-order").unwrap(), 3).unwrap();
    let mut outer = BeatNode::sequence("Outer Repeat", vec![]);
    outer.kind = NodeKind::Repeat {
        child: id("processor"),
        iterations: order.clone(),
        gap: None,
        escalation: None,
    };
    wire["nodes"]["outer"] = serde_json::to_value(outer).unwrap();
    wire["nodes"]["exterior"] = serde_json::to_value(voice()).unwrap();
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["outer", "exterior"]);
    wire["overrides"]["outer"] = serde_json::to_value(
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: order.at(1).unwrap(),
            root: id("copy-processor"),
        }])
        .unwrap(),
    )
    .unwrap();
    let mut document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    if partial {
        let captured = capture_unbound_audio_bindings(
            &document,
            AudioTimingId {
                allocation: RevisionId::new("shared-fixed-old-clocks").unwrap(),
                ordinal: 0,
            },
        )
        .unwrap();
        wire = serde_json::to_value(&document).unwrap();
        wire["audio_bindings"] = serde_json::to_value(captured).unwrap();
        for owner in ["speech", "tail", "exterior"] {
            if owner != "exterior" && !base.nodes().contains_key(&id(owner)) {
                continue;
            }
            assert!(
                wire["audio_bindings"]["bindings"]
                    .as_object_mut()
                    .unwrap()
                    .remove(owner)
                    .is_some()
            );
        }
        document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    }
    document
}

fn semantic_resize(document: &ProjectDocument, frames: u32, outer_play: u32) -> SemanticPlan {
    let NodeKind::Repeat {
        iterations: outer, ..
    } = &document.nodes()[&id("outer")].kind
    else {
        unreachable!()
    };
    let inner = document.nodes().get(&id("repeat")).map(|node| {
        let NodeKind::Repeat { iterations, .. } = &node.kind else {
            unreachable!()
        };
        iterations.at(0).unwrap()
    });
    let authority = SemanticSharedHoldDurationContext {
        root: id("outer"),
        target: ScopedNodeTarget {
            node: id("pause"),
            repeats: std::iter::once("outer")
                .chain(inner.as_ref().map(|_| "repeat"))
                .map(|name| RepeatEditStep {
                    repeat: id(name),
                    branch: RepeatEditBranch::Default,
                })
                .collect(),
        },
        presentation: Some(InstancePath {
            node: id("pause"),
            repeats: std::iter::once(RepeatInstance {
                node: id("outer"),
                iteration: outer.at(outer_play).unwrap(),
            })
            .chain(inner.map(|iteration| RepeatInstance {
                node: id("repeat"),
                iteration,
            }))
            .collect(),
        }),
    };
    let visible = document
        .shared_hold_duration_visibility(&id("root"), &authority)
        .unwrap()
        .unwrap();
    let context = SemanticContext {
        original: None,
        scoped: None,
        hold_duration: None,
        mixed_hold_duration: None,
        selected_hold_duration: None,
        shared_hold_duration: Some(authority),
        parent: id("root"),
        cursor: visible.frames.start(),
        selected_child: Some(id("outer")),
        visual_selection: None,
    };
    let mut parameter_edits = 0;
    let plan = plan_semantic(
        document,
        &context,
        &SemanticProgram::new(vec![SemanticInstruction::SetHoldDuration {
            length: PauseLength::Frames {
                frames: NonZeroU32::new(frames).unwrap(),
            },
        }])
        .unwrap(),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 0,
        },
        RevisionId::new("shared-fixed-semantic").unwrap(),
        |request| {
            let SemanticAllocationRequest::ParameterEdit { step_index } = request else {
                panic!("duration must not isolate")
            };
            parameter_edits += 1;
            Ok(SemanticAllocation::ParameterEdit {
                new_revision: RevisionId::new(format!("shared-fixed-stage-{step_index}")).unwrap(),
            })
        },
        |_, _| panic!("no Original resolver"),
    )
    .unwrap();
    assert_eq!(parameter_edits, 1);
    assert_eq!(
        plan.context.shared_hold_duration,
        context.shared_hold_duration
    );
    let request = plan.request.as_ref().unwrap();
    let replay = replay_compound::<EditError>(document, request, |_| Ok(())).unwrap();
    assert_eq!(replay.document, plan.document);
    assert_eq!(
        replay.edit.inverse.apply(&plan.document).unwrap(),
        *document
    );
    assert_eq!(
        plan.document.nodes().keys().collect::<Vec<_>>(),
        document.nodes().keys().collect::<Vec<_>>()
    );
    assert_eq!(plan.document.overrides(), document.overrides());
    assert_eq!(plan.document.gap_overrides(), document.gap_overrides());
    assert_eq!(
        plan.document.duration().unwrap(),
        document.duration().unwrap()
    );
    for (name, node) in document.nodes() {
        if *name != id("pause") {
            assert_eq!(&plan.document.nodes()[name], node, "unchanged {name}");
        }
    }
    assert_eq!(
        ProjectDocument::from_json(&plan.document.to_json().unwrap()).unwrap(),
        plan.document
    );
    plan
}

fn input_at(
    plan: &Arc<RenderPlan>,
    provider: &mut FixtureProvider,
    root_frame: i64,
    support: Range<ExactRatio>,
    total: i64,
    chunks: &[u32],
) -> Recorded {
    provider
        .revisions
        .insert(plan.metadata().revision_id.clone());
    // Query inside the exact stage occurrence. RoundEven can fall before
    // its start (frame9 ->14414 while the first point at/after it is14415).
    let at = point(root_frame);
    let query = plan
        .audio_signal()
        .query(
            SignalSample(at)..SignalSample(at + 1),
            AudioQueryLimits::default(),
        )
        .unwrap();
    let stage = match query.spans.into_iter().next().unwrap().content {
        AudioSignalContent::Stage(stage) => stage,
        AudioSignalContent::Bound(bound) => bound.intrinsic_stage().unwrap().expect("fixed stage"),
        other => panic!("fixed stage at {root_frame}: {other:?}"),
    };
    let stage_frames = stage.descriptor().duration.frames();
    assert_eq!(
        stage.descriptor().instance.node,
        id(if root_frame == stage_frames {
            "copy-processor"
        } else {
            "processor"
        }),
        "the query must resolve the requested physical stage, not its neighbor"
    );
    let signal = stage.input_signal();
    assert_eq!(signal.support(), support);
    let tape = AudioSignalTape::new(
        plan,
        support.clone(),
        vec![AudioSignalTapeRun::new(support.clone(), support, signal)],
    )
    .unwrap();
    assert_eq!(tape.sample_count().unwrap().0, total);
    let mut renderer = StageAudio::new(Arc::clone(plan));
    let mut result = Recorded {
        samples: Vec::new(),
        suppressed: Vec::new(),
    };
    for count in chunks.iter().copied().cycle() {
        let start = result.samples.len() as i64;
        if start == total {
            break;
        }
        let count = count.min((total - start) as u32);
        let block = renderer
            .read_tape(
                provider,
                &tape,
                SignalSample(start),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        result
            .suppressed
            .extend((start..start + i64::from(count)).map(|sample| {
                block
                    .suppressed
                    .iter()
                    .any(|range| range.contains(&SignalSample(sample)))
            }));
        result.samples.extend(block.samples);
    }
    result
}

fn shared_fixed(pitch: PitchPolicy) {
    let stage_frames = if pitch == PitchPolicy::Preserve {
        9
    } else {
        18
    };
    for partial in [false, true] {
        let before = fixture_with_outer(pitch, partial);
        let (_, mut provider) = mono_original(2);
        let old_plan = Arc::new(RenderPlan::compile(&before).unwrap());
        let old_output = record(&before, &mut provider, false, &[251, 3]);
        assert_eq!(before.duration().unwrap().frames(), 3 * stage_frames + 4);
        for outer in [0, 2] {
            let old = input_at(
                &old_plan,
                &mut provider,
                i64::from(outer) * stage_frames,
                ratio(0, 1)..ratio(18, 1),
                28829,
                &[251, 3],
            );
            same_recorded(
                &old,
                &expected_input(&provider, "pause", 2),
                0,
                "old nested physical owner phases",
            );
            for frames in [1, 3] {
                let planned = semantic_resize(&before, frames, outer);
                let plan = Arc::new(RenderPlan::compile(&planned.document).unwrap());
                for inspected in [0, 2] {
                    let actual = input_at(
                        &plan,
                        &mut provider,
                        inspected * stage_frames,
                        ratio(0, 1)..ratio(18, 1),
                        28829,
                        &[127, 3, 251],
                    );
                    let expected = expected_input(&provider, "pause", i64::from(frames));
                    same_recorded(
                        &actual,
                        &expected,
                        0,
                        "shared fixed stage exact PointCeil input",
                    );
                    same_recorded(
                        &input_at(
                            &plan,
                            &mut provider,
                            inspected * stage_frames,
                            ratio(0, 1)..ratio(18, 1),
                            28829,
                            &[1, 239],
                        ),
                        &actual,
                        0,
                        "shared fixed chunks",
                    );
                }
                let owned = input_at(
                    &plan,
                    &mut provider,
                    stage_frames,
                    ratio(0, 1)..ratio(18, 1),
                    28829,
                    &[127, 3, 251],
                );
                same_recorded(
                    &owned,
                    &expected_input(&provider, "pause", 2),
                    0,
                    "independent outer stage input",
                );
                let output = record(&planned.document, &mut provider, false, &[127, 3, 251]);
                let owned_start = ntsc_boundary(stage_frames);
                let owned_end = ntsc_boundary(2 * stage_frames);
                assert_eq!(
                    output.samples[owned_start..owned_end],
                    old_output.samples[owned_start..owned_end]
                );
                assert_eq!(
                    output.suppressed[owned_start..owned_end],
                    old_output.suppressed[owned_start..owned_end]
                );
                let tail = ntsc_boundary(3 * stage_frames);
                assert_eq!(
                    output.samples[tail..],
                    old_output.samples[tail..],
                    "unbound exterior tail"
                );
                assert_eq!(output.suppressed[tail..], old_output.suppressed[tail..]);
                assert_eq!(
                    record(&planned.document, &mut provider, false, &[1, 239]),
                    output,
                    "whole output chunks"
                );
            }
        }
    }
}

#[test]
fn shared_fixed_preserve_duration_keeps_nested_repeat_pointceil_phases_and_exterior_tail() {
    shared_fixed(PitchPolicy::Preserve);
}

#[test]
fn shared_fixed_shift_duration_keeps_nested_repeat_pointceil_phases_and_owned_stage() {
    shared_fixed(PitchPolicy::Shift { semitones: 3 });
}

#[test]
fn shared_fixed_cropped_duration_keeps_pointceil_phase_and_single_sample_exhaustion() {
    for pitch in [PitchPolicy::Preserve, PitchPolicy::Shift { semitones: 3 }] {
        let stage_frames = if pitch == PitchPolicy::Preserve { 4 } else { 7 };
        for partial in [false, true] {
            let before = wrap_outer(super::super::fixture(pitch, 1, false), partial);
            let (_, mut provider) = mono_original(2);
            let old_plan = Arc::new(RenderPlan::compile(&before).unwrap());
            let old_input = input_at(
                &old_plan,
                &mut provider,
                0,
                ratio(1, 1)..ratio(8, 1),
                11212,
                &[127, 3, 251],
            );
            same_recorded(
                &old_input,
                &super::super::direct_input_reference(&provider, 1, 2),
                0,
                "shared cropped pre-edit input",
            );
            for frames in [1, 3] {
                let planned = semantic_resize(&before, frames, 2);
                let plan = Arc::new(RenderPlan::compile(&planned.document).unwrap());
                let expected =
                    super::super::direct_input_reference(&provider, 1, i64::from(frames));
                for outer in [0, 2] {
                    let actual = input_at(
                        &plan,
                        &mut provider,
                        outer * stage_frames,
                        ratio(1, 1)..ratio(8, 1),
                        11212,
                        &[127, 3, 251],
                    );
                    same_recorded(
                        &actual,
                        &expected,
                        0,
                        &format!(
                            "shared cropped exact input pitch={pitch:?} partial={partial} frames={frames} outer={outer}"
                        ),
                    );
                    assert_eq!(actual.samples[11211], [0.; 2]);
                    assert!(actual.suppressed[11211]);
                    same_recorded(
                        &input_at(
                            &plan,
                            &mut provider,
                            outer * stage_frames,
                            ratio(1, 1)..ratio(8, 1),
                            11212,
                            &[1, 239],
                        ),
                        &actual,
                        0,
                        "shared cropped chunk parity",
                    );
                }
            }
        }
    }
}
