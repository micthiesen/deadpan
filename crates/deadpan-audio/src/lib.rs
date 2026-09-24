//! Bounded source preparation for worker threads, never an audio callback.
//! Exact affine sampling and explicit channel matrices are shared by consumers;
//! continuous Preserve stages retain canonical history across source seams.
//! Informational peak/loudness meters leave PCM untouched. Voice effects,
//! mastering gain and the device engine remain separate work.

mod domain_transfer;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod edges;
mod loudness;
mod matrix;
mod reference_policy;
mod resample;
mod room_tone;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod sequence;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod session;
mod signal_transfer;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod stages;
mod true_peak;

pub use domain_transfer::{DomainSignalTransfer, DomainTransferDescriptor};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use edges::EDGE_FADE_ID;
pub use loudness::{
    LOUDNESS_ID, LoudnessError, LoudnessMeter, LoudnessReport, MAX_LOUDNESS_FRAMES,
};
pub use matrix::StereoMatrix;
pub use reference_policy::{ReferencePolicyError, RetainedRootPolicy};
pub use resample::{PcmWindow, ResampleRecipe, Resampler, StereoBlock};
pub use room_tone::{ROOM_TONE_ID, RoomTone, RoomToneRecipe};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use sequence::{AudioSourceProvider, SequenceAudio, SequenceAudioError, SourceStageBlock};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use session::PreparedSource;
pub use signal_transfer::{
    RootSignalBlock, RootSignalTransfer, SignalTransferError, TransferredSignalBlock,
};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use stages::{
    DefinitionAudioBlock, DomainAudioBlock, EdgeFadedBlock, PointDomainAudioBlock, StageAudio,
    StageAudioError, StageLimits, TimeMappedBlock, TransferredDomainBlock, TransferredRootBlock,
};
pub use true_peak::{
    MAX_TRUE_PEAK_FRAMES, TRUE_PEAK_ID, TruePeakError, TruePeakMeter, TruePeakReport,
};

use std::sync::atomic::{AtomicBool, Ordering};

pub const RESAMPLER_ID: &str = "deadpan-exact-sinc-bh4-128-v1";
pub const MATRIX_ID: &str = "deadpan-stereo-speaker-matrix-v1";
pub const BOUNDARY_ID: &str = "authored-trim-zero-extension-v1";
pub const MAX_OUTPUT_FRAMES: u32 = 256;
pub const MAX_SOURCE_FRAMES: u32 = 32_706;
pub const MAX_INPUT_MAGNITUDE: f32 = 16.0;

#[derive(Debug, thiserror::Error)]
pub enum PreparationError {
    #[error("invalid source preparation recipe: {0}")]
    InvalidRecipe(&'static str),
    #[error("source channel layout has no supported explicit stereo interpretation")]
    UnsupportedLayout,
    #[error("source PCM does not match the requested window or contains invalid samples")]
    InvalidSamples,
    #[error("source preparation was cancelled")]
    Cancelled,
    #[error("reopened source audio differs from its qualified index")]
    IndexMismatch,
    #[error("qualified source audio is unavailable: {0}")]
    SourceUnavailable(String),
    #[error(transparent)]
    Time(#[from] deadpan_core::TimeError),
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[error(transparent)]
    Session(#[from] deadpan_media::audio_session::AudioSessionError),
}

pub(crate) fn check_cancel(cancelled: &AtomicBool) -> Result<(), PreparationError> {
    if cancelled.load(Ordering::Relaxed) {
        Err(PreparationError::Cancelled)
    } else {
        Ok(())
    }
}
