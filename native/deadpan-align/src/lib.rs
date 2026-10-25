//! Frozen alpha0.6 phoneme features and CPU inference for annotation proposals.
//!
//! Construct and use this on an analysis worker. No project, media, network,
//! dictionary, CTC policy or authored-state ownership crosses this boundary.
//! The original checkpoint is data: only compiled, hashed byte ranges are read.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
use thiserror::Error;

pub mod assembly;
mod features;
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod ffi;

pub use features::{Features, Normalization, extract_features};

pub const ENGINE_ID: &str = "deadpan-phoneme-alpha06-ort-cpu-1";
pub const ORT_VERSION: &str = "1.31.0";
pub const SAMPLE_RATE: u32 = 16_000;
pub const MAX_SAMPLES: usize = 480_000;
pub const MEL_BINS: usize = 80;
pub const LABELS: usize = 41;
pub const MAX_FEATURE_FRAMES: usize = MAX_SAMPLES / 160 + 1;
pub const MAX_OUTPUT_FRAMES: usize = MAX_FEATURE_FRAMES.div_ceil(2);
pub const GLOBAL_STATS_SHA256: &str =
    "be2f80418ef66bad30dcd5556e194f95234d75c5d2b94a92d7a766d31900df87";

#[derive(Debug, Error)]
pub enum AlignError {
    #[error("native phoneme inference is supported only on macOS")]
    Unsupported,
    #[error("phoneme inference was cancelled")]
    Cancelled,
    #[error("phoneme inference deadline expired")]
    Deadline,
    #[error("invalid phoneme input: {0}")]
    Input(&'static str),
    #[error("phoneme data integrity failure: {0}")]
    Integrity(&'static str),
    #[error("native phoneme runtime: {0}")]
    Native(String),
}

/// Shared cancellation and one absolute monotonic deadline for all phases.
/// Cancellation is cooperative; the owning worker supervisor enforces process
/// teardown if an underlying native call does not return promptly.
#[derive(Clone, Debug)]
pub struct Control {
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
}

impl Control {
    pub fn new(cancelled: Arc<AtomicBool>, deadline: Instant) -> Self {
        Self {
            cancelled,
            deadline,
        }
    }

    pub fn check(&self) -> Result<(), AlignError> {
        if self.cancelled.load(Ordering::Relaxed) {
            Err(AlignError::Cancelled)
        } else if Instant::now() >= self.deadline {
            Err(AlignError::Deadline)
        } else {
            Ok(())
        }
    }
}

/// Owned row-major [T, 41] model log probabilities, before CTC policy.
#[derive(Clone, Debug)]
pub struct Emissions {
    pub frames: usize,
    pub log_probs: Vec<f32>,
}

impl Emissions {
    pub fn widened(&self) -> Vec<f64> {
        self.log_probs.iter().copied().map(f64::from).collect()
    }
}

/// A CPU-only ONNX session. Not Send or Sync; construct on its owning worker.
pub struct Session {
    #[cfg(target_os = "macos")]
    inner: ffi::NativeSession,
    #[cfg(not(target_os = "macos"))]
    _private: (),
}

impl Session {
    pub fn from_checkpoint(checkpoint: Vec<u8>, control: &Control) -> Result<Self, AlignError> {
        control.check()?;
        #[cfg(target_os = "macos")]
        {
            let graph = assembly::assemble(&checkpoint, control)?;
            drop(checkpoint);
            let inner = ffi::NativeSession::new(&graph, control)?;
            control.check()?;
            Ok(Self { inner })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = checkpoint;
            Err(AlignError::Unsupported)
        }
    }

    pub fn infer(
        &mut self,
        features: &Features,
        control: &Control,
    ) -> Result<Emissions, AlignError> {
        control.check()?;
        #[cfg(target_os = "macos")]
        {
            self.inner.infer(features, control)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = features;
            Err(AlignError::Unsupported)
        }
    }
}
