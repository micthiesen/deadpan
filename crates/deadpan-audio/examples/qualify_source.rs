//! Headless production-kernel measurements. Run with --release; this measures
//! worker preparation, not a device callback, full mix, decode, or listening.
use std::f64::consts::TAU;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use deadpan_audio::{
    BOUNDARY_ID, MATRIX_ID, MAX_OUTPUT_FRAMES, MAX_SOURCE_FRAMES, PcmWindow, RESAMPLER_ID,
    ResampleRecipe, Resampler, StereoMatrix,
};
use deadpan_core::{AudioSample, ExactRatio};
use deadpan_media::audio_index::AudioChannelLayout;
use serde_json::json;
use sha2::{Digest, Sha256};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cancelled = AtomicBool::new(false);
    let mut cases = Vec::new();
    for (numerator, denominator, phase) in [
        (1, 64, 3),
        (147, 160, 3),
        (1, 1, 1),
        (1, 1, 3),
        (2, 1, 3),
        (8, 1, 3),
        (64, 1, 3),
    ] {
        let origin = ExactRatio::new(1, phase)?;
        let step = ExactRatio::new(numerator, denominator)?;
        let recipe = ResampleRecipe::new(
            -100_000..100_000,
            origin,
            AudioSample(0),
            step,
            AudioSample(0)..AudioSample(256),
        )?;
        let sampler = Resampler::new(
            recipe.clone(),
            StereoMatrix::new(AudioChannelLayout::Native {
                channels: 2,
                mask: 3,
            })?,
        );
        let range = sampler.required_source_range(AudioSample(0), 256)?.unwrap();
        let speed = numerator as f64 / denominator as f64;
        let frequency = 0.45 / speed.max(1.0);
        let input: Vec<f32> = range
            .clone()
            .flat_map(|at| {
                let value = (TAU * frequency * at as f64 + 0.37).sin() as f32;
                [value, -value * 0.5]
            })
            .collect();
        let mut timings = Vec::new();
        let mut hash = None;
        let mut max_error = 0.0_f64;
        for _ in 0..7 {
            let start = Instant::now();
            let output = sampler.render(
                AudioSample(0),
                256,
                Some(PcmWindow {
                    start: range.start,
                    samples: input.clone(),
                }),
                &cancelled,
            )?;
            timings.push(start.elapsed().as_secs_f64() * 1000.0);
            let bytes: Vec<u8> = output
                .samples
                .iter()
                .flatten()
                .flat_map(|value| value.to_le_bytes())
                .collect();
            let current: String = Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            if let Some(expected) = &hash {
                assert_eq!(expected, &current);
            }
            hash = Some(current);
            for (index, frame) in output.samples.iter().enumerate() {
                let expected =
                    (TAU * frequency * (1.0 / phase as f64 + index as f64 * speed) + 0.37).sin();
                max_error = max_error.max((f64::from(frame[0]) - expected).abs());
            }
        }
        timings.sort_by(f64::total_cmp);
        let stopband_peak = if speed > 1.0 {
            let input = range
                .clone()
                .flat_map(|at| [(TAU * 0.5 / speed * at as f64 + 0.37).sin() as f32; 2])
                .collect();
            let output = sampler.render(
                AudioSample(0),
                256,
                Some(PcmWindow {
                    start: range.start,
                    samples: input,
                }),
                &cancelled,
            )?;
            Some(
                output
                    .samples
                    .iter()
                    .map(|frame| frame[0].abs())
                    .fold(0.0_f32, f32::max),
            )
        } else {
            None
        };
        assert!(max_error < 3e-6);
        assert!(stopband_peak.is_none_or(|value| value < 3e-6));
        cases.push(json!({
            "recipe": recipe,
            "source_frames": range.end - range.start,
            "input_bytes": input.len() * size_of::<f32>(),
            "mixed_staging_bytes": (range.end - range.start) * 2 * 8,
            "output_sha256_f32le": hash,
            "maximum_passband_sample_error": max_error,
            "lower_nyquist_stopband_peak": stopband_peak,
            "milliseconds_sorted": timings,
            "median_ms": timings[3],
        }));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "resampler": RESAMPLER_ID,
            "matrix": MATRIX_ID,
            "boundary": BOUNDARY_ID,
            "debug_assertions": cfg!(debug_assertions),
            "sample_rate": 48000,
            "maximum_output_frames": MAX_OUTPUT_FRAMES,
            "maximum_source_frames": MAX_SOURCE_FRAMES,
            "scope": "Synthetic stereo source windows through production worker sampler; seven sequential timings per recipe include input clone, mixing and convolution. No decoding, device output, stretch, effects or listening.",
            "cases": cases,
        }))?
    );
    Ok(())
}
