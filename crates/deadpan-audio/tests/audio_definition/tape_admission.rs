use super::*;
use deadpan_plan::{AudioSignalContent, AudioSignalTape, AudioSignalTapeRun};

fn divided_root(plan: &RenderPlan, cut: i64, end: i64) -> AudioSignalTape<'_> {
    let cut = ExactRatio::integer(cut);
    let end = ExactRatio::integer(end);
    AudioSignalTape::new(
        plan,
        ExactRatio::ZERO..end,
        vec![
            AudioSignalTapeRun::new(
                ExactRatio::ZERO..cut,
                ExactRatio::ZERO..cut,
                plan.audio_signal(),
            ),
            AudioSignalTapeRun::new(cut..end, cut..end, plan.audio_signal()),
        ],
    )
    .unwrap()
}

#[test]
fn bound_source_reads_keep_the_physical_allocation_anchor_across_tape_seams() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let original = document(
        rate,
        &["a"],
        [("a", source(rate, 1024, 512..1536))],
        BTreeMap::new(),
    );
    let bindings = capture_unbound_audio_bindings(
        &original,
        AudioTimingId {
            allocation: RevisionId::new("tape-capture").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    let mut wire = serde_json::to_value(&original).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    let bound = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = compile(&bound, false);
    let tape = divided_root(&plan, 512, 1024);
    let query = tape
        .query(SignalSample(504)..SignalSample(520), Default::default())
        .unwrap();
    assert_eq!(query.spans.len(), 2);
    for span in &query.spans {
        assert!(matches!(span.content, AudioSignalContent::Bound(_)));
        assert_eq!(span.allocated_samples, SignalSample(0)..SignalSample(1024));
    }
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let mut provider = FixtureProvider::new();
    for (start, count) in [(640, 128), (504, 16), (0, 64)] {
        let actual = renderer
            .read_tape(
                &mut provider,
                &tape,
                SignalSample(start),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        let expected: Vec<_> = (512 + start..512 + start + i64::from(count))
            .map(fixture_sample)
            .collect();
        assert_eq!(actual.samples, expected, "bound read at {start}");
        assert!(actual.suppressed.is_empty());
    }
}

#[test]
fn tape_reads_reject_foreign_handles_bad_ranges_and_cancellation() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let doc = document(
        rate,
        &["a"],
        [("a", source(rate, 1024, 0..1024))],
        BTreeMap::new(),
    );
    let plan = compile(&doc, false);
    let tape = divided_root(&plan, 512, 1024);
    let mut provider = FixtureProvider::new();
    let mut foreign = StageAudio::new(compile(&doc, false));
    assert!(matches!(
        foreign.read_tape(
            &mut provider,
            &tape,
            SignalSample(0),
            1,
            TIMEOUT,
            &AtomicBool::new(false),
        ),
        Err(StageAudioError::ForeignDomain)
    ));
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    for (start, count, timeout) in [
        (-1, 1, TIMEOUT),
        (0, 0, TIMEOUT),
        (0, 257, TIMEOUT),
        (1024, 1, TIMEOUT),
        (1023, 2, TIMEOUT),
        (i64::MAX, 1, TIMEOUT),
        (0, 1, Duration::ZERO),
        (0, 1, Duration::from_secs(61)),
    ] {
        assert!(
            renderer
                .read_tape(
                    &mut provider,
                    &tape,
                    SignalSample(start),
                    count,
                    timeout,
                    &AtomicBool::new(false),
                )
                .is_err()
        );
    }
    assert!(
        renderer
            .read_tape(
                &mut provider,
                &tape,
                SignalSample(0),
                1,
                TIMEOUT,
                &AtomicBool::new(true),
            )
            .unwrap_err()
            .is_cancelled()
    );
    assert_eq!(provider.calls, 0);
    provider.cancel_on_call = true;
    assert!(
        renderer
            .read_tape(
                &mut provider,
                &tape,
                SignalSample(504),
                16,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap_err()
            .is_cancelled()
    );
    assert_eq!(renderer.cached_stage_count(), 0);
}

#[test]
fn tape_preflights_all_runs_and_shares_preparation_limits_between_them() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let unsupported = document(
        rate,
        &["a", "tail"],
        [
            ("a", source(rate, 2, 512..514)),
            (
                "tail",
                hold(
                    2,
                    HoldAudio::Tail {
                        source: audio(512..514),
                        maximum: frames(2),
                    },
                ),
            ),
        ],
        BTreeMap::new(),
    );
    let plan = compile(&unsupported, false);
    let tape = divided_root(&plan, 2, 4);
    let mut provider = FixtureProvider::new();
    assert!(matches!(
        StageAudio::new(Arc::clone(&plan)).read_tape(
            &mut provider,
            &tape,
            SignalSample(0),
            4,
            TIMEOUT,
            &AtomicBool::new(false),
        ),
        Err(StageAudioError::Unsupported("effect tails"))
    ));
    assert_eq!(
        provider.calls, 0,
        "the second run must preflight before the first reads media"
    );

    let doc = document(
        rate,
        &["a", "b"],
        [
            (
                "a",
                hold(
                    128,
                    HoldAudio::RoomTone {
                        source: audio(512..640),
                    },
                ),
            ),
            (
                "b",
                hold(
                    128,
                    HoldAudio::RoomTone {
                        source: audio(512..640),
                    },
                ),
            ),
        ],
        BTreeMap::new(),
    );
    let plan = compile(&doc, false);
    let tape = divided_root(&plan, 128, 256);
    let mut renderer = StageAudio::with_limits(
        Arc::clone(&plan),
        StageLimits {
            maximum_prepared_stages: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        renderer.read_tape(
            &mut provider,
            &tape,
            SignalSample(0),
            256,
            TIMEOUT,
            &AtomicBool::new(false),
        ),
        Err(StageAudioError::Limit(_))
    ));
    assert_eq!(
        provider.calls, 1,
        "the second run cannot reset stage admission"
    );
    assert_eq!(renderer.cached_stage_count(), 1);
}
