//! Roll keeps absolute sample labels fixed. Expected source phases below are
//! literal arithmetic on 8008/5 mix samples per frame, independent of the plan.
use super::*;

fn pair(lead: i64, right_start: ExactRatio, offset: i64) -> (ProjectDocument, Provider) {
    let (left, provider) = linked(2, ExactRatio::ZERO, offset, span(0, 20_000));
    let (right, _) = linked(2, right_start, offset, span(0, 20_000));
    let mut a = left.nodes()[&id("a")].clone();
    let mut b = right.nodes()[&id("a")].clone();
    a.audio_edges = Default::default();
    b.audio_edges = Default::default();
    let mut wire = serde_json::to_value(left).unwrap();
    wire["nodes"]["a"] = serde_json::to_value(a).unwrap();
    wire["nodes"]["b"] = serde_json::to_value(b).unwrap();
    wire["nodes"]["lead"] = serde_json::to_value(hold(lead)).unwrap();
    wire["nodes"]["root"] = serde_json::to_value(BeatNode::sequence(
        "Pair",
        vec![id("lead"), id("a"), id("b")],
    ))
    .unwrap();
    (
        ProjectDocument::from_json(&wire.to_string()).unwrap(),
        provider,
    )
}

fn rolled(
    before: &ProjectDocument,
    left: &str,
    right: &str,
    delta: i64,
    prefix: i64,
) -> (ProjectDocument, EditTransaction) {
    let immutable = before.clone();
    let resolved = before
        .source_roll(&id("root"), &id(left), &id(right), delta)
        .unwrap();
    assert_eq!(
        resolved.applied_delta_frames, delta,
        "fixture must not clamp"
    );
    assert_eq!(resolved.left.physical_prefix, frames(0));
    assert_eq!(resolved.right.physical_prefix, frames(prefix));
    let allocation = revision("roll-result");
    let tx = apply(
        before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: allocation.clone(),
            command: Command::RollSources {
                parent: id("root"),
                left: id(left),
                right: id(right),
                delta_frames: delta,
                left_wrapper: resolved.left.needs_wrapper.then(|| id("roll-left")),
                right_wrapper: resolved.right.needs_wrapper.then(|| id("roll-right")),
                timing: AudioTimingId {
                    allocation,
                    ordinal: 0,
                },
            },
        },
    )
    .unwrap();
    assert_eq!(before, &immutable);
    let after = tx.forward.apply(before).unwrap();
    for (timing, layout) in before.audio_bindings().timings() {
        assert_eq!(after.audio_bindings().timings().get(timing), Some(layout));
    }
    assert_eq!(tx.inverse.apply(&after).unwrap(), *before);
    (after, tx)
}

fn seam_ramp(raw: &[[f32; 2]], entry: bool) -> Vec<[f32; 2]> {
    assert_eq!(raw.len(), 96);
    raw.iter()
        .enumerate()
        .map(|(at, sample)| {
            let distance = if entry { at } else { 95 - at };
            let gain = ((distance as f64 + 0.5) / 96.0) as f32;
            [sample[0] * gain, sample[1] * gain]
        })
        .collect()
}

