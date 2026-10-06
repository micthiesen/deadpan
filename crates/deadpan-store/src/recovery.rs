//! What writable opens found and recovered.
//!
//! Opening a package for writing already validates the database, reconciles
//! abandoned worker attempts and keeps every committed revision. This report
//! says what that recovery actually changed, so a host can tell the person
//! truthfully instead of silently interrupting work. Findings are retained in
//! `Reports/recovery-pending.json` until a host acknowledges them, so an
//! intervening headless writer cannot swallow them. They are operational
//! data outside authored history; SQLite stays authoritative.

use std::fs;
use std::io::{Read as _, Write as _};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::StoreError;

const PENDING_NAME: &str = "recovery-pending.json";
/// Interrupted generation attempts the person discarded from the retry list.
const DISMISSED_NAME: &str = "dismissed-interruptions.json";
const MAX_PENDING_BYTES: u64 = 256 * 1024;

/// Bound on the attempts one report lists; the counts stay exact.
pub const MAX_REPORTED_ATTEMPTS: usize = 64;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenRecovery {
    /// The previous writer's marker was still present, so it never closed.
    pub unclean_previous_writer: Option<PreviousWriter>,
    /// Render attempts that were active and are now `Interrupted`.
    pub interrupted_renders: Vec<InterruptedRender>,
    pub interrupted_render_count: usize,
    /// Generation attempts that were active and are now failed `interrupted`.
    pub interrupted_generations: Vec<InterruptedGeneration>,
    pub interrupted_generation_count: usize,
    /// Destination publications reconciled without touching external files.
    pub interrupted_publications: Vec<InterruptedPublication>,
    pub interrupted_publication_count: usize,
    /// Recovery evidence could not be read or recorded (for example on a full
    /// disk). The project still opened; a later crash may then go unreported.
    #[serde(skip)]
    pub record_error: Option<String>,
}

