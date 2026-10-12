use super::*;

fn eq(gains: [i32; 3]) -> EqualizerRecipe {
    EqualizerRecipe::new(gains.map(|value| GainDb::new(value).unwrap()), None, None).unwrap()
}

fn authored(
    renderer: &mut StageAudio,
    provider: &mut Provider,
    start: i64,
    count: u32,
) -> Vec<[f32; 2]> {
    renderer
        .prepare_authored_bus(
            provider,
            AudioSample(start),
            count,
            Duration::from_secs(30),
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples
}

fn filtered(input: &[[f64; 2]], recipe: EqualizerRecipe) -> Vec<[f64; 2]> {
    let cancelled = AtomicBool::new(false);
    let mut filter = crate::Equalizer::new(recipe, &cancelled).unwrap();
    let mut result = Vec::new();
    for start in (0..input.len()).step_by(1024) {
        let mut left = vec![0.0; 5120];
        let mut right = vec![0.0; 5120];
        for index in 0..5120 {
            let at = start as i64 + index as i64 - 2048;
            if let Ok(at) = usize::try_from(at)
                && let Some(frame) = input.get(at)
            {
                left[index] = frame[0];
                right[index] = frame[1];
            }
        }
        let mut out_left = vec![0.0; 1024];
        let mut out_right = vec![0.0; 1024];
        filter
            .process_tile(&left, &right, &mut out_left, &mut out_right, &cancelled)
            .unwrap();
        result.extend(
            out_left
                .into_iter()
                .zip(out_right)
                .take((input.len() - start).min(1024))
                .map(|(l, r)| [l, r]),
        );
    }
    result
}

fn close(actual: &[[f32; 2]], expected: &[[f64; 2]]) {
    assert_eq!(actual.len(), expected.len());
    let worst = actual
        .iter()
        .zip(expected)
        .flat_map(|(a, b)| a.iter().zip(b).map(|(a, b)| (f64::from(*a) - b).abs()))
        .fold(0.0_f64, f64::max);
    assert!(worst < 2e-6, "PCM error {worst}");
}

#[test]
fn authored_eq_stored_order_matches_explicit_owner_processing_and_silence() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let recipe = eq([6000, -6000, 3000]);
    for order in [
        vec![
            AudioTreatmentStage::ClipGain,
            AudioTreatmentStage::Saturation,
            AudioTreatmentStage::StereoBalance,
        ],
        vec![
            AudioTreatmentStage::Saturation,
            AudioTreatmentStage::ClipGain,
            AudioTreatmentStage::StereoBalance,
        ],
    ] {
        let mut a = source(rate, 0, 8192, 8192);
        a.audio_treatments = AudioTreatments::with_all_stages(
            order.clone(),
            Some(
                ClipGain::new(
                    GainDb::new(6000).unwrap(),
                    false,
                    vec![],
                    vec![
                        GainRange::new(ExactRatio::integer(2048), ExactRatio::integer(2080))
                            .unwrap(),
                    ],
                )
                .unwrap(),
            ),
            Some(Saturation::new(GainDb::new(9000).unwrap()).unwrap()),
            Some(StereoBalance::new(-25).unwrap()),
        )
        .unwrap();
        let (before, mut provider) = document(rate, &["a"], vec![("a", a.clone())]);
        let mut stored_order = order;
        stored_order.insert(1, AudioTreatmentStage::Equalizer);
        a.audio_treatments = AudioTreatments::with_equalizer_stages(
            stored_order,
            a.audio_treatments.clip_gain().cloned(),
            a.audio_treatments.saturation(),
            a.audio_treatments.balance(),
            Some(recipe),
        )
        .unwrap();
        let (stored, _) = document(rate, &["a"], vec![("a", a)]);
        let before = Arc::new(RenderPlan::compile(&before).unwrap());
        let stored = Arc::new(RenderPlan::compile(&stored).unwrap());
        assert!(stored.has_audio_equalizers());
        let input = before
            .audio_owner_input(
                &owner(&before, "a", 2048),
                AudioSample(2048),
                Default::default(),
            )
            .unwrap();
        let mut explicit =
            PreparedOwnerEqualizer::new(input, 1, recipe, &AtomicBool::new(false)).unwrap();
        let mut reference = StageAudio::new(Arc::clone(&before));
        let mut renderer = StageAudio::new(Arc::clone(&stored));
        for at in [1984, 0, 8100] {
            let expected = reference
                .read_owner_equalized(
                    &mut provider,
                    &mut explicit,
                    AudioSample(at),
                    64,
                    Duration::from_secs(10),
                    &AtomicBool::new(false),
                )
                .unwrap();
            close(
                &authored(&mut renderer, &mut provider, at, 64),
                &expected.samples,
            );
        }
    }
}

