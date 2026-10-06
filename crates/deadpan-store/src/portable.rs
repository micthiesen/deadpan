//! Self-contained portable project copies.
//!
//! `copy_portable` builds a new package that needs nothing outside itself:
//!
//! 1. The source opens read-only (its history is validated), so a running
//!    editor may keep the project open. SQLite's backup API copies one
//!    consistent database snapshot, never the open main file alone.
//! 2. Every original moves into `Media/Originals` as a managed copy: an
//!    existing managed copy is cloned or copied, a linked original is read
//!    from its location, and both are verified against the recorded BLAKE3
//!    and SHA-256 identity before publication. The link is then dropped.
//! 3. Every generated object a retained row references (see
//!    [`crate::storage`]) is copied through a verified snapshot. Discarded
//!    and stale variants, unreferenced objects, abandoned temporaries and
//!    render candidates stay behind; render job records remain as history,
//!    and a retry in the copy reports their missing candidate.
//! 4. The copy is reopened read-only, its complete history recomputed
//!    (`validate --full`, including the hash chain), every copied object
//!    re-verified, and every asset of every head document checked present.
//!
//! The copy is assembled under a hidden sibling staging name and appears at
//! the destination only through one no-replace rename after verification. A
//! failure removes the staging package; a crash can leave a hidden
//! `.<name>-<uuid>.partial.deadpan` directory beside the destination, never
//! a partial package under the requested name.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use deadpan_core::{GeneratedContentId, GeneratedObjectRef};
use rusqlite::{Connection, OpenFlags, backup::Backup};
use serde::Serialize;

use crate::generated_media::GeneratedMediaLimits;
use crate::original_media::{OriginalMediaLimits, OriginalOwnership, OriginalRetentionMethod};
use crate::storage::DEFAULT_GRACE;
use crate::{AccessMode, HistoryValidation, ProjectStore, StoreError, schema};

const MAX_OBJECT_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const ORIGINAL_TIMEOUT: Duration = Duration::from_secs(3600);

#[derive(Debug, Clone, Serialize)]
pub struct PortableOriginal {
    pub content: String,
    pub label: String,
    pub bytes: u64,
    /// `managed` when the source already owned a copy, `linked` when the
    /// bytes were read from the original's linked location.
    pub from: &'static str,
    pub method: OriginalRetentionMethod,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortableObject {
    pub digest: String,
    pub bytes: u64,
}

/// What a portable copy contains and how it was verified.
#[derive(Debug, Clone, Serialize)]
pub struct PortableCopyReport {
    pub schema_version: u32,
    pub source: PathBuf,
    pub destination: PathBuf,
    pub project_id: String,
    pub revision_id: String,
    pub originals: Vec<PortableOriginal>,
    pub generated: Vec<PortableObject>,
    /// Generated objects no retained row references, left behind.
    pub omitted_generated: u64,
    /// Render candidates left behind.
    pub omitted_render_candidates: u64,
    pub total_bytes: u64,
    /// The destination's complete recomputed history.
    pub verified_history: HistoryValidation,
}

fn invalid(message: impl Into<String>) -> StoreError {
    StoreError::Storage(message.into())
}

/// Build a verified self-contained copy of `source` at `destination`, which
/// must not exist and must name a `.deadpan` package in an existing directory.
pub fn copy_portable(
    source: &Path,
    destination: &Path,
    cancelled: &AtomicBool,
) -> Result<PortableCopyReport, StoreError> {
    copy_portable_observed(source, destination, cancelled, &mut || {})
}

/// [`copy_portable`] with `after_database` called once the database
/// snapshot is copied and before any media is listed, so tests can change
/// the source in between.
#[doc(hidden)]
pub fn copy_portable_observed(
    source: &Path,
    destination: &Path,
    cancelled: &AtomicBool,
    after_database: &mut dyn FnMut(),
) -> Result<PortableCopyReport, StoreError> {
    crate::validate_extension(destination)?;
    let check = || {
        if cancelled.load(Ordering::Acquire) {
            Err(invalid("the portable copy was cancelled"))
        } else {
            Ok(())
        }
    };
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| invalid("the destination needs a UTF-8 file name"))?;
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = fs::canonicalize(parent)?;
    let destination = parent.join(name);
    if fs::symlink_metadata(&destination).is_ok() {
        return Err(StoreError::PackageAlreadyExists(destination));
    }
    let source_store = ProjectStore::open(source, AccessMode::ReadOnly)?;
    let source = source_store.package.clone();
    if parent.starts_with(&source) {
        return Err(invalid(
            "the portable copy cannot be inside its source project",
        ));
    }
    let stem = name.strip_suffix(".deadpan").unwrap_or(name);
    let staging = parent.join(format!(
        ".{stem}-{}.partial.deadpan",
        uuid::Uuid::new_v4().simple()
    ));
    fs::create_dir(&staging)?;
    let result = build(
        &source_store,
        &staging,
        &destination,
        cancelled,
        &check,
        after_database,
    );
    match result {
        Ok(mut report) => {
            let published = check().and_then(|()| rename_no_replace(&staging, &destination));
            if let Err(error) = published {
                let _ = fs::remove_dir_all(&staging);
                return Err(error);
            }
            File::open(&parent)?.sync_all()?;
            report.destination = destination;
            Ok(report)
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            Err(error)
        }
    }
}

