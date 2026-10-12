use super::*;

fn recipe() -> EqualizerRecipe {
    EqualizerRecipe::new(
        [6000, -6000, 3000].map(|value| GainDb::new(value).unwrap()),
        None,
        Some(EqCutoffHz::new(8000).unwrap()),
    )
    .unwrap()
}

fn prepared<'a>(
    plan: &'a RenderPlan,
    name: &str,
    at: i64,
    boundary: usize,
) -> PreparedOwnerEqualizer<'a> {
    let input = plan
        .audio_owner_input(&owner(plan, name, at), AudioSample(at), Default::default())
        .unwrap();
    PreparedOwnerEqualizer::new(input, boundary, recipe(), &AtomicBool::new(false)).unwrap()
}

fn equalized(
    renderer: &mut StageAudio,
    provider: &mut Provider,
    eq: &mut PreparedOwnerEqualizer<'_>,
    start: i64,
    count: u32,
) -> OwnerEqualizedBlock {
    renderer
        .read_owner_equalized(
            provider,
            eq,
            AudioSample(start),
            count,
            Duration::from_secs(10),
            &AtomicBool::new(false),
        )
        .unwrap()
}

fn close(a: &[[f64; 2]], b: &[[f64; 2]]) {
    assert_eq!(a.len(), b.len());
    let worst = a
        .iter()
        .zip(b)
        .flat_map(|(a, b)| a.iter().zip(b).map(|(a, b)| (a - b).abs()))
        .fold(0.0_f64, f64::max);
    assert!(worst < 2e-11, "PCM error {worst}");
}

#[test]
fn ordered_eq_matches_direct_convolution_and_mute_stays_on_its_authored_side() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut source = source(rate, 0, 8192, 8192);
    source.audio_treatments = AudioTreatments::with_all_stages(
        vec![
            AudioTreatmentStage::ClipGain,
            AudioTreatmentStage::Saturation,
            AudioTreatmentStage::StereoBalance,
        ],
        Some(
            ClipGain::new(
                GainDb::new(6000).unwrap(),
                false,
                vec![],
                vec![GainRange::new(ExactRatio::integer(2048), ExactRatio::integer(2080)).unwrap()],
            )
            .unwrap(),
        ),
        Some(Saturation::new(GainDb::new(9000).unwrap()).unwrap()),
        Some(StereoBalance::new(-25).unwrap()),
    )
    .unwrap();
    let (doc, mut provider) = document(rate, &["a"], vec![("a", source)]);
    let plan = Arc::new(RenderPlan::compile(&doc).unwrap());
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let raw = root_samples(&mut renderer, &mut provider, 0, 8192);
    let kernel = crate::equalizer_coefficients(recipe(), &AtomicBool::new(false)).unwrap();
    let factor = 10.0_f64.powf(6000.0 / 20_000.0);
    for boundary in [0, 1, 2, 3] {
        let mut eq = prepared(&plan, "a", 2048, boundary);
        let actual = equalized(&mut renderer, &mut provider, &mut eq, 2016, 128);
        let mut expected = Vec::new();
        for at in 2016..2144 {
            let mut value = [0.0; 2];
            for (tap, coefficient) in kernel.iter().enumerate() {
                let sample = at as i64 + tap as i64 - 2048;
                if !(0..8192).contains(&sample) {
                    continue;
                }
                let mut input = raw[sample as usize].map(f64::from);
                if boundary >= 1 {
                    input = if (2048..2080).contains(&sample) {
                        [0.0; 2]
                    } else {
                        input.map(|x| x * factor)
                    };
                }
                if boundary >= 2 {
                    input = input.map(|x| {
                        Saturation::new(GainDb::new(9000).unwrap())
                            .unwrap()
                            .shape(x)
                    });
                }
                if boundary >= 3 {
                    input[1] *= 0.75;
                }
                for channel in 0..2 {
                    value[channel] += input[channel] * coefficient;
                }
            }
            if boundary == 0 {
                value = if (2048..2080).contains(&at) {
                    [0.0; 2]
                } else {
                    value.map(|x| x * factor)
                };
            }
            if boundary < 2 {
                value = value.map(|x| {
                    Saturation::new(GainDb::new(9000).unwrap())
                        .unwrap()
                        .shape(x)
                });
            }
            if boundary < 3 {
                value[1] *= 0.75;
            }
            expected.push(value);
        }
        close(&actual.samples, &expected);
        assert!(
            actual.suppressed.is_empty(),
            "authored mute is not silence policy"
        );
        if boundary == 0 {
            assert!(actual.samples[32..64].iter().all(|x| *x == [0.0; 2]));
        } else {
            assert!(
                actual.samples[32..64].iter().any(|x| x[0].abs() > 1e-8),
                "pre-filter mute must retain the filter's surrounding history"
            );
        }
    }
}

