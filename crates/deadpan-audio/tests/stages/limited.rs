//! Whole-plan preparation and cache behavior, with real source and Preserve PCM.
use super::*;
use deadpan_audio::{
    LimitedAudio, LimitedAudioError, LimitedTile, LimiterContext, MAX_CACHED_LIMITED_TILES,
    MAX_CACHED_LIMITER_BUS_BLOCKS,
};
use std::time::Instant;

// A limited read prepares its real DSP halo under the production batch budget.
// The explicit short-deadline test below still verifies timeout rejection.
const TIMEOUT: Duration = Duration::from_secs(60);

fn read(
    renderer: &mut LimitedAudio,
    provider: &mut impl AudioSourceProvider,
    start: i64,
    frames: u32,
) -> deadpan_audio::LimitedAudioBlock {
    renderer
        .read(
            provider,
            AudioSample(start),
            frames,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
}

#[test]
fn limited_bus_matches_direct_full_context_and_cold_shuffled_reads() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let plan = plan(
        rate,
        &["room", "stretch", "silent"],
        [
            ("room", room_tone(4096, audio(1024, 1536))),
            ("inner", source(rate, 4096, 0..4096)),
            (
                "stretch",
                retime("inner", 6144, 0..4096, PitchPolicy::Preserve),
            ),
            ("silent", hold(1024)),
        ],
    );
    let project = AudioSample(0)..plan.audio_duration().unwrap();
    let mut provider = FixtureProvider::new();
    let active = AtomicBool::new(false);
    let bus = StageAudio::new(Arc::clone(&plan))
        .prepare_edge_faded(
            &mut provider,
            AudioSample(0),
            project.end.0 as u32,
            TIMEOUT,
            &active,
        )
        .unwrap();
    let mut renderer = LimitedAudio::new(Arc::clone(&plan));
    let mut expected = Vec::new();
    let mut gains = Vec::new();
    for start in (0..project.end.0).step_by(8192) {
        let end = (start + 8192).min(project.end.0);
        let direct = LimitedTile::prepare(
            LimiterContext {
                project_samples: project.clone(),
                start: AudioSample(0),
                samples: bus.samples.clone(),
            },
            AudioSample(start)..AudioSample(end),
            Instant::now() + TIMEOUT,
            &active,
        )
        .unwrap();
        let actual = read(&mut renderer, &mut provider, start, (end - start) as u32);
        assert_eq!(actual.samples, direct.samples);
        assert_eq!(actual.gain, direct.gain);
        assert_eq!(actual.verified_tiles.len(), 1);
        expected.extend(actual.samples);
        gains.extend(actual.gain);
    }
    for (start, frames) in [(8191, 2), (8100, 1000), (123, 8192), (10200, 1064), (0, 1)] {
        for cold in [false, true] {
            let mut fresh = LimitedAudio::new(Arc::clone(&plan));
            let renderer = if cold { &mut fresh } else { &mut renderer };
            let block = read(renderer, &mut provider, start, frames);
            assert_eq!(
                block.samples,
                expected[start as usize..start as usize + frames as usize]
            );
            assert_eq!(
                block.gain,
                gains[start as usize..start as usize + frames as usize]
            );
            for range in &block.suppressed {
                assert!(range.start.0 >= start && range.end.0 <= start + i64::from(frames));
                assert!(
                    block.samples[(range.start.0 - start) as usize..(range.end.0 - start) as usize]
                        .iter()
                        .flatten()
                        .all(|value| *value == 0.0)
                );
            }
        }
    }
    assert!(
        expected[10240..]
            .iter()
            .flatten()
            .all(|value| *value == 0.0)
    );
    assert!(
        expected[..10240]
            .iter()
            .flatten()
            .any(|value| *value != 0.0)
    );
}

