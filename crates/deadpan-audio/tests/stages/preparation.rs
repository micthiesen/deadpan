//! Batch preparation shares the established canonical reader and its budgets.
use super::*;
use deadpan_audio::MAX_EDGE_PREPARATION_FRAMES;

fn suppression(start: i64, frames: usize, ranges: &[Range<AudioSample>]) -> Vec<bool> {
    let mut result = vec![false; frames];
    for range in ranges {
        result[(range.start.0 - start) as usize..(range.end.0 - start) as usize].fill(true);
    }
    result
}

#[test]
fn preparation_matches_irregular_reads_and_cold_crops_across_all_stages() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let plan = plan(
        rate,
        &["source", "room", "stretch", "silent"],
        [
            ("source", source(rate, 513, 0..513)),
            ("room", room_tone(287, audio(1024, 1152))),
            ("inner", source(rate, 512, 512..1024)),
            (
                "stretch",
                retime("inner", 768, 0..512, PitchPolicy::Preserve),
            ),
            ("silent", hold(513)),
        ],
    );
    let mut provider = FixtureProvider::new();
    let active = AtomicBool::new(false);
    let frames = plan.audio_duration().unwrap().0 as usize;
    let mut reference = StageAudio::new(Arc::clone(&plan));
    let mut expected = Vec::new();
    let mut silent = Vec::new();
    for count in [31, 256, 1, 173].into_iter().cycle() {
        if expected.len() == frames {
            break;
        }
        let start = expected.len() as i64;
        let count = count.min((frames - expected.len()) as u32);
        let block = reference
            .read_edge_faded(&mut provider, AudioSample(start), count, TIMEOUT, &active)
            .unwrap();
        silent.extend(suppression(start, count as usize, &block.suppressed));
        expected.extend(block.samples);
    }
    for (start, count) in [(0, frames), (255, 1320), (1567, 514), (512, 513), (800, 1)] {
        let mut cold = StageAudio::new(Arc::clone(&plan));
        let actual = cold
            .prepare_edge_faded(
                &mut provider,
                AudioSample(start as i64),
                count as u32,
                TIMEOUT,
                &active,
            )
            .unwrap();
        assert_eq!(actual.start, AudioSample(start as i64));
        assert_eq!(actual.project_id, plan.metadata().project_id);
        assert_eq!(actual.revision_id, plan.metadata().revision_id);
        assert_eq!(actual.samples, expected[start..start + count]);
        assert_eq!(
            suppression(start as i64, count, &actual.suppressed),
            silent[start..start + count]
        );
        assert!(
            actual
                .suppressed
                .windows(2)
                .all(|ranges| ranges[0].end < ranges[1].start)
        );
    }
}

#[test]
fn preparation_does_not_renew_stage_allowance_at_each_small_read() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let plan = plan(
        rate,
        &["a", "b"],
        [
            ("a", room_tone(512, audio(0, 128))),
            ("b", room_tone(512, audio(512, 640))),
        ],
    );
    let limits = StageLimits {
        maximum_prepared_stages: 1,
        ..Default::default()
    };
    let mut provider = FixtureProvider::new();
    let active = AtomicBool::new(false);
    // Separate requests may each spend their own admitted preparation allowance.
    let mut separate = StageAudio::with_limits(Arc::clone(&plan), limits).unwrap();
    for start in [0, 512] {
        separate
            .read_edge_faded(&mut provider, AudioSample(start), 256, TIMEOUT, &active)
            .unwrap();
    }
    let mut batch = StageAudio::with_limits(plan, limits).unwrap();
    assert!(matches!(
        batch.prepare_edge_faded(&mut provider, AudioSample(0), 1024, TIMEOUT, &active),
        Err(StageAudioError::Limit("prepared stages per read"))
    ));
}

#[test]
fn preparation_rejects_bad_admission_and_cancellation_without_a_partial_block() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let plan = plan(rate, &["source"], [("source", source(rate, 4096, 0..4096))]);
    let mut renderer = StageAudio::new(plan);
    let mut provider = FixtureProvider::new();
    let active = AtomicBool::new(false);
    for (start, count) in [
        (0, 0),
        (-1, 1),
        (4095, 2),
        (i64::MAX, 2),
        (0, MAX_EDGE_PREPARATION_FRAMES + 1),
    ] {
        assert!(matches!(
            renderer.prepare_edge_faded(&mut provider, AudioSample(start), count, TIMEOUT, &active),
            Err(StageAudioError::PreparationRange)
        ));
    }
    assert_eq!(provider.calls, 0);
    assert!(matches!(
        renderer.read_edge_faded(&mut provider, AudioSample(0), 257, TIMEOUT, &active),
        Err(StageAudioError::Range)
    ));
    provider.cancel_on_call = true;
    let error = renderer
        .prepare_edge_faded(&mut provider, AudioSample(0), 4096, TIMEOUT, &active)
        .unwrap_err();
    assert!(error.is_cancelled());
    assert_eq!(provider.calls, 1);
    active.store(false, Ordering::Relaxed);
    provider.cancel_on_call = false;
    let recovered = renderer
        .prepare_edge_faded(&mut provider, AudioSample(0), 4096, TIMEOUT, &active)
        .unwrap();
    assert_eq!(recovered.samples.len(), 4096);
}
