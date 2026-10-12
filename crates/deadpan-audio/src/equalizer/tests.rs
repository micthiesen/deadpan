use super::*;
use deadpan_core::{EqCutoffHz, GainDb};

fn recipe(gains: [i32; 3], low: Option<u16>, high: Option<u16>) -> EqualizerRecipe {
    EqualizerRecipe::new(
        gains.map(|value| GainDb::new(value).unwrap()),
        low.map(|hz| EqCutoffHz::new(hz).unwrap()),
        high.map(|hz| EqCutoffHz::new(hz).unwrap()),
    )
    .unwrap()
}
fn coefficients(gains: [i32; 3], low: Option<u16>, high: Option<u16>) -> [f64; TAPS] {
    equalizer_coefficients(recipe(gains, low, high), &AtomicBool::new(false)).unwrap()
}
fn response(kernel: &[f64; TAPS], hz: f64) -> (f64, f64) {
    let mut real = 0.0;
    let mut imaginary = 0.0;
    for (index, weight) in kernel.iter().enumerate() {
        let phase = std::f64::consts::TAU * hz * (index as f64 - RADIUS as f64) / 48000.0;
        real += weight * phase.cos();
        imaginary += weight * phase.sin();
    }
    (real, imaginary)
}
fn amplitude(db: i32) -> f64 {
    10.0_f64.powf(f64::from(db) / 20000.0)
}

#[test]
fn complementary_bands_cancel_to_exact_uniform_gain_and_preserve_phase() {
    for gain in [-24000, -6000, 0, 7500, 24000] {
        let kernel = coefficients([gain; 3], None, None);
        for (index, value) in kernel.iter().enumerate() {
            assert_eq!(
                *value,
                if index == RADIUS {
                    amplitude(gain)
                } else {
                    0.0
                }
            );
        }
    }
    let kernel = coefficients([6000, -6000, 3000], None, None);
    for (hz, gain) in [
        (0.0, 6000),
        (80.0, 6000),
        (1000.0, -6000),
        (12000.0, 3000),
        (24000.0, 3000),
    ] {
        let (real, imaginary) = response(&kernel, hz);
        assert!((real / amplitude(gain) - 1.0).abs() < 3e-5, "{hz}: {real}");
        assert!(imaginary.abs() < 1e-12);
    }
}

#[test]
fn cutoffs_have_measured_transition_centers_and_stopbands_including_extremes() {
    for cutoff in [80, 250, 1000, 4000, 12000, 20000] {
        let lp = coefficients([0; 3], None, Some(cutoff));
        let hp = coefficients([0; 3], Some(cutoff), None);
        for kernel in [&lp, &hp] {
            let (real, imaginary) = response(kernel, f64::from(cutoff));
            assert!((real - 0.5).abs() < 0.001, "cutoff {cutoff}: {real}");
            assert!(imaginary.abs() < 1e-12);
        }
        let pass = response(&lp, f64::from(cutoff - 80)).0;
        let stop = response(&lp, f64::from(cutoff + 80)).0;
        assert!((pass - 1.0).abs() < 0.0001, "cutoff {cutoff}, pass {pass}");
        assert!(stop.abs() < 0.0001, "cutoff {cutoff}, stop {stop}");
        assert!(response(&hp, 0.0).0.abs() < 1e-12);
        for hz in [0.0, 80.0, 1000.0, 10000.0, 24000.0] {
            assert!((response(&lp, hz).0 + response(&hp, hz).0 - 1.0).abs() < 1e-12);
        }
    }
    let band = coefficients([0; 3], Some(1000), Some(1080));
    assert!(response(&band, 800.0).0.abs() < 0.0001);
    assert!(response(&band, 1280.0).0.abs() < 0.0001);
    assert!((response(&band, 1040.0).0 - 1.0).abs() < 0.002);
}

#[test]
fn bounded_extreme_tone_and_cut_combinations_keep_one_symmetric_kernel() {
    for bass in [-24000, 0, 24000] {
        for middle in [-24000, 0, 24000] {
            for treble in [-24000, 0, 24000] {
                for (low, high) in [
                    (None, None),
                    (Some(80), Some(20000)),
                    (Some(200), Some(280)),
                    (Some(19920), Some(20000)),
                ] {
                    let kernel = coefficients([bass, middle, treble], low, high);
                    assert!(kernel.iter().all(|value| value.is_finite()
                        && value.abs() <= deadpan_dsp::MAX_FILTER_COEFFICIENT_MAGNITUDE));
                    for index in 0..TAPS {
                        assert_eq!(kernel[index].to_bits(), kernel[TAPS - 1 - index].to_bits());
                    }
                    // Every admitted compiled kernel is accepted by the real adapter.
                    FiniteStereoFilter::new(&kernel).unwrap();
                }
            }
        }
    }
}

fn signal(frequencies: &[(f64, f64)], start: usize, frames: usize) -> Vec<f64> {
    (start..start + frames)
        .map(|at| {
            frequencies
                .iter()
                .map(|(hz, gain)| gain * (std::f64::consts::TAU * hz * at as f64 / 48000.0).sin())
                .sum()
        })
        .collect()
}

