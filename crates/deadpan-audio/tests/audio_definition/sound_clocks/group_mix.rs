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
fn divergent_interior_automation_rejects_before_source_access() {
    // The two clocks agree outside this tiny mute range, including both ends
    // of the read. At output sample 5000 only the current sound clock is muted.
    let gain = ClipGain::new(
        GainDb::UNITY,
        false,
        vec![],
        vec![GainRange::new(ratio(975, 8008), ratio(977, 8008)).unwrap()],
    )
    .unwrap();
    let treated = treated(&moved(true), gain);
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
