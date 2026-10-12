use super::*;
use deadpan_plan::{AudioOwnerClock, AudioOwnerInputScope};

fn owner<'a>(plan: &'a RenderPlan, name: &str, at: i64) -> AudioOwnerClock<'a> {
    plan.audio_gain_owners(AudioSample(at)..AudioSample(at + 1), Default::default())
        .unwrap()
        .spans()[0]
        .owners()
        .iter()
        .find(|clock| clock.instance().node == id(name))
        .unwrap()
        .clone()
}

fn crop(child: &str, selected: Range<i64>) -> BeatNode {
    let mut node = preserve(child, selected.end, selected.end - selected.start);
    let NodeKind::Retime {
        mapping,
        pitch,
        purpose,
        ..
    } = &mut node.kind
    else {
        unreachable!()
    };
    *mapping = FrameRange::new(ProjectFrame(selected.start), ProjectFrame(selected.end)).unwrap();
    *pitch = PitchPolicy::FollowSpeed;
    *purpose = RetimePurpose::Partition;
    node
}

fn group() -> BeatNode {
    BeatNode::sequence("Group", vec![id("a"), id("b")])
}

fn root_samples(
    renderer: &mut StageAudio,
    provider: &mut Provider,
    start: i64,
    frames: u32,
) -> Vec<[f32; 2]> {
    let mut samples = Vec::new();
    while samples.len() < frames as usize {
        let count = (frames - samples.len() as u32).min(MAX_OUTPUT_FRAMES);
        samples.extend(
            renderer
                .read_edge_faded(
                    provider,
                    AudioSample(start + samples.len() as i64),
                    count,
                    Duration::from_secs(10),
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples,
        );
    }
    samples
}

#[test]
fn eq_input_uses_hidden_group_audio_and_matches_uncropped_processing() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let nodes = || {
        vec![
            ("a", source(rate, 0, 4096, 4096)),
            ("b", source(rate, 4096, 8192, 4096)),
            ("group", group()),
        ]
    };
    let (whole, mut provider) = document(rate, &["group"], nodes());
    let mut cropped_nodes = nodes();
    cropped_nodes.push(("crop", crop("group", 2048..6144)));
    let (cropped, _) = document(rate, &["crop"], cropped_nodes);
    let plan = Arc::new(RenderPlan::compile(&cropped).unwrap());
    let input = plan
        .audio_owner_input(
            &owner(&plan, "group", 1024),
            AudioSample(1024),
            Default::default(),
        )
        .unwrap();
    assert_eq!(input.scope(), AudioOwnerInputScope::Owner);
    assert_eq!(
        input.context().root_samples(),
        AudioSample(-2048)..AudioSample(6144)
    );
    let mut expected_renderer = StageAudio::new(Arc::new(RenderPlan::compile(&whole).unwrap()));
    let expected = root_samples(&mut expected_renderer, &mut provider, 1024, 5120);
    let mut actual = vec![[0.0; 2]; 5120];
    // Cold, reverse-order requests exercise hidden negative context as well as
    // the two independently faded Sources at their interior cut.
    for index in (0..20).rev() {
        let offset = index * 256;
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let block = renderer
            .read_owner_input(
                &mut provider,
                &input,
                AudioSample(offset as i64 - 1024),
                256,
                Duration::from_secs(10),
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(block.stage, "owner_input_after_time_pitch_and_edges");
        actual[offset..offset + 256].copy_from_slice(&block.samples);
    }
    assert_eq!(actual, expected);
    assert!(
        actual[..1024]
            .iter()
            .any(|sample| sample.iter().any(|value| *value != 0.0)),
        "a root-only halo would lose these samples"
    );
    let recipe = EqualizerRecipe::new(
        [
            GainDb::new(6000).unwrap(),
            GainDb::new(-6000).unwrap(),
            GainDb::new(3000).unwrap(),
        ],
        None,
        None,
    )
    .unwrap();
    let filter = |samples: &[[f32; 2]]| {
        let cancelled = AtomicBool::new(false);
        let mut eq = crate::Equalizer::new(recipe, &cancelled).unwrap();
        let left: [f64; 5120] = std::array::from_fn(|i| f64::from(samples[i][0]));
        let right: [f64; 5120] = std::array::from_fn(|i| f64::from(samples[i][1]));
        let mut output = [[0.0; 1024]; 2];
        let [out_left, out_right] = &mut output;
        eq.process_tile(&left, &right, out_left, out_right, &cancelled)
            .unwrap();
        output
    };
    assert_eq!(filter(&actual), filter(&expected));
    let mut clipped = actual.clone();
    clipped[..1024].fill([0.0; 2]);
    assert_ne!(
        filter(&actual),
        filter(&clipped),
        "discarding hidden context changes EQ output"
    );
}

#[test]
fn descendant_input_runs_enclosing_time_pitch_before_edges_and_keeps_cold_parity() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let nodes = || {
        vec![
            ("a", source(rate, 0, 1200, 1200)),
            ("b", source(rate, 2400, 3600, 1200)),
            ("group", group()),
            ("stage", preserve("group", 2400, 3600)),
        ]
    };
    let (whole, mut provider) = document(rate, &["stage"], nodes());
    let mut cropped_nodes = nodes();
    cropped_nodes.push(("crop", crop("stage", 1200..2400)));
    let (cropped, _) = document(rate, &["crop"], cropped_nodes);
    let plan = Arc::new(RenderPlan::compile(&cropped).unwrap());
    let input = plan
        .audio_owner_input(
            &owner(&plan, "group", 200),
            AudioSample(200),
            Default::default(),
        )
        .unwrap();
    assert_eq!(input.scope(), AudioOwnerInputScope::EnclosingProcessor);
    assert_eq!(input.context().instance().node, id("stage"));
    assert_eq!(
        input.context().root_samples(),
        AudioSample(-1200)..AudioSample(2400)
    );
    let mut expected = StageAudio::new(Arc::new(RenderPlan::compile(&whole).unwrap()));
    let full = root_samples(&mut expected, &mut provider, 0, 3600);
    for start in [3344, 0, 1728, 700, 2400] {
        let mut cold = StageAudio::new(Arc::clone(&plan));
        let actual = cold
            .read_owner_input(
                &mut provider,
                &input,
                AudioSample(start as i64 - 1200),
                256,
                Duration::from_secs(10),
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(actual.samples, full[start..start + 256]);
    }
    // A nominal FollowSpeed reconstruction does not reproduce Preserve output.
    let mut wire = serde_json::to_value(&whole).unwrap();
    wire["nodes"]["stage"]["kind"]["pitch"] =
        serde_json::to_value(PitchPolicy::FollowSpeed).unwrap();
    let nominal = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut nominal = StageAudio::new(Arc::new(RenderPlan::compile(&nominal).unwrap()));
    assert_ne!(
        root_samples(&mut nominal, &mut provider, 1500, 256),
        full[1500..1756]
    );
}

#[test]
fn retained_ntsc_binding_and_silence_keep_the_current_output_policy() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let nodes = || {
        vec![
            ("a", source(rate, 0, 3203, 2)),
            ("b", hold(2)),
            ("group", group()),
            ("stage", preserve("group", 4, 8)),
        ]
    };
    let (old, mut provider) = document(rate, &["stage"], nodes());
    let mut moved = nodes();
    moved.push(("prefix", hold(1)));
    let (current, _) = document(rate, &["prefix", "stage"], moved);
    let bound = bind(
        &old,
        &current,
        vec![(
            "stage",
            lattice("stage", AudioClockRoot::ProjectRootRoundEven),
            None,
        )],
    );
    let plan = Arc::new(RenderPlan::compile(&bound).unwrap());
    let input = plan
        .audio_owner_input(
            &owner(&plan, "group", 1602),
            AudioSample(1602),
            Default::default(),
        )
        .unwrap();
    assert_eq!(input.scope(), AudioOwnerInputScope::EnclosingProcessor);
    let mut current = StageAudio::new(Arc::clone(&plan));
    let retained = input
        .retained_at(AudioSample(1602), Default::default())
        .unwrap()
        .unwrap();
    for start in [1602, 7900, 8100, 12_000] {
        let expected = root_samples(&mut current, &mut provider, start, 128);
        let mut cold = StageAudio::new(Arc::clone(&plan));
        let actual = cold
            .read_owner_input(
                &mut provider,
                &input,
                AudioSample(start),
                128,
                Duration::from_secs(10),
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(actual.samples, expected);
        let mut history_reader = StageAudio::new(Arc::clone(&plan));
        let history = history_reader
            .read_retained_owner_input(
                &mut provider,
                &retained,
                AudioSample(start),
                128,
                Duration::from_secs(10),
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(history.samples, expected);
        if start >= 8100 {
            assert!(actual.samples.iter().all(|sample| *sample == [0.0; 2]));
            assert!(!actual.suppressed.is_empty());
            assert!(!history.suppressed.is_empty());
        }
    }
}

#[test]
fn owner_input_rejections_happen_before_media_admission() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let (doc, mut provider) = document(rate, &["a"], vec![("a", source(rate, 0, 1024, 1024))]);
    let plan = Arc::new(RenderPlan::compile(&doc).unwrap());
    let input = plan
        .audio_owner_input(&owner(&plan, "a", 10), AudioSample(10), Default::default())
        .unwrap();
    let other = Arc::new(RenderPlan::compile(&doc).unwrap());
    let mut foreign = StageAudio::new(other);
    assert!(matches!(
        foreign.read_owner_input(
            &mut provider,
            &input,
            AudioSample(10),
            1,
            Duration::from_secs(1),
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::ForeignDomain)
    ));
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    assert!(matches!(
        renderer.read_owner_input(
            &mut provider,
            &input,
            AudioSample(10),
            1,
            Duration::from_secs(1),
            &AtomicBool::new(true)
        ),
        Err(error) if error.is_cancelled()
    ));
    for (start, frames) in [(-1, 1), (1024, 1), (0, 0), (0, 257)] {
        assert!(
            renderer
                .read_owner_input(
                    &mut provider,
                    &input,
                    AudioSample(start),
                    frames,
                    Duration::from_secs(1),
                    &AtomicBool::new(false)
                )
                .is_err()
        );
    }
    assert_eq!(provider.calls, 0);
}

#[test]
fn resumed_owner_exposes_readable_reference_history_before_current_delivery() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let (old, mut provider) = document(rate, &["a"], vec![("a", source(rate, 0, 8192, 8192))]);
    let (current, _) = document(
        rate,
        &["prefix", "a"],
        vec![("prefix", hold(512)), ("a", source(rate, 0, 8192, 8192))],
    );
    let bound = bind(
        &old,
        &current,
        vec![(
            "a",
            lattice("a", AudioClockRoot::ProjectRootRoundEven),
            Some(AudioResume {
                local_boundary: ExactRatio::ZERO,
                phase: AudioLocalPhase {
                    constant: ExactRatio::integer(2048),
                    terms: vec![],
                },
            }),
        )],
    );
    let plan = Arc::new(RenderPlan::compile(&bound).unwrap());
    let input = plan
        .audio_owner_input(
            &owner(&plan, "a", 512),
            AudioSample(512),
            Default::default(),
        )
        .unwrap();
    assert_eq!(input.context().root_samples().start, AudioSample(512));
    let retained = input
        .retained_at(AudioSample(512), Default::default())
        .unwrap()
        .unwrap();
    let deadpan_plan::AudioBoundDomain::Root(domain) = retained.domain() else {
        panic!("expected retained root grid");
    };
    assert_eq!(
        retained.reference_at(AudioSample(-512)).unwrap(),
        ExactRatio::integer(1024)
    );
    let transfer = crate::DomainSignalTransfer::new(
        domain.clone(),
        retained.reference_at(AudioSample(-512)).unwrap(),
        SignalSample(0),
        retained.reference_samples_per_output_sample(),
        SignalSample(0)..SignalSample(256),
    )
    .unwrap();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let actual = renderer
        .read_domain_transferred(
            &mut provider,
            &transfer,
            SignalSample(0),
            256,
            Duration::from_secs(10),
            &AtomicBool::new(false),
        )
        .unwrap();
    let mut original = StageAudio::new(Arc::new(RenderPlan::compile(&old).unwrap()));
    let expected = render(&mut original, &mut provider, 1024, 256);
    assert_eq!(actual.samples, expected.samples);
    assert!(actual.samples.iter().any(|sample| sample[1] != 0.0));

    // Post-time/pitch creative fades retain the same reference history, even
    // before current root zero, and endpoint exhaustion is explicitly silent.
    for reference_start in [-16_i64, 0, 32, 8064, 8170, 8192] {
        let start = reference_start - 1536;
        let actual = renderer
            .read_retained_owner_input(
                &mut provider,
                &retained,
                AudioSample(start),
                128,
                Duration::from_secs(10),
                &AtomicBool::new(false),
            )
            .unwrap();
        let mut expected = vec![[0.0; 2]; 128];
        let first = reference_start.max(0);
        let last = (reference_start + 128).min(8192);
        if first < last {
            let pcm = root_samples(&mut original, &mut provider, first, (last - first) as u32);
            let offset = (first - reference_start) as usize;
            expected[offset..offset + pcm.len()].copy_from_slice(&pcm);
        }
        assert_eq!(
            actual.samples, expected,
            "retained edge input at {reference_start}"
        );
        assert_eq!(actual.start, AudioSample(start));
        if reference_start < 0 || reference_start + 128 > 8192 {
            assert!(!actual.suppressed.is_empty());
        }
    }
}