#[test]
fn authored_eq_nested_owners_keep_f64_order_hidden_crops_and_cold_partitioning() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let child_eq = eq([6000, -6000, 3000]);
    let group_eq = eq([-3000, 6000, -9000]);
    let mut a = source(rate, 0, 8192, 8192);
    a.audio_treatments = AudioTreatments::default()
        .with_equalizer(Some(child_eq))
        .unwrap()
        .with_saturation(Some(Saturation::new(GainDb::new(9000).unwrap()).unwrap()))
        .unwrap();
    let mut group = BeatNode::sequence("Group", vec![id("a")]);
    group.audio_treatments = AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::new(-3000).unwrap(), false, vec![], vec![]).unwrap(),
    )
    .with_equalizer(Some(group_eq))
    .unwrap();
    let (whole, mut provider) = document(
        rate,
        &["group"],
        vec![("a", a.clone()), ("group", group.clone())],
    );
    let (cropped, _) = document(
        rate,
        &["crop"],
        vec![
            ("a", a),
            ("group", group),
            ("crop", crop("group", 2001..7001)),
        ],
    );
    let whole = Arc::new(RenderPlan::compile(&whole).unwrap());
    let cropped = Arc::new(RenderPlan::compile(&cropped).unwrap());
    let mut raw_renderer = StageAudio::new(Arc::clone(&whole));
    let raw: Vec<_> = root_samples(&mut raw_renderer, &mut provider, 0, 8192)
        .into_iter()
        .map(|frame| frame.map(f64::from))
        .collect();
    let input: Vec<_> = filtered(&raw, child_eq)
        .into_iter()
        .map(|frame| {
            frame.map(|value| {
                Saturation::new(GainDb::new(9000).unwrap())
                    .unwrap()
                    .shape(value)
                    * 10.0_f64.powf(-3000.0 / 20_000.0)
            })
        })
        .collect();
    let expected = filtered(&input, group_eq);
    for at in [0, 1010, 2095, 4900] {
        let mut cold = StageAudio::new(Arc::clone(&cropped));
        let actual = authored(&mut cold, &mut provider, at, 96);
        close(&actual, &expected[at as usize + 2001..at as usize + 2097]);
        assert_eq!(
            authored(&mut cold, &mut provider, at + 63, 1)[0],
            actual[63]
        );
        assert_eq!(cold.active_frames, 0);
    }
}

#[test]
fn authored_eq_retained_nested_history_matches_unmoved_owner_clocks() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut a = source(rate, 0, 8192, 8192);
    a.audio_treatments = AudioTreatments::default()
        .with_equalizer(Some(eq([6000, -6000, 3000])))
        .unwrap();
    let mut group = BeatNode::sequence("Group", vec![id("a")]);
    group.audio_treatments = AudioTreatments::default()
        .with_equalizer(Some(eq([-3000, 6000, 0])))
        .unwrap();
    let (old, mut provider) = document(
        rate,
        &["stage"],
        vec![
            ("a", a.clone()),
            ("group", group.clone()),
            ("stage", preserve("group", 8192, 10000)),
        ],
    );
    let (moved, _) = document(
        rate,
        &["prefix", "stage"],
        vec![
            ("prefix", hold(512)),
            ("a", a),
            ("group", group),
            ("stage", preserve("group", 8192, 10000)),
        ],
    );
    let moved = bind(
        &old,
        &moved,
        vec![(
            "stage",
            lattice("stage", AudioClockRoot::ProjectRootRoundEven),
            Some(AudioResume {
                local_boundary: ExactRatio::ZERO,
                phase: AudioLocalPhase {
                    constant: ExactRatio::integer(2048),
                    terms: vec![],
                },
            }),
        )],
    );
    let old = Arc::new(RenderPlan::compile(&old).unwrap());
    let moved = Arc::new(RenderPlan::compile(&moved).unwrap());
    let mut reference = StageAudio::new(old);
    let mut renderer = StageAudio::new(moved);
    for at in [512, 1000, 2048, 5500] {
        let expected = authored(&mut reference, &mut provider, at + 1536, 64);
        let actual = authored(&mut renderer, &mut provider, at, 64);
        for (a, b) in actual.iter().zip(expected) {
            for (a, b) in a.iter().zip(b) {
                assert!((*a - b).abs() < 1e-6);
            }
        }
    }
}

#[test]
fn authored_eq_silent_hold_masks_filter_history_and_limits_release_pcm() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut group = group();
    group.audio_treatments = AudioTreatments::default()
        .with_equalizer(Some(eq([6000, -6000, 3000])))
        .unwrap();
    let (doc, mut provider) = document(
        rate,
        &["group"],
        vec![
            ("a", source(rate, 0, 4096, 4096)),
            ("b", hold(4096)),
            ("group", group),
        ],
    );
    let plan = Arc::new(RenderPlan::compile(&doc).unwrap());
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let block = renderer
        .prepare_authored_bus(
            &mut provider,
            AudioSample(4064),
            96,
            Duration::from_secs(10),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert!(block.samples[..32].iter().any(|frame| *frame != [0.0; 2]));
    assert!(block.samples[32..].iter().all(|frame| *frame == [0.0; 2]));
    assert_eq!(block.suppressed, vec![AudioSample(4096)..AudioSample(4160)]);
    let mut small = StageAudio::with_limits(
        Arc::clone(&plan),
        StageLimits {
            maximum_resident_frames: 1024,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        small.prepare_authored_bus(
            &mut provider,
            AudioSample(2048),
            64,
            Duration::from_secs(10),
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Limit(_))
    ));
    assert_eq!(small.active_frames, 0);
    assert_eq!(renderer.active_frames, 0);
}

