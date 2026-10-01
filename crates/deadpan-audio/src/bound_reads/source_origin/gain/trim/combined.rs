//! Mixed complete intent with two physical prefixes and retained chronology.
use super::*;

fn pair_with_history() -> (ProjectDocument, Provider) {
    let (base, provider) = linked(6, ExactRatio::ZERO, 7, span(-32032, 128128));
    let (right, _) = linked(2, ratio(-30000, 8008), 7, span(-32032, 128128));
    let mut wire = serde_json::to_value(&base).unwrap();
    wire["nodes"]["b"] = serde_json::to_value(&right.nodes()[&id("a")]).unwrap();
    wire["nodes"]["root"] = serde_json::to_value(BeatNode::sequence(
        "Pair",
        vec![id("lead"), id("a"), id("b")],
    ))
    .unwrap();
    let base = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let at = |lead| {
        let mut wire = serde_json::to_value(&base).unwrap();
        wire["nodes"]["lead"] = serde_json::to_value(hold(lead)).unwrap();
        ProjectDocument::from_json(&wire.to_string()).unwrap()
    };
    let mut wire = serde_json::to_value(&base).unwrap();
    wire["nodes"].as_object_mut().unwrap().remove("lead");
    wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("Old pair", vec![id("a"), id("b")])).unwrap();
    let old = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let first = at(1);
    let second = at(2);
    let mut wire = serde_json::to_value(at(4)).unwrap();
    wire["nodes"]["window"] = serde_json::to_value(partition("a", 2, 6)).unwrap();
    wire["nodes"]["root"] = serde_json::to_value(BeatNode::sequence(
        "Current pair",
        vec![id("lead"), id("window"), id("b")],
    ))
    .unwrap();
    let current = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let state = AudioBindingState::new(
        [&old, &first, &second]
            .into_iter()
            .enumerate()
            .map(|(n, doc)| AudioTimingRecord {
                id: historical_placement(u32::try_from(n).unwrap())
                    .reference
                    .timing,
                layout: FrozenAudioLayout::capture(doc).unwrap(),
            })
            .collect(),
        BTreeMap::from([(
            id("a"),
            OwnedAudioBinding {
                lattice: historical_placement(0),
                resume: Some(AudioResume {
                    local_boundary: ExactRatio::ZERO,
                    phase: AudioLocalPhase {
                        constant: ratio(64 * 5, 8008),
                        terms: vec![AudioPhaseTerm {
                            placement: historical_placement(0),
                            from_local: ExactRatio::ZERO,
                            to_local: ExactRatio::ONE,
                        }],
                    },
                }),
                reanchors: [(1, 2, 7), (2, 4, 8)]
                    .into_iter()
                    .map(|(n, start, end)| {
                        AudioReanchorStep::for_allocation(
                            historical_placement(n),
                            Some(
                                ExactFrameRange::new(
                                    ExactRatio::integer(start),
                                    ExactRatio::integer(end),
                                )
                                .unwrap(),
                            ),
                        )
                    })
                    .collect(),
            },
        )]),
    )
    .unwrap();
    (with_bindings(&current, &state), provider)
}

