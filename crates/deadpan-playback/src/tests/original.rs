use super::*;

fn original(snapshot: &Snapshot) -> Arc<Original> {
    let asset = AssetId::new("original").unwrap();
    Arc::new(
        Original::new(
            snapshot.document.presentation_basis().frame_rate,
            asset.clone(),
            snapshot.sources[&asset].receipt.clone(),
        )
        .unwrap(),
    )
}

fn canonical(snapshot: &Arc<Snapshot>, target: &Target, start: i64, count: u32) -> Vec<f32> {
    let document = target.document(snapshot).unwrap();
    let mut audio = LimitedAudio::new(Arc::new(RenderPlan::compile(&document).unwrap()));
    audio
        .read(
            &mut Sources::new(snapshot.clone()),
            AudioSample(start),
            count,
            Duration::from_secs(60),
            &cancelled(),
        )
        .unwrap()
        .samples
        .into_iter()
        .flatten()
        .collect()
}

#[test]
fn original_pcm_ignores_edits_and_target_cache_never_reuses_sequence_pcm() {
    let permit = crate::tests::resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut basis = empty().presentation_basis().clone();
    basis.frame_rate = FrameRate::new(24, 1).unwrap();
    let document = ProjectDocument::new(
        empty().project_id().clone(),
        revision("initial"),
        basis,
        node("root"),
    )
    .unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("project.deadpan"), &document).unwrap();
    register_media(&mut store, "cfr-bframes.mp4", true);
    let before = store.snapshot().unwrap();
    store
        .commit(&CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision("lead-pause"),
            command: Command::Insert {
                parent: node("root"),
                index: 0,
                subtree: Subtree {
                    root: node("pause"),
                    overrides: Default::default(),
                    gap_overrides: Default::default(),
                    nodes: std::collections::BTreeMap::from([(
                        node("pause"),
                        BeatNode::hold(
                            "Pause",
                            HoldRecipe {
                                picture_context: None,
                                duration: FrameDuration::new(24).unwrap(),
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                            },
                        ),
                    )]),
                },
            },
        })
        .unwrap();
    let snapshot = snapshot(&store, 21);
    let original = original(&snapshot);
    assert_eq!(original.frame_count(), 120);
    assert_eq!(
        original.duration().frames(),
        97,
        "source ordinals are not project frames"
    );
    assert_eq!(original.sample_at_boundary(1).unwrap(), AudioSample(1602));
    let target = Target::Original(original);
    let expected = canonical(&snapshot, &target, 160, 256);
    assert!(expected.iter().any(|v| v.abs() > 0.001));
    let saved = store.snapshot().unwrap();
    let history = store.history_availability().unwrap();
    let (engine, devices) = engine(&permit);
    for (index, selected) in [Target::Sequence, target, Target::Sequence]
        .into_iter()
        .enumerate()
    {
        engine
            .play_target(
                index as u64,
                snapshot.clone(),
                selected,
                AudioSample(160),
                0.25,
            )
            .unwrap();
        let device = playing_device(&engine, &devices, index);
        let (_, pcm) = device.render(256, 0, 10_000_000);
        if index == 1 {
            assert_eq!(
                pcm,
                expected
                    .iter()
                    .map(|value| value * 0.25)
                    .collect::<Vec<_>>()
            );
        } else {
            assert!(pcm.iter().all(|value| *value == 0.0));
        }
        engine.stop();
        update(&engine, Phase::Stopped);
    }
    assert_eq!(store.snapshot().unwrap(), saved);
    assert_eq!(store.history_availability().unwrap(), history);
}