#[test]
fn limited_cache_rechecks_nested_stage_provenance_and_recovers_after_cancellation() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let plan = plan(
        rate,
        &["outer"],
        [
            ("source", source(rate, 1024, 512..1536)),
            (
                "inner",
                retime("source", 1536, 0..1024, PitchPolicy::Preserve),
            ),
            (
                "outer",
                retime("inner", 2048, 0..1536, PitchPolicy::Preserve),
            ),
        ],
    );
    let mut renderer = LimitedAudio::new(Arc::clone(&plan));
    let mut provider = FixtureProvider::new();
    let original = read(&mut renderer, &mut provider, 101, 200);
    let before = provider.calls;
    assert_eq!(
        read(&mut renderer, &mut provider, 101, 200).samples,
        original.samples
    );
    assert_eq!(
        provider.calls,
        before + 1,
        "warm tiles re-admit their retained dependency"
    );
    provider = FixtureProvider::with_layout(AudioChannelLayout::Native {
        channels: 2,
        mask: 5,
    });
    let changed = read(&mut renderer, &mut provider, 101, 200);
    assert_ne!(changed.samples, original.samples);
    assert_eq!(
        changed.samples,
        read(&mut LimitedAudio::new(plan), &mut provider, 101, 200).samples
    );
    provider.cancel_on_call = true;
    let cancelled = AtomicBool::new(false);
    assert!(matches!(
        renderer.read(&mut provider, AudioSample(101), 200, TIMEOUT, &cancelled),
        Err(LimitedAudioError::Stage(StageAudioError::Preparation(
            PreparationError::Cancelled
        )))
    ));
    assert_eq!(renderer.cached_tile_count(), 0);
    provider.cancel_on_call = false;
    assert_eq!(
        read(&mut renderer, &mut provider, 101, 200).samples,
        changed.samples
    );
}

#[test]
fn two_tile_read_cannot_mix_source_interpretations_even_when_both_tiles_are_warm() {
    struct Changing {
        original: FixtureProvider,
        later: FixtureProvider,
        calls: usize,
    }
    impl AudioSourceProvider for Changing {
        fn source(
            &mut self,
            project: &ProjectId,
            revision: &RevisionId,
            asset: &AssetId,
            cancelled: &AtomicBool,
        ) -> Result<&PreparedSource, PreparationError> {
            self.calls += 1;
            if self.calls == 1 {
                self.original.source(project, revision, asset, cancelled)
            } else {
                self.later.source(project, revision, asset, cancelled)
            }
        }
    }
    let rate = FrameRate::new(48_000, 1).unwrap();
    let plan = plan(rate, &["room"], [("room", room_tone(9000, audio(0, 512)))]);
    let mut renderer = LimitedAudio::new(plan);
    let mut original = FixtureProvider::new();
    read(&mut renderer, &mut original, 8191, 2);
    assert_eq!(renderer.cached_tile_count(), 2);
    let mut changing = Changing {
        original,
        later: FixtureProvider::with_layout(AudioChannelLayout::Native {
            channels: 2,
            mask: 5,
        }),
        calls: 0,
    };
    assert!(matches!(
        renderer.read(
            &mut changing,
            AudioSample(8191),
            2,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(LimitedAudioError::Stage(StageAudioError::Preparation(
            PreparationError::IndexMismatch
        )))
    ));
    assert_eq!(changing.calls, 2);
}

#[test]
fn limited_admission_cache_residency_and_empty_dependency_cancellation_are_bounded() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let plan = plan(rate, &["silent"], [("silent", hold(6 * 8192))]);
    let mut renderer = LimitedAudio::new(plan);
    let mut provider = FixtureProvider::new();
    let active = AtomicBool::new(false);
    for (start, frames) in [(-1, 1), (0, 0), (0, 8193), (i64::MAX, 2), (6 * 8192, 1)] {
        assert!(matches!(
            renderer.read(&mut provider, AudioSample(start), frames, TIMEOUT, &active),
            Err(LimitedAudioError::Range)
        ));
    }
    for tile in 0..6 {
        assert_eq!(
            read(&mut renderer, &mut provider, tile * 8192, 1).samples,
            [[0.0; 2]]
        );
        assert!(renderer.cached_tile_count() <= MAX_CACHED_LIMITED_TILES);
        assert!(renderer.cached_bus_block_count() <= MAX_CACHED_LIMITER_BUS_BLOCKS);
    }
    assert_eq!(renderer.cached_tile_count(), MAX_CACHED_LIMITED_TILES);
    assert_eq!(provider.calls, 0);
    assert!(
        renderer
            .read(
                &mut provider,
                AudioSample(5 * 8192),
                1,
                TIMEOUT,
                &AtomicBool::new(true)
            )
            .is_err()
    );
    assert!(matches!(
        renderer.read(
            &mut provider,
            AudioSample(5 * 8192),
            1,
            Duration::from_nanos(1),
            &active
        ),
        Err(LimitedAudioError::Stage(StageAudioError::Timeout))
    ));
    assert_eq!(
        read(&mut renderer, &mut provider, 5 * 8192, 1).samples,
        [[0.0; 2]]
    );
}

