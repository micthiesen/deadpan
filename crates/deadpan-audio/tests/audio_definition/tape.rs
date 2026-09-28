use super::*;

use deadpan_plan::{AudioQueryLimits, AudioSignal, AudioSignalTape, AudioSignalTapeRun};

fn run<'plan>(
    destination: Range<ExactRatio>,
    source: Range<ExactRatio>,
    signal: AudioSignal<'plan>,
) -> AudioSignalTapeRun<'plan> {
    AudioSignalTapeRun::new(destination, source, signal)
}

#[test]
fn tape_uses_one_ntsc_point_ceil_grid_across_an_exact_run_seam() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let doc = document(
        rate,
        &["source"],
        [("source", source(rate, 3, 0..4805))],
        BTreeMap::new(),
    );
    let plan = compile(&doc, false);
    let signal = plan.audio_signal();
    let tape = AudioSignalTape::new(
        &plan,
        ExactRatio::ZERO..ratio(3, 1),
        vec![
            run(
                ExactRatio::ZERO..ratio(3, 2),
                ExactRatio::ZERO..ratio(3, 2),
                signal.clone(),
            ),
            run(ratio(3, 2)..ratio(3, 1), ratio(3, 2)..ratio(3, 1), signal),
        ],
    )
    .unwrap();

    assert_eq!(tape.sample_count().unwrap(), SignalSample(4805));
    let query = tape
        .query(
            SignalSample(2400)..SignalSample(2405),
            AudioQueryLimits::default(),
        )
        .unwrap();
    assert_eq!(
        query
            .spans
            .iter()
            .map(|span| span.samples.clone())
            .collect::<Vec<_>>(),
        vec![
            SignalSample(2400)..SignalSample(2403),
            SignalSample(2403)..SignalSample(2405)
        ]
    );

    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let actual = renderer
        .read_tape(
            &mut FixtureProvider::new(),
            &tape,
            SignalSample(2400),
            5,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    let expected = sample_reference(
        0..4805,
        ExactRatio::ZERO,
        ExactRatio::ONE,
        4805,
        fixture_sample,
    );
    assert_eq!(actual.samples, expected[2400..2405]);
    assert_eq!(actual.support, ExactRatio::ZERO..ratio(3, 1));
    assert!(actual.suppressed.is_empty());
}

#[test]
fn tape_run_seams_do_not_crop_source_filter_context() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let doc = document(
        rate,
        &["source"],
        [("source", source(rate, 2048, 0..2048))],
        BTreeMap::new(),
    );
    let plan = compile(&doc, false);
    let signal = plan.audio_signal();
    let tape = AudioSignalTape::new(
        &plan,
        ExactRatio::ZERO..ratio(2048, 1),
        vec![
            run(
                ExactRatio::ZERO..ratio(700, 1),
                ExactRatio::ZERO..ratio(699, 1),
                signal.clone(),
            ),
            run(
                ratio(700, 1)..ratio(2048, 1),
                ratio(699, 1)..ratio(2048, 1),
                signal,
            ),
        ],
    )
    .unwrap();

    let first = sample_reference(
        0..2048,
        ExactRatio::ZERO,
        ratio(699, 700),
        700,
        fixture_sample,
    );
    let second = sample_reference(
        0..2048,
        ratio(699, 1),
        ratio(1349, 1348),
        1348,
        fixture_sample,
    );
    let expected: Vec<_> = first.into_iter().chain(second).collect();
    let cropped = sample_reference(
        0..699,
        ExactRatio::ZERO,
        ratio(699, 700),
        700,
        fixture_sample,
    );
    assert_ne!(
        &expected[694..700],
        &cropped[694..700],
        "the fixture must distinguish full source support from a crop at the run seam"
    );

    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let actual = renderer
        .read_tape(
            &mut FixtureProvider::new(),
            &tape,
            SignalSample(694),
            12,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(actual.samples, expected[694..706]);
}

