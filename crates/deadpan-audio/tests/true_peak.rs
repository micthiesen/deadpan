use std::f64::consts::PI;
use std::sync::atomic::{AtomicBool, Ordering};

use deadpan_audio::{
    MAX_TRUE_PEAK_FRAMES, TRUE_PEAK_ID, TruePeakError, TruePeakMeter, TruePeakReport,
};

fn measure(samples: &[[f32; 2]], chunks: &[usize]) -> TruePeakReport {
    let cancel = AtomicBool::new(false);
    let mut meter = TruePeakMeter::new(samples.len().max(1) as u64).unwrap();
    let mut position = 0;
    for count in chunks.iter().cycle() {
        if position == samples.len() {
            break;
        }
        let end = (position + count).min(samples.len());
        meter.push(&samples[position..end], &cancel).unwrap();
        position = end;
    }
    meter.finish()
}

fn tone(divisor: u32, amplitude: f64, phase_degrees: f64) -> Vec<[f32; 2]> {
    // Analytically synthesized from EBU Tech 3341 (2023), Table 1 cases 15-19.
    // The 10 ms taper avoids making the fixture's cut its largest peak.
    let frames = 4_800;
    (0..frames)
        .map(|n| {
            let fade = (n as f64 / 480.0)
                .min((frames - 1 - n) as f64 / 480.0)
                .min(1.0);
            let sample = (fade
                * amplitude
                * (2.0 * PI * n as f64 / f64::from(divisor) + phase_degrees.to_radians()).sin())
                as f32;
            [sample; 2]
        })
        .collect()
}

#[test]
fn generated_ebu_3341_peak_cases_15_through_19_meet_published_tolerances() {
    for (case, divisor, amplitude, phase, expected) in [
        (15, 4, 0.50, 0.0, -6.0),
        (16, 4, 0.50, 45.0, -6.0),
        (17, 6, 0.50, 60.0, -6.0),
        (18, 8, 0.50, 67.5, -6.0),
        (19, 4, 1.41, 45.0, 3.0),
    ] {
        let report = measure(&tone(divisor, amplitude, phase), &[256]);
        for db in report.true_peak_dbtp.map(Option::unwrap) {
            assert!(
                (expected - 0.4..=expected + 0.2).contains(&db),
                "case {case}: {db} dBTP"
            );
        }
        assert_eq!(report.algorithm, TRUE_PEAK_ID);
        assert_eq!(report.true_peak[0], report.true_peak[1]);
    }
}

fn transient(offset: usize) -> Vec<[f32; 2]> {
    // Table 1 cases 20-23: one continuous-phase fs/4 cycle at unity
    // inside an fs/6 tone at half amplitude, created at 4*fs and lowpassed
    // before decimation. Generated mathematics, not copied EBU audio files.
    const FRAMES: usize = 19_200;
    const BURST: usize = 9_600; // an fs/6 zero crossing on the 4*fs grid
    const RADIUS: i64 = 256;
    let high: Vec<f64> = (0..FRAMES)
        .map(|n| {
            let (amplitude, cycles) = if n < BURST {
                (0.5, n as f64 / 24.0)
            } else if n < BURST + 16 {
                (1.0, BURST as f64 / 24.0 + (n - BURST) as f64 / 16.0)
            } else {
                (
                    0.5,
                    BURST as f64 / 24.0 + 1.0 + (n - BURST - 16) as f64 / 24.0,
                )
            };
            let fade = (n as f64 / 1_920.0)
                .min((FRAMES - 1 - n) as f64 / 1_920.0)
                .min(1.0);
            fade * amplitude * (2.0 * PI * cycles).sin()
        })
        .collect();
    let mut filter: Vec<f64> = (-RADIUS..=RADIUS)
        .map(|j| {
            let x = j as f64;
            let sinc = if j == 0 {
                0.25
            } else {
                (PI * x * 0.25).sin() / (PI * x)
            };
            let window = 0.35875
                + 0.48829 * (PI * x / RADIUS as f64).cos()
                + 0.14128 * (2.0 * PI * x / RADIUS as f64).cos()
                + 0.01168 * (3.0 * PI * x / RADIUS as f64).cos();
            sinc * window
        })
        .collect();
    let sum: f64 = filter.iter().sum();
    for h in &mut filter {
        *h /= sum;
    }
    (offset..FRAMES)
        .step_by(4)
        .map(|n| {
            let value: f64 = filter
                .iter()
                .enumerate()
                .map(|(tap, h)| {
                    let source = n as i64 + tap as i64 - RADIUS;
                    if (0..FRAMES as i64).contains(&source) {
                        high[source as usize] * h
                    } else {
                        0.0
                    }
                })
                .sum();
            [value as f32; 2]
        })
        .collect()
}

