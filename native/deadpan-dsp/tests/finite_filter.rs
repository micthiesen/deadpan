use std::sync::atomic::{AtomicBool, Ordering};

use deadpan_dsp::{
    FINITE_FILTER_INPUT_FRAMES as INPUT, FINITE_FILTER_OUTPUT_FRAMES as OUTPUT,
    FINITE_FILTER_RADIUS as RADIUS, FINITE_FILTER_TAPS as TAPS, FilterError, FiniteStereoFilter,
    MAX_FILTER_SAMPLE_MAGNITUDE,
};

fn kernel() -> [f64; TAPS] {
    let mut kernel = std::array::from_fn(|i| ((i * 31 % 127) as f64 - 63.0) / 20_000.0);
    kernel[0] = -0.125;
    kernel[RADIUS] = 0.75;
    kernel[TAPS - 1] = 0.25;
    kernel
}

fn samples(count: usize) -> Vec<f64> {
    let mut seed = 0x6a09_e667_f3bc_c909u64;
    (0..count)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed as f64 / u64::MAX as f64) * 2.0 - 1.0
        })
        .collect()
}

fn process(filter: &mut FiniteStereoFilter, left: &[f64], right: &[f64]) -> [Vec<f64>; 2] {
    let mut a = vec![f64::NAN; OUTPUT];
    let mut b = vec![f64::NAN; OUTPUT];
    filter
        .process_tile(left, right, &mut a, &mut b, &AtomicBool::new(false))
        .unwrap();
    [a, b]
}

#[test]
fn signed_asymmetric_kernel_matches_direct_convolution_in_both_channels() {
    let weights = kernel();
    let left = samples(INPUT);
    let right: Vec<_> = left.iter().rev().map(|x| -3.0 * x + 0.01).collect();
    let mut filter = FiniteStereoFilter::new(&weights).unwrap();
    let actual = process(&mut filter, &left, &right);
    for (channel, input) in [&left, &right].into_iter().enumerate() {
        for frame in 0..OUTPUT {
            let expected: f64 = input[frame..frame + TAPS]
                .iter()
                .zip(weights)
                .map(|(sample, weight)| sample * weight)
                .sum();
            assert!(
                (actual[channel][frame] - expected).abs() < 2e-12,
                "channel {channel}, frame {frame}: {} != {expected}",
                actual[channel][frame]
            );
        }
    }
    assert!(actual[0].iter().any(|value| *value < -0.1));
    assert!(actual[1].iter().any(|value| *value > 1.0));
}

#[test]
fn outermost_context_samples_have_the_correct_orientation_and_no_wraparound() {
    let mut weights = [0.0; TAPS];
    weights[0] = -2.0;
    weights[TAPS - 1] = 3.0;
    let mut left = vec![0.0; INPUT];
    let mut right = vec![0.0; INPUT];
    left[0] = 0.5;
    right[INPUT - 1] = -0.25;
    let mut filter = FiniteStereoFilter::new(&weights).unwrap();
    let actual = process(&mut filter, &left, &right);
    for (frame, (&left, &right)) in actual[0].iter().zip(&actual[1]).enumerate() {
        let expected_left = if frame == 0 { -1.0 } else { 0.0 };
        let expected_right = if frame == OUTPUT - 1 { -0.75 } else { 0.0 };
        assert!((left - expected_left).abs() < 1e-12);
        assert!((right - expected_right).abs() < 1e-12);
    }
}

#[test]
fn dc_nyquist_and_center_impulses_keep_scale_phase_and_channel_independence() {
    let mut weights = [0.0; TAPS];
    weights[RADIUS] = 0.75;
    weights[RADIUS - 1] = 0.125;
    weights[RADIUS + 1] = 0.125;
    let mut filter = FiniteStereoFilter::new(&weights).unwrap();
    let dc = vec![2.0; INPUT];
    let nyquist: Vec<_> = (0..INPUT)
        .map(|i| if i % 2 == 0 { 2.0 } else { -2.0 })
        .collect();
    let actual = process(&mut filter, &dc, &nyquist);
    for frame in 0..OUTPUT {
        assert!((actual[0][frame] - 2.0).abs() < 1e-12);
        assert!((actual[1][frame] - nyquist[frame + RADIUS] * 0.5).abs() < 1e-12);
    }
    let mut impulse = vec![0.0; INPUT];
    impulse[RADIUS + 200] = -2.0;
    let actual = process(&mut filter, &impulse, &vec![0.0; INPUT]);
    for (frame, (&left, &right)) in actual[0].iter().zip(&actual[1]).enumerate() {
        let expected = match frame {
            199 | 201 => -0.25,
            200 => -1.5,
            _ => 0.0,
        };
        assert!((left - expected).abs() < 1e-12);
        assert_eq!(right, 0.0);
    }
}

