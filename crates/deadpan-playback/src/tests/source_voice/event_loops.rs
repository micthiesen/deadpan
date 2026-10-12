//! Real registered sound, canonical treatments and returns through the production
//! loop feeder and output callback. The device only supplies delivery reports.
use super::*;

fn treatments() -> AudioTreatments {
    let range = GainRange::new(ExactRatio::ZERO, ExactRatio::integer(28)).unwrap();
    AudioTreatments::from_clip_gain(
        ClipGain::new(
            GainDb::new(-3000).unwrap(),
            false,
            vec![
                GainEnvelope::new(
                    GainClock::OwnerOutput,
                    range,
                    GainDb::new(-9000).unwrap(),
                    vec![
                        GainSegment::new(
                            range.end(),
                            GainDb::new(3000).unwrap(),
                            GainCurve::Smoothstep,
                        )
                        .unwrap(),
                    ],
                )
                .unwrap(),
            ],
            vec![GainRange::new(ExactRatio::integer(8), ExactRatio::integer(9)).unwrap()],
        )
        .unwrap(),
    )
    .with_equalizer(Some(
        EqualizerRecipe::new(
            [GainDb::UNITY, GainDb::new(-6000).unwrap(), GainDb::UNITY],
            Some(EqCutoffHz::new(80).unwrap()),
            None,
        )
        .unwrap(),
    ))
    .unwrap()
    .with_saturation(Some(Saturation::new(GainDb::new(9000).unwrap()).unwrap()))
    .unwrap()
    .with_balance(Some(StereoBalance::new(-35).unwrap()))
    .unwrap()
    .with_wet_sends(
        TailSends::default()
            .with_level(TailEffect::Reverb, Some(GainDb::new(-9000).unwrap()))
            .with_level(TailEffect::Delay, Some(GainDb::new(-6000).unwrap())),
    )
}

fn placed(store: &mut ProjectStore, asset: &AssetId, attached: bool) {
    commit(
        store,
        "sound-owner",
        Command::Group {
            parent: node("root"),
            start: 0,
            end: 1,
            id: node("owner"),
            label: "Owner".into(),
        },
    );
    // Mute only the Original child, leaving the independent sound and its
    // owner's bus active. A loop with no event must be digital silence.
    commit(
        store,
        "mute-original",
        Command::SetAudioTreatments {
            node: node("source"),
            treatments: AudioTreatments::from_clip_gain(
                ClipGain::new(GainDb::UNITY, true, vec![], vec![]).unwrap(),
            ),
        },
    );
    let recipe = recipe(&store.snapshot().unwrap(), asset, 40_000, 137);
    let event = SoundEvent {
        owner: node("root"),
        label: "Treated loop sound".into(),
        source: recipe.source,
        mapping: recipe.mapping,
        offset: recipe.offset,
        treatments: treatments(),
        start_edge: AudioEdgePolicy::Automatic,
        end_edge: AudioEdgePolicy::Automatic,
        overflow: SoundOverflowPolicy::Reject,
    };
    let command = if attached {
        Command::SetBeatSound {
            owner: node("owner"),
            id: SoundId::new("effect").unwrap(),
            event: BeatSound {
                label: event.label,
                source: event.source,
                mapping: event.mapping,
                offset: event.offset,
                treatments: event.treatments,
                start_edge: event.start_edge,
                end_edge: event.end_edge,
                overflow: event.overflow,
            },
        }
    } else {
        Command::SetSound {
            id: SoundId::new("effect").unwrap(),
            event,
        }
    };
    commit(store, "treated-sound", command);
}

#[test]
fn treated_sound_loops_keep_full_history_across_seams_and_resumed_laps() {
    let permit = crate::tests::resources::pcm();
    for attached in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("loop.deadpan");
        let mut store = ProjectStore::create(&path, &empty()).unwrap();
        register_media(&mut store, "cfr-bframes.mp4", true);
        let asset = register_mono(&mut store, directory.path());
        placed(&mut store, &asset, attached);
        let saved = store.snapshot().unwrap();
        drop(store);
        let store = ProjectStore::open(&path, deadpan_store::AccessMode::ReadWrite).unwrap();
        assert_eq!(store.snapshot().unwrap(), saved);
        let history = store.history_availability().unwrap();
        let captured = snapshot(&store, 1);
        let plan = Arc::new(RenderPlan::compile(&saved).unwrap());
        let mut canonical = LimitedAudio::new(plan);
        let mut sources = Sources::new(captured.clone());
        let mut full = Vec::new();
        while full.len() < 48_000 {
            full.extend(
                canonical
                    .read(
                        &mut sources,
                        AudioSample(full.len() as i64),
                        (48_000 - full.len()).min(8192) as u32,
                        TIMEOUT,
                        &cancelled(),
                    )
                    .unwrap_or_else(|error| panic!("canonical at {}: {error:?}", full.len()))
                    .samples,
            );
        }
        assert!(full.iter().flatten().any(|value| value.abs() > 0.01));
        assert!(full[43_674..].iter().all(|frame| *frame == [0.0; 2]));
        let (engine, devices) = engine(&permit);
        let mut generation = 0;
        // Begin after the event's own onset and before a mute exit or its end.
        // Each window is shorter than effect history and crosses neither from
        // project zero, so resetting a processor at a loop seam cannot pass.
        for (first, length) in [(14_301_i64, 1021_i64), (43_301, 997)] {
            let window =
                Window::new(AudioSample(first), AudioSample(first + length), true).unwrap();
            assert!(
                full[first as usize..(first + length) as usize]
                    .iter()
                    .flatten()
                    .any(|value| value.abs() > 0.0001)
            );
            for start in [first, first + 2 * length + 13] {
                engine
                    .play_window(
                        generation as u64,
                        captured.clone(),
                        Target::Sequence,
                        window,
                        AudioSample(start),
                        0.375,
                    )
                    .unwrap();
                let device = playing_device(&engine, &devices, generation);
                generation += 1;
                // Cross two producer boundaries and many content seams. The
                // heard clock remains monotonic even for a resumed later lap.
                for chunk in 0..72_u64 {
                    let delivered = start + chunk as i64 * 256;
                    let now = 10_000_000 + chunk * 5_333_333;
                    let (report, pcm) = device.render(256, chunk * 5_333_333, now);
                    assert_eq!(report.status, RenderStatus::Playing);
                    assert_eq!(report.first_sample, Some(delivered));
                    for (frame, actual) in pcm.chunks_exact(2).enumerate() {
                        let at = first + (delivered - first + frame as i64) % length;
                        assert_eq!(
                            actual,
                            full[at as usize].map(|value| value * 0.375),
                            "attached={attached}, delivered={delivered}, frame={frame}"
                        );
                    }
                    device.now.store(now, Ordering::Release);
                    assert!(wait(|| {
                        engine.poll().is_some_and(|update| {
                            assert_ne!(update.phase, Phase::Failed, "{update:?}");
                            update.phase == Phase::Playing
                                && update.sample == Some(AudioSample(delivered))
                        })
                    }));
                }
                engine.stop_handle().stop();
                update(&engine, Phase::Stopped);
            }
        }
        assert_eq!(store.snapshot().unwrap(), saved);
        assert_eq!(store.history_availability().unwrap(), history);
        store.validate().unwrap();
    }
}
