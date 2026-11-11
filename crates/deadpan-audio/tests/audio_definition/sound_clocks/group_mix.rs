//! Shared nonlinear owners after a one-frame move on a fractional sample grid.
use super::*;

const START: i64 = 4805;
const FRAMES: u32 = 6406;

#[test]
fn independent_envelope_keeps_fractional_onset_clock_through_a_root_move() {
    let base = ntsc(AudioEdgePolicy::Hard);
    let clip = ClipGain::new(
        GainDb::new(-6000).unwrap(),
        false,
        vec![
            GainEnvelope::new(
                GainClock::OwnerOutput,
                GainRange::new(ExactRatio::ZERO, ExactRatio::integer(4)).unwrap(),
                GainDb::new(-12000).unwrap(),
                vec![
                    GainSegment::new(
                        ExactRatio::integer(4),
                        GainDb::new(9000).unwrap(),
                        GainCurve::Linear,
                    )
                    .unwrap(),
                ],
            )
            .unwrap(),
        ],
        vec![],
    )
    .unwrap();
    let mut wire = serde_json::to_value(&base).unwrap();
    wire["beat_sounds"]["owner"]["effect"]["treatments"] =
        serde_json::to_value(AudioTreatments::from_clip_gain(clip.clone())).unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let raw = read(&base, 3203, 6407);
    let expected: Vec<_> = raw
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let local = ExactRatio::new((3203 + index as i128) * 5, 8008)
                .unwrap()
                .checked_sub(ExactRatio::integer(2))
                .unwrap();
            let gain = clip.evaluate(local).unwrap().millidecibels;
            let factor = 10f64.powf(gain.numerator() as f64 / gain.denominator() as f64 / 20_000.0);
            value.map(|v| (f64::from(v) * factor) as f32)
        })
        .collect();
    let actual = read(&document, 3203, 6407);
    assert!(
        actual
            .iter()
            .zip(&expected)
            .all(|(a, b)| a.iter().zip(b).all(|(a, b)| (a - b).abs() < 2e-6))
    );
    let moved = insert_prefix(&document);
    let after = read(&moved, START, FRAMES);
    assert_eq!(after, actual[..FRAMES as usize]);
    for (offset, count) in [(0, 127), (1599, 257), (3200, 256), (6000, 400)] {
        assert_eq!(
            read(&moved, START + offset, count),
            after[offset as usize..offset as usize + count as usize]
        );
    }
}

fn moved(with_original: bool) -> ProjectDocument {
    let mut wire = serde_json::to_value(ntsc(AudioEdgePolicy::Hard)).unwrap();
    if with_original {
        let original = audio(0..6407);
        wire["nodes"]["owner"]["kind"]["source"]["audio"] =
            serde_json::to_value(&original).unwrap();
        wire["nodes"]["owner"]["kind"]["source"]["audio_mapping"] = serde_json::to_value(
            SourceAudioMapping::natural_rate(original.span, FrameRate::new(30000, 1001).unwrap())
                .unwrap(),
        )
        .unwrap();
    }
    insert_prefix(&ProjectDocument::from_json(&wire.to_string()).unwrap())
}

fn treated(document: &ProjectDocument, gain: ClipGain) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"]["owner"]["audio_treatments"] = serde_json::to_value(
        AudioTreatments::from_clip_gain(gain)
            .with_saturation(Some(Saturation::new(GainDb::new(12000).unwrap()).unwrap()))
            .unwrap(),
    )
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn read(document: &ProjectDocument, start: i64, count: u32) -> Vec<[f32; 2]> {
    bus(
        &mut StageAudio::new(compile(document, false)),
        &mut ClockProvider::new(document),
        start,
        count,
    )
    .samples
}

fn ramp() -> ClipGain {
    ClipGain::new(
        GainDb::new(3000).unwrap(),
        false,
        vec![
            GainEnvelope::new(
                GainClock::OwnerOutput,
                GainRange::new(ExactRatio::ZERO, ExactRatio::integer(4)).unwrap(),
                GainDb::new(-12000).unwrap(),
                vec![
                    GainSegment::new(
                        ExactRatio::integer(4),
                        GainDb::new(6000).unwrap(),
                        GainCurve::Linear,
                    )
                    .unwrap(),
                ],
            )
            .unwrap(),
        ],
        vec![],
    )
    .unwrap()
}

