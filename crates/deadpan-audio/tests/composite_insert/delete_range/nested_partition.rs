//! Decoded PCM witnesses for recursive unity-window deletion admission.
use super::*;

// Both fixtures expose leaf frames [3,10). Treated chains already had recursive
// admission; these preservation cases complement the untreated witnesses below.

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
                treatments: deadpan_core::AudioTreatments::default(),
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