#[test]
fn original_preserves_leading_audio_trailing_audio_and_exact_rounded_picture_clock() {
    let permit = crate::tests::resources::pcm();
    for fixture in ["offset-bframes.mp4", "vfr.mp4"] {
        let directory = tempfile::tempdir().unwrap();
        let mut store =
            ProjectStore::create(&directory.path().join("project.deadpan"), &empty()).unwrap();
        register_media(&mut store, fixture, true);
        let snapshot = snapshot(&store, 1);
        let original = original(&snapshot);
        let target = Target::Original(original.clone());
        let view = target.document(&snapshot).unwrap();
        let node = &view.nodes()[&node("audition-original-source")].kind;
        let NodeKind::Source { source } = node else {
            panic!("source view")
        };
        let timing = snapshot.sources[original.asset()]
            .receipt
            .snapshot()
            .derive_timing(original.rate())
            .unwrap();
        assert_eq!(*source, timing.source_node(original.asset().clone()));
        assert_eq!(original.sample_at_boundary(0).unwrap(), AudioSample(0));
        assert_eq!(
            original.sample_at_boundary(original.frame_count()).unwrap(),
            original.end()
        );
        assert_eq!(
            original.frame_at_sample(original.end()).unwrap(),
            original.frame_count()
        );
        assert_eq!(
            original
                .frame_at_sample(AudioSample(original.end().0 - 1))
                .unwrap(),
            original.frame_count() - 1
        );
        for ordinal in 1..original.frame_count() {
            let boundary = original.sample_at_boundary(ordinal).unwrap();
            assert_eq!(original.frame_at_sample(boundary).unwrap(), ordinal);
            assert_eq!(
                original
                    .frame_at_sample(AudioSample(boundary.0 - 1))
                    .unwrap(),
                ordinal - 1
            );
        }
        let read_start = if fixture == "offset-bframes.mp4" {
            // Picture starts 1024 samples after the full union origin.
            assert!(
                source
                    .video_mapping
                    .start_frames()
                    .compare_integer(0)
                    .is_gt()
            );
            assert_eq!(original.frame_at_sample(AudioSample(300)).unwrap(), 0);
            160
        } else {
            // VFR audio continues after the last measured picture interval.
            let picture_end = source
                .video_mapping
                .start_frames()
                .checked_add(
                    source
                        .video_mapping
                        .duration_frames(source.duration)
                        .unwrap(),
                )
                .unwrap();
            let end = picture_end
                .checked_mul(
                    ExactRatio::new(
                        48_000 * i128::from(original.rate().denominator()),
                        i128::from(original.rate().numerator()),
                    )
                    .unwrap(),
                )
                .unwrap()
                .round_even()
                .unwrap();
            let audio = timing.audio.unwrap();
            let audio_end = audio
                .start_frames
                .checked_add(audio.duration_frames)
                .unwrap()
                .checked_mul(
                    ExactRatio::new(
                        48_000 * i128::from(original.rate().denominator()),
                        i128::from(original.rate().numerator()),
                    )
                    .unwrap(),
                )
                .unwrap()
                .round_even()
                .unwrap();
            // This fixture contains impulses, not a continuous tone. Its final
            // impulse is 200 samples before the measured audio end.
            let start = i64::try_from(audio_end).unwrap() - 350;
            assert!(i128::from(start) > end);
            start
        };
        let expected = canonical(&snapshot, &target, read_start, 256);
        assert!(
            expected.iter().any(|value| value.abs() > 0.0001),
            "{fixture} retains measured audio outside picture: read={read_start}, timing={timing:?}, peak={}",
            expected.iter().copied().map(f32::abs).fold(0.0, f32::max)
        );
        let (engine, devices) = engine(&permit);
        engine
            .play_target(1, snapshot, target, AudioSample(read_start), 0.25)
            .unwrap();
        let device = playing_device(&engine, &devices, 0);
        let (_, pcm) = device.render(256, 0, 10_000_000);
        assert_eq!(
            pcm,
            expected
                .iter()
                .map(|value| value * 0.25)
                .collect::<Vec<_>>()
        );
        engine.stop();
        update(&engine, Phase::Stopped);
    }
}

#[test]
fn bounded_window_finishes_at_exact_delivery_deadline() {
    let permit = crate::tests::resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("project.deadpan"), &empty()).unwrap();
    register(&mut store);
    let snapshot = snapshot(&store, 1);
    let expected = canonical(&snapshot, &Target::Sequence, 161, 57);
    let window = Window::new(AudioSample(161), AudioSample(218), false).unwrap();
    let (engine, devices) = engine(&permit);
    engine
        .play_window(1, snapshot, Target::Sequence, window, window.start(), 0.25)
        .unwrap();
    let device = playing_device(&engine, &devices, 0);
    let (report, pcm) = device.render(256, 0, 10_000_000);
    assert_eq!(report.status, RenderStatus::Ended);
    assert_eq!(report.first_sample, Some(161));
    assert_eq!(report.rendered_frames, 57);
    assert_eq!(
        pcm[..114],
        expected.iter().map(|v| v * 0.25).collect::<Vec<_>>()
    );
    assert!(pcm[114..].iter().all(|v| *v == 0.0));
    device.now.store(10_000_000, Ordering::Release);
    assert_eq!(
        update(&engine, Phase::Playing).sample,
        Some(AudioSample(161))
    );
    device.now.store(11_187_500, Ordering::Release);
    assert_eq!(update(&engine, Phase::Ended).sample, Some(AudioSample(218)));
    assert!(!device.started.load(Ordering::Acquire));
}