#[test]
fn real_eq_processing_changes_bands_and_cuts_without_moving_sample_coordinates() {
    let input = signal(&[(100.0, 0.1), (1000.0, 0.1), (12000.0, 0.1)], 0, INPUT);
    let right: Vec<_> = input.iter().map(|value| -0.7 * value).collect();
    for (chosen, expected_bands) in [
        (
            recipe([6000, -6000, 3000], None, None),
            vec![
                (100.0, 0.1 * amplitude(6000)),
                (1000.0, 0.1 * amplitude(-6000)),
                (12000.0, 0.1 * amplitude(3000)),
            ],
        ),
        (recipe([0; 3], Some(300), Some(5000)), vec![(1000.0, 0.1)]),
    ] {
        let cancel = AtomicBool::new(false);
        let mut eq = Equalizer::new(chosen, &cancel).unwrap();
        let mut left_out = vec![0.0; OUTPUT];
        let mut right_out = vec![0.0; OUTPUT];
        eq.process_tile(&input, &right, &mut left_out, &mut right_out, &cancel)
            .unwrap();
        let expected = signal(&expected_bands, RADIUS, OUTPUT);
        for ((left, right), expected) in left_out.iter().zip(&right_out).zip(expected) {
            assert!((left - expected).abs() < 0.00001, "{left} != {expected}");
            assert!((right + 0.7 * expected).abs() < 0.00001);
        }
    }
}

#[test]
fn unity_retains_exact_bits_including_signed_zero_and_tiny_values() {
    let values = [
        0.0,
        -0.0,
        f64::from_bits(1),
        -f64::from_bits(1),
        0.123456789012345,
        -7e-23,
        1e100,
    ];
    let input: Vec<_> = (0..INPUT)
        .map(|index| values[index % values.len()])
        .collect();
    let mut eq = Equalizer::new(EqualizerRecipe::default(), &AtomicBool::new(false)).unwrap();
    let mut left = vec![9.0; OUTPUT];
    let mut right = vec![8.0; OUTPUT];
    eq.process_tile(
        &input,
        &input,
        &mut left,
        &mut right,
        &AtomicBool::new(false),
    )
    .unwrap();
    for ((left, right), input) in left.iter().zip(&right).zip(&input[RADIUS..RADIUS + OUTPUT]) {
        assert_eq!(left.to_bits(), input.to_bits());
        assert_eq!(right.to_bits(), input.to_bits());
    }
}

#[test]
fn bypass_and_active_eq_both_validate_complete_context_and_leave_failed_outputs_untouched() {
    let mut input = vec![0.25; INPUT];
    let mut left = vec![123.0; OUTPUT];
    let mut right = vec![-321.0; OUTPUT];
    let cancel = AtomicBool::new(false);
    for recipe in [
        EqualizerRecipe::default(),
        recipe([6000, 0, -6000], None, None),
    ] {
        let mut eq = Equalizer::new(recipe, &cancel).unwrap();
        input[0] = f64::NAN;
        assert_eq!(
            eq.process_tile(&input, &input, &mut left, &mut right, &cancel),
            Err(FilterError::InvalidInput)
        );
        input[0] = 0.25;
        assert_eq!(
            eq.process_tile(&input[..INPUT - 1], &input, &mut left, &mut right, &cancel),
            Err(FilterError::InputLength)
        );
        assert_eq!(
            eq.process_tile(&input, &input, &mut left, &mut right[..OUTPUT - 1], &cancel),
            Err(FilterError::OutputLength)
        );
        assert_eq!(
            eq.process_tile(
                &input,
                &input,
                &mut left,
                &mut right,
                &AtomicBool::new(true)
            ),
            Err(FilterError::Cancelled)
        );
        assert_eq!(left, vec![123.0; OUTPUT]);
        assert_eq!(right, vec![-321.0; OUTPUT]);
        assert!(matches!(
            Equalizer::new(recipe, &AtomicBool::new(true)),
            Err(FilterError::Cancelled)
        ));
    }
}

#[test]
fn changed_recipes_and_cold_fixed_tiles_never_reuse_another_response() {
    let cancel = AtomicBool::new(false);
    let input = signal(
        &[(100.0, 0.3), (3000.0, 0.2), (6000.0, 0.1)],
        0,
        INPUT + 3 * OUTPUT,
    );
    let chosen = recipe([6000, -6000, 0], Some(80), Some(8000));
    let mut eq = Equalizer::new(chosen, &cancel).unwrap();
    let mut continuous = Vec::new();
    for tile in 0..4 {
        let mut left = vec![0.0; OUTPUT];
        let mut right = vec![0.0; OUTPUT];
        let block = &input[tile * OUTPUT..tile * OUTPUT + INPUT];
        eq.process_tile(block, block, &mut left, &mut right, &cancel)
            .unwrap();
        continuous.push(left);
    }
    for tile in [3, 0, 2, 1] {
        let mut cold = Equalizer::new(chosen, &cancel).unwrap();
        let block = &input[tile * OUTPUT..tile * OUTPUT + INPUT];
        let mut left = vec![0.0; OUTPUT];
        let mut right = vec![0.0; OUTPUT];
        cold.process_tile(block, block, &mut left, &mut right, &cancel)
            .unwrap();
        assert_eq!(left, continuous[tile]);
        assert_eq!(right, continuous[tile]);
    }
    let changed = Equalizer::new(recipe([-6000, 6000, 0], Some(80), Some(8000)), &cancel).unwrap();
    assert_ne!(eq.coefficients(), changed.coefficients());
    assert_eq!(eq.recipe(), chosen);
}
