//! Standalone kernel qualification, deliberately independent of playback.
//! Build first, then run without concurrent compilers or device measurements.
use std::{hint::black_box, sync::atomic::AtomicBool, time::Instant};

use deadpan_dsp::{
    FINITE_FILTER_ENGINE_ID, FINITE_FILTER_INPUT_FRAMES as INPUT,
    FINITE_FILTER_OUTPUT_FRAMES as OUTPUT, FINITE_FILTER_RADIUS as RADIUS,
    FINITE_FILTER_TAPS as TAPS, FiniteStereoFilter,
};
use sha2::{Digest, Sha256};

fn percentile(values: &mut [u128], percent: usize) -> u128 {
    values.sort_unstable();
    values[(values.len() * percent).div_ceil(100).saturating_sub(1)]
}

fn main() {
    // A 4 kHz Blackman-windowed sinc at 48 kHz, normalized at DC. This is a
    // qualification fixture, not the product's as-yet unauthored EQ recipe.
    let mut weights: [f64; TAPS] = std::array::from_fn(|index| {
        let offset = index as f64 - RADIUS as f64;
        let phase = std::f64::consts::TAU * index as f64 / (TAPS - 1) as f64;
        let window = 0.42 - 0.5 * phase.cos() + 0.08 * (2.0 * phase).cos();
        let sinc = if offset == 0.0 {
            1.0 / 6.0
        } else {
            (std::f64::consts::PI * offset / 6.0).sin() / (std::f64::consts::PI * offset)
        };
        window * sinc
    });
    let dc: f64 = weights.iter().sum();
    for value in &mut weights {
        *value /= dc;
    }
    let mut hash = Sha256::new();
    for value in weights {
        hash.update(value.to_le_bytes());
    }
    let kernel_sha256: String = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let left: Vec<_> = (0..INPUT)
        .map(|at| {
            0.3 * (std::f64::consts::TAU * 1000.0 * at as f64 / 48000.0).sin()
                + 0.2 * (std::f64::consts::TAU * 12000.0 * at as f64 / 48000.0).cos()
        })
        .collect();
    let right: Vec<_> = left.iter().rev().map(|value| -*value).collect();
    let mut output_left = vec![0.0; OUTPUT];
    let mut output_right = vec![0.0; OUTPUT];
    let cancel = AtomicBool::new(false);
    let cold_start = Instant::now();
    let mut filter = FiniteStereoFilter::new(&weights).unwrap();
    filter
        .process_tile(&left, &right, &mut output_left, &mut output_right, &cancel)
        .unwrap();
    let first_construct_and_tile_ns = cold_start.elapsed().as_nanos();
    let mut maximum_error = 0.0_f64;
    for (input, output) in [(&left, &output_left), (&right, &output_right)] {
        for (frame, actual) in output.iter().enumerate() {
            let direct: f64 = input[frame..frame + TAPS]
                .iter()
                .zip(weights)
                .map(|(x, h)| x * h)
                .sum();
            maximum_error = maximum_error.max((actual - direct).abs());
        }
    }
    assert!(
        maximum_error < 2e-12,
        "direct oracle mismatch: {maximum_error}"
    );
    let mut timings = Vec::with_capacity(5000);
    for _ in 0..5000 {
        let start = Instant::now();
        filter
            .process_tile(
                black_box(&left),
                black_box(&right),
                &mut output_left,
                &mut output_right,
                &cancel,
            )
            .unwrap();
        timings.push(start.elapsed().as_nanos());
        black_box((&output_left, &output_right));
    }
    let p50 = percentile(&mut timings, 50);
    let p95 = percentile(&mut timings, 95);
    let maximum = *timings.iter().max().unwrap();
    let total: u128 = timings.iter().sum();
    println!(
        "{{\"engine\":\"{FINITE_FILTER_ENGINE_ID}\",\"kernel_sha256\":\"{kernel_sha256}\",\"first_construct_and_tile_ns\":{first_construct_and_tile_ns},\"tile_count\":{},\"frames_per_tile\":{OUTPUT},\"p50_tile_ns\":{p50},\"p95_tile_ns\":{p95},\"maximum_tile_ns\":{maximum},\"total_tile_ns\":{total},\"maximum_direct_error\":{maximum_error:e},\"device_output\":false}}",
        timings.len()
    );
}
