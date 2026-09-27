use super::*;

pub(super) fn register_sound(store: &mut ProjectStore, name: &str, identity: &str) -> AssetId {
    let asset = AssetId::new(identity).unwrap();
    let wave = name.ends_with(".wav");
    let path = if wave {
        fixture()
            .parent()
            .unwrap()
            .join("../audio-fixtures")
            .join(name)
    } else {
        fixture().with_file_name(name)
    }
    .canonicalize()
    .unwrap();
    let original = store
        .retain_original(&path, OriginalOwnership::Managed, limits(), &cancelled())
        .unwrap()
        .record;
    let mut input = store
        .snapshot_original(original.object().content(), limits(), &cancelled())
        .unwrap();
    let audio = AudioSession::open_verified(
        &mut input,
        SourceContentIdentity::new(original.sha256(), original.object().byte_length()).unwrap(),
        if wave { 0 } else { 1 },
        AudioSessionLimits::default(),
        &cancelled(),
    )
    .unwrap();
    let decoded = DecodedSourceQualification::from_sessions(None, Some(&audio)).unwrap();
    store
        .register_source(
            &SourceRegistration {
                expected_revision: store.snapshot().unwrap().revision_id().clone(),
                new_revision: revision(&format!("registered-{identity}")),
                original: original.object().content().clone(),
                new_asset_id: asset.clone(),
                label: identity.into(),
                insertion: None,
            },
            &decoded,
            None,
            limits(),
            &cancelled(),
        )
        .unwrap();
    asset
}