#[test]
fn fixed_grid_seeks_repeat_exactly_after_unrelated_tiles_and_fresh_construction() {
    let input = samples(INPUT + OUTPUT * 4);
    let weights = kernel();
    let mut filter = FiniteStereoFilter::new(&weights).unwrap();
    let mut continuous = Vec::new();
    for tile in 0..5 {
        let slice = &input[tile * OUTPUT..tile * OUTPUT + INPUT];
        continuous.push(process(&mut filter, slice, slice));
    }
    for tile in [4, 0, 3, 1, 2] {
        let slice = &input[tile * OUTPUT..tile * OUTPUT + INPUT];
        assert_eq!(process(&mut filter, slice, slice), continuous[tile]);
        let mut cold = FiniteStereoFilter::new(&weights).unwrap();
        assert_eq!(process(&mut cold, slice, slice), continuous[tile]);
    }
}

#[test]
fn failures_preserve_both_outputs_and_do_not_disable_the_plan() {
    let mut invalid = kernel();
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 16.001, -16.001] {
        invalid[37] = value;
        assert!(matches!(
            FiniteStereoFilter::new(&invalid),
            Err(FilterError::Coefficients)
        ));
    }
    let mut filter = FiniteStereoFilter::new(&kernel()).unwrap();
    let input = samples(INPUT);
    let mut a = vec![123.0; OUTPUT];
    let mut b = vec![-321.0; OUTPUT];
    let cancel = AtomicBool::new(false);
    for input_count in [0, INPUT - 1] {
        assert_eq!(
            filter.process_tile(&input[..input_count], &input, &mut a, &mut b, &cancel),
            Err(FilterError::InputLength)
        );
        assert_eq!(
            filter.process_tile(&input, &input[..input_count], &mut a, &mut b, &cancel),
            Err(FilterError::InputLength)
        );
    }
    assert_eq!(
        filter.process_tile(&input, &input, &mut a[..OUTPUT - 1], &mut b, &cancel),
        Err(FilterError::OutputLength)
    );
    assert_eq!(
        filter.process_tile(&input, &input, &mut a, &mut b[..OUTPUT - 1], &cancel),
        Err(FilterError::OutputLength)
    );
    for bad in [
        f64::NAN,
        f64::INFINITY,
        -f64::INFINITY,
        MAX_FILTER_SAMPLE_MAGNITUDE * 2.0,
    ] {
        let mut invalid = input.clone();
        invalid[INPUT - 1] = bad;
        assert_eq!(
            filter.process_tile(&invalid, &input, &mut a, &mut b, &cancel),
            Err(FilterError::InvalidInput)
        );
        assert_eq!(
            filter.process_tile(&input, &invalid, &mut a, &mut b, &cancel),
            Err(FilterError::InvalidInput)
        );
    }
    cancel.store(true, Ordering::Relaxed);
    assert_eq!(
        filter.process_tile(&input, &input, &mut a, &mut b, &cancel),
        Err(FilterError::Cancelled)
    );
    assert_eq!(a, vec![123.0; OUTPUT]);
    assert_eq!(b, vec![-321.0; OUTPUT]);
    cancel.store(false, Ordering::Relaxed);
    filter
        .process_tile(&input, &input, &mut a, &mut b, &cancel)
        .unwrap();
    assert_eq!(a, b);
}

#[test]
fn maximum_admitted_input_and_coefficients_stay_finite_without_limiting() {
    let mut filter = FiniteStereoFilter::new(&[16.0; TAPS]).unwrap();
    let input = vec![MAX_FILTER_SAMPLE_MAGNITUDE; INPUT];
    let negative = vec![-MAX_FILTER_SAMPLE_MAGNITUDE; INPUT];
    let output = process(&mut filter, &input, &negative);
    let expected = MAX_FILTER_SAMPLE_MAGNITUDE * 16.0 * TAPS as f64;
    for (&left, &right) in output[0].iter().zip(&output[1]) {
        assert!((left / expected - 1.0).abs() < 1e-12);
        assert!((right / expected + 1.0).abs() < 1e-12);
    }
}

#[test]
fn independent_kernels_copy_their_coefficients_and_exact_silence_has_no_residual() {
    let mut weights = kernel();
    let input = samples(INPUT);
    let mut first = FiniteStereoFilter::new(&weights).unwrap();
    weights.fill(0.0);
    let mut second = FiniteStereoFilter::new(&weights).unwrap();
    assert_eq!(
        process(&mut second, &input, &input),
        [vec![0.0; OUTPUT], vec![0.0; OUTPUT]]
    );
    let nonzero = process(&mut first, &input, &input);
    assert!(nonzero[0].iter().any(|value| value.abs() > 0.1));
    assert_eq!(
        process(&mut first, &vec![0.0; INPUT], &vec![0.0; INPUT]),
        [vec![0.0; OUTPUT], vec![0.0; OUTPUT]]
    );
    assert_eq!(process(&mut first, &input, &input), nonzero);
}
