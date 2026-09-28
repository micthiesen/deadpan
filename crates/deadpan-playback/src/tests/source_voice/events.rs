//! Authored sound events through real registration, history and the production
//! pre-master bus. References use decoded fixture samples and scalar sums.

use super::*;
use deadpan_audio::{LimitedTile, LimiterContext};

fn event(document: &ProjectDocument, asset: &AssetId, offset: i64, gain: i32) -> SoundEvent {
    let recipe = recipe(document, asset, 1000, offset);
    SoundEvent {
        owner: document.root().clone(),
        label: "Placed effect".into(),
        source: recipe.source,
        mapping: recipe.mapping,
        offset: recipe.offset,
        gain_millidecibels: gain,
        start_edge: AudioEdgePolicy::Automatic,
        end_edge: AudioEdgePolicy::Automatic,
        overflow: SoundOverflowPolicy::Reject,
    }
}

fn expected_sound(offset: i64, gain: i32, count: usize) -> Vec<[f64; 2]> {
    let pcm = reference(1000, offset, count);
    // Source length on the root RoundEven grid, independently calculated.
    // 1000*160/147 is 1088 + 64/147 samples, not the PointCeil 1089.
    let length = 1088_i64;
    let linear = 10.0_f64.powf(f64::from(gain) / 20_000.0);
    pcm.into_iter()
        .enumerate()
        .map(|(index, frame)| {
            let at = i64::try_from(index).unwrap() - offset;
            if !(0..length).contains(&at) {
                return [0.0; 2];
            }
            let edge = (((2 * at + 1).min(192).min(2 * (length - 1 - at) + 1)) as f32) / 192.0;
            [
                f64::from(frame[0] * edge) * linear,
                f64::from(frame[1] * edge) * linear,
            ]
        })
        .collect()
}

