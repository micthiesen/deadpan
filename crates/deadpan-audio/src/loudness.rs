//! Informational integrated loudness for the fixed 48 kHz stereo mix.
//!
//! K-weighting coefficients and gates follow ITU-R BS.1770-5, Annex 1,
//! Tables 1-3 and equations (3)-(7). EBU Tech 3341 (2023), section 2.3,
//! specifies the same 400 ms windows, 75% overlap and discarded final fragment.
//! This is worker-side analysis, not normalization or a complete EBU Mode meter.
//!
//! Sources:
//! <https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.1770-5-202311-I!!PDF-E.pdf>
//! <https://tech.ebu.ch/docs/tech/tech3341.pdf>

use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use crate::{MAX_INPUT_MAGNITUDE, MAX_OUTPUT_FRAMES};

/// Versioned filter, window and gating interpretation for analysis provenance.
pub const LOUDNESS_ID: &str = "deadpan-bs1770-5-stereo-48k-integrated-v1";
/// Twenty-four hours of 48 kHz stereo frames, not individual channel samples.
pub const MAX_LOUDNESS_FRAMES: u64 = 48_000 * 86_400;

const WINDOW_FRAMES: usize = 19_200;
const HOP_FRAMES: u64 = 4_800;
const CALIBRATION: f64 = -0.691;
const ABSOLUTE_GATE_LUFS: f64 = -70.0;

