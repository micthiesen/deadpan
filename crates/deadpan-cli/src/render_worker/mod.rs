//! Supervised preparation of real committed pictures in a separate process.
//!
//! This bounded raw range is an engineering input boundary. It does not encode
//! or publish a movie. The future encoder consumes these same frames inside
//! the worker instead of spooling an uncompressed full-length product export.

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use deadpan_jobs::Sha256;
use sha2::{Digest, Sha256 as Sha256Hasher};

use crate::picture::ProjectPictureSession;

mod host;
pub mod protocol;
pub(crate) mod worker;

pub use host::{
    PreparedPictureRange, RenderPictureRequest, RenderProgress, RenderWorkerLimits,
    RenderWorkerRuntime, prepare,
};

/// Private process dispatch, deliberately absent from the user command/help registry.
pub const PRIVATE_WORKER_ARGUMENT: &str = "--render-picture-worker";

#[derive(Debug, thiserror::Error)]
pub enum RenderWorkerError {
    #[error("invalid render worker configuration: {0}")]
    Configuration(&'static str),
    #[error("render worker protocol: {0}")]
    Protocol(String),
    #[error("render worker failed: {0}")]
    Worker(String),
    #[error("render preparation was cancelled")]
    Cancelled,
    #[error("render preparation exceeded its shared monotonic deadline")]
    Deadline,
    #[error("rendered frame {frame} has an out-of-range {plane} code")]
    InvalidPixels { frame: u64, plane: &'static str },
    #[error(transparent)]
    Picture(#[from] crate::picture::ProjectPictureError),
    #[error(transparent)]
    Output(#[from] crate::export_picture::ExportPictureError),
    #[error(transparent)]
    Supervisor(#[from] deadpan_jobs::process::SupervisorError),
    #[error(transparent)]
    Artifact(#[from] deadpan_jobs::artifact::ArtifactError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

fn check_control(cancelled: &AtomicBool, deadline: Instant) -> Result<(), RenderWorkerError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(RenderWorkerError::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(RenderWorkerError::Deadline);
    }
    Ok(())
}

/// Bind the complete validated authored snapshot, rather than trusting a
/// project/revision label after the package path is handed to another process.
/// Serialize into a bounded hash writer, not another retained JSON allocation.
pub(super) fn document_sha256(
    pictures: &ProjectPictureSession,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<Sha256, RenderWorkerError> {
    document_hash(pictures.document(), cancelled, deadline)
}

/// Shared immutable document binding for separate picture and audio readers.
pub(crate) fn document_hash(
    document: &deadpan_core::ProjectDocument,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<Sha256, RenderWorkerError> {
    struct DocumentHash<'a> {
        hasher: Sha256Hasher,
        bytes: usize,
        cancelled: &'a AtomicBool,
        deadline: Instant,
    }
    impl Write for DocumentHash<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.cancelled.load(Ordering::Acquire) || Instant::now() >= self.deadline {
                return Err(io::Error::other("render document hashing interrupted"));
            }
            if bytes.len() > deadpan_core::MAX_DOCUMENT_JSON_BYTES.saturating_sub(self.bytes) {
                return Err(io::Error::new(
                    io::ErrorKind::FileTooLarge,
                    "render document hash byte bound",
                ));
            }
            self.bytes += bytes.len();
            self.hasher.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut writer = DocumentHash {
        hasher: Sha256Hasher::new(),
        bytes: 0,
        cancelled,
        deadline,
    };
    let serialized = serde_json::to_writer(&mut writer, document);
    check_control(cancelled, deadline)?;
    serialized?;
    let hex: String = writer
        .hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Sha256::new(hex).map_err(|error| RenderWorkerError::Protocol(error.to_string()))
}

#[cfg(test)]
mod tests;
