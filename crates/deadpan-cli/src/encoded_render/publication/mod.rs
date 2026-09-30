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
mod provenance;

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
    control(cancelled, deadline)?;
    let name = destination.file_name().ok_or_else(|| {
        PublicationDiagnostic::new("invalid_destination", "destination has no filename")
    })?;
    let movie_filename = name.to_str().ok_or_else(|| {
        PublicationDiagnostic::new("invalid_destination", "destination filename must be UTF-8")
    })?;
    if !destination
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("mp4"))
    {
        return Err(PublicationDiagnostic::new(
            "invalid_destination",
            "destination filename must end in .mp4",
        ));
    }
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let movie_destination = filesystem::Destination::pin(parent, name).map_err(fs_error)?;
    progress(PublicationStage::CapturingProvenance);
    control(cancelled, deadline)?;
    let provenance = provenance::capture(package, candidate, cancelled, deadline)
        .map_err(|error| PublicationDiagnostic::new(error.code(), error))?;
    let contains_generated_pictures = provenance.has_generated();
    control(cancelled, deadline)?;
    let publication_id = uuid::Uuid::new_v4().to_string();
    let report_filename = format!("deadpan-render-{publication_id}.json");
    let report_destination = movie_destination
        .for_name(report_filename.as_ref())
        .map_err(fs_error)?;
    let movie_bytes = candidate.report().movie_bytes;
    let movie_sha256 = candidate.report().movie_sha256.clone();
    let mut partial = movie_destination
        .create_partial(movie_bytes)
        .map_err(|error| {
            retained.partial_movie = error.partial_path().map(Path::to_path_buf);
            fs_error(error)
        })?;
    retained.partial_movie = Some(partial.path());
    progress(PublicationStage::CopyingDestination);
    let copied = candidate
        .copy_to(
            &mut partial.writer(cancelled, deadline).map_err(fs_error)?,
            cancelled,
            deadline,
        )
        .map_err(encoded_error)?;
    if copied != movie_bytes {
        return Err(PublicationDiagnostic::new(
            "destination_length_mismatch",
            "copied byte count differs from the verified movie",
        ));
    }
    partial
        .seal(movie_bytes, cancelled, deadline)
        .map_err(fs_error)?;
    progress(PublicationStage::CheckingDestination);
    let actual_hash = hash_reader(
        partial.reader(cancelled, deadline).map_err(fs_error)?,
        movie_bytes,
        cancelled,
        deadline,
    )?;
    if actual_hash != movie_sha256 {
        return Err(PublicationDiagnostic::new(
            "destination_hash_mismatch",
            "destination readback differs from the verified private movie",
        ));
    }
    let report = LocalReport {
        schema_version: 1,
        application_version: env!("CARGO_PKG_VERSION"),
        publication_id: publication_id.clone(),
        movie_filename: movie_filename.into(),
        report_filename,
        scope: "verified_candidate_prepared_for_atomic_publication",
        destination_readback: ByteIdentity {
            bytes: movie_bytes,
            sha256: actual_hash,
        },
        provenance,
    };
    let report_wire = serialize_report(&report)?;
    control(cancelled, deadline)?;
    let report_bytes = u64::try_from(report_wire.len()).expect("bounded report length");
    let report_sha256 = digest(&report_wire);
    let mut report_partial = report_destination
        .create_partial(report_bytes)
        .map_err(|error| {
            retained.partial_report = error.partial_path().map(Path::to_path_buf);
            fs_error(error)
        })?;
    retained.partial_report = Some(report_partial.path());
    progress(PublicationStage::WritingReport);
    report_partial
        .writer(cancelled, deadline)
        .map_err(fs_error)?
        .write_all(&report_wire)
        .map_err(|error| io_error("report_write_failed", error))?;
    report_partial
        .seal(report_bytes, cancelled, deadline)
        .map_err(fs_error)?;
    if hash_reader(
        report_partial
            .reader(cancelled, deadline)
            .map_err(fs_error)?,
        report_bytes,
        cancelled,
        deadline,
    )? != report_sha256
    {
        return Err(PublicationDiagnostic::new(
            "report_hash_mismatch",
            "destination report readback differs from the captured provenance",
        ));
    }
    let report_result = report_partial.commit(cancelled, deadline);
    if report_partial.is_published() {
        retained.partial_report = None;
        retained.published_report = Some(report_destination.path());
    }
    report_result.map_err(fs_error)?;
    confirm_published_bytes(
        &report_partial,
        &report_sha256,
        report_bytes,
        cancelled,
        deadline,
    )?;
    progress(PublicationStage::ReadyToPublish);
    control(cancelled, deadline)?;
    report_partial.confirm_published().map_err(fs_error)?;
    let receipt = PublicationReceipt {
        publication_id,
        movie: movie_destination.path(),
        report: report_destination.path(),
        movie_sha256,
        movie_bytes,
        report_sha256,
        report_bytes,
        contains_generated_pictures,
    };
    let finish_deadline = Instant::now() + POST_COMMIT_READBACK_BUDGET;
    match partial.commit(cancelled, deadline) {
        Ok(()) => match finish_committed(&partial, &report_partial, &receipt, finish_deadline) {
            Ok(()) => Ok(PublicationOutcome::Published(receipt)),
            Err(diagnostic) => Ok(PublicationOutcome::PublishedUnconfirmed {
                receipt,
                diagnostic,
            }),
        },
        Err(error) if error.published() => Ok(PublicationOutcome::PublishedUnconfirmed {
            receipt,
            diagnostic: fs_error(error),
        }),
        Err(error) => Err(fs_error(error)),
    }
}

fn finish_committed(
    movie: &filesystem::PartialFile<'_>,
    report: &filesystem::PartialFile<'_>,
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
    partial: &filesystem::PartialFile<'_>,
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