// Published 48 kHz coefficients, with a0 = 1. These are not coefficients for
// arbitrary source rates; resampling into the mix must precede this meter.
const SHELF: Coefficients = Coefficients {
    b: [1.53512485958697, -2.69169618940638, 1.19839281085285],
    a: [-1.69065929318241, 0.73248077421585],
};
const HIGH_PASS: Coefficients = Coefficients {
    b: [1.0, -2.0, 1.0],
    a: [-1.99004745483398, 0.99007225036621],
};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LoudnessError {
    #[error("loudness frame limit must be between one frame and 24 hours at 48 kHz")]
    InvalidLimit,
    #[error("loudness input must contain between 1 and 256 stereo frames")]
    InvalidBlock,
    #[error("loudness input contains a nonfinite sample or a magnitude above 16")]
    InvalidSamples,
    #[error("loudness input would exceed the admitted frame limit")]
    FrameBudgetExceeded,
    #[error("could not reserve bounded loudness analysis storage")]
    Allocation,
    #[error("loudness analysis was cancelled before the input block")]
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LoudnessReport {
    /// All admitted frames, including the trailing incomplete gating interval.
    pub measured_frames: u64,
    /// Complete 400 ms windows, starting at zero and every 100 ms thereafter.
    pub complete_blocks: u64,
    /// Windows strictly above both the absolute and the relative gate.
    pub gated_blocks: u64,
    /// None means no complete window exceeded the absolute gate.
    pub integrated_lufs: Option<f64>,
    /// Absolute-gated loudness minus 10 LU, before intersecting the two gates.
    /// This value can be below -70 LUFS. None means the absolute gate was empty.
    pub relative_gate_lufs: Option<f64>,
}

/// One continuous analysis beginning at mix sample zero, with zero filter history.
///
/// Construction reserves the power ring and every possible complete block
/// energy for the admitted duration. No input PCM is retained or modified.
/// Calls must supply contiguous ordered PCM: a seek requires a new meter and
/// replay from the same analysis origin. Consumer chunk boundaries have no effect
/// on filtering, window placement, summation order or gates.
pub struct LoudnessMeter {
    max_frames: u64,
    measured_frames: u64,
    filters: [KWeighting; 2],
    powers: Vec<f64>,
    next_power: usize,
    energies: Vec<f64>,
}

impl LoudnessMeter {
    pub fn new(max_frames: u64) -> Result<Self, LoudnessError> {
        if !(1..=MAX_LOUDNESS_FRAMES).contains(&max_frames) {
            return Err(LoudnessError::InvalidLimit);
        }
        let block_capacity = usize::try_from(complete_blocks(max_frames))
            .map_err(|_| LoudnessError::InvalidLimit)?;
        let mut powers = Vec::new();
        powers
            .try_reserve_exact(WINDOW_FRAMES)
            .map_err(|_| LoudnessError::Allocation)?;
        powers.resize(WINDOW_FRAMES, 0.0);
        let mut energies = Vec::new();
        energies
            .try_reserve_exact(block_capacity)
            .map_err(|_| LoudnessError::Allocation)?;
        Ok(Self {
            max_frames,
            measured_frames: 0,
            filters: [KWeighting::default(); 2],
            powers,
            next_power: 0,
            energies,
        })
    }

    /// Admit one bounded transaction. Cancellation is observed once at entry;
    /// after validation, all its frames are processed even if cancellation changes.
    /// Invalid or cancelled input leaves all meter state unchanged.
    ///
    /// At most one hop ends in a call. Its window sum is rebuilt from the fixed
    /// ring, so a long silence cannot inherit running-sum subtraction error.
    pub fn push(
        &mut self,
        samples: &[[f32; 2]],
        cancelled: &AtomicBool,
    ) -> Result<(), LoudnessError> {
        if cancelled.load(Ordering::Acquire) {
            return Err(LoudnessError::Cancelled);
        }
        if samples.is_empty() || samples.len() > MAX_OUTPUT_FRAMES as usize {
            return Err(LoudnessError::InvalidBlock);
        }
        let frames = u64::try_from(samples.len()).map_err(|_| LoudnessError::InvalidBlock)?;
        let end = self
            .measured_frames
            .checked_add(frames)
            .filter(|end| *end <= self.max_frames)
            .ok_or(LoudnessError::FrameBudgetExceeded)?;
        if samples
            .iter()
            .flatten()
            .any(|value| !value.is_finite() || value.abs() > MAX_INPUT_MAGNITUDE)
        {
            return Err(LoudnessError::InvalidSamples);
        }

        for frame in samples {
            let left = self.filters[0].process(f64::from(frame[0]));
            let right = self.filters[1].process(f64::from(frame[1]));
            // Both stereo channels have unit weight, independently of polarity.
            self.powers[self.next_power] = left * left + right * right;
            self.next_power += 1;
            if self.next_power == WINDOW_FRAMES {
                self.next_power = 0;
            }
            self.measured_frames += 1;
            if self.measured_frames >= WINDOW_FRAMES as u64
                && self.measured_frames.is_multiple_of(HOP_FRAMES)
            {
                let energy = self.powers.iter().copied().sum::<f64>() / WINDOW_FRAMES as f64;
                // The entire admitted capacity was reserved by new().
                self.energies.push(energy);
            }
        }
        debug_assert_eq!(self.measured_frames, end);
        Ok(())
    }

    /// Apply both gates to the retained complete block energies. This bounded
    /// final pass runs on the analysis worker; it does not pad an incomplete
    /// window, append a filter tail, normalize, or allocate report storage.
    pub fn finish(self) -> LoudnessReport {
        gated_report(self.measured_frames, &self.energies)
    }
}

fn complete_blocks(frames: u64) -> u64 {
    frames
        .checked_sub(WINDOW_FRAMES as u64)
        .map_or(0, |after_first| after_first / HOP_FRAMES + 1)
}

fn gated_report(measured_frames: u64, energies: &[f64]) -> LoudnessReport {
    let mut report = LoudnessReport {
        measured_frames,
        complete_blocks: energies.len() as u64,
        gated_blocks: 0,
        integrated_lufs: None,
        relative_gate_lufs: None,
    };
    let absolute_gate = 10.0_f64.powf((ABSOLUTE_GATE_LUFS - CALIBRATION) / 10.0);
    let mut absolute_sum = 0.0;
    let mut absolute_count = 0_u64;
    for &energy in energies {
        if energy > absolute_gate {
            absolute_sum += energy;
            absolute_count += 1;
        }
    }
    if absolute_count == 0 {
        return report;
    }
    let absolute_mean = absolute_sum / absolute_count as f64;
    report.relative_gate_lufs = Some(CALIBRATION + 10.0 * absolute_mean.log10() - 10.0);
    let relative_gate = absolute_mean / 10.0;
    let mut gated_sum = 0.0;
    for &energy in energies {
        // ITU equation (7) keeps both strict comparisons. A low relative gate
        // must never readmit a block excluded by the absolute gate.
        if energy > absolute_gate && energy > relative_gate {
            gated_sum += energy;
            report.gated_blocks += 1;
        }
    }
    // A nonempty absolute gate always retains at least its largest energy after
    // the relative gate, since that threshold is one tenth of their mean.
    report.integrated_lufs =
        Some(CALIBRATION + 10.0 * (gated_sum / report.gated_blocks as f64).log10());
    report
}

#[derive(Clone, Copy)]
struct Coefficients {
    b: [f64; 3],
    a: [f64; 2],
}

#[derive(Clone, Copy, Default)]
struct Biquad {
    z1: f64,
    z2: f64,
}

impl Biquad {
    fn process(&mut self, input: f64, coefficients: Coefficients) -> f64 {
        // Transposed direct form II, with a fixed operation order in f64.
        let output = coefficients.b[0] * input + self.z1;
        self.z1 = coefficients.b[1] * input - coefficients.a[0] * output + self.z2;
        self.z2 = coefficients.b[2] * input - coefficients.a[1] * output;
        output
    }
}

#[derive(Clone, Copy, Default)]
struct KWeighting {
    shelf: Biquad,
    high_pass: Biquad,
}

impl KWeighting {
    fn process(&mut self, input: f64) -> f64 {
        self.high_pass
            .process(self.shelf.process(input, SHELF), HIGH_PASS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_gates_are_strict_and_the_absolute_gate_remains_in_force() {
        let absolute = 10.0_f64.powf((-70.0 + 0.691) / 10.0);
        let report = gated_report(0, &[absolute / 2.0, absolute, absolute * 2.0]);
        assert_eq!(report.complete_blocks, 3);
        assert_eq!(report.gated_blocks, 1);
        assert!((report.integrated_lufs.unwrap() - (-70.0 + 10.0 * 2.0_f64.log10())).abs() < 1e-12);
        assert!(report.relative_gate_lufs.unwrap() < -70.0);

        // The mean is exactly 10, so the relative threshold is exactly 1.
        let report = gated_report(0, &[1.0, 19.0]);
        assert_eq!(report.gated_blocks, 1);
        assert!(
            (report.integrated_lufs.unwrap() - (-0.691 + 10.0 * 19.0_f64.log10())).abs() < 1e-12
        );
    }

    #[test]
    fn full_day_capacity_and_window_edges_are_exact() {
        assert_eq!(MAX_LOUDNESS_FRAMES, 4_147_200_000);
        assert_eq!(complete_blocks(0), 0);
        assert_eq!(complete_blocks(19_199), 0);
        assert_eq!(complete_blocks(19_200), 1);
        assert_eq!(complete_blocks(23_999), 1);
        assert_eq!(complete_blocks(24_000), 2);
        assert_eq!(complete_blocks(MAX_LOUDNESS_FRAMES), 863_997);
    }
}
