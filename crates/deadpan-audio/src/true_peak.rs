//! Streaming 48 kHz stereo peak measurement using the BS.1770-5 Annex 2 FIR.
//! This measures PCM; it does not change gain or enforce a mastering ceiling.

use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use crate::{MAX_INPUT_MAGNITUDE, MAX_OUTPUT_FRAMES};

pub const TRUE_PEAK_ID: &str = "deadpan-bs1770-5-48k-4x-48tap-v1";
pub const MAX_TRUE_PEAK_FRAMES: u64 = 48_000 * 86_400;

// ITU-R BS.1770-5 (November 2023), Annex 2, pp. 19-20. Each row is
// one base-rate tap and each column one of the four interpolation phases.
// https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.1770-5-202311-I!!PDF-E.pdf
const FIR: [[f64; 4]; 12] = [
    [
        0.001708984375,
        -0.0291748046875,
        -0.0189208984375,
        -0.00830078125,
    ],
    [0.010986328125, 0.029296875, 0.0330810546875, 0.014892578125],
    [
        -0.0196533203125,
        -0.0517578125,
        -0.0582275390625,
        -0.026611328125,
    ],
    [0.033203125, 0.089111328125, 0.1015625, 0.047607421875],
    [
        -0.0594482421875,
        -0.16650390625,
        -0.2003173828125,
        -0.102294921875,
    ],
    [
        0.1373291015625,
        0.465087890625,
        0.77978515625,
        0.97216796875,
    ],
    [
        0.97216796875,
        0.77978515625,
        0.465087890625,
        0.1373291015625,
    ],
    [
        -0.102294921875,
        -0.2003173828125,
        -0.16650390625,
        -0.0594482421875,
    ],
    [0.047607421875, 0.1015625, 0.089111328125, 0.033203125],
    [
        -0.026611328125,
        -0.0582275390625,
        -0.0517578125,
        -0.0196533203125,
    ],
    [0.014892578125, 0.0330810546875, 0.029296875, 0.010986328125],
    [
        -0.00830078125,
        -0.0189208984375,
        -0.0291748046875,
        0.001708984375,
    ],
];

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TruePeakError {
    #[error("peak measurement must have a frame budget within 1..=4147200000 (24 hours at 48 kHz)")]
    InvalidLimit,
    #[error("peak measurement blocks must contain 1..=256 stereo frames")]
    InvalidBlock,
    #[error("peak measurement accepts only finite samples with magnitude at most 16")]
    InvalidSamples,
    #[error("peak measurement would exceed its admitted frame budget")]
    FrameBudgetExceeded,
    #[error("peak measurement was cancelled")]
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TruePeakReport {
    pub algorithm: &'static str,
    pub measured_frames: u64,
    pub sample_peak: [f64; 2],
    pub true_peak: [f64; 2],
    /// None denotes digital silence, not a fabricated finite dB value.
    pub true_peak_dbtp: [Option<f64>; 2],
}

/// Fixed memory and at most 256 input frames per admission. Run on a preparation
/// or analysis worker, not the device callback. Blocks retain one continuous FIR
/// history; only `finish` appends the finite zero context after the signal.
pub struct TruePeakMeter {
    max_frames: u64,
    measured_frames: u64,
    history: [[f64; 2]; 12],
    next: usize,
    sample_peak: [f64; 2],
    true_peak: [f64; 2],
}

impl TruePeakMeter {
    pub fn new(max_frames: u64) -> Result<Self, TruePeakError> {
        if max_frames == 0 || max_frames > MAX_TRUE_PEAK_FRAMES {
            return Err(TruePeakError::InvalidLimit);
        }
        Ok(Self {
            max_frames,
            measured_frames: 0,
            history: [[0.0; 2]; 12],
            next: 0,
            sample_peak: [0.0; 2],
            true_peak: [0.0; 2],
        })
    }

    /// Admission is atomic: validate the complete bounded block before changing
    /// history or counts. Cancellation is observed between these transactions.
    pub fn push(
        &mut self,
        samples: &[[f32; 2]],
        cancelled: &AtomicBool,
    ) -> Result<(), TruePeakError> {
        if cancelled.load(Ordering::Relaxed) {
            return Err(TruePeakError::Cancelled);
        }
        if samples.is_empty() || samples.len() > MAX_OUTPUT_FRAMES as usize {
            return Err(TruePeakError::InvalidBlock);
        }
        let end = self
            .measured_frames
            .checked_add(samples.len() as u64)
            .ok_or(TruePeakError::FrameBudgetExceeded)?;
        if end > self.max_frames {
            return Err(TruePeakError::FrameBudgetExceeded);
        }
        if samples
            .iter()
            .flatten()
            .any(|x| !x.is_finite() || x.abs() > MAX_INPUT_MAGNITUDE)
        {
            return Err(TruePeakError::InvalidSamples);
        }
        for sample in samples {
            let sample = sample.map(f64::from);
            for (channel, value) in sample.iter().enumerate() {
                self.sample_peak[channel] = self.sample_peak[channel].max(value.abs());
                // The four delayed FIR phases do not necessarily land on an
                // original sample. A true-peak reading cannot under-read those.
                self.true_peak[channel] = self.true_peak[channel].max(value.abs());
            }
            self.consume(sample);
        }
        self.measured_frames = end;
        Ok(())
    }

    fn consume(&mut self, sample: [f64; 2]) {
        self.history[self.next] = sample;
        self.next = (self.next + 1) % self.history.len();
        let mut phases = [[0.0; 2]; 4];
        let ordered = self.history[self.next..]
            .iter()
            .chain(&self.history[..self.next]);
        for (sample, coefficients) in ordered.zip(FIR) {
            for (phase, coefficient) in phases.iter_mut().zip(coefficients) {
                for (value, sample) in phase.iter_mut().zip(sample) {
                    *value += coefficient * sample;
                }
            }
        }
        for phase in phases {
            for (peak, value) in self.true_peak.iter_mut().zip(phase) {
                *peak = peak.max(value.abs());
            }
        }
    }

    /// Includes all FIR response to the final real samples. Zero context changes
    /// neither the input frame count nor the sample peak and is not output audio.
    pub fn finish(mut self) -> TruePeakReport {
        for _ in 1..self.history.len() {
            self.consume([0.0; 2]);
        }
        TruePeakReport {
            algorithm: TRUE_PEAK_ID,
            measured_frames: self.measured_frames,
            sample_peak: self.sample_peak,
            true_peak: self.true_peak,
            true_peak_dbtp: self
                .true_peak
                .map(|peak| (peak > 0.0).then(|| 20.0 * peak.log10())),
        }
    }
}
