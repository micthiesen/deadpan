use std::f64::consts::TAU;
use std::sync::atomic::AtomicBool;

use deadpan_audio::{
    MAX_SOURCE_FRAMES, PcmWindow, PreparationError, ResampleRecipe, Resampler, StereoMatrix,
};
use deadpan_core::{AudioSample, ExactRatio};
use deadpan_media::audio_index::AudioChannelLayout;

fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}

fn sampler(origin: ExactRatio, step: ExactRatio, selection: std::ops::Range<i64>) -> Resampler {
    Resampler::new(
        ResampleRecipe::new(
            selection,
            origin,
            AudioSample(0),
            step,
            AudioSample(0)..AudioSample(1024),
        )
        .unwrap(),
        StereoMatrix::new(AudioChannelLayout::Native {
            channels: 2,
            mask: 3,
        })
        .unwrap(),
    )
}

fn window(
    sampler: &Resampler,
    start: i64,
    count: u32,
    signal: impl Fn(i64) -> [f32; 2],
) -> Option<PcmWindow> {
    sampler
        .required_source_range(AudioSample(start), count)
        .unwrap()
        .map(|range| PcmWindow {
            start: range.start,
            samples: range.flat_map(signal).collect(),
        })
}

fn render(
    sampler: &Resampler,
    start: i64,
    count: u32,
    signal: impl Fn(i64) -> [f32; 2],
) -> Vec<[f32; 2]> {
    sampler
        .render(
            AudioSample(start),
            count,
            window(sampler, start, count, signal),
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples
}

fn impulses(at: i64) -> [f32; 2] {
    [
        if at.rem_euclid(17) == 0 { 0.75 } else { 0.0 },
        (at.rem_euclid(13) as f32 - 6.0) / 16.0,
    ]
}

#[test]
fn partitions_and_isolated_seeks_are_bit_equal_for_fractional_negative_and_large_origins() {
    for (origin, step, base) in [
        (ratio(-17, 7), ratio(147, 160), 0),
        (ratio(1, 2), ExactRatio::ONE, 0),
        (ratio(3003, 7), ratio(1001, 300), 0),
        (
            ExactRatio::integer(9_007_199_254_740_992)
                .checked_add(ratio(1, 3))
                .unwrap(),
            ratio(7, 8),
            9_007_199_254_740_992,
        ),
    ] {
        let sampler = sampler(origin, step, base - 10_000..base + 10_000);
        let source = |at| impulses(at - base);
        let whole = render(&sampler, 0, 256, source);
        let mut divided = Vec::new();
        let mut position = 0;
        for count in [1, 31, 3, 97, 2, 122] {
            divided.extend(render(&sampler, position, count, source));
            position += i64::from(count);
        }
        assert_eq!(whole, divided);
        for at in [255, 0, 128, 3, 97, 254] {
            assert_eq!(render(&sampler, at, 1, source)[0], whole[at as usize]);
        }
    }
}

#[test]
fn translating_huge_signed_source_and_output_clocks_preserves_fractional_phase() {
    let step = ratio(147, 160);
    let baseline = sampler(ratio(-1, 3), step, -1000..1000);
    let expected = render(&baseline, 0, 256, impulses);
    for base in [i64::MIN + 10_000, i64::MAX - 10_000] {
        let output = i64::MAX - 1000;
        let shifted = Resampler::new(
            ResampleRecipe::new(
                base - 1000..base + 1000,
                ExactRatio::integer(base).checked_add(ratio(-1, 3)).unwrap(),
                AudioSample(output),
                step,
                AudioSample(output)..AudioSample(output + 512),
            )
            .unwrap(),
            baseline.matrix().clone(),
        );
        assert_eq!(
            render(&shifted, output, 256, |at| impulses(at - base)),
            expected
        );
    }
}

#[test]
fn integer_unity_is_exact_and_zero_extension_never_reads_outside_authored_trim() {
    let sampler = sampler(ExactRatio::integer(-3), ExactRatio::ONE, 0..7);
    assert_eq!(
        sampler.required_source_range(AudioSample(0), 16).unwrap(),
        Some(0..7)
    );
    let actual = render(&sampler, 0, 16, impulses);
    for (index, frame) in actual.iter().enumerate() {
        assert_eq!(
            *frame,
            if (3..10).contains(&index) {
                impulses(index as i64 - 3)
            } else {
                [0.0; 2]
            }
        );
    }
    assert_eq!(
        sampler.required_source_range(AudioSample(16), 16).unwrap(),
        None
    );
    assert_eq!(
        render(&sampler, 16, 16, |_| panic!("no excluded reads")),
        vec![[0.0; 2]; 16]
    );
    let fractional = self::sampler(ratio(-1, 2), ExactRatio::ONE, 0..1);
    assert_eq!(
        fractional
            .required_source_range(AudioSample(0), 256)
            .unwrap(),
        Some(0..1)
    );
    let response = render(&fractional, 0, 256, |_| [1.0, 0.0]);
    assert_eq!(response[0], response[1]);
    assert!(response[0][0] > 0.6 && response[0][0] < 0.65);
    assert!(response.iter().all(|sample| sample[1] == 0.0));
    assert!(response[129..].iter().all(|sample| *sample == [0.0; 2]));
}

#[test]
fn actual_tones_preserve_passband_phase_and_reject_aliases_and_images() {
    for step in [
        ratio(147, 160),
        ExactRatio::ONE,
        ratio(2, 1),
        ratio(8, 1),
        ratio(64, 1),
    ] {
        let speed = step.numerator() as f64 / step.denominator() as f64;
        for phase in [ratio(0, 1), ratio(1, 3)] {
            let sampler = sampler(phase, step, -20_000..100_000);
            for frequency in [0.025 / speed.max(1.0), 0.45 / speed.max(1.0)] {
                let output = render(&sampler, 0, 64, |at| {
                    [(TAU * frequency * at as f64 + 0.37).sin() as f32, 0.0]
                });
                let max_error = output
                    .iter()
                    .enumerate()
                    .map(|(index, frame)| {
                        let position = phase.numerator() as f64 / phase.denominator() as f64
                            + index as f64 * speed;
                        (f64::from(frame[0]) - (TAU * frequency * position + 0.37).sin()).abs()
                    })
                    .fold(0.0, f64::max);
                assert!(
                    max_error < 3e-6,
                    "step={step:?} phase={phase:?} frequency={frequency} error={max_error}"
                );
                assert!(output.iter().all(|frame| frame[1] == 0.0));
            }
            if speed > 1.0 {
                for frequency in [0.5 / speed, 0.7 / speed] {
                    let output = render(&sampler, 0, 64, |at| {
                        [(TAU * frequency * at as f64 + 0.37).sin() as f32; 2]
                    });
                    let peak = output
                        .iter()
                        .map(|frame| f64::from(frame[0]).abs())
                        .fold(0.0, f64::max);
                    assert!(
                        peak < 3e-6,
                        "alias step={step:?} phase={phase:?} frequency={frequency} peak={peak}"
                    );
                }
            }
        }
    }
    // A low input tone upsampled 4x: direct expected-wave error also bounds
    // unwanted images, rather than testing only filter coefficients.
    let sampler = sampler(ratio(1, 7), ratio(1, 4), -1000..1000);
    let output = render(&sampler, 0, 256, |at| {
        [(TAU * 0.4 * at as f64).cos() as f32; 2]
    });
    for (index, frame) in output.iter().enumerate() {
        let expected = (TAU * 0.4 * (1.0 / 7.0 + index as f64 / 4.0)).cos();
        assert!((f64::from(frame[0]) - expected).abs() < 3e-6);
    }
}

#[test]
fn dc_calibration_is_independent_of_level_and_does_not_renormalize_trim_edges() {
    for step in [ratio(1, 64), ratio(147, 160), ratio(64, 1)] {
        let sampler = sampler(ratio(1, 3), step, -20_000..100_000);
        let quiet = render(&sampler, 0, 16, |_| [0.125, -0.25]);
        let louder = render(&sampler, 0, 16, |_| [0.5, -1.0]);
        for (quiet, louder) in quiet.iter().zip(louder) {
            assert_eq!(*quiet, [0.125, -0.25]);
            assert_eq!(louder, [quiet[0] * 4.0, quiet[1] * 4.0]);
        }
    }
    let trimmed = sampler(ratio(1, 2), ExactRatio::ONE, 0..1);
    assert!(render(&trimmed, 0, 1, |_| [1.0; 2])[0][0] < 0.65);
}

#[test]
fn source_halo_is_bounded_and_missing_or_invalid_pcm_is_never_substituted() {
    let sampler = sampler(ratio(-1, 7), ratio(64, 1), -100_000..100_000);
    let range = sampler
        .required_source_range(AudioSample(0), 256)
        .unwrap()
        .unwrap();
    assert!(range.end - range.start <= i64::from(MAX_SOURCE_FRAMES));
    for input in [
        None,
        Some(PcmWindow {
            start: range.start,
            samples: Vec::new(),
        }),
        Some(PcmWindow {
            start: range.start + 1,
            samples: vec![0.0; (range.end - range.start) as usize * 2],
        }),
    ] {
        assert!(matches!(
            sampler.render(AudioSample(0), 256, input, &AtomicBool::new(false)),
            Err(PreparationError::InvalidSamples)
        ));
    }
    for invalid in [f32::NAN, f32::INFINITY, -16.01] {
        assert!(matches!(
            sampler.render(
                AudioSample(0),
                1,
                window(&sampler, 0, 1, |_| [invalid, 0.0]),
                &AtomicBool::new(false)
            ),
            Err(PreparationError::InvalidSamples)
        ));
    }
    assert!(matches!(
        sampler.render(AudioSample(0), 256, None, &AtomicBool::new(true)),
        Err(PreparationError::Cancelled)
    ));
    for (start, count) in [(0, 0), (0, 257), (-1, 1), (1024, 1), (i64::MAX, 1)] {
        assert!(
            sampler
                .required_source_range(AudioSample(start), count)
                .is_err()
        );
    }
}

#[test]
fn invalid_recipes_and_arithmetic_overflow_fail_without_clamping() {
    for step in [ExactRatio::ZERO, ratio(-1, 1), ratio(1, 65), ratio(65, 1)] {
        assert!(
            ResampleRecipe::new(
                0..100,
                ExactRatio::ZERO,
                AudioSample(0),
                step,
                AudioSample(0)..AudioSample(16)
            )
            .is_err()
        );
    }
    assert!(
        ResampleRecipe::new(
            0..0,
            ExactRatio::ZERO,
            AudioSample(0),
            ExactRatio::ONE,
            AudioSample(0)..AudioSample(16)
        )
        .is_err()
    );
    assert!(
        ResampleRecipe::new(
            0..1,
            ratio(i128::MAX, 1),
            AudioSample(0),
            ExactRatio::ONE,
            AudioSample(0)..AudioSample(16)
        )
        .is_err()
    );
    assert!(
        ResampleRecipe::new(
            0..1,
            ExactRatio::integer(i64::MAX),
            AudioSample(0),
            ExactRatio::ONE,
            AudioSample(0)..AudioSample(16)
        )
        .is_err()
    );
}
