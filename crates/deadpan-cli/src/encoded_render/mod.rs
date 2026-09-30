//! Committed SDR picture and canonical audio encoding in one supervised process.
//!
//! A completed candidate owns private hash-checked bytes after clean teardown.
//! It still requires independent finished-file verification before publication.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

mod host;
pub mod protocol;
pub(crate) mod worker;

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
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
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