fn build(
    source: &ProjectStore,
    staging: &Path,
    destination: &Path,
    cancelled: &AtomicBool,
    check: &dyn Fn() -> Result<(), StoreError>,
    after_database: &mut dyn FnMut(),
) -> Result<PortableCopyReport, StoreError> {
    for directory in [
        "Media/Originals",
        "Media/Generated",
        "Media/RenderCandidates",
        "Media/UserAssets",
        "Analysis/Manual",
        "Snapshots",
        "Reports",
    ] {
        fs::create_dir_all(staging.join(directory))?;
    }
    copy_database(&source.package, &staging.join("project.sqlite"))?;
    let project_id = copied_project_id(&staging.join("project.sqlite"))?;
    write_manifest(staging, &project_id)?;
    File::open(staging)?.sync_all()?;
    check()?;

    // References come from the copied database itself, so an edit the
    // source's editor commits meanwhile cannot leave the copy without media.
    let mut copy = ProjectStore::open(staging, AccessMode::ReadWrite)?;
    let head = copy.snapshot()?;
    let references = copy.storage_references()?;
    let pins = references.media;
    after_database();
    let mut originals = Vec::new();
    let mut original_digests = std::collections::BTreeSet::new();
    let original_limits = OriginalMediaLimits::new(MAX_OBJECT_BYTES, ORIGINAL_TIMEOUT)?;
    let mut after = None;
    loop {
        // The copied database's records, not the live source's.
        let page = copy.original_records(after.as_ref(), 1000)?;
        let Some(last) = page.last() else { break };
        after = Some(last.object().content().clone());
        for record in &page {
            check()?;
            original_digests.insert(record.object().content().digest().to_owned());
            let digest = record.object().content().digest();
            let managed_path = source
                .package
                .join("Media/Originals")
                .join(format!("blake3-{digest}"));
            let managed = record.managed()
                && source.original_availability(record)?
                    == crate::original_media::OriginalAvailability::Present;
            let (path, from) = if managed {
                (managed_path, "managed")
            } else if let Some(link) = record.linked() {
                (link.path().to_path_buf(), "linked")
            } else {
                return Err(invalid(format!(
                    "the original \"{}\" is missing; restore or relink it before copying",
                    record.label()
                )));
            };
            let outcome = copy
                .retain_original(
                    &path,
                    OriginalOwnership::Managed,
                    original_limits,
                    cancelled,
                )
                .map_err(|error| {
                    invalid(format!(
                        "could not copy the original \"{}\" from {}: {error}",
                        record.label(),
                        path.display()
                    ))
                })?;
            copy.detach_original_link(record.object().content())?;
            originals.push(PortableOriginal {
                content: record.object().content().to_string(),
                label: record.label().to_owned(),
                bytes: record.object().byte_length(),
                from,
                method: outcome.method,
            });
        }
        if page.len() < 1000 {
            break;
        }
    }

    // List the source's objects only now, after the database snapshot: an
    // object a snapshot row names was published before that row committed,
    // so it is listed unless it has since been removed.
    let limits = GeneratedMediaLimits::new(MAX_OBJECT_BYTES)?;
    let listed = source
        .namespace_storage(crate::object_storage::StorageNamespace::Generated)
        .list_entries()?;
    let mut generated = Vec::new();
    let mut omitted_generated = 0;
    for entry in &listed {
        check()?;
        let crate::object_storage::EntryKind::Object(digest) = &entry.kind else {
            continue;
        };
        if !pins.contains_key(digest) {
            omitted_generated += 1;
            continue;
        }
        let reference = GeneratedObjectRef::new(
            GeneratedContentId::new(digest.clone()).map_err(|error| invalid(error.to_string()))?,
            entry.bytes,
        )
        .map_err(|error| invalid(error.to_string()))?;
        let mut snapshot = source.snapshot_generated_object(&reference, limits)?;
        copy.promote_generated_object(&mut snapshot, &reference, limits)?;
        generated.push(PortableObject {
            digest: digest.clone(),
            bytes: entry.bytes,
        });
    }
    // Every object the copy's rows name in typed form must now be present:
    // an original record or a copied, verified generated object of the
    // recorded length.
    for (digest, length) in &references.typed {
        if original_digests.contains(digest) {
            continue;
        }
        match generated.iter().find(|object| &object.digest == digest) {
            Some(object) if length.is_none_or(|length| length == object.bytes) => {}
            Some(_) => {
                return Err(invalid(format!(
                    "the project's object blake3:{digest} has a different length than its reference"
                )));
            }
            None => {
                return Err(invalid(format!(
                    "the project references blake3:{digest}, which is missing from its media; the copy would not be self-contained"
                )));
            }
        }
    }
    let omitted_render_candidates = source
        .namespace_storage(crate::object_storage::StorageNamespace::RenderCandidates)
        .list_entries()?
        .len() as u64;
    drop(copy);
    check()?;

    // Verify the copy as a stranger would: reopen read-only, recompute the
    // complete history and re-verify every object it now owns.
    let verified = ProjectStore::open(staging, AccessMode::ReadOnly)?;
    verified.validate_full()?;
    let verified_history = verified.opened;
    let copied_head = verified.snapshot()?;
    if copied_head.project_id() != head.project_id()
        || copied_head.revision_id() != head.revision_id()
    {
        return Err(invalid(
            "the copied database names a different project or revision",
        ));
    }
    for object in &generated {
        check()?;
        let reference = GeneratedObjectRef::new(
            GeneratedContentId::new(object.digest.clone())
                .map_err(|error| invalid(error.to_string()))?,
            object.bytes,
        )
        .map_err(|error| invalid(error.to_string()))?;
        verified.snapshot_generated_object(&reference, limits)?;
    }
    let mut after = None;
    loop {
        let page = verified.original_records(after.as_ref(), 1000)?;
        let Some(last) = page.last() else { break };
        after = Some(last.object().content().clone());
        for record in &page {
            check()?;
            if !record.managed() || record.linked().is_some() {
                return Err(invalid(format!(
                    "the copied original \"{}\" is not a managed copy",
                    record.label()
                )));
            }
            verified.snapshot_original(record.object().content(), original_limits, cancelled)?;
        }
        if page.len() < 1000 {
            break;
        }
    }
    let present = verified.storage_report(DEFAULT_GRACE)?;
    let has = |namespace: &str, digest: &str| {
        present.namespace(namespace).is_some_and(|namespace| {
            namespace
                .entries
                .iter()
                .any(|entry| entry.digest.as_deref() == Some(digest))
        })
    };
    for (id, asset) in copied_head.assets() {
        if let Some(digest) = asset.content_hash.strip_prefix("blake3:")
            && !has("generated", digest)
            && !has("originals", digest)
        {
            return Err(invalid(format!(
                "asset {id} of the current revision has no media in the copy"
            )));
        }
    }
    let total_bytes = present.total_bytes;
    drop(verified);
    sync_tree(staging)?;
    Ok(PortableCopyReport {
        schema_version: 1,
        source: source.package.clone(),
        destination: destination.to_path_buf(),
        project_id: head.project_id().as_str().to_owned(),
        revision_id: head.revision_id().as_str().to_owned(),
        originals,
        generated,
        omitted_generated,
        omitted_render_candidates,
        total_bytes,
        verified_history,
    })
}