#[test]
fn generated_ebu_3341_peak_cases_20_through_23_cover_all_decimation_offsets() {
    for offset in 0..4 {
        let report = measure(&transient(offset), &[7, 256, 1, 83]);
        let db = report.true_peak_dbtp[0].unwrap();
        assert!(
            (-0.4..=0.2).contains(&db),
            "case {}: {db} dBTP",
            20 + offset
        );
    }
}

#[test]
fn final_flush_catches_intersample_peak_after_the_last_input_block() {
    let samples = [[1.0, -0.25]; 2];
    let report = measure(&samples, &[1]);
    assert_eq!(report.measured_frames, 2);
    assert_eq!(report.sample_peak, [1.0, 0.25]);
    assert_eq!(report.true_peak[0], 1.244873046875);
    assert_eq!(report.true_peak[1], report.true_peak[0] / 4.0);
    assert!(report.true_peak_dbtp[0].unwrap() > 1.8);
}

#[test]
fn chunk_partition_and_zero_context_do_not_reset_fir_history() {
    let mut samples = tone(4, 1.41, 45.0);
    samples[255] = [16.0, -1.0];
    samples[256] = [-8.0, 2.0];
    samples[4095] = [0.2, -16.0];
    let reference = measure(&samples, &[256]);
    for partitions in [&[1][..], &[7, 13, 255, 2][..], &[256, 255, 254][..]] {
        assert_eq!(measure(&samples, partitions), reference);
    }
    let mut padded = vec![[0.0; 2]; 43];
    padded.extend_from_slice(&samples);
    padded.extend_from_slice(&[[0.0; 2]; 43]);
    let padded = measure(&padded, &[13]);
    assert_eq!(padded.true_peak, reference.true_peak);
    assert_eq!(padded.sample_peak, reference.sample_peak);
}

#[test]
fn silence_and_empty_input_have_no_fabricated_db_value() {
    for samples in [&[][..], &[[0.0; 2]; 256][..]] {
        let report = measure(samples, &[256]);
        assert_eq!(report.measured_frames, samples.len() as u64);
        assert_eq!(report.true_peak, [0.0; 2]);
        assert_eq!(report.true_peak_dbtp, [None; 2]);
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("NaN") && !json.contains("Infinity"));
    }
}

#[test]
fn rejected_admissions_preserve_counts_peaks_and_history() {
    let cancel = AtomicBool::new(false);
    let mut meter = TruePeakMeter::new(4).unwrap();
    meter.push(&[[0.5, -0.25]; 2], &cancel).unwrap();
    assert_eq!(meter.push(&[], &cancel), Err(TruePeakError::InvalidBlock));
    assert_eq!(
        meter.push(&[[0.0; 2]; 257], &cancel),
        Err(TruePeakError::InvalidBlock)
    );
    assert_eq!(
        meter.push(&[[0.0; 2]; 3], &cancel),
        Err(TruePeakError::FrameBudgetExceeded)
    );
    for invalid in [f32::NAN, f32::INFINITY, -f32::INFINITY, 16.001, -16.001] {
        assert_eq!(
            meter.push(&[[0.0; 2], [0.0, invalid]], &cancel),
            Err(TruePeakError::InvalidSamples)
        );
    }
    cancel.store(true, Ordering::Relaxed);
    assert_eq!(
        meter.push(&[[1.0; 2]; 2], &cancel),
        Err(TruePeakError::Cancelled)
    );
    cancel.store(false, Ordering::Relaxed);
    meter.push(&[[1.0; 2]; 2], &cancel).unwrap();
    assert_eq!(
        meter.finish(),
        measure(&[[0.5, -0.25], [0.5, -0.25], [1.0, 1.0], [1.0, 1.0]], &[4])
    );
    for invalid in [0, MAX_TRUE_PEAK_FRAMES + 1, u64::MAX] {
        assert!(matches!(
            TruePeakMeter::new(invalid),
            Err(TruePeakError::InvalidLimit)
        ));
    }
}

#[test]
fn sample_peak_is_a_floor_and_channels_are_measured_independently() {
    let report = measure(&[[16.0, 0.0]], &[1]);
    assert_eq!(report.sample_peak, [16.0, 0.0]);
    assert_eq!(report.true_peak, [16.0, 0.0]);
    assert_eq!(report.true_peak_dbtp[1], None);
    assert_eq!(report.true_peak_dbtp[0], Some(20.0 * 16.0f64.log10()));
}