#[test]
fn authored_sounds_reopen_undo_and_mix_on_exact_root_samples_before_one_limiter() {
    let _permit = crate::tests::resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("events.deadpan");
    let mut store = ProjectStore::create(&path, &empty()).unwrap();
    register_media(&mut store, "cfr-bframes.mp4", true);
    let asset = register_mono(&mut store, directory.path());
    let before = store.snapshot().unwrap();
    let original_snapshot = snapshot(&store, 1);
    let original_plan = Arc::new(RenderPlan::compile(&before).unwrap());
    let mut original = StageAudio::new(original_plan);
    let mut original_sources = Sources::new(original_snapshot);
    let base = original
        .prepare_edge_faded(
            &mut original_sources,
            AudioSample(0),
            1600,
            TIMEOUT,
            &cancelled(),
        )
        .unwrap()
        .samples;
    let first = event(&before, &asset, 137, -6000);
    let mut second = event(&before, &asset, 311, 0);
    second.start_edge = AudioEdgePolicy::Hard;
    second.end_edge = AudioEdgePolicy::Hard;
    commit(
        &mut store,
        "first-effect",
        Command::SetSound {
            id: SoundId::new("first").unwrap(),
            event: first,
        },
    );
    commit(
        &mut store,
        "second-effect",
        Command::SetSound {
            id: SoundId::new("second").unwrap(),
            event: second,
        },
    );
    assert_eq!(
        store.snapshot().unwrap().duration().unwrap(),
        before.duration().unwrap()
    );
    drop(store);
    let mut store = ProjectStore::open(&path, deadpan_store::AccessMode::ReadWrite).unwrap();
    let current = store.snapshot().unwrap();
    assert_eq!(current.sounds().len(), 2);
    let plan = Arc::new(RenderPlan::compile(&current).unwrap());
    let mut renderer = StageAudio::new(plan.clone());
    let captured = snapshot(&store, 2);
    let mut sources = Sources::new(captured.clone());
    let first = expected_sound(137, -6000, 1600);
    let mut second = reference(1000, 311, 1600);
    second[1399..].fill([0.0; 2]);
    let expected = base
        .iter()
        .zip(&first)
        .zip(&second)
        .map(|((base, a), b)| {
            [
                (f64::from(base[0]) + a[0] + f64::from(b[0])) as f32,
                (f64::from(base[1]) + a[1] + f64::from(b[1])) as f32,
            ]
        })
        .collect::<Vec<_>>();
    for (start, count) in [(1200, 256), (0, 256), (130, 256), (301, 113), (1390, 31)] {
        let actual = renderer
            .prepare_edge_faded(
                &mut sources,
                AudioSample(start),
                count,
                TIMEOUT,
                &cancelled(),
            )
            .unwrap();
        assert_eq!(actual.stage, "authored_bus_pcm_before_mastering");
        let selected = &expected[start as usize..start as usize + count as usize];
        assert_eq!(
            actual.samples, selected,
            "independent scalar bus at {start}"
        );
    }
    // A separately assembled scalar bus goes through one canonical limiter.
    // Use its complete halo, including Original PCM after both sounds end.
    let project = AudioSample(0)..plan.audio_duration().unwrap();
    let requested = AudioSample(0)..AudioSample(8192);
    let context = LimitedTile::required_context(&project, &requested).unwrap();
    assert_eq!(context.start, AudioSample(0));
    let mut scalar = original
        .prepare_edge_faded(
            &mut original_sources,
            context.start,
            u32::try_from(context.end.0 - context.start.0).unwrap(),
            TIMEOUT,
            &cancelled(),
        )
        .unwrap()
        .samples;
    scalar[..expected.len()].copy_from_slice(&expected);
    let reference = LimitedTile::prepare(
        LimiterContext {
            project_samples: project,
            start: context.start,
            samples: scalar,
        },
        requested,
        Instant::now() + TIMEOUT,
        &cancelled(),
    )
    .unwrap();
    let mut limited = LimitedAudio::new(plan);
    let block = limited
        .read(&mut sources, AudioSample(0), 1600, TIMEOUT, &cancelled())
        .unwrap();
    assert_eq!(block.stage, "limited_authored_bus_pcm");
    assert_eq!(
        block.processing_order,
        [
            "per_voice_time_pitch_edges_gain",
            "group_mix",
            "stereo_limiter"
        ]
    );
    assert!(!block.verified_tiles.is_empty());
    assert_eq!(block.samples, reference.samples[..1600]);
    assert_eq!(block.gain, reference.gain[..1600]);
    assert!(
        block.gain.iter().any(|gain| *gain < 1.0),
        "reference must exercise limiting"
    );

    // Original and catalog audition are temporary source views, independent of
    // the edit's authored overlays, even when captured from this revision.
    let original_asset = AssetId::new("original").unwrap();
    let rate = current.presentation_basis().frame_rate;
    let original_target = Target::Original(Arc::new(
        Original::new(
            rate,
            original_asset.clone(),
            captured.sources[&original_asset].receipt.clone(),
        )
        .unwrap(),
    ));
    let sound_target = Target::Sound(Arc::new(
        Sound::new(
            rate,
            asset.clone(),
            captured.sources[&asset].receipt.clone(),
        )
        .unwrap(),
    ));
    for target in [original_target, sound_target] {
        assert!(target.document(&captured).unwrap().sounds().is_empty());
    }
    store
        .undo(
            store.snapshot().unwrap().revision_id(),
            revision("undo-second"),
        )
        .unwrap();
    store
        .undo(
            store.snapshot().unwrap().revision_id(),
            revision("undo-first"),
        )
        .unwrap();
    assert!(store.snapshot().unwrap().sounds().is_empty());
    let mut no_sound = StageAudio::new(Arc::new(
        RenderPlan::compile(&store.snapshot().unwrap()).unwrap(),
    ));
    let restored = no_sound
        .prepare_edge_faded(
            &mut Sources::new(snapshot(&store, 3)),
            AudioSample(0),
            1600,
            TIMEOUT,
            &cancelled(),
        )
        .unwrap();
    assert_eq!(
        restored.samples, base,
        "no-sound path must remain sample-identical"
    );
}

#[test]
fn silent_authored_sound_retains_source_admission_through_limiter_cache_and_recovery() {
    let _permit = crate::tests::resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("hold-events.deadpan"), &hold(40)).unwrap();
    let asset = register_mono(&mut store, directory.path());
    let event = event(&store.snapshot().unwrap(), &asset, 137, 0);
    commit(
        &mut store,
        "effect-over-hold",
        Command::SetSound {
            id: SoundId::new("effect").unwrap(),
            event,
        },
    );
    let plan = Arc::new(RenderPlan::compile(&store.snapshot().unwrap()).unwrap());
    let mut renderer = StageAudio::new(plan.clone());
    let captured = snapshot(&store, 1);
    let block = renderer
        .prepare_edge_faded(
            &mut Sources::new(captured.clone()),
            AudioSample(137),
            256,
            TIMEOUT,
            &cancelled(),
        )
        .unwrap();
    assert_eq!(block.samples, vec![[0.0; 2]; 256]);
    assert_eq!(block.suppressed, vec![AudioSample(137)..AudioSample(393)]);
    let mut limited = LimitedAudio::new(plan);
    let warm = limited
        .read(
            &mut Sources::new(captured.clone()),
            AudioSample(137),
            256,
            TIMEOUT,
            &cancelled(),
        )
        .unwrap();
    assert_eq!(warm.samples, vec![[0.0; 2]; 256]);
    assert_eq!(limited.cached_tile_count(), 1);
    let mut missing_sources = captured.sources.clone();
    missing_sources.remove(&asset);
    let missing = Arc::new(Snapshot {
        session: captured.session,
        document: captured.document.clone(),
        sources: missing_sources,
        originals: captured.originals.clone(),
    });
    let error = limited
        .read(
            &mut Sources::new(missing),
            AudioSample(137),
            256,
            TIMEOUT,
            &cancelled(),
        )
        .unwrap_err();
    assert!(error.to_string().contains("receipt"), "{error}");
    assert_eq!(
        limited.cached_tile_count(),
        0,
        "revoked dependency invalidates silent cached PCM"
    );
    let recovered = limited
        .read(
            &mut Sources::new(captured),
            AudioSample(137),
            256,
            TIMEOUT,
            &cancelled(),
        )
        .unwrap();
    assert_eq!(recovered.samples, warm.samples);
}

