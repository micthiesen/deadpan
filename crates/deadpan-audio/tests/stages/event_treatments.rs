//! Real decoded source through complete independent-voice recipes. The scalar
//! oracle prepares the whole event before applying any delivery cut.
use super::event_returns::{fixture, read};
use super::*;

fn treated(document: &ProjectDocument, beat: bool, recipe: &AudioTreatments) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    let event = if beat {
        &mut wire["beat_sounds"]["owner"]["effect"]
    } else {
        &mut wire["sounds"]["effect"]
    };
    event["treatments"] = serde_json::to_value(recipe).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn recipe(reordered: bool) -> AudioTreatments {
    let range = GainRange::new(ExactRatio::ZERO, ExactRatio::integer(40_000)).unwrap();
    let gain = ClipGain::new(
        GainDb::new(-6000).unwrap(),
        false,
        vec![
            GainEnvelope::new(
                GainClock::OwnerOutput,
                range,
                GainDb::new(-12_000).unwrap(),
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
        vec![GainRange::new(ExactRatio::integer(10_000), ExactRatio::integer(12_000)).unwrap()],
    )
    .unwrap();
    let mut order = vec![
        AudioTreatmentStage::ClipGain,
        AudioTreatmentStage::Equalizer,
        AudioTreatmentStage::Saturation,
        AudioTreatmentStage::StereoBalance,
    ];
    if reordered {
        order.swap(0, 2);
    }
    AudioTreatments::with_equalizer_stages(
        order,
        Some(gain),
        Some(Saturation::new(GainDb::new(9000).unwrap()).unwrap()),
        Some(StereoBalance::new(-35).unwrap()),
        Some(
            EqualizerRecipe::new(
                [
                    GainDb::new(3000).unwrap(),
                    GainDb::new(-6000).unwrap(),
                    GainDb::new(1500).unwrap(),
                ],
                Some(EqCutoffHz::new(100).unwrap()),
                None,
            )
            .unwrap(),
        ),
    )
    .unwrap()
}

fn oracle(input: &[[f32; 2]], recipe: &AudioTreatments) -> Vec<[f32; 2]> {
    let mut signal: Vec<_> = input.iter().map(|sample| sample.map(f64::from)).collect();
    for stage in recipe.order() {
        if *stage == AudioTreatmentStage::Equalizer {
            // Direct convolution over a complete independently assembled input.
            let kernel = deadpan_audio::equalizer_coefficients(
                recipe.equalizer().unwrap(),
                &AtomicBool::new(false),
            )
            .unwrap();
            let mut output = vec![[0.0; 2]; signal.len()];
            for (at, sample) in output.iter_mut().enumerate() {
                for (offset, coefficient) in kernel.iter().enumerate() {
                    let index = at as i64 + offset as i64 - 2048;
                    if let Some(value) = usize::try_from(index).ok().and_then(|i| signal.get(i)) {
                        for channel in 0..2 {
                            sample[channel] += coefficient * value[channel];
                        }
                    }
                }
            }
            signal = output;
        } else {
            for (at, sample) in signal.iter_mut().enumerate() {
                match stage {
                    AudioTreatmentStage::ClipGain => {
                        let gain = recipe
                            .evaluate(ExactRatio::integer(at as i64 - 37))
                            .unwrap();
                        let db = gain.millidecibels.numerator() as f64
                            / gain.millidecibels.denominator() as f64;
                        let scale = if gain.muted {
                            0.0
                        } else {
                            10f64.powf(db / 20_000.0)
                        };
                        *sample = sample.map(|value| value * scale);
                    }
                    AudioTreatmentStage::Saturation => {
                        *sample = sample.map(|value| recipe.saturation().unwrap().shape(value))
                    }
                    AudioTreatmentStage::StereoBalance => sample[1] *= 0.65,
                    AudioTreatmentStage::Equalizer => unreachable!(),
                }
            }
        }
        // The independent voice never gains audible time through a treatment.
        signal[..37].fill([0.0; 2]);
        signal[43_574..].fill([0.0; 2]);
    }
    signal
        .into_iter()
        .map(|sample| sample.map(|v| v as f32))
        .collect()
}

#[test]
fn root_and_attached_treatments_match_scalar_order_and_cold_queries() {
    for beat in [false, true] {
        let base = fixture(beat, None, false);
        let raw = read(
            &treated(&base, beat, &AudioTreatments::default()),
            0,
            48_000,
        );
        for reordered in [false, true] {
            let recipe = recipe(reordered);
            let document = treated(&base, beat, &recipe);
            let expected = oracle(&raw, &recipe);
            let actual = read(&document, 0, 48_000);
            assert_pcm_close(&actual, &expected);
            assert!(
                actual
                    .iter()
                    .zip(&raw)
                    .any(|(a, b)| (a[0] - b[0]).abs() > 0.01)
            );
            for (start, count) in [
                (1, 113),
                (10020, 217),
                (11700, 601),
                (32767, 257),
                (43500, 300),
            ] {
                assert_pcm_close(
                    &read(&document, start, count),
                    &actual[start as usize..start as usize + count as usize],
                );
            }
        }
    }
}

#[test]
fn event_fade_precedes_saturation_once_and_balance_keeps_other_channel() {
    for beat in [false, true] {
        let mut wire = serde_json::to_value(fixture(beat, None, false)).unwrap();
        let event = if beat {
            &mut wire["beat_sounds"]["owner"]["effect"]
        } else {
            &mut wire["sounds"]["effect"]
        };
        event["start_edge"] = serde_json::json!("automatic");
        event["end_edge"] = serde_json::json!("automatic");
        let base = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let raw = read(&treated(&base, beat, &AudioTreatments::default()), 0, 256);
        let saturation = Saturation::new(GainDb::new(18_000).unwrap()).unwrap();
        let recipe = AudioTreatments::default()
            .with_saturation(Some(saturation))
            .unwrap()
            .with_balance(Some(StereoBalance::new(-100).unwrap()))
            .unwrap();
        let expected: Vec<_> = raw
            .iter()
            .map(|sample| [saturation.shape(f64::from(sample[0])) as f32, 0.0])
            .collect();
        assert_pcm_close(&read(&treated(&base, beat, &recipe), 0, 256), &expected);
    }
}

#[test]
fn processed_event_history_and_envelope_progress_survive_insert_and_cut() {
    let before = treated(
        &fixture(false, None, false),
        false,
        &recipe(false).with_wet_sends(
            TailSends::default().with_level(TailEffect::Delay, Some(GainDb::new(-6000).unwrap())),
        ),
    );
    let original = read(&before, 0, 48_000);
    let mut wire = serde_json::to_value(&before).unwrap();
    wire["nodes"]["picture"]["kind"]["source"]["duration"] = serde_json::json!(49_000);
    wire["sound_routes"] = serde_json::json!({"effect":RootSoundRoute {
        recipe_extent: duration(48_000), recipe_grid: RootSoundGrid::root(before.presentation_basis().frame_rate),
        edits: vec![RootSoundEdit { grid: RootSoundGrid::root(before.presentation_basis().frame_rate),
            operation: RootSoundOperation::Insert { at: ProjectFrame(20_000),duration:duration(1000) },
            cuts: RootSoundCutEdges {before:AudioEdgePolicy::Hard,after:AudioEdgePolicy::Hard} }]
    }});
    let after = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut expected = original;
    expected.splice(20_000..20_000, vec![[0.0; 2]; 1000]);
    assert_pcm_close(&read(&after, 0, 49_000), &expected);
    for (start, frames) in [(19500, 1800), (32000, 300), (21500, 2049)] {
        assert_pcm_close(
            &read(&after, start, frames),
            &expected[start as usize..start as usize + frames as usize],
        );
    }
}
