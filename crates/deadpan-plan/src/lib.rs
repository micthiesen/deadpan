//! Immutable structural picture and audio plans shared by preview and export callers.
//!
//! Compilation validates one authored revision and indexes sequence boundaries
//! and compact repeat identity runs. Sampling maps project frame centers through
//! exact affine retimes without rounding intermediate coordinates. This crate
//! performs no decoding, GPU work, effects, audio processing, or external I/O.

mod audio_envelope;
mod audio_sampling;
mod picture;
mod plan;

pub use audio_envelope::{AudioEnvelope, AudioEnvelopeEndpoint};
pub use audio_sampling::{AudioBoundaryRule, AudioSampleGrid, AudioSampleMap};

pub use picture::{Picture, PictureSample};
pub use plan::{
    AudioBoundaries, AudioBoundaryKind, AudioBoundaryOrigin, AudioContent, AudioQuery,
    AudioQueryLimits, AudioRetimeStage, AudioSpan, AudioTransform, LookupStats, NodeInspection,
    NodeType, PlanInspection, PlanMetadata, RenderPlan, SilenceReason, SourceSamplingSupport,
    StorageStats,
};
pub use plan::{
    AudioProcessingQuery, AudioProcessingSpan, AudioSignal, AudioSignalContent, AudioSignalQuery,
    AudioSignalSpan, AudioStage, AudioStageDescriptor, SignalSample, SignalTransform,
};

use deadpan_core::{
    AssetId, DocumentError, FrameDuration, ProjectFrame, SourceFrameId, SourceTimeBase, TimeError,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlanError {
    #[error(transparent)]
    Document(#[from] DocumentError),
    #[error(transparent)]
    Time(#[from] TimeError),
    #[error("project frame {frame:?} is outside the plan's {duration:?} duration")]
    FrameOutOfRange {
        frame: ProjectFrame,
        duration: FrameDuration,
    },
    #[error("picture has no source video frame to select")]
    NoSourceFrame,
    #[error("audio sample range is reversed or outside the plan's mix duration")]
    AudioRangeOutOfRange,
    #[error("audio query limits must be positive and within the supported bounds")]
    InvalidAudioLimits,
    #[error("audio query exceeded its {0} budget")]
    AudioQueryLimit(&'static str),
    #[error("audio span has no linearly mapped source audio")]
    NoSourceAudio,
    #[error("source index belongs to {actual}, expected {expected}")]
    IndexAssetMismatch { expected: AssetId, actual: AssetId },
    #[error("source index clock {actual:?} differs from expected {expected:?}")]
    IndexClockMismatch {
        expected: SourceTimeBase,
        actual: SourceTimeBase,
    },
    #[error("original source frame {frame:?} is missing from the presentation index")]
    MissingSourceFrame { frame: SourceFrameId },
    #[error("compiled plan invariant failed: {0}")]
    InvalidPlan(&'static str),
}
