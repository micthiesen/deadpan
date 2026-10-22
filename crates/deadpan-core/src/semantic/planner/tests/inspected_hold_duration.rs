//! Duration intent uses captured ordinary Hold authority, never a cursor sample.
use super::*;
use crate::{
    AssetId, AssetRecord, AudioEdgePolicy, AudioSample, AudioTreatments, BeatSound, ExactRatio,
    InstancePath, IterationOrder, PitchPolicy, RetimePurpose, SemanticHoldDurationContext, SoundId,
    SoundOverflowPolicy, SourceAudio, SourceAudioMapping, SourceFrameId, SourceQualificationId,
    SourceSpan, SourceTimeBase, SourceTimestamp,
};

fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}

fn stage(child: &str, output: i64, start: i64, end: i64, pitch: PitchPolicy) -> BeatNode {
    let mut result = BeatNode::sequence("Stage", vec![]);
    result.kind = NodeKind::Retime {
        child: node(child),
        duration: FrameDuration::new(output).unwrap(),
        mapping: range(start, end),
        pitch,
        purpose: RetimePurpose::Edit,
    };
    result
}

fn fixture(pitch: PitchPolicy, crop: i64) -> ProjectDocument {
    tree(
        &["lead", "stage", "tail"],
        vec![
            ("lead", hold(4)),
            ("stage", stage("body", 4, crop, 8, pitch)),
            (
                "body",
                BeatNode::sequence("Body", vec![node("before"), node("held"), node("after")]),
            ),
            ("before", hold(3)),
            ("held", hold(3)),
            ("after", hold(6)),
            ("tail", hold(2)),
        ],
    )
}

fn entry() -> SemanticContext {
    SemanticContext {
        selected_child: Some(node("stage")),
        hold_duration: Some(SemanticHoldDurationContext {
            root: node("stage"),
            target: node("held"),
            presentation: InstancePath {
                node: node("held"),
                repeats: vec![],
            },
        }),
        ..context("root", 6)
    }
}

fn duration(frames: u32) -> SemanticInstruction {
    SemanticInstruction::SetHoldDuration {
        length: crate::PauseLength::Frames {
            frames: NonZeroU32::new(frames).unwrap(),
        },
    }
}

fn reversible(before: &ProjectDocument, planned: &SemanticPlan) {
    let request = planned.request.as_ref().unwrap();
    let replay = crate::replay_compound::<EditError>(before, request, |_| Ok(())).unwrap();
    assert_eq!(replay.document, planned.document);
    assert_eq!(
        replay.edit.inverse.apply(&replay.document).unwrap(),
        *before
    );
    assert_eq!(
        ProjectDocument::from_json(&planned.document.to_json().unwrap()).unwrap(),
        planned.document,
    );
}

fn visibility(document: &ProjectDocument, context: &SemanticContext) -> HoldDurationVisibility {
    document
        .ordinary_hold_duration_visibility(&context.parent, context.hold_duration.as_ref().unwrap())
        .unwrap()
}

#[test]
fn inspected_hold_duration_fractional_visibility_grow_shrink_all_pitches_and_inverse() {
    for pitch in [
        PitchPolicy::FollowSpeed,
        PitchPolicy::Preserve,
        PitchPolicy::Shift { semitones: 3 },
    ] {
        for frames in [1, 4] {
            let before = fixture(pitch, 1);
            let context = entry();
            let visible = visibility(&before, &context);
            assert_eq!(
                (visible.exact.start, visible.exact.end),
                (ratio(36, 7), ratio(48, 7))
            );
            assert_eq!(visible.frames, range(5, 7));
            let planned = plan(
                &before,
                context.clone(),
                vec![duration(frames)],
                &BTreeMap::new(),
            )
            .unwrap();
            let mut expected = context;
            expected.cursor = ProjectFrame(5);
            assert_eq!(planned.context, expected);
            assert_eq!(
                planned.document.nodes()[&node("stage")],
                before.nodes()[&node("stage")]
            );
            assert_eq!(
                planned.document.nodes()[&node("lead")],
                before.nodes()[&node("lead")]
            );
            assert_eq!(
                planned.document.nodes()[&node("tail")],
                before.nodes()[&node("tail")]
            );
            assert_eq!(
                planned.document.duration().unwrap(),
                before.duration().unwrap()
            );
            assert_eq!(planned.trace[0].resolved_parent, Some(node("body")));
            assert_eq!(
                planned.trace[0].resolved_selection,
                Some(SliceCaptureSelection::Child { node: node("held") })
            );
            assert_eq!(
                planned.trace[0].resolved_range,
                Some(visibility(&planned.document, &planned.context).frames)
            );
            let Command::Compound { transaction } = &planned.request.as_ref().unwrap().command
            else {
                panic!()
            };
            assert!(
                matches!(transaction.steps()[0].edit().unwrap().command.as_command(),
                Command::SetHoldDuration { node: target, duration } if target == &node("held") && duration.frames() == i64::from(frames))
            );
            reversible(&before, &planned);
        }
    }
}