fn copied_project_id(database: &Path) -> Result<String, StoreError> {
    let connection = Connection::open_with_flags(database, crate::read_flags())?;
    let document: String = connection.query_row(
        "SELECT document FROM revisions WHERE parent_id IS NULL",
        [],
        |row| row.get(0),
    )?;
    let value: serde_json::Value = serde_json::from_str(&document)?;
    value
        .get("project_id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| invalid("the copied database has no project identity"))
}

/// Copy one consistent snapshot of the source database into a standalone
/// rollback-journal file.
fn copy_database(package: &Path, destination: &Path) -> Result<(), StoreError> {
    let source = Connection::open_with_flags(package.join("project.sqlite"), crate::read_flags())?;
    schema::configure(&source)?;
    source.pragma_update(None, "query_only", true)?;
    source.execute_batch("BEGIN DEFERRED")?;
    schema::check_version(&source)?;
    let mut target = Connection::open_with_flags(
        destination,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    schema::configure(&target)?;
    {
        let backup = Backup::new(&source, &mut target)?;
        backup.run_to_completion(256, Duration::from_millis(2), None)?;
    }
    target.pragma_update(None, "journal_mode", "DELETE")?;
    target
        .close()
        .map_err(|(_, error)| StoreError::Database(error))?;
    source.execute_batch("ROLLBACK")?;
    File::open(destination)?.sync_all()?;
    Ok(())
}

fn write_manifest(package: &Path, project_id: &str) -> Result<(), StoreError> {
    #[derive(Serialize)]
    struct Manifest<'a> {
        format: &'static str,
        project_id: &'a str,
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(package.join("manifest.json"))?;
    serde_json::to_writer_pretty(
        &mut file,
        &Manifest {
            format: "deadpan",
            project_id,
        },
    )?;
    writeln!(file)?;
    file.sync_all()?;
    Ok(())
}

/// Synchronize every directory of the copy so the final rename publishes a
/// durable tree. Object files were synchronized when they were published.
fn sync_tree(path: &Path) -> Result<(), StoreError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            sync_tree(&entry?.path())?;
        }
        File::open(path)?.sync_all()?;
    } else if metadata.is_file() {
        File::open(path)?.sync_all()?;
    }
    Ok(())
}

fn rename_no_replace(from: &Path, to: &Path) -> Result<(), StoreError> {
    use rustix::fs::{CWD, RenameFlags, renameat_with};
    renameat_with(CWD, from, CWD, to, RenameFlags::NOREPLACE).map_err(|error| {
        if error == rustix::io::Errno::EXIST {
            StoreError::PackageAlreadyExists(to.to_path_buf())
        } else {
            StoreError::Io(error.into())
        }
    })
}
