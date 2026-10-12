use std::sync::atomic::{AtomicBool, Ordering};

use deadpan_dsp::{
    CAUSAL_FILTER_MAX_TAPS as MAX_TAPS, CAUSAL_FILTER_OUTPUT_FRAMES as OUTPUT,
    CausalFilterError as Error, CausalStereoFilter,
};

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

fn process(filter: &mut CausalStereoFilter, left: &[f64], right: &[f64]) -> [Vec<f64>; 2] {
    let mut a = vec![f64::NAN; OUTPUT];
    let mut b = vec![f64::NAN; OUTPUT];
    filter
        .process_tile(left, right, &mut a, &mut b, &AtomicBool::new(false))
        .unwrap();
    [a, b]
}

#[test]
fn independent_asymmetric_kernels_match_direct_causal_convolution() {
    let kernels: [Vec<_>; 2] = std::array::from_fn(|channel| {
        (0..97)
            .map(|i| ((i * (31 + channel * 6) % 127) as f64 - 63.0) / 100.0)
            .collect()
    });
    let input = samples(OUTPUT + 96);
    let right: Vec<_> = input.iter().rev().map(|x| -3.0 * x + 0.01).collect();
    let mut filter = CausalStereoFilter::new(&kernels[0], &kernels[1]).unwrap();
    let actual = process(&mut filter, &input, &right);
    for (channel, source) in [&input, &right].into_iter().enumerate() {
        for frame in 0..OUTPUT {
            let expected: f64 = kernels[channel]
                .iter()
                .enumerate()
                .map(|(lag, gain)| source[96 + frame - lag] * gain)
                .sum();
            assert!(
                (actual[channel][frame] - expected).abs() < 2e-12,
                "channel {channel}, frame {frame}: {} != {expected}",
                actual[channel][frame]
            );
        }
    }
}

#[test]
fn full_history_and_current_sample_keep_lag_and_never_wrap_or_look_ahead() {
    for taps in [1, 4096, 4097, 8192, MAX_TAPS] {
        let history = taps - 1;
        let mut a = vec![0.0; taps];
        let mut b = vec![0.0; taps];
        a[history] = -2.0;
        b[0] = 3.0;
        let mut filter = CausalStereoFilter::new(&a, &b).unwrap();
        assert_eq!(filter.history_frames(), history);
        let mut left = vec![0.0; filter.input_frames()];
        let mut right = left.clone();
        left[0] = 0.5;
        right[history + OUTPUT - 1] = -0.25;
        let output = process(&mut filter, &left, &right);
        for (frame, (&left, &right)) in output[0].iter().zip(&output[1]).enumerate() {
            assert!((left - if frame == 0 { -1.0 } else { 0.0 }).abs() < 1e-12);
            assert!((right - if frame == OUTPUT - 1 { -0.75 } else { 0.0 }).abs() < 1e-12);
        }
    }
}

#[test]
fn dc_nyquist_scale_and_independent_channels_survive_packed_bins() {
    let mut filter = CausalStereoFilter::new(&[0.75, 0.125, 0.125], &[0.5, -0.25, 0.25]).unwrap();
    let dc = vec![2.0; filter.input_frames()];
    let nyquist: Vec<_> = (0..dc.len())
        .map(|i| if i % 2 == 0 { 2.0 } else { -2.0 })
        .collect();
    let output = process(&mut filter, &dc, &nyquist);
    for frame in 0..OUTPUT {
        assert!((output[0][frame] - 2.0).abs() < 1e-12);
        assert!((output[1][frame] - nyquist[frame + 2]).abs() < 1e-12);
    }
}

#[test]
fn cold_and_shuffled_reads_are_bit_identical_to_linear_fixed_tiles() {
    let mut weights = vec![0.0; 16_001];
    weights[0] = 0.75;
    weights[14400] = -0.5;
    weights[16000] = 0.125;
    let mut filter = CausalStereoFilter::new(&weights, &weights).unwrap();
    let count = filter.input_frames();
    let input = samples(count + OUTPUT * 4);
    let mut linear = Vec::new();
    for tile in 0..5 {
        let slice = &input[tile * OUTPUT..tile * OUTPUT + count];
        linear.push(process(&mut filter, slice, slice));
    }
    for tile in [4, 0, 3, 1, 2] {
        let slice = &input[tile * OUTPUT..tile * OUTPUT + count];
        assert_eq!(process(&mut filter, slice, slice), linear[tile]);
        let mut cold = CausalStereoFilter::new(&weights, &weights).unwrap();
        assert_eq!(process(&mut cold, slice, slice), linear[tile]);
    }
}

