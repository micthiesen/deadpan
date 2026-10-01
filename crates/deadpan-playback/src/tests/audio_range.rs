use super::*;
use deadpan_audio::StageAudio;

fn span(start: i64, end: i64, hz: u32) -> SourceSpan {
    let time_base = SourceTimeBase::new(1, hz).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: start,
            time_base,
        },
        SourceTimestamp {
            ticks: end,
            time_base,
        },
    )
    .unwrap()
}

fn descriptor(snapshot: &Snapshot, asset: &AssetId, selected: SourceSpan) -> Arc<AudioRange> {
    Arc::new(
        AudioRange::new(
            snapshot.document.presentation_basis().frame_rate,
            asset.clone(),
            snapshot.sources[asset].receipt.clone(),
            selected,
        )
        .unwrap(),
    )
}

fn raw(snapshot: &Arc<Snapshot>, target: &Target, start: i64, count: u32) -> Vec<[f32; 2]> {
    let document = target.document(snapshot).unwrap();
    StageAudio::new(Arc::new(RenderPlan::compile(&document).unwrap()))
        .read(
            &mut Sources::new(snapshot.clone()),
            AudioSample(start),
            count,
            Duration::from_secs(60),
            &cancelled(),
        )
        .unwrap()
        .samples
}

fn canonical(snapshot: &Arc<Snapshot>, target: &Target, start: i64, count: u32) -> Vec<f32> {
    let document = target.document(snapshot).unwrap();
    LimitedAudio::new(Arc::new(RenderPlan::compile(&document).unwrap()))
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
fn selected_original_audio_keeps_complete_source_context_and_cold_seek_phase() {
    let _permit = resources::pcm();
    for name in ["cfr-bframes.mp4", "offset-bframes.mp4"] {
        let directory = tempfile::tempdir().unwrap();
        let mut store =
            ProjectStore::create(&directory.path().join("range.deadpan"), &empty()).unwrap();
        register_media(&mut store, name, true);
        let captured = snapshot(&store, 1);
        let asset = AssetId::new("original").unwrap();
        let receipt = captured.sources[&asset].receipt.clone();
        assert!(
            Sound::new(
                captured.document.presentation_basis().frame_rate,
                asset.clone(),
                receipt.clone()
            )
            .is_err()
        );
        let full = captured.document.assets()[&asset].audio.unwrap();
        let selected = span(full.start().ticks + 137, full.start().ticks + 1140, 48_000);
        let range = descriptor(&captured, &asset, selected);
        assert_eq!(range.span(), selected);
        assert_eq!(range.duration_samples(), AudioSample(1003));
        let target = Target::AudioRange(range);
        let view = target.document(&captured).unwrap();
        assert_eq!(view.project_id(), captured.document.project_id());
        assert_eq!(view.revision_id(), captured.document.revision_id());
        let NodeKind::Source { source } = &view.nodes()[&node("audition-audio-range-source")].kind
        else {
            panic!("audio range source missing")
        };
        assert_eq!(source.video, SourceVideo::Blank);
        assert_eq!(source.audio.as_ref().unwrap().span, full);
        assert_eq!(source.audio_offset, AudioSample(0));
        assert_eq!(source.link, LinkRelation::Independent);
        let SourceAudioMapping::SelectedPlacement {
            start,
            frames,
            selection,
        } = source.audio_mapping
        else {
            panic!("selection lost its complete mapping")
        };
        let rate = captured.document.presentation_basis().frame_rate;
        let selected_frames = SourceAudioMapping::natural_rate(selected, rate)
            .unwrap()
            .duration_frames(source.duration)
            .unwrap();
        assert_eq!(
            selection,
            ExactFrameRange::new(ExactRatio::ZERO, selected_frames).unwrap()
        );
        assert!(start.compare_integer(0).is_lt());
        assert_eq!(
            frames,
            SourceAudioMapping::natural_rate(full, rate)
                .unwrap()
                .duration_frames(source.duration)
                .unwrap()
        );
        let plan_end = RenderPlan::compile(&view)
            .unwrap()
            .audio_duration()
            .unwrap();
        assert!(plan_end.0 > 1003);
        assert_eq!(target.effective_end(plan_end).unwrap(), AudioSample(1003));
        let original = Target::Original(Arc::new(Original::new(rate, asset, receipt).unwrap()));
        for at in [0, 700, 0] {
            assert_eq!(
                raw(&captured, &target, at, 256),
                raw(&captured, &original, 137 + at, 256),
                "{name}: sample {at}"
            );
        }
        assert_eq!(store.snapshot().unwrap(), *captured.document);
    }
}

#[test]
fn audio_range_callbacks_stop_at_exact_end_loop_and_replace_same_source_ranges() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("range.deadpan"), &empty()).unwrap();
    register_media(&mut store, "cfr-bframes.mp4", true);
    let captured = snapshot(&store, 1);
    let asset = AssetId::new("original").unwrap();
    let first = Target::AudioRange(descriptor(&captured, &asset, span(37, 1040, 48_000)));
    let second = Target::AudioRange(descriptor(&captured, &asset, span(937, 1940, 48_000)));
    assert_ne!(first, second);
    assert_ne!(
        canonical(&captured, &first, 0, 256),
        canonical(&captured, &second, 0, 256)
    );
    let saved = store.snapshot().unwrap();
    let history = store.history_availability().unwrap();
    let (engine, devices) = engine(&permit);
    for (index, target) in [first.clone(), second, first.clone()]
        .into_iter()
        .enumerate()
    {
        let expected = canonical(&captured, &target, 0, 256);
        engine
            .play_target(
                index as u64 + 1,
                captured.clone(),
                target,
                AudioSample(0),
                0.25,
            )
            .unwrap();
        let device = playing_device(&engine, &devices, index);
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
    let expected = canonical(&captured, &first, 946, 57);
    engine
        .play_target(4, captured.clone(), first.clone(), AudioSample(946), 0.25)
        .unwrap();
    let device = playing_device(&engine, &devices, 3);
    let (report, pcm) = device.render(256, 0, 10_000_000);
    assert_eq!(report.status, RenderStatus::Ended);
    assert_eq!(report.rendered_frames, 57);
    assert_eq!(
        pcm[..114],
        expected
            .iter()
            .map(|value| value * 0.25)
            .collect::<Vec<_>>()
    );
    assert!(pcm[114..].iter().all(|value| *value == 0.0));
    device.now.store(11_187_500, Ordering::Release);
    assert_eq!(
        update(&engine, Phase::Ended).sample,
        Some(AudioSample(1003))
    );
    let window = Window::new(AudioSample(0), AudioSample(1003), true).unwrap();
    let mut seam = canonical(&captured, &first, 986, 17);
    seam.extend(canonical(&captured, &first, 0, 239));
    engine
        .play_window(5, captured, first, window, AudioSample(986), 0.25)
        .unwrap();
    let device = playing_device(&engine, &devices, 4);
    let (report, pcm) = device.render(256, 0, 10_000_000);
    assert_eq!(report.status, RenderStatus::Playing);
    assert_eq!(report.first_sample, Some(986));
    assert_eq!(
        pcm,
        seam.iter().map(|value| value * 0.25).collect::<Vec<_>>()
    );
    assert_eq!(window.sample(AudioSample(1003)), Some(AudioSample(0)));
    engine.stop();
    update(&engine, Phase::Stopped);
    assert_eq!(store.snapshot().unwrap(), saved);
    assert_eq!(store.history_availability().unwrap(), history);
}