#[test]
fn inspected_hold_duration_nested_windows_clip_before_mapping_to_root() {
    let mut before = fixture(PitchPolicy::Preserve, 1);
    let inner = before.nodes()[&node("stage")].clone();
    before.nodes.insert(node("inner"), inner);
    before.nodes.insert(
        node("stage"),
        stage("middle", 8, 1, 7, PitchPolicy::Shift { semitones: -2 }),
    );
    before.nodes.insert(
        node("middle"),
        BeatNode::sequence(
            "Middle",
            vec![node("pad"), node("inner"), node("middle-tail")],
        ),
    );
    before.nodes.insert(node("pad"), hold(1));
    before.nodes.insert(node("middle-tail"), hold(4));
    before.validate().unwrap();
    let mut context = entry();
    context.cursor = ProjectFrame(7);
    let visible = visibility(&before, &context);
    assert_eq!(
        (visible.exact.start, visible.exact.end),
        (ratio(116, 21), ratio(164, 21))
    );
    assert_eq!(visible.frames, range(6, 8));
    let planned = plan(&before, context, vec![duration(2)], &BTreeMap::new()).unwrap();
    assert_eq!(planned.context.cursor, ProjectFrame(6));
    let after = visibility(&planned.document, &planned.context);
    assert_eq!(
        (after.exact.start, after.exact.end),
        (ratio(116, 21), ratio(148, 21))
    );
    assert_eq!(after.frames, range(6, 7));
    reversible(&before, &planned);
}

fn tone_fixture() -> (ProjectDocument, SourceAudio) {
    let mut before = fixture(PitchPolicy::Preserve, 1);
    before.presentation_basis.frame_rate = crate::FrameRate::new(30_000, 1001).unwrap();
    let time_base = SourceTimeBase::new(1, 44_100).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 3100,
            time_base,
        },
        SourceTimestamp {
            ticks: 23_100,
            time_base,
        },
    )
    .unwrap();
    let source = SourceAudio {
        asset: AssetId::new("tone").unwrap(),
        span,
    };
    before.assets.insert(
        source.asset.clone(),
        AssetRecord {
            label: "Qualified tone".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(span),
            still_image: false,
            frame_count: None,
            source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    );
    for id in ["lead", "before", "held", "after", "tail"] {
        let NodeKind::Hold { recipe } = &mut before.nodes.get_mut(&node(id)).unwrap().kind else {
            panic!()
        };
        recipe.audio = HoldAudio::RoomTone {
            source: source.clone(),
        };
    }
    before.validate().unwrap();
    (before, source)
}