#[test]
fn loop_pcm_repeats_exact_content_with_monotonic_delivery_and_resumes_later_laps() {
    let permit = crate::tests::resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("project.deadpan"), &empty()).unwrap();
    register(&mut store);
    let snapshot = snapshot(&store, 1);
    let window = Window::new(AudioSample(160), AudioSample(177), true).unwrap();
    let expected = canonical(&snapshot, &Target::Sequence, 160, 17);
    let (engine, devices) = engine(&permit);
    for (index, start) in [160, 423].into_iter().enumerate() {
        engine
            .play_window(
                index as u64,
                snapshot.clone(),
                Target::Sequence,
                window,
                AudioSample(start),
                0.25,
            )
            .unwrap();
        let device = playing_device(&engine, &devices, index);
        // Cross two 8192-frame producer boundaries as well as thousands of
        // tiny content seams, including the offset resumed generation.
        for chunk in 0..72 {
            let (report, pcm) =
                device.render(256, chunk * 5_333_333, 10_000_000 + chunk * 5_333_333);
            assert_eq!(report.status, RenderStatus::Playing);
            assert_eq!(report.first_sample, Some(start + chunk as i64 * 256));
            for (frame, actual) in pcm.chunks_exact(2).enumerate() {
                let canonical_offset = ((start - 160) as usize + chunk as usize * 256 + frame) % 17;
                assert_eq!(
                    actual,
                    &expected[canonical_offset * 2..canonical_offset * 2 + 2]
                        .iter()
                        .map(|v| v * 0.25)
                        .collect::<Vec<_>>()
                );
            }
            device
                .now
                .store(10_000_000 + chunk * 5_333_333, Ordering::Release);
            let expected_sample = AudioSample(start + chunk as i64 * 256);
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                let heard = update(&engine, Phase::Playing).sample.unwrap();
                assert!(heard <= expected_sample);
                if heard == expected_sample {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "delivery did not reach {expected_sample:?}"
                );
            }
        }
        engine.stop_handle().stop();
        update(&engine, Phase::Stopped);
        let mut pcm = [1.0; 16];
        let report = device.callback.lock().unwrap().render(&mut pcm);
        assert_eq!(report.status, RenderStatus::Paused);
        assert_eq!(pcm, [0.0; 16]);
    }
}

#[test]
fn loop_faults_invalid_windows_and_overflow_are_explicit() {
    let permit = crate::tests::resources::pcm();
    for (start, end, looping) in [(-1, 2, false), (2, 1, false), (2, 2, true)] {
        assert_eq!(
            Window::new(AudioSample(start), AudioSample(end), looping),
            Err(RequestError::InvalidWindow)
        );
    }
    let directory = tempfile::tempdir().unwrap();
    let store = ProjectStore::create(&directory.path().join("project.deadpan"), &hold(1)).unwrap();
    let snapshot = snapshot(&store, 1);
    let (engine, devices) = engine(&permit);
    let window = Window::new(AudioSample(10), AudioSample(27), true).unwrap();
    assert_eq!(window.sample(AudioSample(27)), Some(AudioSample(10)));
    assert_eq!(window.lap(AudioSample(61)), Some(3));
    engine
        .play_window(
            1,
            snapshot.clone(),
            Target::Sequence,
            window,
            AudioSample(i64::MAX - 4),
            0.25,
        )
        .unwrap();
    assert!(
        update(&engine, Phase::Failed)
            .error
            .unwrap()
            .contains("overflow")
    );
    engine
        .play_window(
            2,
            snapshot.clone(),
            Target::Sequence,
            Window::new(AudioSample(0), AudioSample(1601), false).unwrap(),
            AudioSample(0),
            0.25,
        )
        .unwrap();
    assert!(
        update(&engine, Phase::Failed)
            .error
            .unwrap()
            .contains("past the target end")
    );
    engine
        .play_window(3, snapshot, Target::Sequence, window, window.start(), 0.25)
        .unwrap();
    let device = playing_device(&engine, &devices, 2);
    device.dropped.store(1, Ordering::Release);
    assert!(
        update(&engine, Phase::Failed)
            .error
            .unwrap()
            .contains("lost")
    );
    assert!(!device.started.load(Ordering::Acquire));
    assert_eq!(devices.lock().unwrap().len(), 3);
    assert_eq!(engine.poll(), None);
}
