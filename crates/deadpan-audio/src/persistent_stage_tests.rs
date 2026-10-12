use super::*;
use crate::persistent_pcm::PcmCache;
use crate::stages::controlled_reads::fixture_document_at_rate;
use deadpan_core::{FrameRate, ProjectId, RevisionId};
use std::os::unix::fs::{FileExt, PermissionsExt};

fn render(
    cache: Option<PcmCache>,
    pitch: i32,
    cache_work: usize,
) -> (Vec<f32>, Option<SavedAudioStatus>, usize) {
    let (document, mut provider) = fixture_document_at_rate(FrameRate::new(48_000, 1).unwrap());
    // Keep the full fixture graph and select its root signal. An outer stretch
    // exercises nested Preserve output as well as the ordinary source/room tone.
    let plan = Arc::new(RenderPlan::compile(&document).unwrap());
    let mut renderer = StageAudio::with_limits(
        Arc::clone(&plan),
        StageLimits {
            maximum_memory_stage_frames: 1,
            ..Default::default()
        },
    )
    .unwrap();
    renderer.set_persistent_cache(cache);
    let monitor = Arc::new(PreparationMonitor::new(
        PreparationActivity::PreparingOutput,
    ));
    renderer.set_preparation_monitor(Some(Arc::clone(&monitor)));
    let signal = plan.audio_signal();
    let frames = u32::try_from(signal.sample_count().unwrap().0).unwrap();
    let recipe = StreamRecipe::new(frames, frames, StretchRate::new(1, 1).unwrap(), pitch).unwrap();
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork {
        cache_plan_work: cache_work,
        ..Default::default()
    });
    let control = WorkControl {
        cancelled: &cancelled,
        deadline: Instant::now() + Duration::from_secs(10),
        work: &work,
    };
    let block = renderer
        .build_stage(
            (&signal).into(),
            (&signal).into(),
            recipe,
            &mut provider,
            control,
            0,
        )
        .unwrap();
    let samples = block
        .samples
        .read_window(0..frames as usize, &cancelled)
        .unwrap();
    (samples, monitor.saved_audio_status(), provider.calls)
}

#[test]
fn persistent_key_budget_cannot_spend_the_canonical_query_allowance() {
    let root = tempfile::tempdir().unwrap();
    let reference = render(None, 0, 0).0;
    let (actual, status, _) = render(
        Some(PcmCache::at(root.path()).unwrap()),
        0,
        MAX_PLAN_WORK_PER_READ,
    );
    assert_eq!(actual, reference);
    assert_eq!(status.unwrap().event, SavedAudioEvent::BudgetSkipped);
    assert!(
        PcmCache::at(root.path())
            .unwrap()
            .entries()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn persistent_stage_restart_matches_uncached_pcm_and_readmits_sources() {
    let root = tempfile::tempdir().unwrap();
    let reference = render(None, 0, 0).0;
    let (cold, status, _) = render(Some(PcmCache::at(root.path()).unwrap()), 0, 0);
    assert_eq!(cold, reference);
    let status = status.unwrap();
    assert_eq!(status.event, SavedAudioEvent::Saved);
    assert_eq!(status.regenerated_from, Some(SavedAudioEvent::Missing));
    let (warm, restored, calls) = render(Some(PcmCache::at(root.path()).unwrap()), 0, 0);
    assert_eq!(warm, reference);
    assert_eq!(restored.unwrap().event, SavedAudioEvent::Restored);
    assert_eq!(restored.unwrap().key, status.key);
    assert!(calls > 0, "a cache hit must still admit the source");
    let (_, changed, _) = render(Some(PcmCache::at(root.path()).unwrap()), 7, 0);
    assert_eq!(changed.unwrap().event, SavedAudioEvent::Saved);
    assert_ne!(changed.unwrap().key, status.key);
}

#[test]
fn persistent_stage_corruption_rebuilds_exact_pcm_and_reports_why() {
    let root = tempfile::tempdir().unwrap();
    let cache = PcmCache::at(root.path()).unwrap();
    let (reference, status, _) = render(Some(cache.clone()), 0, 0);
    let key = blake3::Hash::from_bytes(status.unwrap().key.unwrap());
    let path = root.path().join(format!("b3-{key}.pcm"));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let file = std::fs::File::options()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    let mut byte = [0];
    file.read_exact_at(&mut byte, 48).unwrap();
    byte[0] ^= 1;
    file.write_all_at(&byte, 48).unwrap();
    let (rebuilt, status, _) = render(Some(cache), 0, 0);
    assert_eq!(rebuilt, reference);
    assert_eq!(status.unwrap().event, SavedAudioEvent::Saved);
    assert_eq!(
        status.unwrap().regenerated_from,
        Some(SavedAudioEvent::Damaged)
    );
}

#[test]
fn persistent_stage_full_cache_preserves_canonical_output() {
    let root = tempfile::tempdir().unwrap();
    let reference = render(None, 0, 0).0;
    let (actual, status, _) = render(Some(PcmCache::with_budget(root.path(), 1).unwrap()), 0, 0);
    assert_eq!(actual, reference);
    assert_eq!(status.unwrap().event, SavedAudioEvent::Full);
    assert!(
        PcmCache::at(root.path())
            .unwrap()
            .entries()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn persistent_stage_never_bypasses_missing_sources_or_cancellation() {
    struct Missing;
    impl AudioSourceProvider for Missing {
        fn source(
            &mut self,
            _: &ProjectId,
            _: &RevisionId,
            _: &AssetId,
            _: &AtomicBool,
        ) -> Result<&crate::PreparedSource, PreparationError> {
            Err(PreparationError::SourceUnavailable("removed".into()))
        }
    }
    let root = tempfile::tempdir().unwrap();
    render(Some(PcmCache::at(root.path()).unwrap()), 0, 0);
    let (document, _) = fixture_document_at_rate(FrameRate::new(48_000, 1).unwrap());
    let plan = Arc::new(RenderPlan::compile(&document).unwrap());
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    renderer.set_persistent_cache(Some(PcmCache::at(root.path()).unwrap()));
    let signal = plan.audio_signal();
    let frames = signal.sample_count().unwrap().0 as u32;
    let recipe = StreamRecipe::new(frames, frames, StretchRate::new(1, 1).unwrap(), 0).unwrap();
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = WorkControl {
        cancelled: &cancelled,
        deadline: Instant::now() + Duration::from_secs(10),
        work: &work,
    };
    assert!(matches!(
        renderer.build_stage(
            (&signal).into(),
            (&signal).into(),
            recipe,
            &mut Missing,
            control,
            0
        ),
        Err(StageAudioError::Preparation(
            PreparationError::SourceUnavailable(_)
        ))
    ));
    cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(
        renderer
            .build_stage(
                (&signal).into(),
                (&signal).into(),
                recipe,
                &mut Missing,
                control,
                0
            )
            .is_err()
    );
    assert_eq!(renderer.cached_stage_count(), 0);
}