#[test]
fn inspected_hold_duration_named_counted_calls_revalidate_after_each_physical_capture() {
    let (before, _) = tone_fixture();
    let bank = BTreeMap::from([
        (name('a'), macro_value(vec![call('b', 1)])),
        (name('b'), macro_value(vec![duration(4), duration(2)])),
    ]);
    let snapshot = bank.clone();
    let planned = plan(&before, entry(), vec![call('a', 2)], &bank).unwrap();
    assert!(!planned.document.audio_bindings().timings.is_empty());
    assert_eq!(
        planned
            .document
            .node_duration(&node("held"))
            .unwrap()
            .frames(),
        2
    );
    assert_eq!(planned.context.cursor, ProjectFrame(5));
    assert_eq!(planned.context.hold_duration, entry().hold_duration);
    assert_eq!(planned.context.selected_child, entry().selected_child);
    let Command::Compound { transaction } = &planned.request.as_ref().unwrap().command else {
        panic!()
    };
    assert_eq!(transaction.steps().len(), 4);
    assert!(transaction.steps().iter().all(|step| matches!(
        step.edit().unwrap().command.as_command(), Command::SetHoldDuration { node: target, .. } if target == &node("held")
    )));
    assert_eq!(bank, snapshot);
    reversible(&before, &planned);
}