#[test]
fn invalid_lengths_kernels_samples_and_cancellation_are_atomic_and_retryable() {
    for (a, b) in [
        (vec![], vec![]),
        (vec![0.0], vec![]),
        (vec![0.0; MAX_TAPS + 1], vec![0.0; MAX_TAPS + 1]),
    ] {
        assert!(matches!(
            CausalStereoFilter::new(&a, &b),
            Err(Error::KernelLength)
        ));
    }
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 16.001, -16.001] {
        assert!(matches!(
            CausalStereoFilter::new(&[bad], &[1.0]),
            Err(Error::Coefficients)
        ));
        assert!(matches!(
            CausalStereoFilter::new(&[1.0], &[bad]),
            Err(Error::Coefficients)
        ));
    }
    let mut filter = CausalStereoFilter::new(&[0.75, 0.25], &[-0.5, 0.25]).unwrap();
    let count = filter.input_frames();
    let input = samples(count);
    let mut a = vec![123.0; OUTPUT];
    let mut b = vec![-321.0; OUTPUT];
    let cancel = AtomicBool::new(false);
    for len in [0, count - 1] {
        assert_eq!(
            filter.process_tile(&input[..len], &input, &mut a, &mut b, &cancel),
            Err(Error::InputLength)
        );
        assert_eq!(
            filter.process_tile(&input, &input[..len], &mut a, &mut b, &cancel),
            Err(Error::InputLength)
        );
    }
    assert_eq!(
        filter.process_tile(&input, &input, &mut a[..OUTPUT - 1], &mut b, &cancel),
        Err(Error::OutputLength)
    );
    assert_eq!(
        filter.process_tile(&input, &input, &mut a, &mut b[..OUTPUT - 1], &cancel),
        Err(Error::OutputLength)
    );
    for bad in [f64::NAN, f64::INFINITY, -f64::INFINITY, 1.01e100, -1.01e100] {
        let mut invalid = input.clone();
        invalid[count - 1] = bad;
        assert_eq!(
            filter.process_tile(&invalid, &input, &mut a, &mut b, &cancel),
            Err(Error::InvalidInput)
        );
        assert_eq!(
            filter.process_tile(&input, &invalid, &mut a, &mut b, &cancel),
            Err(Error::InvalidInput)
        );
    }
    cancel.store(true, Ordering::Relaxed);
    assert_eq!(
        filter.process_tile(&input, &input, &mut a, &mut b, &cancel),
        Err(Error::Cancelled)
    );
    assert_eq!(a, vec![123.0; OUTPUT]);
    assert_eq!(b, vec![-321.0; OUTPUT]);
    cancel.store(false, Ordering::Relaxed);
    filter
        .process_tile(&input, &input, &mut a, &mut b, &cancel)
        .unwrap();
    assert_ne!(a, b);
}

#[test]
fn maximum_admitted_coefficients_and_samples_stay_finite_without_limiting() {
    let mut filter =
        CausalStereoFilter::new(&vec![16.0; MAX_TAPS], &vec![-16.0; MAX_TAPS]).unwrap();
    let input = vec![1e100; filter.input_frames()];
    let output = process(&mut filter, &input, &input);
    let expected = 1e100 * 16.0 * MAX_TAPS as f64;
    for (&left, &right) in output[0].iter().zip(&output[1]) {
        assert!((left / expected - 1.0).abs() < 1e-12);
        assert!((right / expected + 1.0).abs() < 1e-12);
    }
}

#[test]
fn coefficients_are_copied_and_zero_input_has_no_state_or_residual() {
    let mut weights = [1.0, -0.5];
    let mut filter = CausalStereoFilter::new(&weights, &[0.0; 2]).unwrap();
    weights.fill(0.0);
    let input = samples(filter.input_frames());
    let output = process(&mut filter, &input, &input);
    assert!(output[0].iter().any(|x| x.abs() > 0.1));
    assert_eq!(output[1], vec![0.0; OUTPUT]);
    let silence = vec![0.0; filter.input_frames()];
    assert_eq!(
        process(&mut filter, &silence, &silence),
        [vec![0.0; OUTPUT], vec![0.0; OUTPUT]]
    );
    assert_eq!(process(&mut filter, &input, &input), output);
}

#[test]
fn memory_admission_refuses_invalid_sizes_and_covers_all_fft_steps() {
    for taps in [0, MAX_TAPS + 1, usize::MAX] {
        assert_eq!(
            CausalStereoFilter::native_bytes_bound(taps),
            Err(Error::KernelLength)
        );
    }
    let mut previous = 0;
    for taps in [1, 4096, 4097, 8192, 16_001, 48_001, 192_001, MAX_TAPS] {
        let bytes = CausalStereoFilter::native_bytes_bound(taps).unwrap();
        assert!(bytes >= previous);
        assert!(bytes <= 65 * 1024 * 1024);
        previous = bytes;
    }
}
