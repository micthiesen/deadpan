use super::*;

fn authored(document: &ProjectDocument, provider: &mut Provider) -> Vec<[f32; 2]> {
    let end = ntsc()
        .audio_boundary(ProjectFrame(document.duration().unwrap().frames()))
        .unwrap()
        .0;
    let mut reader = renderer(document, provider);
    (0..end)
        .step_by(193)
        .flat_map(|at| {
            reader
                .prepare_authored_bus(
                    provider,
                    AudioSample(at),
                    u32::try_from((end - at).min(193)).unwrap(),
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples
        })
        .collect()
}

#[test]
fn exact_forest_delete_keeps_ntsc_preserve_pcm_and_transforms_root_sound_once() {
    let initial = document(
        ntsc(),
        &["lead", "group", "tail"],
        vec![
            ("lead", source(ntsc(), 1)),
            (
                "group",
                BeatNode::sequence("Group", vec![id("left"), id("voice"), id("right")]),
            ),
            ("left", BeatNode::sequence("Left", vec![])),
            ("voice", source(ntsc(), 3)),
            ("right", BeatNode::sequence("Right", vec![])),
            ("tail", preserve("tail-voice", 4, 7)),
            ("tail-voice", source(ntsc(), 4)),
        ],
    );
    let mut wire = serde_json::to_value(initial).unwrap();
    wire["assets"]["media"]["source_qualification"] =
        serde_json::to_value(SourceQualificationId::new("b".repeat(64)).unwrap()).unwrap();
    let initial = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let full = audio(0..44_117);
    let natural = SourceAudioMapping::natural_rate(full.span, ntsc())
        .unwrap()
        .duration_frames(frames(11))
        .unwrap();
    let before = edit(
        &initial,
        "sound",
        Command::SetSound {
            id: SoundId::new("effect").unwrap(),
            event: SoundEvent {
                owner: id("root"),
                label: "Separate bus".into(),
                source: full,
                mapping: SourceAudioMapping::SelectedPlacement {
                    start: ExactRatio::ZERO,
                    frames: natural,
                    selection: ExactFrameRange::new(ExactRatio::ZERO, ExactRatio::integer(10))
                        .unwrap(),
                },
                offset: AudioSample(7),
                gain_millidecibels: -6000,
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        },
    );
    let after = edit(
        &before,
        "forest",
        Command::DeleteChildren {
            parent: id("group"),
            first: id("left"),
            last: id("right"),
            timing: AudioTimingId {
                allocation: revision("forest"),
                ordinal: 0,
            },
        },
    );
    let reference = cut(&before, "group", 1..4, "range-reference");
    assert_eq!(after.duration().unwrap(), frames(8));
    assert_eq!(after.sounds(), reference.sounds());
    assert_eq!(after.sound_routes(), reference.sound_routes());
    assert!(!after.sound_routes().is_empty());
    assert!(!after.nodes().contains_key(&id("left")));
    assert!(reference.nodes().contains_key(&id("left")));
    let mut old_provider = Provider::new();
    let prefix = pcm(&before, &mut old_provider, 0, 1602);
    // Keep the old entry at B(4), but allocate the destination's exact
    // B(8)-B(1) samples. NTSC rounding makes that one less than B(11)-B(4).
    let suffix_count = ntsc().audio_boundary(ProjectFrame(8)).unwrap().0
        - ntsc().audio_boundary(ProjectFrame(1)).unwrap().0;
    let suffix = pcm(
        &before,
        &mut old_provider,
        6406,
        usize::try_from(suffix_count).unwrap(),
    );
    let mut new_provider = Provider::new();
    retained(&after, &mut new_provider, 0, &prefix);
    retained(&after, &mut new_provider, 1602, &suffix);
    let expected = authored(&reference, &mut Provider::new());
    let actual = authored(&after, &mut Provider::new());
    assert!(actual.iter().flatten().any(|sample| sample.abs() > 0.001));
    assert_eq!(actual.len(), 12_813);
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert!(
            actual
                .iter()
                .chain(&expected)
                .all(|sample| sample.is_finite())
        );
        assert_eq!(
            actual.map(f32::to_bits),
            expected.map(f32::to_bits),
            "authored sample {index}"
        );
    }
}
