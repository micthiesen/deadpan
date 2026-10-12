//! Numerical EQ response evidence. No editor, device or export qualification.
use std::sync::atomic::AtomicBool;

use deadpan_audio::{EQ_FILTER_ID, Equalizer};
use deadpan_core::{EqCutoffHz, EqualizerRecipe, GainDb};
use deadpan_dsp::{FINITE_FILTER_RADIUS, FINITE_FILTER_TAPS};
use serde_json::json;

fn response(kernel: &[f64; FINITE_FILTER_TAPS], hz: f64) -> (f64, f64) {
    let mut real = 0.0;
    let mut imaginary = 0.0;
    for (index, &weight) in kernel.iter().enumerate() {
        let phase =
            std::f64::consts::TAU * hz * (index as f64 - FINITE_FILTER_RADIUS as f64) / 48000.0;
        real += weight * phase.cos();
        imaginary += weight * phase.sin();
    }
    (real, imaginary)
}

fn main() {
    let cases = [
        ("flat", [0, 0, 0], None, None),
        ("three_bands", [6000, -6000, 3000], None, None),
        ("speech_band", [0, 0, 0], Some(300), Some(5000)),
        ("lowest_high_cut", [0, 0, 0], None, Some(80)),
        ("highest_low_cut", [0, 0, 0], Some(20000), None),
        ("narrow_passband", [0, 0, 0], Some(1000), Some(1080)),
    ];
    let mut reports = Vec::new();
    for (name, gains, low, high) in cases {
        let recipe = EqualizerRecipe::new(
            gains.map(|value| GainDb::new(value).unwrap()),
            low.map(|hz| EqCutoffHz::new(hz).unwrap()),
            high.map(|hz| EqCutoffHz::new(hz).unwrap()),
        )
        .unwrap();
        let eq = Equalizer::new(recipe, &AtomicBool::new(false)).unwrap();
        let kernel = eq.coefficients();
        let mut hasher = blake3::Hasher::new();
        for value in kernel {
            hasher.update(&value.to_le_bytes());
        }
        let mut frequencies: Vec<_> = (0..=300)
            .map(|index| 20.0 * 1200.0_f64.powf(f64::from(index) / 300.0))
            .collect();
        frequencies.extend([
            0.0, 80.0, 100.0, 170.0, 250.0, 330.0, 920.0, 1000.0, 1040.0, 1080.0, 1160.0, 3920.0,
            4000.0, 4080.0, 12000.0, 19920.0, 20000.0, 20080.0, 24000.0,
        ]);
        frequencies.sort_by(f64::total_cmp);
        frequencies.dedup();
        let values: Vec<_> = frequencies
            .into_iter()
            .map(|hz| {
                let (real, imaginary) = response(kernel, hz);
                assert!(real.is_finite() && imaginary.is_finite());
                json!({"hz":hz,"real":real,"imaginary":imaginary,
                   "amplitude":real.hypot(imaginary)})
            })
            .collect();
        reports.push(json!({"name":name,"recipe":recipe,
            "kernel_blake3":hasher.finalize().to_hex().to_string(),
            "maximum_coefficient":kernel.iter().map(|value| value.abs()).fold(0.0,f64::max),
            "response":values}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "engine":EQ_FILTER_ID,"sample_rate":48000,"taps":FINITE_FILTER_TAPS,
            "history_frames":FINITE_FILTER_RADIUS,"look_ahead_frames":FINITE_FILTER_RADIUS,
            "compensated_delay_frames":0,"editor_integrated":false,"cases":reports,
        }))
        .unwrap()
    );
}