fn sound(snapshot: &Snapshot, asset: &AssetId) -> Arc<Sound> {
    Arc::new(
        Sound::new(
            snapshot.document.presentation_basis().frame_rate,
            asset.clone(),
            snapshot.sources[asset].receipt.clone(),
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
fn measured_sound_end_governs_default_audition_and_full_source_loop() {
    let permit = crate::tests::resources::pcm();
    for (name, native_start, native_end, end) in [
        ("cfr-bframes.mp4", 0, 192_192, 192_192),
        ("offset-bframes.mp4", 95_072, 288_288, 193_216),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let mut store =
            ProjectStore::create(&directory.path().join("sound.deadpan"), &empty()).unwrap();
        let asset = register_sound(&mut store, name, "sound");
        let captured = snapshot(&store, 1);
        let sound = sound(&captured, &asset);
        let target = Target::Sound(sound.clone());
        assert_eq!(sound.asset(), &asset);
        assert_eq!(
            sound.qualification_id(),
            captured.sources[&asset].receipt.id()
        );
        assert_eq!(sound.duration_samples(), AudioSample(end));
        let view = target.document(&captured).unwrap();
        assert_eq!(view.project_id(), captured.document.project_id());
        assert_eq!(view.revision_id(), captured.document.revision_id());
        let NodeKind::Source { source } = &view.nodes()[&node("audition-sound-source")].kind else {
            panic!("sound view source missing");
        };
        assert_eq!(source.video, SourceVideo::Blank);
        let span = source.audio.as_ref().unwrap().span;
        assert_eq!(span.start().ticks, native_start);
        assert_eq!(span.end().ticks, native_end);
        assert_eq!(
            span.start().time_base,
            SourceTimeBase::new(1, 48_000).unwrap()
        );
        let plan_end = RenderPlan::compile(&view)
            .unwrap()
            .audio_duration()
            .unwrap();
        assert!(
            plan_end.0 > end,
            "fixture must expose whole-frame enclosure slack"
        );
        assert_eq!(target.effective_end(plan_end).unwrap(), AudioSample(end));
        let expected = canonical(&captured, &target, end - 57, 57);
        let saved = store.snapshot().unwrap();
        let history = store.history_availability().unwrap();
        let (engine, devices) = engine(&permit);
        engine
            .play_target(
                1,
                captured.clone(),
                target.clone(),
                AudioSample(end - 57),
                0.25,
            )
            .unwrap();
        let device = playing_device(&engine, &devices, 0);
        let (report, pcm) = device.render(256, 0, 10_000_000);
        assert_eq!(report.status, RenderStatus::Ended);
        assert_eq!(report.rendered_frames, 57);
        assert_eq!(report.first_sample, Some(end - 57));
        assert_eq!(
            pcm[..114],
            expected.iter().map(|v| v * 0.25).collect::<Vec<_>>()
        );
        assert!(pcm[114..].iter().all(|value| *value == 0.0));
        device.now.store(11_187_500, Ordering::Release);
        assert_eq!(update(&engine, Phase::Ended).sample, Some(AudioSample(end)));

        let window = Window::new(AudioSample(0), AudioSample(end), true).unwrap();
        let mut seam = canonical(&captured, &target, end - 17, 17);
        seam.extend(canonical(&captured, &target, 0, 239));
        engine
            .play_window(2, captured, target, window, AudioSample(end - 17), 0.25)
            .unwrap();
        let device = playing_device(&engine, &devices, 1);
        let (report, pcm) = device.render(256, 0, 10_000_000);
        assert_eq!(report.status, RenderStatus::Playing);
        assert_eq!(report.first_sample, Some(end - 17));
        assert_eq!(
            pcm,
            seam.iter().map(|value| value * 0.25).collect::<Vec<_>>()
        );
        assert_eq!(window.sample(AudioSample(end)), Some(AudioSample(0)));
        engine.stop();
        update(&engine, Phase::Stopped);
        assert_eq!(store.snapshot().unwrap(), saved);
        assert_eq!(store.history_availability().unwrap(), history);
        assert_eq!(
            saved.nodes().len(),
            1,
            "catalog audition must not insert a beat"
        );
    }
}

#[test]
fn sound_target_switches_canonical_caches_without_mutating_the_edit() {
    let permit = crate::tests::resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("sound.deadpan"), &hold(40)).unwrap();
    let first = register_sound(&mut store, "cfr-bframes.mp4", "first");
    let second = register_sound(&mut store, "offset-bframes.mp4", "second");
    let captured = snapshot(&store, 4);
    let first = Target::Sound(sound(&captured, &first));
    let second = Target::Sound(sound(&captured, &second));
    // The fixtures contain impulses at source sample 100. The offset receipt
    // retains 1,024 unknown-priming samples, so this common window includes
    // both distinct impulses rather than inspecting their near-silent decay.
    let first_pcm = canonical(&captured, &first, 0, 2_048);
    let second_pcm = canonical(&captured, &second, 0, 2_048);
    assert_ne!(first_pcm, second_pcm);
    assert!(first_pcm.iter().any(|value| value.abs() > 0.001));
    assert!(second_pcm.iter().any(|value| value.abs() > 0.001));
    let saved = store.snapshot().unwrap();
    let history = store.history_availability().unwrap();
    let (engine, devices) = engine(&permit);
    for (index, target) in [first.clone(), Target::Sequence, second, first]
        .into_iter()
        .enumerate()
    {
        let expected = canonical(&captured, &target, 0, 2_048);
        engine
            .play_target(index as u64, captured.clone(), target, AudioSample(0), 0.25)
            .unwrap();
        let device = playing_device(&engine, &devices, index);
        let (_, pcm) = device.render(2_048, 0, 10_000_000);
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
    assert_eq!(store.snapshot().unwrap(), saved);
    assert_eq!(store.history_availability().unwrap(), history);
}

#[test]
fn sound_contract_rejects_wrong_receipt_rate_asset_revision_and_extended_windows() {
    let permit = crate::tests::resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("sound.deadpan"), &empty()).unwrap();
    let first = register_sound(&mut store, "cfr-bframes.mp4", "first");
    let before_second = snapshot(&store, 1);
    let second = register_sound(&mut store, "offset-bframes.mp4", "second");
    let captured = snapshot(&store, 1);
    let descriptor = sound(&captured, &first);
    let target = Target::Sound(descriptor.clone());
    let second_target = Target::Sound(sound(&captured, &second));
    assert!(second_target.document(&before_second).is_err());
    let wrong_rate = Sound::new(
        FrameRate::new(24, 1).unwrap(),
        first.clone(),
        captured.sources[&first].receipt.clone(),
    )
    .unwrap();
    assert!(
        Target::Sound(Arc::new(wrong_rate))
            .document(&captured)
            .is_err()
    );
    let wrong_asset = Sound::new(
        descriptor.rate(),
        second.clone(),
        captured.sources[&first].receipt.clone(),
    )
    .unwrap();
    assert!(
        Target::Sound(Arc::new(wrong_asset))
            .document(&captured)
            .is_err()
    );
    let mut mismatched = Snapshot {
        session: captured.session,
        document: captured.document.clone(),
        sources: captured.sources.clone(),
        originals: captured.originals.clone(),
    };
    mismatched.sources.get_mut(&first).unwrap().receipt = captured.sources[&second].receipt.clone();
    assert!(target.document(&mismatched).is_err());
    mismatched.sources.remove(&first);
    assert!(target.document(&mismatched).is_err());
    let mut sources = Sources::new(captured.clone());
    assert!(
        sources
            .source(
                &ProjectId::new("other-project").unwrap(),
                captured.document.revision_id(),
                &first,
                &cancelled(),
            )
            .is_err()
    );
    assert!(
        sources
            .source(
                captured.document.project_id(),
                before_second.document.revision_id(),
                &first,
                &cancelled(),
            )
            .is_err()
    );

    let end = descriptor.duration_samples();
    let (engine, _) = engine(&permit);
    engine
        .play_target(
            1,
            captured.clone(),
            target.clone(),
            AudioSample(end.0 + 1),
            0.25,
        )
        .unwrap();
    assert!(
        update(&engine, Phase::Failed)
            .error
            .unwrap()
            .contains("outside the audition window")
    );
    let extended = Window::new(AudioSample(0), AudioSample(end.0 + 1), true).unwrap();
    engine
        .play_window(2, captured, target, extended, AudioSample(0), 0.25)
        .unwrap();
    assert!(
        update(&engine, Phase::Failed)
            .error
            .unwrap()
            .contains("past the target end")
    );
}

#[test]
fn sound_rejects_a_picture_qualification() {
    let _permit = crate::tests::resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("picture.deadpan"), &empty()).unwrap();
    register_media(&mut store, "cfr-bframes.mp4", true);
    let captured = snapshot(&store, 1);
    let asset = AssetId::new("original").unwrap();
    assert!(
        Sound::new(
            captured.document.presentation_basis().frame_rate,
            asset.clone(),
            captured.sources[&asset].receipt.clone(),
        )
        .is_err()
    );
}

#[test]
fn sound_retains_unknown_priming_and_original_sample_coordinates() {
    let _permit = crate::tests::resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("offset.deadpan"), &empty()).unwrap();
    register_media(&mut store, "offset-bframes.mp4", false);
    let captured = snapshot(&store, 1);
    let asset = AssetId::new("original").unwrap();
    let descriptor = sound(&captured, &asset);
    assert_eq!(descriptor.duration_samples(), AudioSample(193_216));
    let view = Target::Sound(descriptor).document(&captured).unwrap();
    let NodeKind::Source { source } = &view.nodes()[&node("audition-sound-source")].kind else {
        panic!("sound view source missing");
    };
    let span = source.audio.as_ref().unwrap().span;
    assert_eq!(span.start().ticks, 95_072);
    assert_eq!(span.end().ticks, 288_288);
    assert_eq!(source.audio_mapping.start_frames(), ExactRatio::integer(0));
    assert_eq!(source.audio_offset, AudioSample(0));
}