#[test]
fn independent_clocks_share_one_eq_processing_context() {
    let mut wire = serde_json::to_value(post_treated(ramp())).unwrap();
    wire["nodes"]["owner"]["audio_treatments"] = serde_json::to_value(
        AudioTreatments::from_clip_gain(ramp())
            .with_equalizer(Some(
                deadpan_core::EqualizerRecipe::new(
                    [6000, -3000, 1500].map(|db| GainDb::new(db).unwrap()),
                    None,
                    None,
                )
                .unwrap(),
            ))
            .unwrap(),
    )
    .unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let expected = read(&document, START + 2700, 128);
    let mut renderer = StageAudio::with_limits(
        compile(&document, false),
        StageLimits {
            maximum_prepared_stages: 1,
            ..StageLimits::default()
        },
    )
    .unwrap();
    assert_eq!(
        bus(
            &mut renderer,
            &mut ClockProvider::new(&document),
            START + 2700,
            128
        )
        .samples,
        expected
    );
}

#[test]
fn independent_automation_is_invariant_across_owner_activation_boundaries() {
    let document = post_treated(ramp());
    let total = u32::try_from(compile(&document, false).audio_duration().unwrap().0).unwrap();
    let complete = read(&document, 0, total);
    for (start, count) in [
        (0, 1),
        (START - 257, 513),
        (START - 1, 2),
        (START, 1),
        (START + FRAMES as i64 - 257, 513),
        (START + FRAMES as i64 - 1, 2),
    ] {
        assert_eq!(
            read(&document, start, count),
            complete[start as usize..start as usize + count as usize]
        );
    }
}

#[test]
fn independent_eq_automation_keeps_rebound_input_history_across_a_new_pause() {
    use deadpan_core::{AudioTreatmentStage as S, EqualizerRecipe};
    let mut wire = serde_json::to_value(post_treated(ramp())).unwrap();
    let recipe = AudioTreatments::with_equalizer_stages(
        vec![S::Saturation, S::Equalizer, S::ClipGain],
        Some(ramp()),
        Some(Saturation::new(GainDb::new(12000).unwrap()).unwrap()),
        None,
        Some(
            EqualizerRecipe::new(
                [6000, -3000, 1500].map(|db| GainDb::new(db).unwrap()),
                None,
                None,
            )
            .unwrap(),
        ),
    )
    .unwrap();
    wire["nodes"]["owner"]["audio_treatments"] = serde_json::to_value(recipe).unwrap();
    let attached = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let sound: BeatSound =
        serde_json::from_value(wire["beat_sounds"]["owner"]["effect"].clone()).unwrap();
    wire["sounds"] = serde_json::json!({"effect": SoundEvent {
        owner: id("root"), label: sound.label, source: sound.source, mapping: sound.mapping,
        offset: AudioSample(START), treatments: deadpan_core::AudioTreatments::default().with_trim(deadpan_core::GainDb::new(sound.treatments.trim().millidecibels()).unwrap()).unwrap(),
        start_edge: sound.start_edge, end_edge: sound.end_edge, overflow: sound.overflow,
    }});
    wire["beat_sounds"] = serde_json::json!({});
    wire["audio_bindings"]["sound_clocks"] = serde_json::json!({});
    wire["nodes"]["root"]["audio_treatments"] = wire["nodes"]["owner"]["audio_treatments"].take();
    wire["nodes"]["owner"]
        .as_object_mut()
        .unwrap()
        .remove("audio_treatments");
    let source = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let at = ProjectFrame(4);
    let count = source
        .insert_time_target(at)
        .unwrap()
        .split
        .map_or(0, |split| split.required_ids);
    let NodeKind::Hold { recipe } = hold(1, HoldAudio::Silence).kind else {
        unreachable!()
    };
    let command = Command::InsertTime {
        at,
        hold: recipe,
        id: id("inner-pause"),
        identities: SplitIdentities {
            nodes: (0..count)
                .map(|n| NodeId::new(format!("inner-split-{n}")).unwrap())
                .collect(),
        },
        timing: AudioTimingId {
            allocation: RevisionId::new("held-again").unwrap(),
            ordinal: 0,
        },
    };
    let split_attachment = edit(&attached, "held-again", command.clone());
    assert_eq!(split_attachment.beat_sounds().len(), 2);
    let edited = edit(&source, "held-again", command);
    for edited in [edited, split_attachment] {
        let total = u32::try_from(compile(&edited, false).audio_duration().unwrap().0).unwrap();
        let complete = read(&edited, 0, total);
        for (start, count) in [(START - 17, 129), (6405, 129), (8007, 129), (11210, 129)] {
            let actual = read(&edited, start, count);
            for (offset, actual) in actual.iter().enumerate() {
                assert_eq!(
                    *actual,
                    complete[start as usize + offset],
                    "sample={}",
                    start + offset as i64
                );
            }
        }
    }
}