#[test]
fn roll_ntsc_transfers_1601_samples_and_marks_only_the_changed_join() {
    let (plain, mut provider) = pair(1, ratio(-15_000, 8008), 17);
    for (name, before) in [
        ("unbound", plain.clone()),
        ("captured", captured(&plain, None)),
    ] {
        let old_head = pcm(&before, &mut provider, 1619..1715, 31, true);
        let old_tail = pcm(&before, &mut provider, 7912..8008, 31, true);
        let old_right = source_oracle(&provider, 2983..6187, ratio(22921, 5), 256);
        same_pcm(
            &pcm(&before, &mut provider, 6406..6662, 193, false),
            &old_right,
            "independent right baseline",
        );
        let (after, tx) = rolled(&before, "a", "b", 1, 0);
        assert_eq!(
            RenderPlan::compile(&after)
                .unwrap()
                .audio_duration()
                .unwrap(),
            AudioSample(8008)
        );
        if name == "captured" {
            assert_eq!(after.audio_bindings(), before.audio_bindings());
        } else {
            assert_eq!(after.audio_bindings().timings().len(), 1);
        }
        // B(3)=4805, B(4)=6406. Exactly1601 newly delivered samples
        // move to the left, whose source phase is sample-1601.6-17.
        let transferred = source_oracle(&provider, 0..4788, ratio(15932, 5), 1601);
        assert_ne!(
            transferred,
            source_oracle(&provider, 0..4788, ratio(15937, 5), 1601)
        );
        for chunk in [193, 239] {
            same_pcm(
                &pcm(&after, &mut provider, 4805..6406, chunk, false),
                &transferred,
                name,
            );
            same_pcm(
                &pcm(&after, &mut provider, 6406..6662, chunk, false),
                &old_right,
                "right absolute phase unchanged",
            );
            same_pcm(
                &pcm(&after, &mut provider, 1619..1715, chunk, true),
                &old_head,
                "outside In fade unchanged",
            );
            same_pcm(
                &pcm(&after, &mut provider, 7912..8008, chunk, true),
                &old_tail,
                "outside Out fade unchanged",
            );
        }
        for (range, support, phase, entry) in [
            (6310..6406, 0..4788, ratio(23457, 5), false),
            (6406..6502, 2983..6187, ratio(22921, 5), true),
        ] {
            let raw = source_oracle(&provider, support, phase, 96);
            let faded = seam_ramp(&raw, entry);
            assert_ne!(raw, faded);
            for chunk in [31, 73] {
                same_pcm(
                    &pcm(&after, &mut provider, range.clone(), chunk, false),
                    &raw,
                    "new seam raw",
                );
                same_pcm(
                    &pcm(&after, &mut provider, range.clone(), chunk, true),
                    &faded,
                    "new seam single fade",
                );
            }
        }
        let restored = tx.inverse.apply(&after).unwrap();
        same_pcm(
            &pcm(&restored, &mut provider, 6406..6662, 251, false),
            &old_right,
            "exact inverse PCM",
        );
    }
}

#[test]
fn reverse_roll_grows_the_right_physical_prefix_without_shifting_its_retained_body() {
    let (plain, mut provider) = pair(2, ratio(-30_000, 8008), 17);
    let before = captured(&plain, None);
    let body = source_oracle(&provider, 5983..8197, ratio(30413, 5), 256);
    same_pcm(
        &pcm(&before, &mut provider, 6506..6762, 193, false),
        &body,
        "old right body",
    );
    let (after, _) = rolled(&before, "a", "b", -1, 1);
    assert_eq!(
        RenderPlan::compile(&after)
            .unwrap()
            .audio_duration()
            .unwrap(),
        AudioSample(9610)
    );
    assert_eq!(
        after.audio_bindings().bindings()[&id("a")],
        before.audio_bindings().bindings()[&id("a")]
    );
    assert_eq!(
        after.audio_bindings().bindings()[&id("b")],
        before.audio_bindings().bindings()[&id("b")]
            .rebase_local(ExactRatio::ONE)
            .unwrap()
    );
    let prefix = source_oracle(&provider, 4382..8197, ratio(22408, 5), 256);
    let expanded_body = source_oracle(&provider, 4382..8197, ratio(30413, 5), 256);
    // The first body probe is only 99.6 samples beyond the old support edge,
    // inside the sinc radius of 128. Extension changes those filtering taps.
    assert_ne!(expanded_body, body);
    let interior = source_oracle(&provider, 5983..8197, ratio(30913, 5), 256);
    same_pcm(
        &pcm(&before, &mut provider, 6606..6862, 193, false),
        &interior,
        "old body beyond the changed filter halo",
    );
    // New seam B(3)=4805; sample4905 is100 samples into the prefix.
    // 4905-6406.4+6000-17=4481.6. Old body sample6506 remains6082.6.
    assert_ne!(
        prefix,
        source_oracle(&provider, 4382..8197, ratio(22413, 5), 256)
    );
    for chunk in [193, 239] {
        same_pcm(
            &pcm(&after, &mut provider, 4905..5161, chunk, false),
            &prefix,
            "audible physical prefix",
        );
        same_pcm(
            &pcm(&after, &mut provider, 6506..6762, chunk, false),
            &expanded_body,
            "retained phase with expanded filtering support",
        );
        same_pcm(
            &pcm(&after, &mut provider, 6606..6862, chunk, false),
            &interior,
            "absolute retained body",
        );
    }
}

