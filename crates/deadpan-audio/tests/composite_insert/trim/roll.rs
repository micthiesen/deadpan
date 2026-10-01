//! Real44.1kHz PCM and unchanged composite suffixes through fixed-duration Roll.
use super::*;

fn fixture() -> ProjectDocument {
    let mut repeated = repeat("repeated-voice", 2);
    let NodeKind::Repeat { gap, .. } = &mut repeated.kind else {
        unreachable!()
    };
    *gap = Some(room(1, 700..921));
    let base = document(
        ntsc(),
        &["lead", "group", "outer"],
        vec![
            ("lead", silence(1)),
            (
                "group",
                BeatNode::sequence(
                    "Nested",
                    vec![id("target"), id("right"), id("repeat"), id("stage")],
                ),
            ),
            ("target", source(ntsc(), 2)),
            ("right", source(ntsc(), 2)),
            ("repeat", repeated),
            ("repeated-voice", source(ntsc(), 1)),
            ("stage", preserve("stretched-voice", 4, 12)),
            ("stretched-voice", source(ntsc(), 4)),
            ("outer", source(ntsc(), 3)),
        ],
    );
    let linked = linked_target(&base, 2, true);
    let mut target = linked.nodes()[&id("target")].clone();
    let NodeKind::Source { source } = &mut target.kind else {
        unreachable!()
    };
    source.audio_offset = AudioSample(17);
    let SourceAudioMapping::SelectedPlacement { selection, .. } = &mut source.audio_mapping else {
        unreachable!()
    };
    *selection = ExactFrameRange::new(ratio(-85, 8008), ratio(15931, 8008)).unwrap();
    let mut wire = serde_json::to_value(linked).unwrap();
    wire["nodes"]["target"] = serde_json::to_value(&target).unwrap();
    wire["nodes"]["right"] = serde_json::to_value(target).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn roll_44100_keeps_exact_offset_kernel_and_every_composite_suffix_at_its_absolute_position() {
    let before = fixture();
    let immutable = before.clone();
    let resolution = before
        .source_roll(&id("group"), &id("target"), &id("right"), 1)
        .unwrap();
    assert_eq!(resolution.applied_delta_frames, 1);
    assert!(!resolution.left.needs_wrapper);
    assert!(resolution.right.needs_wrapper);
    let allocation = revision("roll-44100");
    let tx = apply(
        &before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: allocation.clone(),
            command: Command::RollSources {
                parent: id("group"),
                left: id("target"),
                right: id("right"),
                delta_frames: 1,
                left_wrapper: None,
                right_wrapper: Some(id("roll-right")),
                timing: AudioTimingId {
                    allocation,
                    ordinal: 0,
                },
            },
        },
    )
    .unwrap();
    assert_eq!(before, immutable);
    let after = tx.forward.apply(&before).unwrap();
    for document in [&before, &after] {
        assert_eq!(
            RenderPlan::compile(document)
                .unwrap()
                .audio_duration()
                .unwrap(),
            AudioSample(36837)
        );
    }
    let mut provider = Provider::new();
    // Source support and positions are Original44.1kHz coordinates, obtained
    // from mix coordinates by147/160. Left grows W0..3, right retains W0..2.
    // Independent+17mix-sample offset is subtracted once before resampling.
    for (range, support, phase, entry) in [
        (6310..6406, 5497..9912, ratio(7858179, 800), false),
        (6406..6502, 5497..8440, ratio(5574387, 800), true),
    ] {
        let raw = oracle(&provider, support.clone(), phase, 96);
        let faded: Vec<_> = raw
            .iter()
            .enumerate()
            .map(|(at, sample)| {
                let distance = if entry { at } else { 95 - at };
                let gain = ((distance as f64 + 0.5) / 96.0) as f32;
                [sample[0] * gain, sample[1] * gain]
            })
            .collect();
        assert_ne!(
            raw,
            oracle(
                &provider,
                support,
                phase.checked_add(ratio(147, 160)).unwrap(),
                96
            )
        );
        for chunk in [31, 73] {
            exact(
                &read_chunks(&after, &mut provider, range.start, 96, chunk, false),
                &raw,
                "44.1kHz Roll raw seam",
            );
            exact(
                &read_chunks(&after, &mut provider, range.start, 96, chunk, true),
                &faded,
                "44.1kHz Roll incident ramp",
            );
        }
    }
    let tail = oracle(&provider, 5497..9912, ratio(1688001, 200), 256);
    for chunk in [193, 239] {
        exact(
            &read_chunks(&after, &mut provider, 4805, 256, chunk, false),
            &tail,
            "new left tail phase",
        );
    }
    let first_play = expected(&provider, ExactRatio::ZERO, 256);
    for document in [&before, &after] {
        exact(
            &read_chunks(document, &mut provider, 8008, 256, 193, false),
            &first_play,
            "Repeat starts at exact B5",
        );
    }
    for (label, start, count) in [
        ("Repeat play", 8008, 1602),
        ("gap", 9610, 1601),
        ("second play", 11211, 1602),
        ("Preserve output", 12813, 19219),
        ("outer sibling", 32032, 4805),
    ] {
        for faded in [false, true] {
            let expected = read_chunks(&before, &mut provider, start, count, 193, faded);
            assert!(
                expected.iter().flatten().any(|sample| sample.abs() > 0.001),
                "{label}"
            );
            for chunk in [193, 239] {
                exact(
                    &read_chunks(&after, &mut provider, start, count, chunk, faded),
                    &expected,
                    label,
                );
            }
        }
    }
    for node in ["repeat", "stage", "outer"] {
        assert_eq!(after.nodes()[&id(node)], before.nodes()[&id(node)]);
    }
    let restored = tx.inverse.apply(&after).unwrap();
    assert_eq!(restored, before);
    exact(
        &read_chunks(&restored, &mut provider, 12813, 512, 251, true),
        &read_chunks(&before, &mut provider, 12813, 512, 193, true),
        "composite inverse",
    );
}
