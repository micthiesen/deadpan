//! Destination-side byte verification and exclusive atomic movie publication.
//!
//! A durable local report is published first. The movie's rename is the commit
//! point; the two files are not a single atomic transaction. No project is edited.

use std::{
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use deadpan_jobs::Sha256;
use serde::Serialize;
use sha2::{Digest, Sha256 as Hasher};

use super::{EncodedRenderError, check_control, verification::VerifiedCandidate};

mod filesystem;
pub mod journal;
mod provenance;
mod staging;

const MAX_REPORT_BYTES: usize = 16 * 1024 * 1024;
const BUFFER_BYTES: usize = 64 * 1024;
// Once rename commits the movie, user cancellation cannot abandon it. Final
// byte inspection has a separate cooperative deadline and fixed byte extents.
const POST_COMMIT_READBACK_BUDGET: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationStage {
    CapturingProvenance,
    CopyingDestination,
    CheckingDestination,
    WritingReport,
    ReadyToPublish,
}

/// Stable diagnostic codes describe failed work. Paths are returned separately
/// as local recovery information and never grant authority to adopt/delete files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[error("{code}: {message}")]
pub struct PublicationDiagnostic {
    pub code: String,
    pub message: String,
}

impl PublicationDiagnostic {
    fn new(code: &str, message: impl ToString) -> Self {
        Self {
            code: code.into(),
            message: message.to_string(),
        }
    }
}

/// Names created during an attempt. They may have changed externally; callers
/// must revalidate identities before any later recovery or cleanup operation.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct RetainedPublicationArtifacts {
    pub partial_movie: Option<PathBuf>,
    pub partial_report: Option<PathBuf>,
    pub published_report: Option<PathBuf>,
}

/// Before the movie rename, every failure preserves the verified candidate.
pub struct PublicationFailure {
    pub error: PublicationDiagnostic,
    pub candidate: VerifiedCandidate,
    pub retained: RetainedPublicationArtifacts,
}

impl std::fmt::Debug for PublicationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PublicationFailure")
            .field("error", &self.error)
            .field("candidate_bytes", &self.candidate.report().movie_bytes)
            .field("retained", &self.retained)
            .finish()
    }
}

impl std::fmt::Display for PublicationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

impl std::error::Error for PublicationFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// A serialized receipt records observations. It cannot recreate a verified
/// candidate or authorize overwriting/recovering either named file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PublicationReceipt {
    pub publication_id: String,
    pub movie: PathBuf,
    pub report: PathBuf,
    pub movie_sha256: Sha256,
    pub movie_bytes: u64,
    pub report_sha256: Sha256,
    pub report_bytes: u64,
    pub contains_generated_pictures: bool,
}

#[derive(Debug)]
pub enum PublicationOutcome {
    Published(PublicationReceipt),
    /// Rename succeeded. A later durability, integrity or identity check failed. The
    /// final file is never deleted or represented as an uncommitted attempt.
    PublishedUnconfirmed {
        receipt: PublicationReceipt,
        diagnostic: PublicationDiagnostic,
    },
}

#[derive(Serialize)]
struct LocalReport {
    schema_version: u32,
    application_version: &'static str,
    publication_id: String,
    movie_filename: String,
    report_filename: String,
    scope: &'static str,
    destination_readback: ByteIdentity,
    provenance: provenance::PublicationProvenance,
}

#[derive(Serialize)]
struct ByteIdentity {
    bytes: u64,
    sha256: Sha256,
}

/// Publish one already verified private candidate. Call off UI/audio threads.
/// The package supplies historical provenance for exactly this committed
/// revision; it is opened read-only. The selected final name is never replaced.
/// One deadline covers precommit work. After the movie rename, required
/// durability and bounded final readback complete despite late cancellation or
/// deadline expiry. Final readback has a separate ten-minute cooperative budget.
pub fn publish(
    mut candidate: VerifiedCandidate,
    package: &Path,
    destination: &Path,
    cancelled: &AtomicBool,
    deadline: Instant,
    progress: impl FnMut(PublicationStage),
) -> Result<PublicationOutcome, Box<PublicationFailure>> {
    let mut retained = RetainedPublicationArtifacts::default();
    match prepare_and_publish(
        &mut candidate,
        package,
        destination,
        cancelled,
        deadline,
        progress,
        &mut retained,
    ) {
        Ok(outcome) => Ok(outcome),
        Err(error) => Err(Box::new(PublicationFailure {
            error,
            candidate,
            retained,
        })),
    }
}

fn prepare_and_publish(
    candidate: &mut VerifiedCandidate,
    package: &Path,
    destination: &Path,
    cancelled: &AtomicBool,
    deadline: Instant,
    mut progress: impl FnMut(PublicationStage),
    retained: &mut RetainedPublicationArtifacts,
) -> Result<PublicationOutcome, PublicationDiagnostic> {
    let publication_id = uuid::Uuid::new_v4().to_string();
    let names = staging::Names {
        report: format!("deadpan-render-{publication_id}.json"),
        movie_partial: format!(".deadpan-{}.partial", uuid::Uuid::new_v4()),
        report_partial: format!(".deadpan-{}.partial", uuid::Uuid::new_v4()),
        publication_id,
    };
    let controls = staging::StageControl {
        cancelled,
        deadline,
        live: &|| Ok(()),
    };
    let mut files = staging::PreparedFiles::prepare(
        candidate,
        staging::Preparation {
            package,
            destination,
            names,
            recoverable: false,
        },
        &controls,
        &mut progress,
        retained,
    )?;
    let report_result = files.commit_report(&controls);
    *retained = files.retained();
    report_result?;
    progress(PublicationStage::ReadyToPublish);
    let outcome = files.commit_movie(&controls);
    *retained = files.retained();
    outcome
}