#[test]
fn roll_exposes_dormant_right_audio_but_keeps_intentionally_absent_audio_silent() {
    let (base, mut provider) = pair(2, ratio(-44_989, 8008), 7);
    for (name, bound, absent) in [
        ("dormant", false, false),
        ("bound dormant", true, false),
        ("absent", true, true),
    ] {
        let mut wire = serde_json::to_value(&base).unwrap();
        if absent {
            let mut node = base.nodes()[&id("b")].clone();
            let NodeKind::Source { source } = &mut node.kind else {
                unreachable!()
            };
            source.audio = None;
            source.audio_mapping = SourceAudioMapping::FitBeat;
            source.audio_offset = AudioSample(0);
            source.link = LinkRelation::Independent;
            wire["nodes"]["b"] = serde_json::to_value(node).unwrap();
        }
        let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let before = if bound {
            captured(&before, None)
        } else {
            before
        };
        provider.calls = 0;
        assert!(
            pcm(&before, &mut provider, 6406..6662, 193, false)
                .iter()
                .flatten()
                .all(|sample| sample.to_bits() == 0)
        );
        assert_eq!(provider.calls, 0);
        let (after, tx) = rolled(&before, "a", "b", -1, 1);
        let expected = if absent {
            vec![[0.; 2]; 256]
        } else {
            // (44989/5)-7 + (4805-6406.4)=7389.4.
            source_oracle(&provider, 7390..8197, ratio(36947, 5), 256)
        };
        provider.calls = 0;
        for chunk in [193, 239] {
            same_pcm(
                &pcm(&after, &mut provider, 4805..5061, chunk, false),
                &expected,
                name,
            );
            assert!(
                pcm(&after, &mut provider, 6406..6662, chunk, false)
                    .iter()
                    .flatten()
                    .all(|sample| sample.to_bits() == 0)
            );
        }
        if absent {
            assert_eq!(provider.calls, 0);
        } else {
            assert!(expected.iter().flatten().any(|sample| sample.abs() > 0.001));
        }
        let restored = tx.inverse.apply(&after).unwrap();
        provider.calls = 0;
        assert!(
            pcm(&restored, &mut provider, 6406..6662, 251, false)
                .iter()
                .flatten()
                .all(|sample| sample.to_bits() == 0)
        );
        assert_eq!(provider.calls, 0);
    }
}

