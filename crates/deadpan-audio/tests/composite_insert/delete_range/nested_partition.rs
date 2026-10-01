//! Decoded PCM witnesses for recursive unity-window deletion admission.
use super::*;

fn treated(mut node: BeatNode, millidb: i32) -> BeatNode {
    node.audio_treatments = AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::new(millidb).unwrap(), false, vec![], vec![]).unwrap(),
    );
    node
}

// Both fixtures expose leaf frames [3,10). Treated chains already had recursive
// admission; these preservation cases complement the untreated witnesses below.
fn nested(depth: usize, leaf: BeatNode) -> ProjectDocument {
    let mut nodes = vec![
        ("leaf", treated(leaf, 6000)),
        ("inner", treated(partition("leaf", 1..12), -3000)),
    ];
    if depth == 2 {
        nodes.push(("outer", partition("inner", 2..9)));
    } else {
        assert_eq!(depth, 3);
        nodes.push(("middle", partition("inner", 1..10)));
        nodes.push(("outer", partition("middle", 1..8)));
    }
    let original = document(ntsc(), &["outer"], nodes);
    let mut wire = serde_json::to_value(original).unwrap();
    wire["nodes"]["root"]["audio_treatments"] =
        serde_json::to_value(treated(BeatNode::sequence("", vec![]), -6000).audio_treatments)
            .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn source_reference(provider: &Provider, phase: ExactRatio, count: usize) -> Vec<[f32; 2]> {
    (0..count)
        .step_by(256)
        .flat_map(|offset| {
            expected(
                provider,
                phase
                    .checked_add(ExactRatio::integer(i64::try_from(offset).unwrap()))
                    .unwrap(),
                u32::try_from((count - offset).min(256)).unwrap(),
            )
        })
        .collect()
}

fn room_samples(wave: &[[f32; 2]], phase: ExactRatio, count: usize) -> Vec<[f32; 2]> {
    (0..count)
        .step_by(256)
        .flat_map(|offset| {
            sample_reference(
                wave,
                phase
                    .checked_add(ExactRatio::integer(i64::try_from(offset).unwrap()))
                    .unwrap(),
                ExactRatio::ONE,
                u32::try_from((count - offset).min(256)).unwrap(),
            )
        })
        .collect()
}

#[test]
fn two_and_three_windows_retain_source_and_roomtone_prefix_suffix_and_owned_gain() {
    let mut provider = Provider::new();
    // Twelve-frame RoomTone host has ceil(12*8008/5)=19220 canonical points.
    let room_wave = room_reference(&provider, 700..921, 19220);
    for depth in [2, 3] {
        for roomtone in [false, true] {
            let leaf = if roomtone {
                BeatNode::hold("Room tone", room(12, 700..921))
            } else {
                source(ntsc(), 12)
            };
            let before = nested(depth, leaf);
            let after = cut(&before, "root", 1..3, "nested-window-cut");
            // Windows add exactly three owner frames (4804.8 points).
            // B1=1602, B3=4805, B7=11211 and B5=8008. Retain original
            // [0,1602) and [4805,11211), with no iterative duration rounding.
            for (old, new, count, phase) in [
                (0, 0, 1602, ratio(24024, 5)),
                (4805, 1602, 6406, ratio(48049, 5)),
            ] {
                let oracle = if roomtone {
                    room_samples(&room_wave, phase, count)
                } else {
                    source_reference(&provider, phase, count)
                };
                let saved = pcm(&before, &mut provider, old, count);
                assert_close(&saved, &oracle);
                // Every retained sample is bit-identical on cold terminal,
                // reverse irregular and repeated warm reads.
                retained(&after, &mut provider, new, &saved);
                let mut reader = renderer(&after, &mut provider);
                let authored = reader
                    .prepare_authored_bus(
                        &mut provider,
                        AudioSample(new + 512),
                        128,
                        TIMEOUT,
                        &AtomicBool::new(false),
                    )
                    .unwrap();
                // Leaf +6, retained intermediate -3 and live root -6 dB,
                // applied once. Interior samples avoid automatic edit fades.
                let gain = 10_f64.powf(-3000. / 20000.);
                let gained: Vec<_> = oracle[512..640]
                    .iter()
                    .map(|sample| sample.map(|x| (f64::from(x) * gain) as f32))
                    .collect();
                assert_close(&authored.samples, &gained);
            }
        }
    }
}

#[test]
fn different_nested_children_keep_partial_source_and_roomtone_with_a_real_extra_sample() {
    let before = document(
        ntsc(),
        &["a-window", "b-window"],
        vec![
            ("a-window", partition("a-inner", 2..9)),
            ("a-inner", treated(partition("a-leaf", 1..12), -3000)),
            ("a-leaf", source(ntsc(), 12)),
            ("b-window", partition("b-middle", 1..8)),
            ("b-middle", partition("b-inner", 1..10)),
            ("b-inner", treated(partition("b-leaf", 1..12), 3000)),
            ("b-leaf", BeatNode::hold("Room tone", room(12, 700..921))),
        ],
    );
    let after = cut(&before, "root", 2..10, "different-window-cut");
    let mut provider = Provider::new();
    let prefix = pcm(&before, &mut provider, 0, 3203);
    assert_close(&prefix, &source_reference(&provider, ratio(24024, 5), 3203));
    retained(&after, &mut provider, 0, &prefix);
    let room_wave = room_reference(&provider, 700..921, 19220);
    // The right leaf starts at exact global frame4 behind three windows.
    // Old global frame10 starts at B10-4*8008/5 = 9609.6 leaf points.
    // Old [10,14) has 6406 samples, new [2,6) has 6407. Its extra point
    // at leaf16015.6 remains within the complete twelve-frame RoomTone owner.
    let oracle = room_samples(&room_wave, ratio(48048, 5), 6407);
    let saved = pcm(&before, &mut provider, 16016, 6406);
    assert_close(&saved, &oracle[..6406]);
    let actual = pcm(&after, &mut provider, 3203, 6407);
    assert_close(&actual, &oracle);
    assert_eq!(&actual[..6406], saved);
    assert_ne!(actual[6406], [0.; 2]);
    retained(&after, &mut provider, 3203, &actual);
}

#[test]
fn nested_terminal_allocation_keeps_the_last_supported_sample_and_suppresses_only_the_extra() {
    let before = document(
        ntsc(),
        &["outer"],
        vec![
            ("outer", partition("middle", 0..4)),
            ("middle", partition("inner", 0..4)),
            ("inner", partition("leaf", 0..4)),
            ("leaf", source(ntsc(), 4)),
        ],
    );
    let after = cut(&before, "root", 2..3, "nested-terminal-cut");
    let mut provider = Provider::new();
    let prefix = pcm(&before, &mut provider, 0, 3203);
    retained(&after, &mut provider, 0, &prefix);
    // B4-B3=1601 old points; B3-B2=1602 destination points. The saved
    // root support is [0,6406), although its continuous extent is 6406.4.
    // Physical Source support ends at ceil(100+4*1471.47)=5986.
    let oracle: Vec<_> = (0..1601)
        .step_by(256)
        .flat_map(|offset| {
            provider
                .prepared
                .prepare(
                    ResampleRecipe::new(
                        100..5986,
                        ratio(100 * 160 + 4805 * 147, 160),
                        AudioSample(0),
                        ratio(147, 160),
                        AudioSample(0)..AudioSample(1601),
                    )
                    .unwrap(),
                    AudioSample(offset),
                    u32::try_from((1601 - offset).min(256)).unwrap(),
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples
        })
        .collect();
    let saved = pcm(&before, &mut provider, 4805, 1601);
    assert_close(&saved, &oracle);
    assert_ne!(saved[1600], [0.; 2]);
    let mut allocated = saved;
    allocated.push([0.; 2]);
    retained(&after, &mut provider, 3203, &allocated);
    let mut cold = renderer(&after, &mut provider);
    let extra = cold
        .read(
            &mut provider,
            AudioSample(4804),
            1,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(extra.samples, vec![[0.; 2]]);
    assert_eq!(extra.suppressed, vec![AudioSample(4804)..AudioSample(4805)]);
}

#[test]
fn repeated_nested_deletions_transform_the_existing_root_sound_route_exactly_once() {
    let mut leaf = source(ntsc(), 8);
    let NodeKind::Source { source } = &mut leaf.kind else {
        unreachable!()
    };
    source.video = SourceVideo::Still {
        asset: AssetId::new("media").unwrap(),
    };
    source.audio = None;
    source.audio_mapping = SourceAudioMapping::FitBeat;
    let original = document(
        ntsc(),
        &["outer"],
        vec![
            ("outer", partition("middle", 0..8)),
            ("middle", partition("inner", 0..8)),
            ("inner", partition("leaf", 0..8)),
            ("leaf", leaf),
        ],
    );
    let mut wire = serde_json::to_value(original).unwrap();
    wire["assets"]["media"]["source_qualification"] =
        serde_json::to_value(SourceQualificationId::new("b".repeat(64)).unwrap()).unwrap();
    let original = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let original = edit(
        &original,
        "root-event",
        Command::SetSound {
            id: SoundId::new("effect").unwrap(),
            event: SoundEvent {
                owner: id("root"),
                label: "Independent root bus".into(),
                source: audio(100..20_100),
                mapping: SourceAudioMapping::SelectedPlacement {
                    start: ExactRatio::ZERO,
                    frames: ratio(2_000_000, 147_147),
                    selection: ExactFrameRange::new(ExactRatio::ZERO, ExactRatio::integer(8))
                        .unwrap(),
                },
                offset: AudioSample(0),
                gain_millidecibels: 0,
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        },
    );
    let before = cut(&original, "root", 0..1, "first-window-cut");
    let after = cut(&before, "root", 2..3, "second-window-cut");
    assert_eq!(after.sounds(), original.sounds());
    let route = &after.sound_routes()[&SoundId::new("effect").unwrap()];
    assert_eq!(route.edits.len(), 2);
    for (edit, (start, end)) in route.edits.iter().zip([(0, 1), (2, 3)]) {
        assert_eq!(
            edit.operation,
            RootSoundOperation::Delete {
                range: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
            }
        );
    }
    let mut provider = Provider::new();
    let mut reader = renderer(&after, &mut provider);
    // Chronological route labels are independently composed: first remove
    // [0,B1), then [B2,B3). Hence saved labels are B1+n before the second
    // join and B1+B3+(n-B2) after it. Reverse reads use the same warm reader.
    for (at, original) in [(3303, 6507), (100, 1702), (3903, 7107)] {
        let actual = reader
            .prepare_authored_bus(
                &mut provider,
                AudioSample(at),
                128,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_close(
            &actual.samples,
            &expected(&provider, ExactRatio::integer(original), 128),
        );
    }
}