#[test]
fn ordered_eq_cold_cropped_group_keeps_hidden_history_and_is_partition_invariant() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let nodes = || {
        let mut a = source(rate, 0, 4096, 4096);
        a.audio_treatments = AudioTreatments::default()
            .with_saturation(Some(Saturation::new(GainDb::new(12000).unwrap()).unwrap()))
            .unwrap();
        let mut group = group();
        group.audio_treatments = AudioTreatments::from_clip_gain(
            ClipGain::new(GainDb::new(-3000).unwrap(), false, vec![], vec![]).unwrap(),
        );
        vec![
            ("a", a),
            ("b", source(rate, 4096, 8192, 4096)),
            ("group", group),
        ]
    };
    let (whole, mut provider) = document(rate, &["group"], nodes());
    let mut cropped = nodes();
    cropped.push(("crop", crop("group", 1501..6501)));
    let (cropped, _) = document(rate, &["crop"], cropped);
    let whole = Arc::new(RenderPlan::compile(&whole).unwrap());
    let cropped = Arc::new(RenderPlan::compile(&cropped).unwrap());
    let mut reference = StageAudio::new(Arc::clone(&whole));
    let mut reference_eq = prepared(&whole, "group", 2048, 1);
    for at in [4090, 0, 1020, 2400, -1000, 5000] {
        let mut cold = StageAudio::new(Arc::clone(&cropped));
        let mut eq = prepared(&cropped, "group", 0, 1);
        let actual = equalized(&mut cold, &mut provider, &mut eq, at, 128);
        let expected = equalized(
            &mut reference,
            &mut provider,
            &mut reference_eq,
            at + 1501,
            128,
        );
        close(&actual.samples, &expected.samples);
        let one = equalized(&mut cold, &mut provider, &mut eq, at + 63, 1);
        assert_eq!(one.samples[0], actual.samples[63]);
    }
}

#[test]
fn ordered_eq_resumed_history_keeps_reference_mute_and_excludes_exterior_gain() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut voice = source(rate, 0, 8192, 8192);
    voice.audio_treatments = AudioTreatments::from_clip_gain(
        ClipGain::new(
            GainDb::new(6000).unwrap(),
            false,
            vec![],
            vec![GainRange::new(ExactRatio::integer(2040), ExactRatio::integer(2070)).unwrap()],
        )
        .unwrap(),
    );
    let (old, mut provider) = document(rate, &["a"], vec![("a", voice.clone())]);
    let (current, _) = document(
        rate,
        &["prefix", "a"],
        vec![("prefix", hold(512)), ("a", voice)],
    );
    let mut wire = serde_json::to_value(&current).unwrap();
    wire["nodes"]["root"]["audio_treatments"] =
        serde_json::to_value(AudioTreatments::from_clip_gain(
            ClipGain::new(GainDb::new(0).unwrap(), true, vec![], vec![]).unwrap(),
        ))
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
    let old = Arc::new(RenderPlan::compile(&old).unwrap());
    let plan = Arc::new(RenderPlan::compile(&bound).unwrap());
    let mut reference = StageAudio::new(Arc::clone(&old));
    for before in [0, 1] {
        let mut reference_eq = prepared(&old, "a", 2048, before);
        for at in [-1400, -800, 504, 1020, 5000, 6700] {
            let mut renderer = StageAudio::new(Arc::clone(&plan));
            let mut eq = prepared(&plan, "a", 512, before);
            let actual = equalized(&mut renderer, &mut provider, &mut eq, at, 96);
            let expected = equalized(
                &mut reference,
                &mut provider,
                &mut reference_eq,
                at + 1536,
                96,
            );
            close(&actual.samples, &expected.samples);
            if at == 504 && before == 1 {
                assert!(actual.samples.iter().any(|sample| sample[0] != 0.0));
            }
        }
    }
}