#[test]
fn roll_right_prefix_preserves_symbolic_resume_and_chronological_reanchors() {
    let (base, mut provider) = linked(6, ExactRatio::ZERO, 7, span(-32032, 128128));
    let layout_at = |lead: i64| {
        let mut wire = serde_json::to_value(&base).unwrap();
        wire["nodes"]["lead"] = serde_json::to_value(hold(lead)).unwrap();
        ProjectDocument::from_json(&wire.to_string()).unwrap()
    };
    let mut old_wire = serde_json::to_value(&base).unwrap();
    old_wire["nodes"].as_object_mut().unwrap().remove("lead");
    old_wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("Root", vec![id("a")])).unwrap();
    let old = ProjectDocument::from_json(&old_wire.to_string()).unwrap();
    let first = layout_at(1);
    let second = layout_at(2);
    let mut wire = serde_json::to_value(layout_at(4)).unwrap();
    wire["nodes"]["window"] = serde_json::to_value(partition("a", 2, 6)).unwrap();
    wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("Root", vec![id("lead"), id("window")])).unwrap();
    // Give the old four-frame lead real Source handles without moving a.
    let mut left = base.nodes()[&id("a")].clone();
    let NodeKind::Source { source } = &mut left.kind else {
        unreachable!()
    };
    source.duration = frames(4);
    source.edit_window =
        Some(SourceEditWindow::new(ExactRatio::ZERO, ExactRatio::integer(4)).unwrap());
    let SourceVideoMapping::SelectedPlacement { selection, .. } = &mut source.video_mapping else {
        unreachable!()
    };
    *selection = ExactFrameRange::new(ExactRatio::ZERO, ExactRatio::integer(4)).unwrap();
    let SourceAudioMapping::SelectedPlacement { selection, .. } = &mut source.audio_mapping else {
        unreachable!()
    };
    *selection = ExactFrameRange::new(ExactRatio::ZERO, ratio(31_997, 8008)).unwrap();
    wire["nodes"]["lead"] = serde_json::to_value(left).unwrap();
    let current = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let state = AudioBindingState::new(
        [&old, &first, &second]
            .into_iter()
            .enumerate()
            .map(|(ordinal, document)| AudioTimingRecord {
                id: historical_placement(u32::try_from(ordinal).unwrap())
                    .reference
                    .timing,
                layout: FrozenAudioLayout::capture(document).unwrap(),
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
                    .map(|(ordinal, start, end)| AudioReanchorStep {
                        anchor: Default::default(),
                        placement: historical_placement(ordinal),
                        window: Some(
                            ExactFrameRange::new(
                                ExactRatio::integer(start),
                                ExactRatio::integer(end),
                            )
                            .unwrap(),
                        ),
                    })
                    .collect(),
            },
        )]),
    )
    .unwrap();
    let before = with_bindings(&current, &state);
    // Historic owner sample64+1602+1601+1601=4868; offset7 occurs once.
    let body = source_oracle(&provider, 0..8197, ExactRatio::integer(4861), 256);
    same_pcm(
        &pcm(&before, &mut provider, 6406..6662, 193, false),
        &body,
        "chronological baseline",
    );
    let old_binding = before.audio_bindings().bindings()[&id("a")].clone();
    let (after, tx) = rolled(&before, "lead", "window", -3, 1);
    assert_eq!(
        after.audio_bindings().bindings()[&id("a")],
        old_binding.rebase_local(ExactRatio::ONE).unwrap()
    );
    // Fixed absolute positions: B(4)-B(1)=6406-1602=4804, so
    // prefix phase is4861-4804=57. A ripple-style4805 shift is wrong.
    let prefix = source_oracle(&provider, 0..8197, ExactRatio::integer(57), 256);
    assert_ne!(
        prefix,
        source_oracle(&provider, 0..8197, ExactRatio::integer(56), 256)
    );
    for chunk in [193, 239] {
        same_pcm(
            &pcm(&after, &mut provider, 1602..1858, chunk, false),
            &prefix,
            "resumed Roll prefix",
        );
        same_pcm(
            &pcm(&after, &mut provider, 6406..6662, chunk, false),
            &body,
            "resumed body remains at same labels",
        );
    }
    let restored = tx.inverse.apply(&after).unwrap();
    same_pcm(
        &pcm(&restored, &mut provider, 6406..6662, 251, false),
        &body,
        "chronological inverse",
    );
}

#[test]
fn roll_preserves_explicit_hard_on_one_side_without_suppressing_the_other_sides_default_fade() {
    let (plain, mut provider) = pair(1, ratio(-15_000, 8008), 17);
    let mut node = plain.nodes()[&id("a")].clone();
    node.audio_edges.node_end = AudioEdgePolicy::Hard;
    let mut wire = serde_json::to_value(plain).unwrap();
    wire["nodes"]["a"] = serde_json::to_value(node).unwrap();
    let before = captured(
        &ProjectDocument::from_json(&wire.to_string()).unwrap(),
        None,
    );
    let (after, _) = rolled(&before, "a", "b", 1, 0);
    let outgoing = source_oracle(&provider, 0..4788, ratio(23457, 5), 96);
    let incoming = source_oracle(&provider, 2983..6187, ratio(22921, 5), 96);
    for chunk in [31, 73] {
        same_pcm(
            &pcm(&after, &mut provider, 6310..6406, chunk, true),
            &outgoing,
            "explicit outgoing Hard wins",
        );
        same_pcm(
            &pcm(&after, &mut provider, 6406..6502, chunk, true),
            &seam_ramp(&incoming, true),
            "incoming default fade remains",
        );
    }
}
