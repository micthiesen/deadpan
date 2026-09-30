//! Committed SDR picture and canonical audio encoding in one supervised process.
//!
//! A completed candidate owns private hash-checked bytes after clean teardown.
//! It still requires independent finished-file verification before publication.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

pub mod admission;
mod host;
pub mod jobs;
pub mod protocol;
pub mod publication;
pub mod runtime;
pub mod verification;
pub(crate) mod worker;
pub mod workflow;

pub use host::{EncodedCandidate, EncodedProgress, EncodedWorkerLimits, encode};

/// Private host-selected worker dispatch, absent from ordinary user commands.
pub const PRIVATE_WORKER_ARGUMENT: &str = "--render-encode-worker";

#[derive(Debug, thiserror::Error)]
pub enum EncodedRenderError {
    #[error("invalid encoded render configuration: {0}")]
    Configuration(&'static str),
    #[error("encoded render protocol: {0}")]
    Protocol(String),
    #[error("encoded render worker failed: {0}")]
    Worker(String),
    #[error("encoded render worker failed: {0}")]
    WorkerFailure(#[from] protocol::EncodedFailure),
    #[error("{primary}; worker failure report is untrusted after supervision fault: {fault}")]
    WorkerFault {
        #[source]
        primary: Box<EncodedRenderError>,
        fault: String,
    },
    #[error("encoded rendering was cancelled")]
    Cancelled,
    #[error("encoded rendering exceeded its shared monotonic deadline")]
    Deadline,
    #[error(transparent)]
    Picture(#[from] crate::picture::ProjectPictureError),
    #[error(transparent)]
    Output(#[from] crate::export_picture::ExportPictureError),
    #[error(transparent)]
    Audio(#[from] crate::audio::OfflineAudioError),
    #[error(transparent)]
    Encode(#[from] deadpan_encode::EncodeError),
    #[error(transparent)]
    Render(#[from] crate::render_worker::RenderWorkerError),
    #[error(transparent)]
    Supervisor(#[from] deadpan_jobs::process::SupervisorError),
    #[error(transparent)]
    Artifact(#[from] deadpan_jobs::artifact::ArtifactError),
    #[error(transparent)]
    RetainedMedia(deadpan_store::render_media::RenderMediaError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("{primary}; render worker cleanup remains unconfirmed: {cleanup}")]
    CleanupUnconfirmed {
        #[source]
        primary: Box<EncodedRenderError>,
        cleanup: deadpan_jobs::process::CleanupFailure,
    },
}

impl EncodedRenderError {
    /// Whether a host stage established no child or explicit owned-group,
    /// leader and pipe teardown. Only such failures may become durable Failed
    /// or Cancelled attempts. Unconfirmed cleanup must remain Cancelling.
    pub fn cleanup_confirmed(&self) -> bool {
        match self {
            Self::CleanupUnconfirmed { .. } => false,
            Self::WorkerFault { primary, .. } => primary.cleanup_confirmed(),
            Self::Supervisor(error) => error.cleanup_confirmed(),
            _ => true,
        }
    }
}

impl From<deadpan_store::render_media::RenderMediaError> for EncodedRenderError {
    fn from(error: deadpan_store::render_media::RenderMediaError) -> Self {
        // Preserve the host's control categories across the storage boundary.
        // Other stable storage codes remain available on the retained source.
        match error.code() {
            "OperationCancelled" => Self::Cancelled,
            "DeadlineExceeded" => Self::Deadline,
            _ => Self::RetainedMedia(error),
        }
    }
}

fn check_control(cancelled: &AtomicBool, deadline: Instant) -> Result<(), EncodedRenderError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(EncodedRenderError::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(EncodedRenderError::Deadline);
    }
    Ok(())
}
