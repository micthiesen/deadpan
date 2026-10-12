//! Finite causal stereo convolution for worker-owned effect returns.
//!
//! Each channel has its own immutable kernel. Complete declared history makes
//! a fixed output tile independent of every previous read and seek. This owns
//! no media, timing policy, authored state or output device.

use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;

use crate::ffi;

pub const CAUSAL_FILTER_OUTPUT_FRAMES: usize = 4096;
/// Eight seconds at the canonical 48 kHz, including the coefficient at lag 0.
pub const CAUSAL_FILTER_MAX_TAPS: usize = 384_001;
pub const CAUSAL_FILTER_ENGINE_ID: &str =
    "deadpan-causal-stereo-signalsmith-realfft-f64-t4096-pow2-v1";

/// Neither Send nor Sync: construct and retain on one preparation worker.
pub struct CausalStereoFilter {
    native: ffi::CausalFilterEngine,
    taps: usize,
}

impl CausalStereoFilter {
    pub fn new(left: &[f64], right: &[f64]) -> Result<Self, CausalFilterError> {
        if left.is_empty() || left.len() > CAUSAL_FILTER_MAX_TAPS || left.len() != right.len() {
            return Err(CausalFilterError::KernelLength);
        }
        if left
            .iter()
            .chain(right)
            .any(|x| !x.is_finite() || x.abs() > 16.0)
        {
            return Err(CausalFilterError::Coefficients);
        }
        Ok(Self {
            native: ffi::CausalFilterEngine::new(left, right)?,
            taps: left.len(),
        })
    }

    pub fn history_frames(&self) -> usize {
        self.taps - 1
    }

    pub fn input_frames(&self) -> usize {
        self.history_frames() + CAUSAL_FILTER_OUTPUT_FRAMES
    }

    /// Conservative admission for native arrays and the pinned portable FFT.
    /// Caller-owned input and output buffers are separate. Calculate before
    /// allocating the handle; qualification measures the maximum allocation.
    pub fn native_bytes_bound(taps: usize) -> Result<usize, CausalFilterError> {
        if !(1..=CAUSAL_FILTER_MAX_TAPS).contains(&taps) {
            return Err(CausalFilterError::KernelLength);
        }
        Ok((taps - 1 + CAUSAL_FILTER_OUTPUT_FRAMES).next_power_of_two() * 128 + 1_048_576)
    }

    /// One fixed tile. Any refusal or cancellation leaves both outputs intact;
    /// caller errors and cancellation permit a later retry with valid input.
    pub fn process_tile(
        &mut self,
        left: &[f64],
        right: &[f64],
        output_left: &mut [f64],
        output_right: &mut [f64],
        cancelled: &AtomicBool,
    ) -> Result<(), CausalFilterError> {
        if left.len() != self.input_frames() || right.len() != self.input_frames() {
            return Err(CausalFilterError::InputLength);
        }
        if output_left.len() != CAUSAL_FILTER_OUTPUT_FRAMES
            || output_right.len() != CAUSAL_FILTER_OUTPUT_FRAMES
        {
            return Err(CausalFilterError::OutputLength);
        }
        if cancelled.load(Ordering::Relaxed) {
            return Err(CausalFilterError::Cancelled);
        }
        if left
            .iter()
            .chain(right)
            .any(|x| !x.is_finite() || x.abs() > 1e100)
        {
            return Err(CausalFilterError::InvalidInput);
        }
        let mut staged_left = [0.0; CAUSAL_FILTER_OUTPUT_FRAMES];
        let mut staged_right = [0.0; CAUSAL_FILTER_OUTPUT_FRAMES];
        self.native
            .read(left, right, &mut staged_left, &mut staged_right)?;
        if cancelled.load(Ordering::Relaxed) {
            return Err(CausalFilterError::Cancelled);
        }
        if staged_left
            .iter()
            .chain(&staged_right)
            .any(|x| !x.is_finite())
        {
            return Err(CausalFilterError::NativeReport);
        }
        output_left.copy_from_slice(&staged_left);
        output_right.copy_from_slice(&staged_right);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CausalFilterError {
    #[error("causal FIR requires matching kernels of 1..384001 coefficients")]
    KernelLength,
    #[error("causal FIR coefficients must be finite with magnitude at most 16")]
    Coefficients,
    #[error("causal FIR input requires its complete history followed by 4096 core frames")]
    InputLength,
    #[error("causal FIR output requires exactly 4096 frames in each channel")]
    OutputLength,
    #[error("causal FIR input must be finite with magnitude at most 1e100")]
    InvalidInput,
    #[error("causal FIR preparation cancelled between fixed tiles")]
    Cancelled,
    #[error("native causal FIR allocation failed")]
    NativeAllocation,
    #[error("native causal FIR rejected a validated request")]
    NativeRejected,
    #[error("native causal FIR processing failed")]
    NativeFailure,
    #[error("native causal FIR produced nonfinite output")]
    NonFiniteOutput,
    #[error("native causal FIR is unusable after an earlier processing failure")]
    Poisoned,
    #[error("native causal FIR returned an invalid report")]
    NativeReport,
}