fn post_treated(gain: ClipGain) -> ProjectDocument {
    let mut wire = serde_json::to_value(treated(&moved(true), gain)).unwrap();
    wire["nodes"]["owner"]["audio_treatments"]["order"] =
        serde_json::json!(["saturation", "clip_gain"]);
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn independent_post_saturation_inputs_rejoin_the_parent_bus() {
    let child = post_treated(ramp());
    let reference = read(&child, START, FRAMES);
    let drive = Saturation::new(GainDb::new(9000).unwrap()).unwrap();
    let mut wire = serde_json::to_value(&child).unwrap();
    wire["nodes"]["root"]["audio_treatments"] = serde_json::to_value(
        AudioTreatments::from_clip_gain(
            ClipGain::new(GainDb::new(-3000).unwrap(), false, vec![], vec![]).unwrap(),
        )
        .with_saturation(Some(drive))
        .unwrap(),
    )
    .unwrap();
    let nested = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let actual = read(&nested, START, FRAMES);
    let factor = 10_f64.powf(-3.0 / 20.0);
    for (actual, reference) in actual.iter().zip(reference) {
        for ch in 0..2 {
            assert!(
                (f64::from(actual[ch]) - drive.shape(f64::from(reference[ch]) * factor)).abs()
                    <= 4.0 * f64::from(f32::EPSILON)
            );
        }
    }
    for (offset, count) in [(193, 7), (195, 61), (6011, 395), (131, 509)] {
        assert_eq!(
            read(&nested, START + offset, count),
            actual[offset as usize..offset as usize + count as usize]
        );
    }
}

#[test]
fn contribution_pcm_is_admitted_before_media_and_cancellation_recovers() {
    let treated = post_treated(ramp());
    let plan = compile(&treated, false);
    let mut provider = ClockProvider::new(&treated);
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
            AudioSample(START + 193),
            64,
            TIMEOUT,
            &AtomicBool::new(false),
        ),
        Err(StageAudioError::Limit(_))
    ));
    assert_eq!(provider.fixture.calls, 0);
    assert_eq!(provider.fixture.context_calls, 0);
    let mut renderer = StageAudio::new(plan);
    assert!(
        renderer
            .prepare_authored_bus(
                &mut provider,
                AudioSample(START + 193),
                64,
                TIMEOUT,
                &AtomicBool::new(true),
            )
            .is_err()
    );
    assert_eq!(provider.fixture.calls, 0);
    let expected = read(&treated, START + 193, 64);
    assert_eq!(
        bus(&mut renderer, &mut provider, START + 193, 64).samples,
        expected
    );
    provider.fixture.unavailable = true;
    assert!(
        renderer
            .prepare_authored_bus(
                &mut provider,
                AudioSample(START + 193),
                64,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .is_err()
    );
    provider.fixture.unavailable = false;
    assert_eq!(
        bus(&mut renderer, &mut provider, START + 193, 64).samples,
        expected
    );
}

