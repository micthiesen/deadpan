//! Shared nonlinear owners after a one-frame move on a fractional sample grid.
use super::*;

const START: i64 = 4805;
const FRAMES: u32 = 6406;

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

fn original(document: &ProjectDocument, start: i64, count: u32) -> Vec<[f32; 2]> {
    let mut renderer = StageAudio::new(compile(document, false));
    let mut provider = ClockProvider::new(document);
    (0..count)
        .step_by(256)
        .flat_map(|offset| {
            renderer
                .read_edge_faded(
                    &mut provider,
                    AudioSample(start + i64::from(offset)),
                    256.min(count - offset),
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples
        })
        .collect()
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

fn amplitude(gain: &ClipGain, clock: ExactRatio) -> f64 {
    let value = gain.evaluate(clock).unwrap();
    if value.muted {
        0.0
    } else {
        10.0_f64.powf(
            value.millidecibels.numerator() as f64
                / value.millidecibels.denominator() as f64
                / 20000.0,
        )
    }
}

#[test]
fn shared_eq_independent_input_automation_keeps_gain_filter_and_saturation_order() {
    use deadpan_core::{AudioTreatmentStage as S, EqualizerRecipe};
    let source = moved(true);
    let gain = ramp();
    let eq = EqualizerRecipe::new(
        [6000, -3000, 1500].map(|db| GainDb::new(db).unwrap()),
        None,
        None,
    )
    .unwrap();
    let kernel = deadpan_audio::equalizer_coefficients(eq, &AtomicBool::new(false)).unwrap();
    let start = START + 2700;
    let count = 128u32;
    let halo_start = start - 2048;
    let halo_count = count + 4096;
    let original = original(&source, halo_start, halo_count);
    let sound = read(&moved(false), halo_start, halo_count);
    let input: Vec<[f64; 2]> = original
        .iter()
        .zip(&sound)
        .enumerate()
        .map(|(offset, (original, sound))| {
            let sample = i128::from(halo_start + offset as i64);
            let a = amplitude(&gain, ratio(sample * 5 - 24026, 8008));
            let b = amplitude(&gain, ratio(sample * 5 - 24024, 8008));
            std::array::from_fn(|ch| f64::from(original[ch]) * a + f64::from(sound[ch]) * b)
        })
        .collect();
    let drive = Saturation::new(GainDb::new(12000).unwrap()).unwrap();
    for saturation_first in [false, true] {
        let treatments = AudioTreatments::from_clip_gain(gain.clone())
            .with_equalizer(Some(eq))
            .unwrap()
            .with_saturation(Some(drive))
            .unwrap();
        let mut wire = serde_json::to_value(&source).unwrap();
        wire["nodes"]["owner"]["audio_treatments"] = serde_json::to_value(treatments).unwrap();
        if saturation_first {
            wire["nodes"]["owner"]["audio_treatments"]["order"] =
                serde_json::to_value([S::ClipGain, S::Saturation, S::Equalizer]).unwrap();
        }
        let treated = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let expected: Vec<[f32; 2]> = (0..count as usize)
            .map(|offset| {
                std::array::from_fn(|ch| {
                    let filtered: f64 = kernel
                        .iter()
                        .enumerate()
                        .map(|(k, weight)| {
                            let input = input[offset + k][ch];
                            weight
                                * if saturation_first {
                                    drive.shape(input)
                                } else {
                                    input
                                }
                        })
                        .sum();
                    (if saturation_first {
                        filtered
                    } else {
                        drive.shape(filtered)
                    }) as f32
                })
            })
            .collect();
        let actual = read(&treated, start, count);
        let error = actual
            .iter()
            .zip(&expected)
            .flat_map(|(a, b)| (0..2).map(move |ch| (a[ch] - b[ch]).abs()))
            .fold(0.0_f32, f32::max);
        // The oracle's raw inspection samples are f32; the temporal graph
        // retains f64 intermediates. This covers four f32 ulps at unity.
        assert!(
            error <= 4.0 * f32::EPSILON,
            "saturation_first={saturation_first}, error={error}"
        );
        assert!(actual.iter().flatten().any(|sample| sample.abs() > 0.01));
        for (offset, frames) in [(73, 55), (0, 37), (37, 36)] {
            assert_eq!(
                read(&treated, start + offset, frames),
                actual[offset as usize..offset as usize + frames as usize]
            );
        }
    }
}

#[test]
fn divergent_post_saturation_automation_still_refuses_before_source_access() {
    let gain = ClipGain::new(
        GainDb::UNITY,
        false,
        vec![],
        vec![GainRange::new(ratio(975, 8008), ratio(977, 8008)).unwrap()],
    )
    .unwrap();
    let mut wire = serde_json::to_value(treated(&moved(true), gain)).unwrap();
    wire["nodes"]["owner"]["audio_treatments"]["order"] =
        serde_json::json!(["saturation", "clip_gain"]);
    let treated = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut provider = ClockProvider::new(&treated);
    let error = StageAudio::new(compile(&treated, false))
        .prepare_authored_bus(
            &mut provider,
            AudioSample(START),
            256,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap_err();
    assert!(
        matches!(
            error,
            StageAudioError::Unsupported(
                "different automation on independently timed inputs to one nonlinear owner"
            )
        ),
        "{error:?}"
    );
    assert_eq!(provider.fixture.calls, 0);
    assert_eq!(provider.fixture.context_calls, 0);
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
fn independent_pre_saturation_mute_preserves_each_input_clock() {
    // The two clocks agree outside this tiny mute range, including both ends
    // of the read. At output sample 5000 only the current sound clock is muted.
    let gain = ClipGain::new(
        GainDb::UNITY,
        false,
        vec![],
        vec![GainRange::new(ratio(975, 8008), ratio(977, 8008)).unwrap()],
    )
    .unwrap();
    compare_independent_prefix(gain);
}

fn compare_independent_prefix(gain: ClipGain) {
    let sound = read(&moved(false), START, FRAMES);
    let moved = moved(true);
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
    let drive = Saturation::new(GainDb::new(12000).unwrap()).unwrap();
    let factor = |clock| {
        let value = gain.evaluate(clock).unwrap();
        if value.muted {
            0.0
        } else {
            10.0_f64.powf(
                value.millidecibels.numerator() as f64
                    / value.millidecibels.denominator() as f64
                    / 20000.0,
            )
        }
    };
    let expected: Vec<_> = original
        .iter()
        .zip(&sound)
        .enumerate()
        .map(|(offset, (original, sound))| {
            let sample = i128::from(START + offset as i64);
            // Inserting one NTSC frame translates Original's rounded origin by
            // 1602 samples. The sound's owner stays on the exact current grid.
            let original_gain = factor(ratio(sample * 5 - 24026, 8008));
            let sound_gain = factor(ratio(sample * 5 - 24024, 8008));
            std::array::from_fn(|ch| {
                drive.shape(
                    f64::from(original[ch]) * original_gain + f64::from(sound[ch]) * sound_gain,
                ) as f32
            })
        })
        .collect();
    let treated = treated(&moved, gain);
    assert_eq!(read(&treated, START, FRAMES), expected);
    for (offset, count) in [(193, 7), (195, 61), (6011, 395), (131, 509)] {
        assert_eq!(
            read(&treated, START + offset, count),
            expected[offset as usize..offset as usize + count as usize]
        );
    }
}

#[test]
fn independent_pre_saturation_envelope_preserves_each_input_clock() {
    compare_independent_prefix(
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
        .unwrap(),
    );
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
