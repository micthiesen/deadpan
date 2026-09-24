use std::sync::atomic::AtomicBool;

use deadpan_dsp::{
    FIXED_PEAK_ENGINE_ID, FIXED_PEAK_INPUT_FRAMES, FIXED_PEAK_OUTPUT_FRAMES, FIXED_PEAK_RADIUS,
    FIXED_PEAK_ROWS, FIXED_PEAK_TAPS, FixedPeakBank, PeakBankError,
};

type Coefficients = [[f64; FIXED_PEAK_TAPS]; FIXED_PEAK_ROWS];

fn coefficients() -> Coefficients {
    let mut rows = [[0.0; FIXED_PEAK_TAPS]; FIXED_PEAK_ROWS];
    for (row, values) in rows.iter_mut().enumerate() {
        for (tap, value) in values.iter_mut().enumerate() {
            let residue = (row * 31 + tap * 17) % 101;
            *value = (residue as f64 - 50.0) / 10_000.0;
        }
        values[FIXED_PEAK_RADIUS] += 0.75 + row as f64 / 100.0;
    }
    rows
}

fn input() -> (Vec<f32>, Vec<f32>) {
    let mut left = vec![0.0; FIXED_PEAK_INPUT_FRAMES];
    let mut right = vec![0.0; FIXED_PEAK_INPUT_FRAMES];
    let mut state = 0x6a09_e667_f3bc_c909u64;
    for frame in 0..FIXED_PEAK_INPUT_FRAMES {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let noise = (state as f64 / u64::MAX as f64) * 2.0 - 1.0;
        left[frame] = (0.61 * noise) as f32;
        right[frame] = (-0.39 * noise + 0.07 * (frame % 13) as f64 / 13.0) as f32;
    }
    left[FIXED_PEAK_RADIUS + 31] = 0.94;
    right[FIXED_PEAK_RADIUS + 718] = -0.87;
    left[FIXED_PEAK_INPUT_FRAMES - 2] = -0.73;
    (left, right)
}

fn direct(left: &[f32], right: &[f32], rows: &Coefficients) -> Vec<f64> {
    let mut output = vec![0.0f64; FIXED_PEAK_OUTPUT_FRAMES];
    for (frame, peak) in output.iter_mut().enumerate() {
        for row in rows {
            let mut left_value = 0.0;
            let mut right_value = 0.0;
            for tap in 0..FIXED_PEAK_TAPS {
                left_value += f64::from(left[frame + tap]) * row[tap];
                right_value += f64::from(right[frame + tap]) * row[tap];
            }
            *peak = peak.max(left_value.abs()).max(right_value.abs());
        }
    }
    output
}

#[test]
fn fft_rows_match_direct_finite_f64_convolution_at_all_core_boundaries() {
    assert_eq!(
        FIXED_PEAK_ENGINE_ID,
        "deadpan-finite-peak-signalsmith-realfft-f64-t1024-r64-n1280-v1"
    );
    let rows = coefficients();
    let (left, right) = input();
    let expected = direct(&left, &right, &rows);
    let mut bank = FixedPeakBank::new(rows).unwrap();
    let mut actual = [f64::NAN; FIXED_PEAK_OUTPUT_FRAMES];
    bank.process_tile(&left, &right, &mut actual, &AtomicBool::new(false))
        .unwrap();
    for (frame, (&actual, &expected)) in actual.iter().zip(&expected).enumerate() {
        assert!(
            (actual - expected).abs() <= 2.0e-12,
            "frame {frame}: FFT {actual:.17e}, direct {expected:.17e}"
        );
    }

    let first = actual;
    bank.process_tile(&left, &right, &mut actual, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(
        actual, first,
        "reusing one plan must not retain prior input"
    );
}

#[test]
fn plans_keep_their_own_coefficients_and_leave_silent_tiles_silent() {
    let first_rows = coefficients();
    let mut second_rows = first_rows;
    for row in &mut second_rows {
        for coefficient in row {
            *coefficient *= 0.5;
        }
    }
    let mut first_bank = FixedPeakBank::new(first_rows).unwrap();
    let mut second_bank = FixedPeakBank::new(second_rows).unwrap();
    let (left, right) = input();
    let mut first = [0.0; FIXED_PEAK_OUTPUT_FRAMES];
    let mut second = [0.0; FIXED_PEAK_OUTPUT_FRAMES];
    let cancelled = AtomicBool::new(false);
    first_bank
        .process_tile(&left, &right, &mut first, &cancelled)
        .unwrap();
    second_bank
        .process_tile(&left, &right, &mut second, &cancelled)
        .unwrap();
    for (&actual, &reference) in second.iter().zip(&first) {
        assert!((actual - reference * 0.5).abs() <= 2.0e-12);
    }
    first_bank
        .process_tile(&left, &right, &mut first, &cancelled)
        .unwrap();
    assert_eq!(second, first.map(|value| value * 0.5));

    let silent_rows = [[0.0; FIXED_PEAK_TAPS]; FIXED_PEAK_ROWS];
    let mut silent_bank = FixedPeakBank::new(silent_rows).unwrap();
    let silence = [0.0; FIXED_PEAK_INPUT_FRAMES];
    let mut silent_output = [1.0; FIXED_PEAK_OUTPUT_FRAMES];
    silent_bank
        .process_tile(&silence, &silence, &mut silent_output, &cancelled)
        .unwrap();
    assert_eq!(silent_output, [0.0; FIXED_PEAK_OUTPUT_FRAMES]);
}

#[test]
fn fixed_bank_rejects_bad_bounds_and_preserves_output_on_every_rejection() {
    let mut bad_coefficients = coefficients();
    bad_coefficients[3][91] = f64::NAN;
    assert!(matches!(
        FixedPeakBank::new(bad_coefficients),
        Err(PeakBankError::Coefficients)
    ));
    let mut bad_coefficients = coefficients();
    bad_coefficients[3][91] = 16.01;
    assert!(matches!(
        FixedPeakBank::new(bad_coefficients),
        Err(PeakBankError::Coefficients)
    ));

    let mut bank = FixedPeakBank::new(coefficients()).unwrap();
    let (left, right) = input();
    let mut output = [123.0; FIXED_PEAK_OUTPUT_FRAMES];
    assert_eq!(
        bank.process_tile(
            &left[..FIXED_PEAK_INPUT_FRAMES - 1],
            &right,
            &mut output,
            &AtomicBool::new(false)
        ),
        Err(PeakBankError::InputLength)
    );
    assert_eq!(
        bank.process_tile(
            &left,
            &right,
            &mut output[..FIXED_PEAK_OUTPUT_FRAMES - 1],
            &AtomicBool::new(false)
        ),
        Err(PeakBankError::OutputLength)
    );
    assert_eq!(output, [123.0; FIXED_PEAK_OUTPUT_FRAMES]);

    let mut invalid = left.clone();
    invalid[100] = f32::NAN;
    assert_eq!(
        bank.process_tile(&invalid, &right, &mut output, &AtomicBool::new(false)),
        Err(PeakBankError::InvalidInput)
    );
    invalid[100] = 16.01;
    assert_eq!(
        bank.process_tile(&invalid, &right, &mut output, &AtomicBool::new(false)),
        Err(PeakBankError::InvalidInput)
    );
    assert_eq!(
        bank.process_tile(&left, &right, &mut output, &AtomicBool::new(true)),
        Err(PeakBankError::Cancelled)
    );
    assert_eq!(output, [123.0; FIXED_PEAK_OUTPUT_FRAMES]);
}