#[test]
fn nonlinear_owner_mixes_ntsc_sound_after_retained_clock_move() {
    let sound = read(&moved(false), START, FRAMES);
    let drive = Saturation::new(GainDb::new(12000).unwrap()).unwrap();
    for with_original in [false, true] {
        let moved = moved(with_original);
        let mut renderer = StageAudio::new(compile(&moved, false));
        let mut provider = ClockProvider::new(&moved);
        let original: Vec<_> = (0..FRAMES)
            .step_by(256)
            .flat_map(|offset| {
                renderer
                    .read_edge_faded(
                        &mut provider,
                        AudioSample(START + i64::from(offset)),
                        256.min(FRAMES - offset),
                        TIMEOUT,
                        &AtomicBool::new(false),
                    )
                    .unwrap()
                    .samples
            })
            .collect();
        if with_original {
            assert!(original.iter().flatten().any(|sample| sample.abs() > 0.01));
        }
        let expected: Vec<_> = original
            .iter()
            .zip(&sound)
            .map(|(original, sound)| {
                std::array::from_fn(|channel| {
                    drive.shape(f64::from(original[channel]) + f64::from(sound[channel])) as f32
                })
            })
            .collect();
        let treated = treated(
            &moved,
            ClipGain::new(GainDb::UNITY, false, vec![], vec![]).unwrap(),
        );
        assert_eq!(read(&treated, START, FRAMES), expected);
        for (offset, count) in [(3, 37), (195, 61), (6011, 395)] {
            assert_eq!(
                read(&treated, START + offset, count),
                expected[offset as usize..offset as usize + count as usize]
            );
        }
    }
}

#[test]
fn absent_original_does_not_supply_a_nonlinear_automation_clock() {
    let moved = moved(false);
    let sound = read(&moved, START, FRAMES);
    let gain = ClipGain::new(
        GainDb::UNITY,
        false,
        vec![
            GainEnvelope::new(
                GainClock::OwnerOutput,
                GainRange::new(ExactRatio::ZERO, ExactRatio::integer(4)).unwrap(),
                GainDb::new(-12000).unwrap(),
                vec![
                    GainSegment::new(ExactRatio::integer(4), GainDb::UNITY, GainCurve::Linear)
                        .unwrap(),
                ],
            )
            .unwrap(),
        ],
        vec![],
    )
    .unwrap();
    let treated = treated(&moved, gain.clone());
    let drive = Saturation::new(GainDb::new(12000).unwrap()).unwrap();
    let expected: Vec<_> = sound
        .iter()
        .enumerate()
        .map(|(offset, sample)| {
            // Owner starts at project frame 3, on the current 30000/1001 clock.
            let local = ratio(i128::from(START + offset as i64) * 5 - 24024, 8008);
            let db = gain.evaluate(local).unwrap().millidecibels;
            let factor = 10.0_f64.powf(db.numerator() as f64 / db.denominator() as f64 / 20000.0);
            sample.map(|value| drive.shape(f64::from(value) * factor) as f32)
        })
        .collect();
    assert_eq!(read(&treated, START, FRAMES), expected);
    assert_eq!(read(&treated, START + 131, 509), expected[131..640]);
}

#[test]
fn muted_independent_clocks_keep_dependency_admission() {
    let treated = treated(
        &moved(true),
        ClipGain::new(
            GainDb::UNITY,
            true,
            vec![
                GainEnvelope::new(
                    GainClock::OwnerOutput,
                    GainRange::new(ExactRatio::ZERO, ExactRatio::integer(4)).unwrap(),
                    GainDb::new(-12000).unwrap(),
                    vec![
                        GainSegment::new(ExactRatio::integer(4), GainDb::UNITY, GainCurve::Linear)
                            .unwrap(),
                    ],
                )
                .unwrap(),
            ],
            vec![],
        )
        .unwrap(),
    );
    let mut provider = ClockProvider::new(&treated);
    let result = bus(
        &mut StageAudio::new(compile(&treated, false)),
        &mut provider,
        START,
        256,
    );
    assert_eq!(result.samples, vec![[0.0; 2]; 256]);
    assert!(provider.fixture.calls > 0);
    assert!(provider.fixture.context_calls > 0);
}
