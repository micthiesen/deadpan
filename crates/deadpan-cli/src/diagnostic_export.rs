//! Explicit, local diagnostic export (specification Section 27.4).
//!
//! Build an allowlisted report instead of attempting to redact a project dump
//! or raw errors. No authored identifiers, labels, paths, URLs, transcript,
//! media bytes, credentials, environment or worker logs enter this type.
//! Capture project structure on a preparation worker, not the UI thread.

use std::fs::{self, File};
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use deadpan_core::{NodeKind, ProjectDocument};
use deadpan_plan::RenderPlan;
use deadpan_store::{AccessMode, ProjectStore};
use serde::Serialize;

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_BYTES: usize = 256 * 1024;

/// Only closed categories may be attached to the report. Raw diagnostic text
/// can contain source names, paths, URLs, transcripts or authentication data.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Startup,
    Import,
    Edit,
    Preview,
    Playback,
    Render,
    Analysis,
    Generation,
    Storage,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    Unavailable,
    InvalidInput,
    RevisionConflict,
    Unsupported,
    Cancelled,
    Deadline,
    WorkerFailed,
    CleanupUnconfirmed,
    IoFailure,
    DiskFull,
    PermissionDenied,
    VerificationFailed,
    Internal,
}

#[derive(Serialize)]
struct Context {
    operation: Operation,
    failure: Failure,
}

#[derive(Serialize)]
struct Component {
    name: String,
    version: String,
}

#[derive(Serialize)]
struct Versions {
    application: &'static str,
    operating_system: &'static str,
    architecture: &'static str,
    sqlite: &'static str,
    document_schema: u32,
    database_schema: u32,
    // These describe compiled baselines, not a claim about the user's active
    // updates or installed models. Reading those stores is unnecessary here.
    compiled_helpers: Vec<Component>,
    compiled_model_packs: Vec<Component>,
}

#[derive(Default, Serialize)]
struct NodeCounts {
    source: usize,
    sequence: usize,
    hold: usize,
    repeat: usize,
    retime: usize,
}

#[derive(Serialize)]
struct Structure {
    document_schema: u32,
    nodes: NodeCounts,
    assets: usize,
    qualified_assets: usize,
    marks: usize,
    root_sounds: usize,
    beat_sounds: usize,
    targets: usize,
    repeat_override_owners: usize,
    repeat_gap_override_owners: usize,
    raster: [u32; 2],
    frame_rate: [u32; 2],
    duration_frames: Option<i64>,
    plan_error_code: Option<&'static str>,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Project {
    NotRequested,
    Available { structure: Structure },
    Unavailable { error_code: &'static str },
}

/// Fields are private: callers cannot insert arbitrary JSON or private text.
#[derive(Serialize)]
pub struct DiagnosticReport {
    schema_version: u32,
    versions: Versions,
    project: Project,
    context: Option<Context>,
    counters: serde_json::Value,
    counter_scope: &'static str,
    privacy: &'static str,
}

/// Read the optional package through the normal bounded, read-only store API.
/// An unreadable package is evidence and does not prevent diagnostic export.
pub fn capture(package: Option<&Path>) -> DiagnosticReport {
    match package {
        None => DiagnosticReport::from_document(None),
        Some(package) => match ProjectStore::open(package, AccessMode::ReadOnly)
            .and_then(|store| store.snapshot())
        {
            Ok(document) => DiagnosticReport::from_document(Some(&document)),
            Err(error) => {
                let mut report = DiagnosticReport::from_document(None);
                report.project = Project::Unavailable {
                    error_code: error.code(),
                };
                report
            }
        },
    }
}

impl DiagnosticReport {
    /// Use the native host's captured committed document without reopening its
    /// writable package. Compilation is bounded by the document/plan limits.
    pub fn from_document(document: Option<&ProjectDocument>) -> Self {
        let project = document.map_or(Project::NotRequested, |document| {
            let mut nodes = NodeCounts::default();
            for node in document.nodes().values() {
                match &node.kind {
                    NodeKind::Source { .. } => nodes.source += 1,
                    NodeKind::Sequence { .. } => nodes.sequence += 1,
                    NodeKind::Hold { .. } => nodes.hold += 1,
                    NodeKind::Repeat { .. } => nodes.repeat += 1,
                    NodeKind::Retime { .. } => nodes.retime += 1,
                }
            }
            let basis = document.presentation_basis();
            let (duration_frames, plan_error_code) = match RenderPlan::compile(document) {
                Ok(plan) => (Some(plan.duration().frames()), None),
                Err(deadpan_plan::PlanError::Time(_)) => (None, Some("TimingOverflow")),
                Err(deadpan_plan::PlanError::Document(_)) => (None, Some("ProjectInvalid")),
                Err(_) => (None, Some("PlanInvalid")),
            };
            Project::Available {
                structure: Structure {
                    document_schema: document.schema_version(),
                    nodes,
                    assets: document.assets().len(),
                    qualified_assets: document
                        .assets()
                        .values()
                        .filter(|asset| asset.source_qualification.is_some())
                        .count(),
                    marks: document.marks().len(),
                    root_sounds: document.sounds().len(),
                    beat_sounds: document
                        .beat_sounds()
                        .values()
                        .map(|sounds| sounds.len())
                        .sum(),
                    targets: document.targets().len(),
                    repeat_override_owners: document.overrides().len(),
                    repeat_gap_override_owners: document.gap_overrides().len(),
                    raster: [basis.width, basis.height],
                    frame_rate: [basis.frame_rate.numerator(), basis.frame_rate.denominator()],
                    duration_frames,
                    plan_error_code,
                },
            }
        });
        Self {
            schema_version: SCHEMA_VERSION,
            versions: Versions {
                application: env!("CARGO_PKG_VERSION"),
                operating_system: std::env::consts::OS,
                architecture: std::env::consts::ARCH,
                sqlite: deadpan_store::sqlite_version(),
                document_schema: deadpan_core::DOCUMENT_SCHEMA_VERSION,
                database_schema: deadpan_store::DATABASE_SCHEMA_VERSION,
                compiled_helpers: crate::youtube::helpers::BUNDLE
                    .iter()
                    .map(|pin| Component {
                        name: pin.name.into(),
                        version: pin.version.into(),
                    })
                    .collect(),
                compiled_model_packs: deadpan_models::packs::approved_packs()
                    .into_iter()
                    .map(|pack| Component {
                        name: pack.pack_id,
                        version: pack.pack_version,
                    })
                    .collect(),
            },
            project,
            context: None,
            counters: crate::doctor::diagnostics(&deadpan_diagnostics::snapshot()),
            counter_scope: "current process only; independent atomic samples; no other app session",
            privacy: "structural counts and fixed categories only; no user content, identifiers, paths, URLs, credentials, logs or media attached",
        }
    }