#[test]
fn authored_eq_whole_repeat_reuses_one_owner_across_many_play_clocks() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let recipe = eq([6000, -6000, 3000]);
    let mut repeat = hold(1);
    repeat.kind = NodeKind::Repeat {
        child: id("a"),
        iterations: IterationOrder::new(revision("plays"), 70).unwrap(),
        gap: None,
        escalation: None,
    };
    repeat.audio_treatments = AudioTreatments::default()
        .with_equalizer(Some(recipe))
        .unwrap();
    let (doc, mut provider) = document(
        rate,
        &["repeat"],
        vec![("a", source(rate, 0, 32, 32)), ("repeat", repeat)],
    );
    let plan = Arc::new(RenderPlan::compile(&doc).unwrap());
    let mut renderer = StageAudio::new(plan);
    let raw: Vec<_> = root_samples(&mut renderer, &mut provider, 0, 2240)
        .into_iter()
        .map(|sample| sample.map(f64::from))
        .collect();
    let expected = filtered(&raw, recipe);
    close(&authored(&mut renderer, &mut provider, 0, 2240), &expected);
    assert_eq!(renderer.active_frames, 0);
}

#[test]
fn authored_eq_tail_uses_processed_input_and_shared_parent_once() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut a = source(rate, 0, 4096, 4096);
    a.audio_treatments = AudioTreatments::default()
        .with_equalizer(Some(eq([3000, -6000, 0])))
        .unwrap();
    let mut tail = hold(20000);
    let NodeKind::Hold { recipe } = &mut tail.kind else {
        unreachable!()
    };
    recipe.audio = HoldAudio::Tail {
        maximum: frames(18000),
        effect: TailEffect::Delay,
    };
    let (doc, mut provider) = document(rate, &["a", "tail"], vec![("a", a), ("tail", tail)]);
    let plan = Arc::new(RenderPlan::compile(&doc).unwrap());
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let heard = authored(&mut renderer, &mut provider, 0, 4096);
    let expected = crate::render_tail(
        crate::TailRecipe::new(TailEffect::Delay, 18000, 20000).unwrap(),
        &heard,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(
        expected.iter().any(|frame| frame[0].abs() > 1e-4),
        "the delay must actually ring"
    );
    let mut wire = serde_json::to_value(&doc).unwrap();
    wire["nodes"]["root"]["audio_treatments"] = serde_json::to_value(
        AudioTreatments::default()
            .with_equalizer(Some(eq([6000; 3])))
            .unwrap(),
    )
    .unwrap();
    let with_parent = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(&with_parent).unwrap()));
    let actual = authored(&mut renderer, &mut provider, 4096, 20000);
    let factor = 10.0_f32.powf(6000.0 / 20_000.0);
    for (a, b) in actual.iter().zip(&expected) {
        for (a, b) in a.iter().zip(b) {
            assert!((a - b * factor).abs() < 1e-6);
        }
    }
    assert_eq!(renderer.active_frames, 0);
}

#[test]
fn authored_eq_independent_sounds_fail_explicitly_before_source_admission() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut a = source(rate, 0, 4096, 4096);
    a.audio_treatments = AudioTreatments::default()
        .with_equalizer(Some(eq([3000, -6000, 0])))
        .unwrap();
    let NodeKind::Source { source } = &a.kind else {
        unreachable!()
    };
    let audio = source.audio.clone().unwrap();
    let (doc, mut provider) = document(rate, &["a"], vec![("a", a)]);
    let mut wire = serde_json::to_value(&doc).unwrap();
    let mut asset = doc.assets()[&AssetId::new("media").unwrap()].clone();
    asset.source_qualification = Some(SourceQualificationId::new("b".repeat(64)).unwrap());
    asset.still_image = false;
    wire["assets"]["media"] = serde_json::to_value(asset).unwrap();
    wire["sounds"] = serde_json::to_value(BTreeMap::from([(
        SoundId::new("sound").unwrap(),
        SoundEvent {
            owner: id("root"),
            label: "Independent".into(),
            mapping: SourceAudioMapping::natural_rate(audio.span, rate).unwrap(),
            source: audio,
            offset: AudioSample(0),
            gain_millidecibels: 0,
            start_edge: AudioEdgePolicy::Hard,
            end_edge: AudioEdgePolicy::Hard,
            overflow: SoundOverflowPolicy::Reject,
        },
    )]))
    .unwrap();
    let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(&doc).unwrap()));
    assert!(matches!(
        renderer.prepare_authored_bus(
            &mut provider,
            AudioSample(0),
            256,
            Duration::from_secs(10),
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Unsupported(
            "EQ with independent sound contributions"
        ))
    ));
    assert_eq!(provider.calls, 0);
    assert_eq!(renderer.active_frames, 0);
}