#[test]
fn inspected_hold_duration_output_owned_sound_eligibility_uses_existing_reducer() {
    let (mut before, mut source) = tone_fixture();
    source.span = SourceSpan::new(
        source.span.start(),
        SourceTimestamp {
            ticks: 3200,
            time_base: source.span.start().time_base,
        },
    )
    .unwrap();
    let event = BeatSound {
        label: "Output effect".into(),
        mapping: SourceAudioMapping::natural_rate(
            source.span,
            before.presentation_basis().frame_rate,
        )
        .unwrap(),
        source,
        offset: AudioSample(37),
        treatments: AudioTreatments::default(),
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    };
    before.beat_sounds.insert(
        node("stage"),
        BTreeMap::from([(SoundId::new("effect").unwrap(), event)]),
    );
    before.validate().unwrap();
    assert!(before.derived_clocks().is_empty());
    visibility(&before, &entry());
    let planned = plan(
        &before,
        entry(),
        vec![duration(4), duration(2)],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(planned.document.beat_sounds(), before.beat_sounds());
    assert!(planned.document.derived_clocks().is_empty());
    reversible(&before, &planned);
}

#[test]
fn inspected_hold_duration_crops_and_picture_center_ties_reject_unsampled_targets() {
    let before = fixture(PitchPolicy::FollowSpeed, 4);
    let visible = visibility(&before, &entry());
    assert_eq!(
        (visible.exact.start, visible.exact.end),
        (ExactRatio::integer(4), ExactRatio::integer(6))
    );
    assert_eq!(visible.frames, range(4, 6));
    // The ordinary command itself is legal, but removes this Hold's visible
    // interval. Semantic invocation cannot invent a neighboring continuation.
    let snapshot = before.clone();
    let error = plan(&before, entry(), vec![duration(1)], &BTreeMap::new()).unwrap_err();
    assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
    assert!(error.message.contains("hidden"));
    assert_eq!(before, snapshot);

    for (prefix, held, tail, expected) in [
        (1, 1, 0, Some(range(0, 1))), // In at .5 owns picture0.
        (0, 1, 1, None),              // Out at .5 excludes picture0.
        (1, 1, 8, None),              // [.1,.2) has no picture center.
    ] {
        let before = tree(
            &["stage"],
            vec![
                (
                    "stage",
                    stage("body", 1, 0, prefix + held + tail, PitchPolicy::FollowSpeed),
                ),
                (
                    "body",
                    BeatNode::sequence("Body", vec![node("before"), node("held"), node("after")]),
                ),
                (
                    "before",
                    if prefix == 0 {
                        BeatNode::sequence("Empty", vec![])
                    } else {
                        hold(prefix)
                    },
                ),
                ("held", hold(held)),
                (
                    "after",
                    if tail == 0 {
                        BeatNode::sequence("Empty", vec![])
                    } else {
                        hold(tail)
                    },
                ),
            ],
        );
        // Empty Sequence siblings preserve endpoint geometry without time.
        before.validate().unwrap();
        let mut context = entry();
        context.cursor = ProjectFrame(0);
        let result = before.ordinary_hold_duration_visibility(
            &context.parent,
            context.hold_duration.as_ref().unwrap(),
        );
        if let Some(expected) = expected {
            assert_eq!(result.unwrap().frames, expected);
        } else {
            assert_eq!(
                result.unwrap_err().code,
                EditErrorCode::SelectionUnavailable
            );
            assert!(
                plan_semantic(
                    &before,
                    &context,
                    &program(vec![duration(2)]),
                    SemanticRegisterBank {
                        entries: &BTreeMap::new(),
                        version: 0
                    },
                    revision("unsampled"),
                    |_| panic!("invalid initial visibility allocated"),
                    no_original
                )
                .is_err()
            );
        }
    }
}

#[test]
fn inspected_hold_duration_noop_late_mixed_and_wrong_allocation_refuse_atomically() {
    let before = fixture(PitchPolicy::FollowSpeed, 1);
    let snapshot = before.clone();
    let bank = BTreeMap::from([(name('a'), macro_value(vec![duration(4), motion(true, 1)]))]);
    let bank_snapshot = bank.clone();
    for instructions in [
        vec![duration(3)],
        vec![duration(4), duration(4)],
        vec![call('a', 1)],
        vec![
            duration(4),
            SemanticInstruction::InsertPause {
                length: crate::PauseLength::Frames {
                    frames: NonZeroU32::new(1).unwrap(),
                },
                black: true,
            },
        ],
        vec![
            duration(4),
            SemanticInstruction::Zoom {
                kind: crate::framing_zoom::ZoomKind::Off,
                center: crate::SemanticZoomCenter::Keep,
            },
        ],
    ] {
        assert!(plan(&before, entry(), instructions, &bank).is_err());
        assert_eq!(before, snapshot);
        assert_eq!(bank, bank_snapshot);
    }
    let error = plan_semantic(
        &before,
        &entry(),
        &program(vec![duration(4)]),
        SemanticRegisterBank {
            entries: &bank,
            version: 0,
        },
        revision("wrong-allocation"),
        |_| {
            Ok(SemanticAllocation::SetRepeatPlays {
                new_revision: revision("wrong-leaf"),
            })
        },
        no_original,
    )
    .unwrap_err();
    assert!(error.message.contains("parameter allocation"));
    let error = plan_semantic(
        &before,
        &entry(),
        &program(vec![duration(3)]),
        SemanticRegisterBank {
            entries: &bank,
            version: 0,
        },
        revision("noop"),
        |_| panic!("no-op allocated"),
        no_original,
    )
    .unwrap_err();
    assert!(error.message.contains("already has"));
    assert_eq!(before, snapshot);
}

#[test]
fn inspected_hold_duration_forged_and_conflicting_authorities_refuse_before_allocation() {
    let before = fixture(PitchPolicy::FollowSpeed, 1);
    let mut cases = Vec::new();
    let mut wrong = entry();
    wrong.selected_child = Some(node("tail"));
    cases.push(wrong);
    let mut wrong = entry();
    wrong.hold_duration.as_mut().unwrap().target = node("tail");
    cases.push(wrong);
    let mut wrong = entry();
    let authority = wrong.hold_duration.as_mut().unwrap();
    authority.target = node("tail");
    authority.presentation.node = node("tail");
    cases.push(wrong);
    let mut wrong = entry();
    wrong.hold_duration.as_mut().unwrap().root = node("body");
    cases.push(wrong);
    let mut wrong = entry();
    wrong.hold_duration.as_mut().unwrap().target = node("missing");
    cases.push(wrong);
    let mut wrong = entry();
    wrong.visual_selection = Some(SemanticVisualSelection::Time {
        anchor: ProjectFrame(5),
        head: ProjectFrame(6),
        extending: false,
    });
    cases.push(wrong);
    let mut wrong = entry();
    wrong.original = Some(SemanticOriginalContext {
        asset: AssetId::new("absent").unwrap(),
        qualification: SourceQualificationId::new("b".repeat(64)).unwrap(),
        ordinal: SourceFrameId(0),
    });
    cases.push(wrong);
    let mut wrong = entry();
    wrong.scoped = Some(SemanticScopedContext {
        root: node("stage"),
        target: crate::ScopedNodeTarget {
            node: node("held"),
            repeats: vec![],
        },
        presentation: InstancePath {
            node: node("held"),
            repeats: vec![],
        },
    });
    cases.push(wrong);
    let mut wrong = entry();
    wrong
        .hold_duration
        .as_mut()
        .unwrap()
        .presentation
        .repeats
        .push(crate::RepeatInstance {
            node: node("stage"),
            iteration: crate::IterationId {
                allocation: revision("forged"),
                ordinal: 0,
            },
        });
    cases.push(wrong);
    for context in cases {
        assert!(
            plan_semantic(
                &before,
                &context,
                &program(vec![duration(4)]),
                SemanticRegisterBank {
                    entries: &BTreeMap::new(),
                    version: 0
                },
                revision("invalid"),
                |_| panic!("malformed authority allocated"),
                no_original
            )
            .is_err()
        );
    }

    let mut repeated = before.clone();
    repeated.nodes.insert(node("repeat"), {
        let mut beat = BeatNode::sequence("Repeat", vec![]);
        beat.kind = NodeKind::Repeat {
            child: node("body"),
            iterations: IterationOrder::new(revision("plays"), 2).unwrap(),
            gap: None,
            escalation: None,
        };
        beat
    });
    let NodeKind::Retime { child, .. } = &mut repeated.nodes.get_mut(&node("stage")).unwrap().kind
    else {
        panic!()
    };
    *child = node("repeat");
    repeated.validate().unwrap();
    assert_eq!(
        repeated
            .ordinary_hold_duration_visibility(
                &node("root"),
                entry().hold_duration.as_ref().unwrap()
            )
            .unwrap_err()
            .code,
        EditErrorCode::SelectionUnavailable
    );

    let mut partition = before.clone();
    let NodeKind::Retime {
        duration,
        mapping,
        purpose,
        pitch,
        ..
    } = &mut partition.nodes.get_mut(&node("stage")).unwrap().kind
    else {
        panic!()
    };
    *duration = mapping.duration();
    *purpose = RetimePurpose::Partition;
    *pitch = PitchPolicy::Preserve;
    partition.validate().unwrap();
    assert!(
        partition
            .ordinary_hold_duration_visibility(
                &node("root"),
                entry().hold_duration.as_ref().unwrap()
            )
            .is_err()
    );
}

#[test]
fn inspected_hold_duration_clocked_stage_refuses_without_broadening_this_play() {
    let before = tree(
        &["stage"],
        vec![
            ("stage", stage("held", 24, 0, 12, PitchPolicy::FollowSpeed)),
            ("held", hold(12)),
        ],
    );
    let crate::PauseInsertionTarget::Retimed(target) =
        before.pause_insertion_target(ProjectFrame(3)).unwrap()
    else {
        panic!()
    };
    let NodeKind::Hold { recipe } = hold(3).kind else {
        panic!()
    };
    let request = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: revision("clock-pause"),
        command: Command::InsertTime {
            at: ProjectFrame(3),
            hold: recipe,
            id: node("inserted"),
            identities: SplitIdentities {
                nodes: (0..target.required_ids)
                    .map(|n| node(&format!("split-{n}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: revision("clock-pause"),
                ordinal: 0,
            },
        },
    };
    let clocked = crate::apply(&before, &request)
        .unwrap()
        .forward
        .apply(&before)
        .unwrap();
    assert!(!clocked.derived_clocks().is_empty());
    let hold = clocked
        .nodes()
        .iter()
        .find_map(|(id, beat)| matches!(beat.kind, NodeKind::Hold { .. }).then_some(id.clone()))
        .unwrap();
    let context = SemanticContext {
        cursor: ProjectFrame(3),
        hold_duration: Some(SemanticHoldDurationContext {
            root: node("stage"),
            target: hold.clone(),
            presentation: InstancePath {
                node: hold,
                repeats: vec![],
            },
        }),
        ..entry()
    };
    let error = plan_semantic(
        &clocked,
        &context,
        &program(vec![duration(4)]),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 0,
        },
        revision("reject-clocked"),
        |_| panic!("clocked authority allocated"),
        no_original,
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
    assert!(error.message.contains("without exact pause clocks"));
}