    pub fn with_failure(mut self, operation: Operation, failure: Failure) -> Self {
        self.context = Some(Context { operation, failure });
        self
    }

    /// Save a complete report as a new owner-only JSON file. Existing files,
    /// symlinks and directories are never replaced. No network or worker runs.
    pub fn write(&self, destination: &Path) -> Result<Receipt, ExportError> {
        if destination.file_name().is_none() {
            return Err(ExportError::InvalidDestination);
        }
        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let bytes = self.bytes()?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary.persist_noclobber(destination).map_err(|error| {
            if error.error.kind() == io::ErrorKind::AlreadyExists {
                ExportError::AlreadyExists
            } else {
                ExportError::Io(error.error)
            }
        })?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(ExportError::Durability)?;
        Ok(Receipt {
            schema_version: SCHEMA_VERSION,
            byte_length: bytes.len(),
        })
    }

    fn bytes(&self) -> Result<Vec<u8>, ExportError> {
        let mut writer = BoundedBytes::default();
        if let Err(error) = serde_json::to_writer_pretty(&mut writer, self) {
            return Err(if writer.exceeded {
                ExportError::TooLarge
            } else {
                ExportError::Json(error)
            });
        }
        writer.write_all(b"\n").map_err(|_| ExportError::TooLarge)?;
        Ok(writer.bytes)
    }
}

#[derive(Debug, Serialize)]
pub struct Receipt {
    pub schema_version: u32,
    pub byte_length: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("Choose a new file for the diagnostic report.")]
    InvalidDestination,
    #[error("The diagnostic report destination already exists; choose a new file.")]
    AlreadyExists,
    #[error("The diagnostic report exceeded its 256 KiB limit.")]
    TooLarge,
    #[error("The diagnostic report could not be encoded.")]
    Json(#[source] serde_json::Error),
    #[error("The diagnostic report could not be written.")]
    Io(#[from] io::Error),
    #[error("The diagnostic report was saved, but directory durability could not be confirmed.")]
    Durability(#[source] io::Error),
}

impl ExportError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidDestination => "DiagnosticDestinationInvalid",
            Self::AlreadyExists => "DiagnosticAlreadyExists",
            Self::TooLarge => "DiagnosticLimitExceeded",
            Self::Json(_) => "DiagnosticEncodingFailed",
            Self::Io(_) => "DiagnosticWriteFailed",
            Self::Durability(_) => "DiagnosticDurabilityUnconfirmed",
        }
    }
}

#[derive(Default)]
struct BoundedBytes {
    bytes: Vec<u8>,
    exceeded: bool,
}

impl Write for BoundedBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_BYTES.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(io::Error::other("diagnostic size limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