#[test]
fn authored_sound_fades_only_at_its_own_ends_and_silent_hold_edges_without_restarting() {
    let _permit = crate::tests::resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("gate-edges.deadpan"), &empty()).unwrap();
    register(&mut store);
    let asset = register_mono(&mut store, directory.path());
    commit(
        &mut store,
        "first-cut",
        Command::Split {
            node: node("source"),
            at: FrameDuration::new(1).unwrap(),
            identities: SplitIdentities {
                nodes: ["left", "right", "right-source"].map(node).to_vec(),
            },
        },
    );
    commit(
        &mut store,
        "second-cut",
        Command::Split {
            node: node("right"),
            at: FrameDuration::new(1).unwrap(),
            identities: SplitIdentities {
                nodes: ["tail", "tail-source"].map(node).to_vec(),
            },
        },
    );
    commit(
        &mut store,
        "pause",
        Command::Insert {
            parent: node("root"),
            index: 1,
            subtree: Subtree {
                root: node("pause"),
                overrides: Default::default(),
                gap_overrides: Default::default(),
                nodes: std::collections::BTreeMap::from([(
                    node("pause"),
                    BeatNode::hold(
                        "Silence",
                        HoldRecipe {
                            duration: FrameDuration::new(1).unwrap(),
                            video: HoldVideo::Background,
                            audio: HoldAudio::Silence,
                            picture_context: None,
                        },
                    ),
                )]),
            },
        },
    );
    let before = store.snapshot().unwrap();
    let mut original = StageAudio::new(Arc::new(RenderPlan::compile(&before).unwrap()));
    let base = original
        .prepare_edge_faded(
            &mut Sources::new(snapshot(&store, 1)),
            AudioSample(0),
            5600,
            TIMEOUT,
            &cancelled(),
        )
        .unwrap()
        .samples;
    let recipe = recipe(&before, &asset, 5000, 0);
    let mut sound = event(&before, &asset, 0, 0);
    sound.source = recipe.source;
    sound.mapping = recipe.mapping;
    commit(
        &mut store,
        "sound",
        Command::SetSound {
            id: SoundId::new("effect").unwrap(),
            event: sound,
        },
    );
    let raw = reference(5000, 0, 5600);
    let expected = base
        .iter()
        .zip(raw)
        .enumerate()
        .map(|(index, (original, sound))| {
            let selected = if index < 1600 {
                Some((0, 1600))
            } else if (3200..5442).contains(&index) {
                Some((3200, 5442))
            } else {
                None
            };
            let gain = selected.map_or(0.0, |(start, end)| {
                ((2 * (index - start) + 1)
                    .min(192)
                    .min(2 * (end - 1 - index) + 1)) as f32
                    / 192.0
            });
            [
                (f64::from(original[0]) + f64::from(sound[0] * gain)) as f32,
                (f64::from(original[1]) + f64::from(sound[1] * gain)) as f32,
            ]
        })
        .collect::<Vec<_>>();
    let captured = snapshot(&store, 2);
    let plan = Arc::new(RenderPlan::compile(&captured.document).unwrap());
    let mut renderer = StageAudio::new(plan.clone());
    let mut sources = Sources::new(captured.clone());
    for (start, count) in [(1500, 256), (3100, 256), (4700, 256), (5350, 200), (7, 101)] {
        let warm = renderer
            .prepare_edge_faded(
                &mut sources,
                AudioSample(start),
                count,
                TIMEOUT,
                &cancelled(),
            )
            .unwrap();
        let cold = StageAudio::new(plan.clone())
            .prepare_edge_faded(
                &mut Sources::new(captured.clone()),
                AudioSample(start),
                count,
                TIMEOUT,
                &cancelled(),
            )
            .unwrap();
        let selected = &expected[start as usize..start as usize + count as usize];
        assert_eq!(warm.samples, selected, "warm sound gate at {start}");
        assert_eq!(cold.samples, selected, "cold sound gate at {start}");
    }
}