#[test]
fn selected_44100_samples_round_once_and_reject_wrong_contracts() {
    let _permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("range.deadpan"), &empty()).unwrap();
    let asset = sound::register_sound(&mut store, "pcm-mono-44100.wav", "wave");
    let other = sound::register_sound(&mut store, "cfr-bframes.mp4", "other");
    let captured = snapshot(&store, 1);
    let selected = span(43, 1000, 44_100);
    let range = descriptor(&captured, &asset, selected);
    assert_eq!(range.duration_samples(), AudioSample(1042));
    assert_eq!(range.asset(), &asset);
    assert_eq!(
        range.qualification_id(),
        captured.sources[&asset].receipt.id()
    );
    let target = Target::AudioRange(range.clone());
    let view = target.document(&captured).unwrap();
    let NodeKind::Source { source } = &view.nodes()[&node("audition-audio-range-source")].kind
    else {
        panic!("audio range source missing")
    };
    assert_eq!(
        source.audio.as_ref().unwrap().span,
        captured.document.assets()[&asset].audio.unwrap()
    );
    assert_eq!(
        source.audio_mapping.start_frames(),
        ExactRatio::new(-43, 1470).unwrap()
    );
    let end = RenderPlan::compile(&view)
        .unwrap()
        .audio_duration()
        .unwrap();
    assert!(end.0 > 1042);
    assert_eq!(target.effective_end(end).unwrap(), AudioSample(1042));
    for invalid in [
        span(-1, 100, 44_100),
        span(44_100, 44_118, 44_100),
        span(87, 1999, 88_200),
    ] {
        assert!(
            AudioRange::new(
                range.rate(),
                asset.clone(),
                captured.sources[&asset].receipt.clone(),
                invalid
            )
            .is_err()
        );
    }
    let wrong_rate = AudioRange::new(
        FrameRate::new(24, 1).unwrap(),
        asset.clone(),
        captured.sources[&asset].receipt.clone(),
        selected,
    )
    .unwrap();
    assert!(
        Target::AudioRange(Arc::new(wrong_rate))
            .document(&captured)
            .is_err()
    );
    let wrong_asset = AudioRange::new(
        range.rate(),
        other.clone(),
        captured.sources[&asset].receipt.clone(),
        selected,
    )
    .unwrap();
    assert!(
        Target::AudioRange(Arc::new(wrong_asset))
            .document(&captured)
            .is_err()
    );
    let mut mismatched = Snapshot::committed(
        captured.session,
        captured.document.clone(),
        captured.sources.as_ref().clone(),
        captured.originals.clone(),
    );
    Arc::make_mut(&mut mismatched.sources)
        .get_mut(&asset)
        .unwrap()
        .original = captured.sources[&other].original.clone();
    assert!(target.document(&mismatched).is_err());
    Arc::make_mut(&mut mismatched.sources)
        .get_mut(&asset)
        .unwrap()
        .original = captured.sources[&asset].original.clone();
    Arc::make_mut(&mut mismatched.sources)
        .get_mut(&asset)
        .unwrap()
        .receipt = captured.sources[&other].receipt.clone();
    assert!(target.document(&mismatched).is_err());
    Arc::make_mut(&mut mismatched.sources).remove(&asset);
    assert!(target.document(&mismatched).is_err());
}