#[test]
fn ordered_eq_cannot_fill_silent_hold_or_extend_owner_support() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let (doc, mut provider) = document(
        rate,
        &["group"],
        vec![
            ("a", source(rate, 0, 4096, 4096)),
            ("b", hold(4096)),
            ("group", group()),
        ],
    );
    let plan = Arc::new(RenderPlan::compile(&doc).unwrap());
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let mut eq = prepared(&plan, "group", 2000, 0);
    let crossing = equalized(&mut renderer, &mut provider, &mut eq, 4064, 96);
    assert!(crossing.samples[..32].iter().any(|x| *x != [0.0; 2]));
    assert!(crossing.samples[32..].iter().all(|x| *x == [0.0; 2]));
    assert_eq!(
        crossing.suppressed,
        vec![AudioSample(4096)..AudioSample(4160)]
    );
    for at in [-100, 8192] {
        let silent = equalized(&mut renderer, &mut provider, &mut eq, at, 64);
        assert!(silent.samples.iter().all(|x| *x == [0.0; 2]));
        assert_eq!(
            silent.suppressed,
            vec![AudioSample(at)..AudioSample(at + 64)]
        );
    }
}

#[test]
fn ordered_eq_rejects_invalid_admission_and_releases_failed_reservations() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let (doc, mut provider) = document(rate, &["a"], vec![("a", source(rate, 0, 8192, 8192))]);
    let plan = Arc::new(RenderPlan::compile(&doc).unwrap());
    let input = plan
        .audio_owner_input(&owner(&plan, "a", 0), AudioSample(0), Default::default())
        .unwrap();
    assert!(PreparedOwnerEqualizer::new(input, 1, recipe(), &AtomicBool::new(false)).is_err());
    let mut eq = prepared(&plan, "a", 0, 0);
    let mut foreign = StageAudio::new(Arc::new(RenderPlan::compile(&doc).unwrap()));
    assert!(matches!(
        foreign.read_owner_equalized(
            &mut provider,
            &mut eq,
            AudioSample(0),
            1,
            Duration::from_secs(1),
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::ForeignDomain)
    ));
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    for (at, frames) in [(0, 0), (0, 257), (i64::MAX, 1), (i64::MIN, 1)] {
        assert!(
            renderer
                .read_owner_equalized(
                    &mut provider,
                    &mut eq,
                    AudioSample(at),
                    frames,
                    Duration::from_secs(1),
                    &AtomicBool::new(false)
                )
                .is_err()
        );
        assert_eq!(renderer.active_frames, 0);
    }
    assert!(
        matches!(renderer.read_owner_equalized(&mut provider, &mut eq, AudioSample(0), 1, Duration::from_secs(1), &AtomicBool::new(true)), Err(error) if error.is_cancelled())
    );
    for limits in [
        StageLimits {
            maximum_resident_frames: 1024,
            ..Default::default()
        },
        StageLimits {
            maximum_input_frames: 5119,
            ..Default::default()
        },
        StageLimits {
            maximum_prepared_frames: 1000,
            ..Default::default()
        },
    ] {
        let mut small = StageAudio::with_limits(Arc::clone(&plan), limits).unwrap();
        assert!(matches!(
            small.read_owner_equalized(
                &mut provider,
                &mut eq,
                AudioSample(0),
                1,
                Duration::from_secs(1),
                &AtomicBool::new(false)
            ),
            Err(StageAudioError::Limit(_))
        ));
        assert_eq!(small.active_frames, 0);
    }
    assert_eq!(provider.calls, 0);
    let valid = equalized(&mut renderer, &mut provider, &mut eq, 2000, 64);
    assert!(valid.samples.iter().any(|x| *x != [0.0; 2]));
    assert_eq!(renderer.active_frames, 0);
}