#[test]
fn retained_point_input_fades_keep_fractional_source_phase_before_delivery() {
    let rate = FrameRate::new(32_000, 1).unwrap();
    let (old, mut provider) = document(
        rate,
        &["stage"],
        vec![
            ("prefix", hold(1)),
            ("a", source(rate, 0, 600, 400)),
            (
                "group",
                BeatNode::sequence("Group", vec![id("prefix"), id("a")]),
            ),
            ("stage", preserve("group", 401, 802)),
        ],
    );
    let (base, _) = document(rate, &["a"], vec![("a", source(rate, 0, 600, 400))]);
    let template = || {
        lattice(
            "a",
            AudioClockRoot::PreserveInputPointCeil { stage: id("stage") },
        )
    };
    let reference = bind(&old, &base, vec![("a", template(), None)]);
    let (moved, _) = document(
        rate,
        &["prefix", "a"],
        vec![("prefix", hold(5)), ("a", source(rate, 0, 600, 400))],
    );
    let moved = bind(
        &old,
        &moved,
        vec![(
            "a",
            template(),
            Some(AudioResume {
                local_boundary: ExactRatio::ZERO,
                phase: AudioLocalPhase {
                    constant: ExactRatio::integer(2),
                    terms: vec![],
                },
            }),
        )],
    );
    let plan = Arc::new(RenderPlan::compile(&moved).unwrap());
    let input = plan
        .audio_owner_input(&owner(&plan, "a", 8), AudioSample(8), Default::default())
        .unwrap();
    let retained = input
        .retained_at(AudioSample(8), Default::default())
        .unwrap()
        .unwrap();
    assert!(matches!(
        retained.domain(),
        deadpan_plan::AudioBoundDomain::Point(_)
    ));
    assert_eq!(
        retained.reference_at(AudioSample(5)).unwrap(),
        ExactRatio::integer(2)
    );
    let mut expected = StageAudio::new(Arc::new(RenderPlan::compile(&reference).unwrap()));
    for start in [0, 64, 256, 472, 536] {
        let mut cold = StageAudio::new(Arc::clone(&plan));
        let actual = cold
            .read_retained_owner_input(
                &mut provider,
                &retained,
                AudioSample(start + 5),
                64,
                Duration::from_secs(10),
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(
            actual.samples,
            root_samples(&mut expected, &mut provider, start, 64)
        );
    }
}

#[test]
fn retained_treatments_keep_reference_gain_mute_and_order_without_exterior_owners() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut voice = source(rate, 0, 8192, 8192);
    let gain = |value| GainDb::new(value).unwrap();
    let range =
        |start, end| GainRange::new(ExactRatio::integer(start), ExactRatio::integer(end)).unwrap();
    voice.audio_treatments = AudioTreatments::with_all_stages(
        vec![
            AudioTreatmentStage::Saturation,
            AudioTreatmentStage::ClipGain,
            AudioTreatmentStage::StereoBalance,
        ],
        Some(
            ClipGain::new(
                gain(6000),
                false,
                vec![
                    GainEnvelope::new(
                        GainClock::OwnerOutput,
                        range(1000, 4096),
                        gain(-3000),
                        vec![
                            GainSegment::new(
                                ExactRatio::integer(4096),
                                gain(6000),
                                GainCurve::Linear,
                            )
                            .unwrap(),
                        ],
                    )
                    .unwrap(),
                ],
                vec![range(2040, 2070)],
            )
            .unwrap(),
        ),
        Some(Saturation::new(gain(9000)).unwrap()),
        Some(StereoBalance::new(-25).unwrap()),
    )
    .unwrap();
    let (old, mut provider) = document(rate, &["a"], vec![("a", voice.clone())]);
    let (current, _) = document(
        rate,
        &["prefix", "a"],
        vec![("prefix", hold(512)), ("a", voice)],
    );
    let mut wire = serde_json::to_value(&current).unwrap();
    wire["nodes"]["root"]["audio_treatments"] = serde_json::to_value(
        AudioTreatments::from_clip_gain(ClipGain::new(gain(0), true, vec![], vec![]).unwrap()),
    )
    .unwrap();
    let current = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let bound = bind(
        &old,
        &current,
        vec![(
            "a",
            lattice("a", AudioClockRoot::ProjectRootRoundEven),
            Some(AudioResume {
                local_boundary: ExactRatio::ZERO,
                phase: AudioLocalPhase {
                    constant: ExactRatio::integer(2048),
                    terms: vec![],
                },
            }),
        )],
    );
    let plan = Arc::new(RenderPlan::compile(&bound).unwrap());
    let input = plan
        .audio_owner_input(
            &owner(&plan, "a", 512),
            AudioSample(512),
            Default::default(),
        )
        .unwrap();
    let retained = input
        .retained_at(AudioSample(512), Default::default())
        .unwrap()
        .unwrap();
    let mut expected = StageAudio::new(Arc::new(RenderPlan::compile(&old).unwrap()));
    let cancelled = AtomicBool::new(false);
    for at in [0, 32, 1984, 2040, 2060, 4096, 8096] {
        let reference = expected
            .prepare_authored_bus(
                &mut provider,
                AudioSample(at),
                96,
                Duration::from_secs(10),
                &cancelled,
            )
            .unwrap();
        let mut cold = StageAudio::new(Arc::clone(&plan));
        let actual = cold
            .read_retained_owner_treatments(
                &mut provider,
                &retained,
                AudioSample(at - 1536),
                96,
                Duration::from_secs(10),
                &cancelled,
            )
            .unwrap();
        assert_eq!(
            actual.samples, reference.samples,
            "reference owner gain at {at}"
        );
        assert_eq!(actual.stage, "retained_original_owner_treatments");
        if at == 2040 {
            assert!(
                actual.samples[..30]
                    .iter()
                    .all(|sample| *sample == [0.0; 2])
            );
            assert!(actual.samples[30..].iter().any(|sample| sample[1] != 0.0));
        }
    }
    let mut current_bus = StageAudio::new(Arc::clone(&plan));
    assert!(
        current_bus
            .prepare_authored_bus(
                &mut provider,
                AudioSample(1024),
                96,
                Duration::from_secs(10),
                &cancelled
            )
            .unwrap()
            .samples
            .iter()
            .all(|sample| *sample == [0.0; 2]),
        "the exterior root mute is intentionally absent from the retained operand"
    );
    provider.calls = 0;
    let mut small = StageAudio::with_limits(
        Arc::clone(&plan),
        StageLimits {
            maximum_resident_frames: 128,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        small.read_retained_owner_treatments(
            &mut provider,
            &retained,
            AudioSample(0),
            128,
            Duration::from_secs(10),
            &cancelled
        ),
        Err(StageAudioError::Limit(_))
    ));
    let mut foreign = StageAudio::new(Arc::new(RenderPlan::compile(&old).unwrap()));
    assert!(matches!(
        foreign.read_retained_owner_input(
            &mut provider,
            &retained,
            AudioSample(0),
            1,
            Duration::from_secs(1),
            &cancelled,
        ),
        Err(StageAudioError::ForeignDomain)
    ));
    assert!(matches!(current_bus.read_retained_owner_input(
        &mut provider, &retained, AudioSample(0), 1, Duration::from_secs(1), &AtomicBool::new(true),
    ), Err(error) if error.is_cancelled()));
    for (start, frames) in [(0, 0), (0, 257), (i64::MAX, 1)] {
        assert!(
            current_bus
                .read_retained_owner_treatments(
                    &mut provider,
                    &retained,
                    AudioSample(start),
                    frames,
                    Duration::from_secs(1),
                    &cancelled,
                )
                .is_err()
        );
    }
    assert_eq!(provider.calls, 0);
    let work = RefCell::new(ReadWork::default());
    let control = WorkControl {
        cancelled: &cancelled,
        deadline: Instant::now() + Duration::from_secs(10),
        work: &work,
    };
    assert!(matches!(
        current_bus.read_retained_controlled(
            &mut provider,
            &retained,
            AudioSample(0),
            16,
            control,
            (current_bus.limits.maximum_depth + 1, true),
        ),
        Err(StageAudioError::Limit("nested stage depth"))
    ));
    work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ;
    assert!(matches!(
        current_bus.read_retained_controlled(
            &mut provider,
            &retained,
            AudioSample(0),
            16,
            control,
            (0, true),
        ),
        Err(StageAudioError::Limit(_))
    ));
    *work.borrow_mut() = ReadWork::default();
    assert!(
        current_bus
            .read_retained_controlled(
                &mut provider,
                &retained,
                AudioSample(0),
                16,
                WorkControl {
                    deadline: Instant::now() - Duration::from_secs(1),
                    ..control
                },
                (0, true),
            )
            .is_err()
    );
    assert_eq!(provider.calls, 0);
    for _ in 0..2 {
        let muted = current_bus
            .read_retained_controlled(
                &mut provider,
                &retained,
                AudioSample(504),
                16,
                control,
                (0, true),
            )
            .unwrap();
        assert!(muted.samples.iter().all(|sample| *sample == [0.0; 2]));
        assert!(
            muted
                .dependencies
                .contains_key(&AssetId::new("media").unwrap())
        );
    }
    assert!(provider.calls > 0, "mute cannot bypass source admission");
}