#[test]
fn decoded_44100_and_48000_spans_round_once_without_project_frame_slack() {
    let _permit = crate::tests::resources::pcm();
    // These plain WAVs have an unspecified channel layout. They establish
    // measured source clocks, not playable speaker interpretation; the AAC
    // tests above cover the production PCM reader without weakening admission.
    for (name, native_rate, native_samples, end) in [
        ("pcm-stereo-48000.wav", 48_000, 8_197, 8_197),
        ("pcm-mono-44100.wav", 44_100, 44_117, 48_019),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let mut store =
            ProjectStore::create(&directory.path().join("measured.deadpan"), &empty()).unwrap();
        let asset = register_sound(&mut store, name, "sound");
        let captured = snapshot(&store, 1);
        let descriptor = sound(&captured, &asset);
        assert_eq!(descriptor.duration_samples(), AudioSample(end));
        let target = Target::Sound(descriptor);
        let view = target.document(&captured).unwrap();
        let span = view.assets()[&asset].audio.unwrap();
        assert_eq!(span.start().ticks, 0);
        assert_eq!(span.end().ticks, native_samples);
        assert_eq!(
            span.start().time_base,
            SourceTimeBase::new(1, native_rate).unwrap()
        );
        let plan_end = RenderPlan::compile(&view)
            .unwrap()
            .audio_duration()
            .unwrap();
        assert!(plan_end.0 > end);
        assert_eq!(target.effective_end(plan_end).unwrap(), AudioSample(end));
    }
}
