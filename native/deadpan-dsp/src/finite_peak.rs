//! Fixed-size finite fractional-phase peak analysis for one 1024-frame tile.
//!
//! The caller supplies the original-rate tile plus 64 frames of real adjacent
//! context on each side. Coefficients are authored and versioned by the audio
//! layer; this adapter only evaluates their finite FIR rows. It does not apply
//! gain or implement a limiter.

use std::sync::atomic::{AtomicBool, Ordering};

use thiserror::Error;

use crate::{MAX_INPUT_PEAK, ffi};

pub const FIXED_PEAK_ROWS: usize = 15;
pub const FIXED_PEAK_TAPS: usize = 129;
pub const FIXED_PEAK_RADIUS: usize = 64;
pub const FIXED_PEAK_INPUT_FRAMES: usize = 1152;
pub const FIXED_PEAK_OUTPUT_FRAMES: usize = 1024;
pub const MAX_PEAK_COEFFICIENT_MAGNITUDE: f64 = 16.0;

/// Versions the portable f64 FFT and the 1024-frame, 64-frame-halo bank shape.
/// Cache keys must also include the actual coefficient table and source PCM.
pub const FIXED_PEAK_ENGINE_ID: &str =
    "deadpan-finite-peak-signalsmith-realfft-f64-t1024-r64-n1280-v1";

/// A worker-owned fixed phase bank. Construction precomputes its 15 FIR spectra;
/// each tile call reuses those kernels and its FFT work buffers without heap
/// allocation. The opaque native plan must stay on its construction thread.
pub struct FixedPeakBank {
    native: ffi::PeakEngine,
}

impl FixedPeakBank {
    /// Build a bank from 15 rows, each containing taps ordered from offset -64
    /// through +64. The table is validated and copied into native-owned memory.
    pub fn new(
        coefficients: [[f64; FIXED_PEAK_TAPS]; FIXED_PEAK_ROWS],
    ) -> Result<Self, PeakBankError> {
        if coefficients
            .iter()
            .flatten()
            .any(|value| !value.is_finite() || value.abs() > MAX_PEAK_COEFFICIENT_MAGNITUDE)
        {
            return Err(PeakBankError::Coefficients);
        }
        let native = ffi::PeakEngine::new(&coefficients)?;
        Ok(Self { native })
    }

    /// Analyze a tile and publish the linked (max of both channels and all 15
    /// phases) absolute peak for each core frame. Inputs have exactly 1152
    /// samples: 64 left-context frames, 1024 core frames, then 64 right-context
    /// frames. `output` has exactly 1024 samples. Cancellation is checked before
    /// and after one bounded tile; a cancelled or failed call leaves output
    /// unchanged. Input samples must be finite and have magnitude at most 16.
    pub fn process_tile(
        &mut self,
        left: &[f32],
        right: &[f32],
        output: &mut [f64],
        cancelled: &AtomicBool,
    ) -> Result<(), PeakBankError> {
        if left.len() != FIXED_PEAK_INPUT_FRAMES || right.len() != FIXED_PEAK_INPUT_FRAMES {
            return Err(PeakBankError::InputLength);
        }
        if output.len() != FIXED_PEAK_OUTPUT_FRAMES {
            return Err(PeakBankError::OutputLength);
        }
        if cancelled.load(Ordering::Relaxed) {
            return Err(PeakBankError::Cancelled);
        }
        if left
            .iter()
            .chain(right)
            .any(|sample| !sample.is_finite() || sample.abs() > MAX_INPUT_PEAK)
        {
            return Err(PeakBankError::InvalidInput);
        }

        let mut staged = [0.0; FIXED_PEAK_OUTPUT_FRAMES];
        self.native.read(left, right, &mut staged)?;
        if cancelled.load(Ordering::Relaxed) {
            return Err(PeakBankError::Cancelled);
        }
        if staged
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err(PeakBankError::NativeReport);
        }
        output.copy_from_slice(&staged);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum PeakBankError {
    #[error("peak bank coefficients must be finite and have magnitude at most 16")]
    Coefficients,
    #[error("peak bank requires exactly 1152 frames in each input channel")]
    InputLength,
    #[error("peak bank requires exactly 1024 output magnitudes")]
    OutputLength,
    #[error("peak bank input must be finite with magnitude at most 16")]
    InvalidInput,
    #[error("peak analysis cancelled between fixed tiles")]
    Cancelled,
    #[error("native peak bank allocation failed")]
    NativeAllocation,
    #[error("native peak bank rejected a validated request")]
    NativeRejected,
    #[error("native peak bank failed")]
    NativeFailure,
    #[error("native peak bank produced an invalid magnitude")]
    NonFiniteOutput,
    #[error("native peak bank is unusable after an earlier processing failure")]
    Poisoned,
    #[error("native peak bank returned an invalid report")]
    NativeReport,
}
