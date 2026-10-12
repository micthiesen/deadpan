//! Numerical qualification only; this does not exercise the authored mixer.
use std::{hint::black_box, sync::atomic::AtomicBool, time::Instant};

use deadpan_audio::{FiniteWetEffect, finite_wet_coefficients, wet_effect_descriptor};
use deadpan_core::TailEffect;
use deadpan_dsp::CAUSAL_FILTER_ENGINE_ID;
use serde_json::json;
use sha2::{Digest, Sha256};

fn hash(channels: &[Vec<f64>; 2]) -> String {
    let mut hash = Sha256::new();
    for channel in channels {
        for value in channel {
            hash.update(value.to_le_bytes());
        }
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn main() {
    let cancel = AtomicBool::new(false);
    let mut reports = Vec::new();
    for effect in [TailEffect::Reverb, TailEffect::Delay] {
        let descriptor = wet_effect_descriptor(effect);
        let kernel = finite_wet_coefficients(effect, &cancel).unwrap();
        let start = Instant::now();
        let mut processor = FiniteWetEffect::new(effect, &cancel).unwrap();
        let construction_ns = start.elapsed().as_nanos();
        let input: [Vec<_>; 2] = std::array::from_fn(|channel| {
            (0..processor.input_frames())
                .map(|i| ((i * (43 + 18 * channel) % 1009) as f64 - 504.0) / 1000.0)
                .collect()
        });
        let mut left = vec![0.0; descriptor.output_frames];
        let mut right = left.clone();
        let mut timings = Vec::with_capacity(101);
        for _ in 0..101 {
            let start = Instant::now();
            processor
                .process_tile(
                    black_box(&input[0]),
                    black_box(&input[1]),
                    &mut left,
                    &mut right,
                    &cancel,
                )
                .unwrap();
            timings.push(start.elapsed().as_nanos());
            black_box((&left, &right));
        }
        let actual = [left, right];
        let mut maximum_error = 0.0_f64;
        // Include both edges, packed-bin-sensitive periodic input, and interior
        // samples. Direct f64 summation is independent of the native FFT.
        let probes = [0, 1, 2, 127, 511, 1023, 2047, 3071, 4094, 4095];
        for channel in 0..2 {
            for frame in probes {
                let expected: f64 = kernel[channel]
                    .iter()
                    .enumerate()
                    .map(|(lag, weight)| {
                        input[channel][descriptor.history_frames + frame - lag] * weight
                    })
                    .sum();
                maximum_error = maximum_error.max((actual[channel][frame] - expected).abs());
            }
        }
        assert!(maximum_error < 2e-12);
        let mut cold = FiniteWetEffect::new(effect, &cancel).unwrap();
        let mut cold_left = vec![0.0; descriptor.output_frames];
        let mut cold_right = cold_left.clone();
        cold.process_tile(
            &input[0],
            &input[1],
            &mut cold_left,
            &mut cold_right,
            &cancel,
        )
        .unwrap();
        assert_eq!([cold_left, cold_right], actual);
        timings.sort_unstable();
        reports.push(json!({
            "effect": effect,
            "algorithm": descriptor.algorithm_id,
            "history_frames": descriptor.history_frames,
            "tail_frames": descriptor.tail_frames,
            "latency_frames": descriptor.latency_frames,
            "tile_frames": descriptor.output_frames,
            "kernel_sha256_planar_f64le": hash(&kernel),
            "input_sha256_planar_f64le": hash(&input),
            "output_sha256_planar_f64le": hash(&actual),
            "preparation_admission_bytes": FiniteWetEffect::preparation_bytes_bound(effect),
            "caller_input_output_bytes": (processor.input_frames() + descriptor.output_frames) * 16,
            "construction_ns": construction_ns,
            "measured_tiles": timings.len(),
            "tile_p50_ns": timings[50], "tile_p99_ns": timings[99], "tile_max_ns": timings[100],
            "maximum_direct_error": maximum_error,
            "direct_samples_compared": probes.len() * 2,
            "cold_warm_bit_identical": true,
        }));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "format": "deadpan-finite-wet-probe-1", "native_engine": CAUSAL_FILTER_ENGINE_ID,
            "scope": "numerical kernels; no authored mixer, media, output device or export",
            "cases": reports,
        }))
        .unwrap()
    );
}