fn finish_committed(
    movie: &filesystem::PartialFile,
    report: &filesystem::PartialFile,
    receipt: &PublicationReceipt,
    deadline: Instant,
) -> Result<(), PublicationDiagnostic> {
    let uncancelled = AtomicBool::new(false);
    confirm_published_bytes(
        movie,
        &receipt.movie_sha256,
        receipt.movie_bytes,
        &uncancelled,
        deadline,
    )?;
    confirm_published_bytes(
        report,
        &receipt.report_sha256,
        receipt.report_bytes,
        &uncancelled,
        deadline,
    )?;
    // A movie change while reading the report must not escape final admission.
    movie.confirm_published().map_err(fs_error)?;
    report.confirm_published().map_err(fs_error)?;
    Ok(())
}

fn confirm_published_bytes(
    partial: &filesystem::PartialFile,
    expected: &Sha256,
    bytes: u64,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<(), PublicationDiagnostic> {
    let observed = hash_reader(
        partial
            .published_reader(cancelled, deadline)
            .map_err(fs_error)?,
        bytes,
        cancelled,
        deadline,
    )?;
    if &observed != expected {
        return Err(PublicationDiagnostic::new(
            "published_hash_mismatch",
            "published bytes differ from their verified pre-rename identity",
        ));
    }
    partial.confirm_published().map_err(fs_error)
}

fn encoded_error(error: EncodedRenderError) -> PublicationDiagnostic {
    if let EncodedRenderError::Io(error) = error {
        return io_error("candidate_copy_failed", error);
    }
    let code = match error {
        EncodedRenderError::Cancelled => "cancelled",
        EncodedRenderError::Deadline => "deadline_exceeded",
        _ => "candidate_copy_failed",
    };
    PublicationDiagnostic::new(code, error)
}

fn fs_error(error: filesystem::FsError) -> PublicationDiagnostic {
    PublicationDiagnostic::new(error.code(), error)
}

fn io_error(fallback: &'static str, error: io::Error) -> PublicationDiagnostic {
    if let Some(diagnostic) = error
        .get_ref()
        .and_then(|source| source.downcast_ref::<PublicationDiagnostic>())
    {
        return diagnostic.clone();
    }
    let code = error
        .get_ref()
        .and_then(|source| source.downcast_ref::<filesystem::FsError>())
        .map_or(fallback, filesystem::FsError::code);
    PublicationDiagnostic::new(code, &error)
}

fn control(cancelled: &AtomicBool, deadline: Instant) -> Result<(), PublicationDiagnostic> {
    check_control(cancelled, deadline).map_err(encoded_error)
}

fn digest(bytes: &[u8]) -> Sha256 {
    let bytes = Hasher::digest(bytes);
    digest_bytes(&bytes)
}

fn digest_bytes(bytes: &[u8]) -> Sha256 {
    Sha256::new(
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
    )
    .expect("SHA-256 hexadecimal digest")
}

fn hash_reader(
    mut reader: impl Read,
    length: u64,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<Sha256, PublicationDiagnostic> {
    let mut digest = Hasher::new();
    let mut remaining = length;
    let mut buffer = [0_u8; BUFFER_BYTES];
    while remaining != 0 {
        control(cancelled, deadline)?;
        let count = usize::try_from(remaining.min(BUFFER_BYTES as u64)).expect("bounded read");
        reader
            .read_exact(&mut buffer[..count])
            .map_err(|error| io_error("destination_readback_failed", error))?;
        digest.update(&buffer[..count]);
        remaining -= u64::try_from(count).expect("bounded read");
    }
    control(cancelled, deadline)?;
    if reader
        .read(&mut buffer[..1])
        .map_err(|error| io_error("destination_readback_failed", error))?
        != 0
    {
        return Err(PublicationDiagnostic::new(
            "destination_length_mismatch",
            "destination has unexpected trailing bytes",
        ));
    }
    control(cancelled, deadline)?;
    Ok(digest_bytes(&digest.finalize()))
}

fn serialize_report(report: &impl Serialize) -> Result<Vec<u8>, PublicationDiagnostic> {
    struct BoundedReport(Vec<u8>);
    impl Write for BoundedReport {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > MAX_REPORT_BYTES - self.0.len() {
                return Err(io::Error::other("publication report exceeds 16 MiB"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut bytes = BoundedReport(Vec::new());
    serde_json::to_writer_pretty(&mut bytes, report)
        .map_err(|error| PublicationDiagnostic::new("report_serialization_failed", error))?;
    bytes
        .write_all(b"\n")
        .map_err(|error| PublicationDiagnostic::new("report_serialization_failed", error))?;
    Ok(bytes.0)
}

#[cfg(test)]
mod tests;