impl OpenRecovery {
    /// Nothing was interrupted and the previous writer closed cleanly.
    pub fn is_clean(&self) -> bool {
        self.unclean_previous_writer.is_none()
            && self.interrupted_render_count == 0
            && self.interrupted_generation_count == 0
            && self.interrupted_publication_count == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviousWriter {
    /// The bounded marker text the earlier writer recorded.
    pub marker: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterruptedRender {
    pub job_id: String,
    pub attempt_id: String,
    pub ordinal: u64,
    /// The retained encoding this attempt was producing or verifying. A
    /// checkpoint can be verified and saved again without encoding.
    pub checkpoint_attempt_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterruptedGeneration {
    pub request_id: String,
    pub attempt_id: String,
    pub hold_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterruptedPublication {
    pub publication_id: String,
    /// The movie name was committed at its destination before the
    /// interruption; its report may be missing.
    pub movie_committed: bool,
}

/// The marker a writer records while it owns the package.
pub(crate) fn writer_marker() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    format!(
        "deadpan writer pid={} opened_unix={seconds}",
        std::process::id()
    )
}

fn merge_list<T: PartialEq>(older: &mut Vec<T>, newer: Vec<T>) {
    for item in newer {
        if !older.contains(&item) {
            older.push(item);
        }
    }
    older.truncate(MAX_REPORTED_ATTEMPTS);
}

impl OpenRecovery {
    /// Earlier unacknowledged findings plus this open's. An attempt is
    /// interrupted at most once, so counts add.
    fn merge(mut self, newer: Self) -> Self {
        if newer.unclean_previous_writer.is_some() {
            self.unclean_previous_writer = newer.unclean_previous_writer;
        }
        self.interrupted_render_count += newer.interrupted_render_count;
        merge_list(&mut self.interrupted_renders, newer.interrupted_renders);
        self.interrupted_generation_count += newer.interrupted_generation_count;
        merge_list(
            &mut self.interrupted_generations,
            newer.interrupted_generations,
        );
        self.interrupted_publication_count += newer.interrupted_publication_count;
        merge_list(
            &mut self.interrupted_publications,
            newer.interrupted_publications,
        );
        self.record_error = newer.record_error;
        self
    }
}

/// Merges this open's findings with retained unacknowledged ones and records
/// the result. Failing to read or record never prevents opening; it is
/// reported in `record_error`.
pub(crate) fn retain_pending(package: &Path, found: OpenRecovery) -> OpenRecovery {
    let mut errors = Vec::new();
    let earlier = match read_pending(package) {
        Ok(earlier) => earlier.unwrap_or_default(),
        Err(error) => {
            errors.push(format!("earlier recovery report unreadable: {error}"));
            OpenRecovery::default()
        }
    };
    let mut merged = earlier.clone().merge(found);
    if merged != earlier
        && !merged.is_clean()
        && let Err(error) = write_pending(package, &merged)
    {
        errors.push(format!("recovery report not recorded: {error}"));
    }
    if let Some(error) = merged.record_error.take() {
        errors.insert(0, error);
    }
    merged.record_error = (!errors.is_empty()).then(|| errors.join("; "));
    merged
}

fn reports(package: &Path) -> Result<std::path::PathBuf, StoreError> {
    let directory = package.join("Reports");
    match fs::symlink_metadata(&directory) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(directory),
        Ok(_) => Err(StoreError::UnsafePath(directory)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&directory)?;
            Ok(directory)
        }
        Err(error) => Err(error.into()),
    }
}

fn read_pending(package: &Path) -> Result<Option<OpenRecovery>, StoreError> {
    let path = package.join("Reports").join(PENDING_NAME);
    let metadata = match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(metadata) => metadata,
    };
    if !metadata.file_type().is_file() {
        return Err(StoreError::UnsafePath(path));
    }
    let mut bytes = Vec::new();
    fs::File::open(&path)?
        .take(MAX_PENDING_BYTES)
        .read_to_end(&mut bytes)?;
    Ok(Some(serde_json::from_slice(&bytes)?))
}

fn write_pending(package: &Path, recovery: &OpenRecovery) -> Result<(), StoreError> {
    let directory = reports(package)?;
    let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
    serde_json::to_writer(&mut temporary, recovery)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(directory.join(PENDING_NAME))
        .map_err(|error| error.error)?;
    fs::File::open(&directory)?.sync_all()?;
    Ok(())
}

pub(crate) fn clear_pending(package: &Path) -> Result<(), StoreError> {
    let directory = package.join("Reports");
    match fs::remove_file(directory.join(PENDING_NAME)) {
        Ok(()) => fs::File::open(&directory)?.sync_all().map_err(Into::into),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// The most dismissals one record keeps. Dismissals are pruned to offered
/// attempts (one per current request), so this is far above real use.
pub const MAX_DISMISSED_GENERATIONS: usize = 1024;

/// Interrupted generation attempts offered for retry or discard.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InterruptedGenerations {
    pub attempts: Vec<InterruptedGeneration>,
    /// Why earlier dismissals could not be read, when they could not; every
    /// offered attempt is then listed.
    pub warning: Option<String>,
}

/// `(request, attempt)` identities dismissed from the interrupted-generation
/// list, and why they could not be read, if so.
pub(crate) struct Dismissals {
    pub(crate) entries: Vec<(String, String)>,
    pub(crate) warning: Option<String>,
}

/// Read the dismissal record. Never fails: a missing record means none, and
/// an unreadable, oversized or corrupt one hides nothing (with a warning)
/// and is replaced by the next dismissal.
pub(crate) fn read_dismissed_generations(package: &Path) -> Dismissals {
    let path = package.join("Reports").join(DISMISSED_NAME);
    let read = || -> Result<Option<Vec<(String, String)>>, String> {
        let metadata = match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.to_string()),
            Ok(metadata) => metadata,
        };
        if !metadata.file_type().is_file() {
            return Err("it is not a regular file".into());
        }
        let mut bytes = Vec::new();
        // One byte past the limit tells an oversized record from a full one;
        // a truncated record is never parsed.
        fs::File::open(&path)
            .and_then(|file| file.take(MAX_PENDING_BYTES + 1).read_to_end(&mut bytes))
            .map_err(|error| error.to_string())?;
        if bytes.len() as u64 > MAX_PENDING_BYTES {
            return Err(format!("it is larger than {MAX_PENDING_BYTES} bytes"));
        }
        let entries: Vec<(String, String)> =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        if entries.len() > MAX_DISMISSED_GENERATIONS {
            return Err(format!(
                "it lists more than {MAX_DISMISSED_GENERATIONS} attempts"
            ));
        }
        Ok(Some(entries))
    };
    match read() {
        Ok(entries) => Dismissals {
            entries: entries.unwrap_or_default(),
            warning: None,
        },
        Err(reason) => Dismissals {
            entries: Vec::new(),
            warning: Some(format!(
                "Earlier discarded AI attempts could not be read ({reason}), so every interrupted attempt is listed again. Discarding one rewrites the record."
            )),
        },
    }
}

pub(crate) fn write_dismissed_generations(
    package: &Path,
    dismissed: &[(String, String)],
) -> Result<(), StoreError> {
    if dismissed.len() > MAX_DISMISSED_GENERATIONS {
        return Err(StoreError::GenerationAttempt(format!(
            "more than {MAX_DISMISSED_GENERATIONS} interrupted attempts are dismissed"
        )));
    }
    let directory = reports(package)?;
    let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
    serde_json::to_writer(&mut temporary, dismissed)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(directory.join(DISMISSED_NAME))
        .map_err(|error| error.error)?;
    fs::File::open(&directory)?.sync_all()?;
    Ok(())
}