#[test]
fn ordered_eq_uses_preserve_output_before_filtering_hidden_group_history() {
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
    let mut cropped = nodes();
    cropped.push(("crop", crop("stage", 1200..2400)));
    let (cropped, _) = document(rate, &["crop"], cropped);
    let plan = Arc::new(RenderPlan::compile(&cropped).unwrap());
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let mut eq = prepared(&plan, "group", 200, 0);
    let mut reference = StageAudio::new(Arc::new(RenderPlan::compile(&whole).unwrap()));
    let raw = root_samples(&mut reference, &mut provider, 0, 3600);
    let kernel = crate::equalizer_coefficients(recipe(), &AtomicBool::new(false)).unwrap();
    for start in [-500, 0, 1100, 2300] {
        let actual = equalized(&mut renderer, &mut provider, &mut eq, start, 96);
        let expected: Vec<_> = (start + 1200..start + 1296)
            .map(|at| {
                let mut result = [0.0; 2];
                for (tap, weight) in kernel.iter().enumerate() {
                    let pos = at + tap as i64 - 2048;
                    if (0..3600).contains(&pos) {
                        for ch in 0..2 {
                            result[ch] += f64::from(raw[pos as usize][ch]) * weight;
                        }
                    }
                }
                result
            })
            .collect();
        close(&actual.samples, &expected);
    }
}

#[test]
fn ordered_eq_retained_point_clock_keeps_fractional_source_phase() {
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
    let reference = Arc::new(RenderPlan::compile(&reference).unwrap());
    let plan = Arc::new(RenderPlan::compile(&moved).unwrap());
    let mut reference_renderer = StageAudio::new(Arc::clone(&reference));
    let mut reference_eq = prepared(&reference, "a", 8, 0);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let mut eq = prepared(&plan, "a", 8, 0);
    for at in [-64, 0, 256, 536, 620] {
        let actual = equalized(&mut renderer, &mut provider, &mut eq, at + 5, 64);
        let expected = equalized(
            &mut reference_renderer,
            &mut provider,
            &mut reference_eq,
            at,
            64,
        );
        close(&actual.samples, &expected.samples);
        assert_eq!(
            actual.suppressed,
            expected
                .suppressed
                .iter()
                .map(|range| AudioSample(range.start.0 + 5)..AudioSample(range.end.0 + 5))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn ordered_eq_repeat_escalation_feeds_filter_once_across_play_boundaries() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut repeat = hold(1);
    repeat.kind = NodeKind::Repeat {
        child: id("a"),
        iterations: IterationOrder::new(revision("plays"), 3).unwrap(),
        gap: None,
        escalation: Some(RepeatEscalation {
            gain_step: GainDb::new(3000).unwrap(),
            zoom: None,
        }),
    };
    repeat.audio_treatments = AudioTreatments::default()
        .with_saturation(Some(Saturation::new(GainDb::new(6000).unwrap()).unwrap()))
        .unwrap();
    let (doc, mut provider) = document(
        rate,
        &["repeat"],
        vec![("a", source(rate, 0, 2048, 2048)), ("repeat", repeat)],
    );
    let plan = Arc::new(RenderPlan::compile(&doc).unwrap());
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let raw = root_samples(&mut renderer, &mut provider, 0, 6144);
    let kernel = crate::equalizer_coefficients(recipe(), &AtomicBool::new(false)).unwrap();
    for before in [0, 1] {
        let mut eq = prepared(&plan, "repeat", 3000, before);
        let actual = equalized(&mut renderer, &mut provider, &mut eq, 4000, 192);
        let expected: Vec<_> = (4000..4192)
            .map(|at| {
                let mut result = [0.0; 2];
                for (tap, weight) in kernel.iter().enumerate() {
                    let pos = at as i64 + tap as i64 - 2048;
                    if !(0..6144).contains(&pos) {
                        continue;
                    }
                    let factor = 10.0_f64.powf((pos / 2048) as f64 * 3000.0 / 20_000.0);
                    let mut input = raw[pos as usize].map(|value| f64::from(value) * factor);
                    if before == 1 {
                        input = input.map(|value| {
                            Saturation::new(GainDb::new(6000).unwrap())
                                .unwrap()
                                .shape(value)
                        });
                    }
                    for channel in 0..2 {
                        result[channel] += input[channel] * weight;
                    }
                }
                if before == 0 {
                    result = result.map(|value| {
                        Saturation::new(GainDb::new(6000).unwrap())
                            .unwrap()
                            .shape(value)
                    });
                }
                result
            })
            .collect();
        close(&actual.samples, &expected);
        assert_eq!(actual.instance.node, id("repeat"));
        assert!(actual.gap_after.is_none());
    }
}
