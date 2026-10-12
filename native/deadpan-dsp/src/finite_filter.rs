//! Bounded signed stereo FIR evaluation, independent of media and ownership.
//!
//! The caller supplies complete context in the effect's clock, gates forbidden
//! tails, and uses one fixed absolute tile grid for both seeks and continuous
//! playback. This adapter does not infer silence, pad missing history, or choose
//! EQ coefficients. A symmetric kernel has compensated delay: core output keeps
//! its original sample label, using 2048 samples of context on either side.

use std::sync::atomic::{AtomicBool, Ordering};

use thiserror::Error;

use crate::ffi;

pub const FINITE_FILTER_RADIUS: usize = 2048;
pub const FINITE_FILTER_TAPS: usize = 2 * FINITE_FILTER_RADIUS + 1;
pub const FINITE_FILTER_OUTPUT_FRAMES: usize = 1024;
pub const FINITE_FILTER_INPUT_FRAMES: usize =
    FINITE_FILTER_OUTPUT_FRAMES + 2 * FINITE_FILTER_RADIUS;
pub const MAX_FILTER_COEFFICIENT_MAGNITUDE: f64 = 16.0;
/// Numeric admission only, never an output ceiling or normalization policy.
/// Leaves ample f64 headroom for the bounded FFT, products and inverse FFT.
pub const MAX_FILTER_SAMPLE_MAGNITUDE: f64 = 1e100;
pub const FINITE_FILTER_ENGINE_ID: &str =
    "deadpan-finite-filter-signalsmith-realfft-f64-t1024-r2048-n5120-v1";

/// One worker's immutable kernel and reusable native FFT work space. No input
/// history is retained between tiles, and processing allocates no heap memory.
/// The handle is deliberately neither Send nor Sync. Construct on the worker.
pub struct FiniteStereoFilter {
    native: ffi::FilterEngine,
}

impl FiniteStereoFilter {
    /// Coefficients are ordered by input offset -2048 through +2048. Native
    /// construction copies the complete finite kernel into its owned spectrum.
    pub fn new(coefficients: &[f64; FINITE_FILTER_TAPS]) -> Result<Self, FilterError> {
        if coefficients
            .iter()
            .any(|value| !value.is_finite() || value.abs() > MAX_FILTER_COEFFICIENT_MAGNITUDE)
        {
            return Err(FilterError::Coefficients);
        }
        Ok(Self {
            native: ffi::FilterEngine::new(coefficients)?,
        })
    }

    /// Evaluate exactly one 1024-frame core with real 2048-frame context on
    /// either side, retaining signed f64 stereo throughout. The same kernel
    /// applies independently to each channel; there is no crossfeed or limiting.
    /// Cancellation is checked before and after the bounded synchronous tile.
    /// Any error leaves both caller outputs unchanged and permits no partial
    /// publication. Input/length errors and cancellation do not poison a plan.
    pub fn process_tile(
        &mut self,
        left: &[f64],
        right: &[f64],
        output_left: &mut [f64],
        output_right: &mut [f64],
        cancelled: &AtomicBool,
    ) -> Result<(), FilterError> {
        let left: &[f64; FINITE_FILTER_INPUT_FRAMES] =
            left.try_into().map_err(|_| FilterError::InputLength)?;
        let right: &[f64; FINITE_FILTER_INPUT_FRAMES] =
            right.try_into().map_err(|_| FilterError::InputLength)?;
        if output_left.len() != FINITE_FILTER_OUTPUT_FRAMES
            || output_right.len() != FINITE_FILTER_OUTPUT_FRAMES
        {
            return Err(FilterError::OutputLength);
        }
        if cancelled.load(Ordering::Relaxed) {
            return Err(FilterError::Cancelled);
        }
        if left
            .iter()
            .chain(right)
            .any(|value| !value.is_finite() || value.abs() > MAX_FILTER_SAMPLE_MAGNITUDE)
        {
            return Err(FilterError::InvalidInput);
        }
        let mut staged_left = [0.0; FINITE_FILTER_OUTPUT_FRAMES];
        let mut staged_right = [0.0; FINITE_FILTER_OUTPUT_FRAMES];
        self.native
            .read(left, right, &mut staged_left, &mut staged_right)?;
        if cancelled.load(Ordering::Relaxed) {
            return Err(FilterError::Cancelled);
        }
        if staged_left
            .iter()
            .chain(&staged_right)
            .any(|value| !value.is_finite())
        {
            return Err(FilterError::NativeReport);
        }
        output_left.copy_from_slice(&staged_left);
        output_right.copy_from_slice(&staged_right);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FilterError {
    #[error("FIR coefficients must be finite with magnitude at most 16")]
    Coefficients,
    #[error("FIR input requires exactly 5120 frames in each channel")]
    InputLength,
    #[error("FIR output requires exactly 1024 frames in each channel")]
    OutputLength,
    #[error("FIR input must be finite with magnitude at most 1e100")]
    InvalidInput,
    #[error("FIR preparation cancelled between fixed tiles")]
    Cancelled,
    #[error("native FIR allocation failed")]
    NativeAllocation,
    #[error("native FIR rejected a validated request")]
    NativeRejected,
    #[error("native FIR processing failed")]
    NativeFailure,
    #[error("native FIR produced nonfinite output")]
    NonFiniteOutput,
    #[error("native FIR is unusable after an earlier processing failure")]
    Poisoned,
    #[error("native FIR returned an invalid report")]
    NativeReport,
}
