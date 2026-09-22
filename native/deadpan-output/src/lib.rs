//! Prepared stereo PCM output, separate from decoding, DSP, and authored state.
//!
//! The queue is headlessly testable. The macOS boundary initially admits only
//! the current default device's existing 48 kHz, stereo, float configuration.
//! It does not implement application transport, mastering, rate conversion,
//! automatic device recovery, or acoustic/loopback verification.

mod queue;
pub use queue::*;

#[cfg(target_os = "macos")]
mod device;
#[cfg(target_os = "macos")]
pub use device::*;

pub const ENGINE_ID: &str = "deadpan-prepared-output-v1";
pub const SAMPLE_RATE: u32 = 48_000;