#[test]
fn combined_mixed_intent_preserves_old_phase_through_distinct_source_prefixes() {
    let (before, mut provider) = pair_with_history();
    let immutable = before.clone();
    // Historical A: 64 + 1602 + 1601 + 1601 - independent offset7 =4861.
    let old_a = source_oracle(&provider, 0..8197, ExactRatio::integer(4861), 256);
    same_pcm(
        &pcm(&before, &mut provider, 6406..6662, 193, false),
        &old_a,
        "A old chronology",
    );
    // B begins at frame8: B(8)-8*(8008/5)+6000-7 =5993.2.
    let old_b = source_oracle(&provider, 5993..8197, ratio(29966, 5), 256);
    same_pcm(
        &pcm(&before, &mut provider, 12813..13069, 193, false),
        &old_b,
        "B old phase",
    );
    let intent = SourceTrimIntent {
        in_frames: -3,
        out_frames: 2,
        slip_frames: 1,
        roll_frames: -2,
        policy: SourceTrimPolicy::Ripple,
    };
    let resolution = before
        .source_trim_edit(&id("root"), &id("window"), Some(&id("b")), intent)
        .unwrap();
    assert_eq!(resolution.geometry.target.physical_prefix, frames(1));
    assert_eq!(
        resolution.geometry.right.as_ref().unwrap().physical_prefix,
        frames(2)
    );
    assert_eq!(resolution.required_split_nodes, 0);
    assert_eq!(resolution.required_filler_nodes, 0);
    let allocation = revision("combined-mixed");
    let tx = apply(
        &before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: allocation.clone(),
            command: Command::ApplySourceTrim {
                parent: id("root"),
                node: id("window"),
                right: Some(id("b")),
                intent,
                resources: SourceTrimResources {
                    target_wrapper: resolution
                        .required_target_wrapper
                        .then(|| id("new-a-window")),
                    right_wrapper: resolution
                        .required_right_wrapper
                        .then(|| id("new-b-window")),
                    split: SplitIdentities::default(),
                    fillers: vec![],
                    timing: Some(AudioTimingId {
                        allocation,
                        ordinal: 0,
                    }),
                },
            },
        },
    )
    .unwrap();
    let after = tx.forward.apply(&before).unwrap();
    assert_eq!(before, immutable);
    assert_eq!(after.duration().unwrap(), frames(15));
    assert_eq!(tx.duration_delta, 5);
    let rebased = before.audio_bindings().bindings()[&id("a")]
        .rebase_local(ExactRatio::ONE)
        .unwrap();
    let actual = &after.audio_bindings().bindings()[&id("a")];
    assert_eq!(actual.lattice, rebased.lattice);
    assert_eq!(actual.resume, rebased.resume);
    assert_eq!(&actual.reanchors[..2], rebased.reanchors.as_slice());
    assert_eq!(actual.reanchors.len(), 3);
    assert_eq!(
        after.audio_bindings().bindings()[&id("b")].reanchors.len(),
        1
    );
    for (id, layout) in before.audio_bindings().timings() {
        assert_eq!(after.audio_bindings().timings().get(id), Some(layout));
    }
    // A's old entry moves4->7: B7-B4=4805. Slip adds8008/5 exactly.
    // B's old entry moves8->13; its new prefix starts11. B13-B11=3203.
    // Filter support follows the selected media, independently of those clocks:
    // A still uses the full0..8197; B grows5993..8197 to2790..8197.
    for (label, start, support, phase) in [
        ("A prefix", 6406, 0..8197, ratio(8288, 5)),
        ("A retained body plus Slip", 11211, 0..8197, ratio(32313, 5)),
        ("B separate prefix", 17618, 2790..8197, ratio(13951, 5)),
        ("B retained body", 20821, 2790..8197, ratio(29966, 5)),
    ] {
        let expected = source_oracle(&provider, support.clone(), phase, 256);
        assert!(
            expected.iter().flatten().any(|x| x.abs() > 0.001),
            "{label}"
        );
        assert_ne!(
            expected,
            source_oracle(
                &provider,
                support,
                phase.checked_add(ExactRatio::ONE).unwrap(),
                256
            )
        );
        for chunk in [193, 239] {
            same_pcm(
                &pcm(&after, &mut provider, start..start + 256, chunk, false),
                &expected,
                label,
            );
        }
    }
    // A's explicit Hard policy wins at its new consuming edge, without changing
    // raw support. This also checks that the new edge does not add a second ramp.
    let prefix = source_oracle(&provider, 0..8197, ratio(8288, 5), 256);
    for chunk in [193, 239] {
        same_pcm(
            &pcm(&after, &mut provider, 6406..6662, chunk, true),
            &prefix,
            "Hard prefix",
        );
    }
    let restored = tx.inverse.apply(&after).unwrap();
    assert_eq!(restored, immutable);
    same_pcm(
        &pcm(&restored, &mut provider, 6406..6662, 239, false),
        &old_a,
        "inverse chronology",
    );
    same_pcm(
        &pcm(&restored, &mut provider, 12813..13069, 239, false),
        &old_b,
        "inverse right support",
    );
}