#[test]
fn adjacent_tiles_reuse_exact_bus_context_and_revalidate_its_source_layout() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let repeated = BeatNode {
        framing: None,
        audio_edges: Default::default(),
        label: "Long original reuse".into(),
        kind: NodeKind::Repeat {
            child: id("source"),
            iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 32).unwrap(),
            gap: None,
        },
    };
    let plan = plan(
        rate,
        &["repeat"],
        [
            ("source", source(rate, 4096, 0..4096)),
            ("repeat", repeated),
        ],
    );
    let mut renderer = LimitedAudio::new(Arc::clone(&plan));
    let mut provider = FixtureProvider::new();
    read(&mut renderer, &mut provider, 40960, 8192);
    let cold_calls = provider.calls;
    let warm = read(&mut renderer, &mut provider, 49152, 8192);
    let next_calls = provider.calls - cold_calls;
    assert!(next_calls > 7, "cached dependencies are still checked");
    assert!(
        next_calls * 3 < cold_calls,
        "overlapping context should avoid most source reads: {next_calls} vs {cold_calls}"
    );
    let cold = read(
        &mut LimitedAudio::new(Arc::clone(&plan)),
        &mut provider,
        49152,
        8192,
    );
    assert_eq!(warm.samples, cold.samples);
    assert_eq!(warm.gain, cold.gain);
    assert!(renderer.cached_bus_block_count() <= MAX_CACHED_LIMITER_BUS_BLOCKS);
    provider = FixtureProvider::with_layout(AudioChannelLayout::Native {
        channels: 2,
        mask: 5,
    });
    // This output tile was never cached, but most of its bus context was.
    let changed = read(&mut renderer, &mut provider, 57344, 8192);
    let fresh = read(&mut LimitedAudio::new(plan), &mut provider, 57344, 8192);
    assert_eq!(changed.samples, fresh.samples);
    assert_eq!(changed.gain, fresh.gain);
    assert_ne!(changed.samples, warm.samples);
}

#[test]
fn bus_cache_does_not_round_context_into_an_unavailable_following_source() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let plan = plan_with_asset(
        rate,
        &["source", "silent", "unavailable"],
        [
            ("source", source(rate, 8192, 0..8192)),
            ("silent", hold(21808)),
            // A valid authored selection whose bytes are unavailable in this
            // fixture's measured 8197-sample source. It begins at sample30000.
            ("unavailable", source(rate, 100, 9000..9100)),
        ],
        BTreeMap::new(),
        audio(0, 10000).span,
    );
    let mut renderer = LimitedAudio::new(plan);
    let mut provider = FixtureProvider::new();
    // The canonical0..8192 tile needs context only through29248. Rounding its
    // final bus cache range to32768 would incorrectly encounter the bad source.
    assert_eq!(read(&mut renderer, &mut provider, 0, 1).samples.len(), 1);
    assert!(
        renderer
            .read(
                &mut provider,
                AudioSample(30000),
                1,
                TIMEOUT,
                &AtomicBool::new(false)
            )
            .is_err()
    );
}