#[test]
fn tape_uses_live_source_mapping_when_one_asset_is_referenced_twice() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let doc = document(
        rate,
        &["a", "b"],
        [("a", source(rate, 4, 0..4)), ("b", source(rate, 4, 8..12))],
        BTreeMap::new(),
    );
    let mut wire = serde_json::to_value(&doc).unwrap();
    wire["nodes"]["b"]["kind"]["source"]["audio_mapping"] =
        serde_json::to_value(SourceAudioMapping::Placement {
            start: ratio(-1, 2),
            frames: ratio(4, 1),
        })
        .unwrap();
    let edited = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let old_plan = compile(&doc, false);
    let plan = compile(&edited, false);
    let a = plan.audio_definition(node("a")).unwrap().signal();
    let b = plan.audio_definition(node("b")).unwrap().signal();
    let tape = AudioSignalTape::new(
        &plan,
        ExactRatio::ZERO..ratio(8, 1),
        vec![
            run(
                ExactRatio::ZERO..ratio(4, 1),
                ExactRatio::ZERO..ratio(4, 1),
                a,
            ),
            run(ratio(4, 1)..ratio(8, 1), ExactRatio::ZERO..ratio(4, 1), b),
        ],
    )
    .unwrap();

    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let actual = renderer
        .read_tape(
            &mut FixtureProvider::new(),
            &tape,
            SignalSample(0),
            8,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    let expected_a = sample_reference(0..4, ExactRatio::ZERO, ExactRatio::ONE, 4, fixture_sample);
    let expected_b = sample_reference(9..12, ratio(17, 2), ExactRatio::ONE, 4, fixture_sample);
    assert_eq!(
        actual.samples,
        expected_a.into_iter().chain(expected_b).collect::<Vec<_>>()
    );

    let old_signal = old_plan.audio_definition(node("b")).unwrap().signal();
    let old_tape = AudioSignalTape::new(
        &old_plan,
        ExactRatio::ZERO..ratio(4, 1),
        vec![run(
            ExactRatio::ZERO..ratio(4, 1),
            ExactRatio::ZERO..ratio(4, 1),
            old_signal,
        )],
    )
    .unwrap();
    let old = StageAudio::new(Arc::clone(&old_plan))
        .read_tape(
            &mut FixtureProvider::new(),
            &old_tape,
            SignalSample(0),
            4,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_ne!(actual.samples[4..], old.samples);
}

#[test]
fn tape_keeps_repeat_default_and_actual_room_tone_occurrence_scopes_distinct() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let repeat = BeatNode {
        framing: None,
        label: "Two RoomTone plays".into(),
        audio_treatments: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: id("room"),
            iterations: IterationOrder::new(RevisionId::new("room-plays").unwrap(), 2).unwrap(),
            gap: None,
        },
    };
    let doc = document(
        rate,
        &["repeat"],
        [
            (
                "room",
                hold(
                    128,
                    HoldAudio::RoomTone {
                        source: audio(512..640),
                    },
                ),
            ),
            ("repeat", repeat),
        ],
        BTreeMap::new(),
    );
    let plan = compile(&doc, false);
    let default = plan
        .audio_definition(AudioDefinitionSelector::RepeatDefault {
            repeat: id("repeat"),
        })
        .unwrap()
        .signal();
    let occurrence = plan.audio_signal();
    let tape = AudioSignalTape::new(
        &plan,
        ExactRatio::ZERO..ratio(256, 1),
        vec![
            run(
                ExactRatio::ZERO..ratio(128, 1),
                ExactRatio::ZERO..ratio(128, 1),
                default,
            ),
            run(
                ratio(128, 1)..ratio(256, 1),
                ratio(128, 1)..ratio(256, 1),
                occurrence,
            ),
        ],
    )
    .unwrap();

    let source: Vec<_> = (512..640).map(fixture_sample).collect();
    let room = RoomTone::new(
        RoomToneRecipe::new(ratio(128, 1), 128).unwrap(),
        &source,
        &AtomicBool::new(false),
    )
    .unwrap()
    .render(AudioSample(0), 128, &AtomicBool::new(false))
    .unwrap()
    .samples;
    let expected: Vec<_> = room.iter().copied().chain(room.iter().copied()).collect();
    let mut provider = FixtureProvider::new();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let actual = renderer
        .read_tape(
            &mut provider,
            &tape,
            SignalSample(0),
            256,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(actual.samples, expected);
    assert!(actual.suppressed.is_empty());
    assert_eq!(provider.calls, 2);
    assert_eq!(renderer.cached_stage_count(), 2);
}

#[test]
fn tape_preserves_nested_preserve_history_suppression_and_cache_admission() {
    let (doc, expected, _) = nested_fixture();
    let plan = compile(&doc, false);
    let signal = plan.audio_definition(node("outer")).unwrap().signal();
    let tape = AudioSignalTape::new(
        &plan,
        ExactRatio::ZERO..ratio(1280, 1),
        vec![
            run(
                ExactRatio::ZERO..ratio(640, 1),
                ExactRatio::ZERO..ratio(640, 1),
                signal.clone(),
            ),
            run(
                ratio(640, 1)..ratio(1280, 1),
                ratio(640, 1)..ratio(1280, 1),
                signal,
            ),
        ],
    )
    .unwrap();
    let mut provider = FixtureProvider::new();
    let mut renderer = StageAudio::new(Arc::clone(&plan));

    let suffix = renderer
        .read_tape(
            &mut provider,
            &tape,
            SignalSample(1024),
            256,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(suffix.samples, expected[1024..1280]);
    assert_eq!(renderer.cached_stage_count(), 3);

    let seam = renderer
        .read_tape(
            &mut provider,
            &tape,
            SignalSample(636),
            8,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(seam.samples, expected[636..644]);
    let silent_hold = renderer
        .read_tape(
            &mut provider,
            &tape,
            SignalSample(800),
            160,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(silent_hold.samples, expected[800..960]);
    assert_eq!(
        silent_hold.suppressed,
        vec![SignalSample(800)..SignalSample(960)]
    );

    provider.unavailable = true;
    assert!(matches!(
        renderer.read_tape(
            &mut provider,
            &tape,
            SignalSample(0),
            1,
            TIMEOUT,
            &AtomicBool::new(false),
        ),
        Err(StageAudioError::Preparation(
            PreparationError::SourceUnavailable(_)
        ))
    ));
}
