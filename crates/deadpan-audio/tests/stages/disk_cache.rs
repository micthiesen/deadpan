use super::*;

#[test]
fn disk_stage_matches_memory_and_releases_evicted_storage() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let plan = plan(
        rate,
        &["a", "b"],
        [
            ("first", source(rate, 4096, 0..4096)),
            ("second", source(rate, 4096, 4096..8192)),
            ("a", retime("first", 6144, 0..4096, PitchPolicy::Preserve)),
            ("b", retime("second", 6144, 0..4096, PitchPolicy::Preserve)),
        ],
    );
    let mut provider = FixtureProvider::new();
    let mut memory = StageAudio::new(Arc::clone(&plan));
    let expected = read_all(&mut memory, &mut provider, &[256]);
    let mut disk = StageAudio::with_limits(
        plan,
        StageLimits {
            maximum_memory_stage_frames: 0,
            maximum_disk_bytes: 6144 * 8,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        read_all(&mut disk, &mut provider, &[17, 251, 103]),
        expected
    );
    assert_eq!(disk.cached_resident_frames(), 0);
    assert_eq!(disk.cached_stage_count(), 1);
    assert!(disk.cached_disk_bytes() > 0);
    let retained = disk.cached_disk_bytes();
    for start in [0, 6500, 33, 12_000] {
        assert_eq!(
            read_block(&mut disk, &mut provider, start, 128).samples,
            expected[start as usize..start as usize + 128]
        );
        assert_eq!(disk.cached_disk_bytes(), retained);
    }
}

#[test]
fn disk_budget_rejects_before_source_access_and_cancellation_can_retry() {
    let plan = cut_stage_plan();
    let mut provider = FixtureProvider::new();
    let mut refused = StageAudio::with_limits(
        Arc::clone(&plan),
        StageLimits {
            maximum_memory_stage_frames: 0,
            maximum_disk_bytes: 6144 * 8 - 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        refused.read(
            &mut provider,
            AudioSample(6000),
            1,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Limit("prepared PCM disk bytes"))
    ));
    assert_eq!(provider.calls, 0);
    assert_eq!(refused.cached_disk_bytes(), 0);
    let mut disk = StageAudio::with_limits(
        Arc::clone(&plan),
        StageLimits {
            maximum_memory_stage_frames: 0,
            maximum_disk_bytes: 6144 * 8,
            ..Default::default()
        },
    )
    .unwrap();
    let cancel = AtomicBool::new(false);
    provider.cancel_on_call = true;
    assert!(
        disk.read(&mut provider, AudioSample(6000), 1, TIMEOUT, &cancel)
            .unwrap_err()
            .is_cancelled()
    );
    assert_eq!(disk.cached_stage_count(), 0);
    assert_eq!(disk.cached_disk_bytes(), 0);
    cancel.store(false, Ordering::Relaxed);
    provider.cancel_on_call = false;
    let recovered = disk
        .read(&mut provider, AudioSample(6000), 1, TIMEOUT, &cancel)
        .unwrap();
    let mut memory = StageAudio::new(plan);
    assert_eq!(
        recovered.samples,
        read_block(&mut memory, &mut provider, 6000, 1).samples
    );
    assert_eq!(disk.cached_disk_bytes(), 6144 * 8);
    assert_eq!(disk.cached_resident_frames(), 0);
}
