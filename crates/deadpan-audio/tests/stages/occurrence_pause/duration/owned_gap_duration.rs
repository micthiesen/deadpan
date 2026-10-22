//! Explicit owned gaps retain their historical default-gap lattice on resize.
use super::*;
use std::num::NonZeroU32;

fn edit_gap(document: &ProjectDocument, key: &str, command: Command) -> ProjectDocument {
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(key).unwrap(),
        command,
    };
    let edit = apply(document, &request).unwrap();
    let after = edit.forward.apply(document).unwrap();
    assert_eq!(edit.inverse.apply(&after).unwrap(), *document);
    ProjectDocument::from_json(&after.to_json().unwrap()).unwrap()
}

fn gap_fixture(room: bool) -> (ProjectDocument, ProjectDocument, FixtureProvider) {
    let (base, provider) = mono_original(4);
    let mut wire = serde_json::to_value(&base).unwrap();
    let audio = audio_at_rate(100, 20_100, 44_100);
    wire["nodes"]["source"]["kind"]["source"]["duration"] =
        serde_json::to_value(duration(2)).unwrap();
    wire["nodes"]["source"]["kind"]["source"]["audio_mapping"] = serde_json::to_value(
        SourceAudioMapping::natural_rate(audio.span, base.presentation_basis().frame_rate).unwrap(),
    )
    .unwrap();
    wire["nodes"]["source"]["kind"]["source"]["audio"] = serde_json::to_value(audio).unwrap();
    wire["nodes"]["lead"] = serde_json::to_value(hold(1)).unwrap();
    wire["nodes"]["tail"] = serde_json::to_value(&base.nodes()[&id("source")]).unwrap();
    wire["nodes"]["body"] =
        serde_json::to_value(BeatNode::sequence("Body", vec![id("source")])).unwrap();
    let mut inner = repeated("body", 3, &audio_at_rate(321, 987, 44_100));
    if !room {
        let NodeKind::Repeat { gap: Some(gap), .. } = &mut inner.kind else {
            unreachable!()
        };
        gap.audio = HoldAudio::Silence;
    }
    wire["nodes"]["inner"] = serde_json::to_value(inner).unwrap();
    let mut outer = repeated("inner", 2, &audio_at_rate(321, 987, 44_100));
    let NodeKind::Repeat { gap, .. } = &mut outer.kind else {
        unreachable!()
    };
    *gap = None;
    wire["nodes"]["outer"] = serde_json::to_value(outer).unwrap();
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["lead", "outer", "tail"]);
    let mut document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_eq!(document.duration().unwrap().frames(), 25);
    let bindings = capture_unbound_audio_bindings(
        &document,
        AudioTimingId {
            allocation: RevisionId::new("owned-gap-capture").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    wire = serde_json::to_value(&document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let source_reference = document.clone();
    document = split_command(&document, &id("source"), 1, "owned-gap-fragment");
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&id("inner")].kind else {
        unreachable!()
    };
    let plays = (0..3)
        .map(|n| iterations.at(n).unwrap())
        .collect::<Vec<_>>();
    for (n, iteration) in plays.iter().take(2).enumerate() {
        let key = format!("materialize-{n}");
        document = edit_gap(
            &document,
            &key,
            Command::IsolateGap {
                node: id("inner"),
                iteration: iteration.clone(),
                id: id(&format!("gap-{n}")),
                timing: AudioTimingId {
                    allocation: RevisionId::new(&key).unwrap(),
                    ordinal: 0,
                },
            },
        );
    }
    document = edit_gap(
        &document,
        "heterogeneous-gap",
        Command::SetHoldDuration {
            node: id("gap-1"),
            duration: duration(4),
        },
    );
    // A final owned gap is a valid dormant definition. It has no physical output.
    document = edit_gap(
        &document,
        "dormant-gap",
        Command::SetGapOverride {
            node: id("inner"),
            iteration: plays[2].clone(),
            subtree: Subtree {
                root: id("gap-2"),
                nodes: BTreeMap::from([(id("gap-2"), hold(4))]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    );
    wire = serde_json::to_value(&document).unwrap();
    wire["audio_bindings"]["bindings"]
        .as_object_mut()
        .unwrap()
        .remove("tail");
    document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_eq!(document.duration().unwrap().frames(), 29);
    assert_eq!(
        document.audio_bindings().bindings()[&id("gap-0")]
            .lattice
            .reference
            .recipe,
        AudioRecipeKind::RepeatGap
    );
    assert!(
        !document
            .audio_bindings()
            .bindings()
            .contains_key(&id("tail"))
    );
    (document, source_reference, provider)
}

fn gap_target(
    document: &ProjectDocument,
    gap: u32,
    shared: bool,
) -> (ScopedNodeTarget, InstancePath) {
    let play = |name: &str, n| {
        let NodeKind::Repeat { iterations, .. } = &document.nodes()[&id(name)].kind else {
            unreachable!()
        };
        RepeatInstance {
            node: id(name),
            iteration: iterations.at(n).unwrap(),
        }
    };
    let instance = InstancePath {
        node: id(&format!("gap-{gap}")),
        repeats: vec![play("outer", 0), play("inner", gap)],
    };
    let target = ScopedNodeTarget {
        node: instance.node.clone(),
        repeats: instance
            .repeats
            .iter()
            .enumerate()
            .map(|(n, step)| RepeatEditStep {
                repeat: step.node.clone(),
                branch: if shared && n == 0 {
                    RepeatEditBranch::Default
                } else {
                    RepeatEditBranch::Play {
                        iteration: step.iteration.clone(),
                    }
                },
            })
            .collect(),
    };
    (target, instance)
}

fn plan_gaps(
    document: &ProjectDocument,
    selected: Option<[u32; 2]>,
    shared: bool,
    frames: u32,
) -> SemanticPlan {
    let (primary, instance) = gap_target(document, selected.map_or(2, |n| n[0]), shared);
    let dormant = selected.is_none_or(|n| n[0] == 2);
    let context = SemanticContext {
        original: None,
        scoped: None,
        hold_duration: None,
        shared_hold_duration: None,
        mixed_hold_duration: selected.is_none().then(|| {
            Box::new(SemanticMixedHoldDurationContext {
                root: id("outer"),
                target: primary.clone(),
                presentation: None,
            })
        }),
        selected_hold_duration: selected.map(|n| {
            Box::new(SemanticSelectedHoldDurationContext {
                root: id("outer"),
                target: primary,
                also: vec![gap_target(document, n[1], shared).0],
                presentation: (!dormant).then_some(instance),
            })
        }),
        parent: id("root"),
        cursor: ProjectFrame(8),
        selected_child: Some(id("outer")),
        visual_selection: None,
    };
    let planned = plan_semantic(
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
        RevisionId::new("owned-gap-final").unwrap(),
        |request| {
            let SemanticAllocationRequest::Isolation {
                step_index,
                nodes,
                marks,
            } = request
            else {
                panic!("duration allocation")
            };
            Ok(SemanticAllocation::Isolation {
                new_revision: RevisionId::new(format!("owned-gap-stage-{step_index}")).unwrap(),
                identities: OccurrenceIdentities {
                    nodes: (0..nodes)
                        .map(|n| id(&format!("owned-gap-{step_index}-{n}")))
                        .collect(),
                    marks: (0..marks)
                        .map(|n| MarkId::new(format!("owned-gap-mark-{step_index}-{n}")).unwrap())
                        .collect(),
                },
            })
        },
        |_, _| panic!("no Original resolver"),
    )
    .unwrap();
    let request = planned.request.as_ref().unwrap();
    let replay = replay_compound::<EditError>(document, request, |_| Ok(())).unwrap();
    assert_eq!(replay.document, planned.document);
    assert_eq!(
        replay.edit.inverse.apply(&planned.document).unwrap(),
        *document
    );
    assert_eq!(
        ProjectDocument::from_json(&planned.document.to_json().unwrap()).unwrap(),
        planned.document
    );
    let proof = derive_scoped_isolation(document, request, &planned.document).unwrap();
    if let Some(authority) = &context.selected_hold_duration {
        let mapped = planned.context.selected_hold_duration.as_ref().unwrap();
        assert_eq!(proof.map_forward(&authority.target).unwrap(), mapped.target);
        assert_eq!(
            proof.map_forward(&authority.also[0]).unwrap(),
            mapped.also[0]
        );
        assert_eq!(mapped.presentation.is_none(), dormant);
    }
    assert_eq!(
        planned.context.cursor,
        ProjectFrame(if dormant {
            8
        } else if selected.unwrap()[0] == 0 {
            3
        } else {
            8
        })
    );
    planned
}

fn assert_source_fragment(
    actual: &Recorded,
    reference: &Recorded,
    range: &Range<i64>,
    original_start: i64,
    original_end: i64,
    shift: i64,
    label: &str,
) {
    let entry = ntsc_boundary(original_start);
    let end = ntsc_boundary(original_end);
    for (offset, sample) in
        (ntsc_boundary(range.start + shift)..ntsc_boundary(range.end + shift)).enumerate()
    {
        let original = entry + offset;
        assert_eq!(
            actual.samples[sample],
            if original < end {
                reference.samples[original]
            } else {
                [0.; 2]
            },
            "{label}: historical Source sample {original}, moved {sample}"
        );
        assert_eq!(
            actual.suppressed[sample],
            original >= end || reference.suppressed[original],
            "{label}: historical Source suppression {original}, moved {sample}"
        );
    }
}

fn check_gap_pcm(room: bool) {
    let (document, source_reference, mut provider) = gap_fixture(room);
    let old = record(&document, &mut provider, false, &[256]);
    let source_pcm = record(&source_reference, &mut provider, false, &[239, 1]);
    // Authored allocation boundaries after the independent second gap grew.
    let ranges = [
        0..1,
        1..2,
        2..3,
        3..5,
        5..6,
        6..7,
        7..11,
        11..12,
        12..13,
        13..14,
        14..15,
        15..17,
        17..18,
        18..19,
        19..23,
        23..24,
        24..25,
        25..29,
    ];
    // Complete Source clocks before the historical split and gap growth.
    // A fragment may expose one extra sample on a shifted NTSC grid. The
    // neighboring split fragment has its own rounded entry, so its first sample
    // is not an oracle for that continuation. Read the complete Source instead.
    let source_origins = BTreeMap::from([
        (1, (1, 3)),
        (2, (2, 3)),
        (5, (5, 7)),
        (6, (6, 7)),
        (11, (9, 11)),
        (12, (10, 11)),
        (13, (11, 13)),
        (14, (12, 13)),
        (17, (15, 17)),
        (18, (16, 17)),
        (23, (19, 21)),
        (24, (20, 21)),
    ]);
    for range in &ranges {
        if let Some(&(start, end)) = source_origins.get(&range.start) {
            assert_source_fragment(
                &old,
                &source_pcm,
                range,
                start,
                end,
                0,
                "fixture Source clock",
            );
        }
    }
    for shared in [false, true] {
        // Reverse primary order proves later-first staging and final remapping.
        for selected in [[1, 0], [0, 1], [2, 0]] {
            let planned = plan_gaps(&document, Some(selected), shared, 3);
            let actual = record(&planned.document, &mut provider, false, &[127, 3, 251]);
            let selected_ranges = ranges
                .iter()
                .filter(|r| {
                    (r.start == 3 || (shared && r.start == 15))
                        || (selected[0] != 2 && (r.start == 7 || (shared && r.start == 19)))
                })
                .cloned()
                .collect::<Vec<_>>();
            let total_shift = selected_ranges
                .iter()
                .map(|r| 3 - (r.end - r.start))
                .sum::<i64>();
            assert_eq!(actual.samples.len(), ntsc_boundary(29 + total_shift));
            for range in &ranges {
                if selected_ranges.contains(range) {
                    continue;
                }
                let shift = selected_ranges
                    .iter()
                    .filter(|r| r.end <= range.start)
                    .map(|r| 3 - (r.end - r.start))
                    .sum::<i64>();
                let label = format!(
                    "owned-gap room={room} shared={shared} selected={selected:?} range={range:?}"
                );
                if let Some(&(start, end)) = source_origins.get(&range.start) {
                    assert_source_fragment(&actual, &source_pcm, range, start, end, shift, &label);
                    continue;
                }
                assert_moved(&actual, &old, range.start, range.end, shift, false, &label);
                let previous = ntsc_boundary(range.start)..ntsc_boundary(range.end);
                let moved = ntsc_boundary(range.start + shift)..ntsc_boundary(range.end + shift);
                for (offset, sample) in moved.enumerate() {
                    assert_eq!(
                        actual.suppressed[sample],
                        if previous.start + offset < previous.end {
                            old.suppressed[previous.start + offset]
                        } else {
                            true
                        }
                    );
                }
            }
            for range in &selected_ranges {
                let shift = selected_ranges
                    .iter()
                    .filter(|r| r.end <= range.start)
                    .map(|r| 3 - (r.end - r.start))
                    .sum::<i64>();
                let allocated =
                    ntsc_boundary(range.start + shift)..ntsc_boundary(range.start + shift + 3);
                if room {
                    // Fresh endpoint support, at the retained pre-edit absolute origin.
                    let origin = range.start - if range.start >= 13 { 2 } else { 0 };
                    let reference = document_with_asset(
                        document.presentation_basis().frame_rate,
                        &["quiet", "room"],
                        [
                            ("quiet", hold(origin)),
                            ("room", room_tone(3, audio_at_rate(321, 987, 44_100))),
                        ],
                        BTreeMap::new(),
                        audio_at_rate(0, 44_117, 44_100).span,
                    );
                    let expected = record(&reference, &mut provider, false, &[199, 7]);
                    assert_resumed(&actual, &expected, allocated, ntsc_boundary(origin));
                } else {
                    assert_pause(&actual, allocated);
                }
            }
            assert_eq!(
                record(&planned.document, &mut provider, false, &[1, 239]),
                actual,
                "owned-gap chunk independence"
            );
        }
    }
    // A duration edit to only the final gap changes authored intent, never output or clocks.
    let dormant = plan_gaps(&document, None, true, 5);
    assert_eq!(
        dormant.document.duration().unwrap(),
        document.duration().unwrap()
    );
    assert_eq!(dormant.document.audio_bindings(), document.audio_bindings());
    assert_eq!(
        record(&dormant.document, &mut provider, false, &[1, 239]),
        old
    );
}

#[test]
fn owned_gap_duration_compound_retains_partial_44100_source_and_suppression_clocks() {
    check_gap_pcm(false);
}

#[test]
fn owned_gap_room_tone_duration_compound_retains_materialized_phase_and_dormant_output() {
    check_gap_pcm(true);
}
