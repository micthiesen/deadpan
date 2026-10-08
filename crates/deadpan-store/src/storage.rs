//! Package storage accounting, reference tracking and explicit cleanup.
//!
//! Every object in `Media/Generated`, `Media/Originals` and
//! `Media/RenderCandidates` is named by its BLAKE3 digest. An object is
//! *referenced* when its digest appears in a row that can still need it:
//!
//! - any revision, history entry, navigation patch or Compound step (so every
//!   retained revision, including undone ones and abandoned branches, pins
//!   what it names), registers and macros, the original inventory, source
//!   qualifications and every other operational row;
//! - a generation bundle receipt that is still present for a current request
//!   (a variant that can still be chosen and accepted), or whose own masters
//!   any of the rows above name (an accepted variant keeps all six objects,
//!   which picture admission verifies);
//! - for render candidates, a checkpoint, attempt or publication of a job that
//!   has no confirmed publication.
//!
//! References are found by scanning the stored text for 64-digit lowercase
//! hexadecimal tokens rather than by decoding each structure. A patch carries
//! every value it installs, so a keyframe plus the patches after it mention
//! everything any revision contains. The scan is a superset: a SHA-256 value
//! that happened to equal an object's BLAKE3 digest would pin that object.
//! Over-retention is the only possible error; an object a retained row names
//! is never unreferenced.
//!
//! Cleanup is explicit and offline from editing: it needs the writable store,
//! so no commit, promotion or import of this session interleaves with it, and
//! it holds the render namespace lock while removing render candidates. An
//! entry is removed only if it is unreferenced and has not changed for the
//! grace period, and only while no reader holds its shared lock. Each removal
//! is one `unlink` after re-checking the entry's identity, so a crash leaves
//! each object either present or absent, never partial; no database row
//! changes.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant, SystemTime};

use rusqlite::types::ValueRef;
use serde::Serialize;

use crate::object_storage::{
    EntryKind, NamespaceEntry, ObjectControl, ObjectStorageError, Removal, StorageNamespace,
};
use crate::{AccessMode, ProjectStore, StoreError};

/// The default grace period: an unreferenced object younger than this is
/// kept, so a write that has published bytes but not yet committed the row
/// that names them is never mistaken for garbage.
pub const DEFAULT_GRACE: Duration = Duration::from_secs(24 * 60 * 60);

/// Why a row can need an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceClass {
    /// A retained revision, history entry, navigation patch or Compound step.
    History,
    /// Saved registers and macros.
    Registers,
    /// A bundle receipt that can still be accepted or was accepted.
    GenerationReceipt,
    /// Generation requests, attempts and other generation records.
    GenerationRecords,
    /// The original inventory, provenance and source qualifications.
    Originals,
    /// A render job's checkpoint, attempt or publication while the job can
    /// still need its candidate.
    RenderJob,
    /// A recovery checkpoint database under `Snapshots`.
    Checkpoint,
    /// Any other stored row.
    OtherRecords,
}

fn class_of(table: &str) -> ReferenceClass {
    match table {
        "revisions" | "history" | "revision_patches" | "transaction_steps" | "redo" | "state" => {
            ReferenceClass::History
        }
        "registers" | "register_contents" | "register_state" => ReferenceClass::Registers,
        "original_media" | "original_provenance" | "source_qualifications" | "single_source" => {
            ReferenceClass::Originals
        }
        table if table.starts_with("generation_") || table == "hold_request_clocks" => {
            ReferenceClass::GenerationRecords
        }
        table if table.starts_with("render_") => ReferenceClass::RenderJob,
        _ => ReferenceClass::OtherRecords,
    }
}

/// What an entry is and whether cleanup may remove it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum EntryState {
    Referenced {
        by: Vec<ReferenceClass>,
    },
    /// No retained row names it.
    Unreferenced,
    /// An unpublished temporary of an interrupted or running write.
    Pending,
    /// A damaged object moved aside by restore; kept for diagnosis.
    Damaged,
    /// Not an entry this namespace creates. Never removed.
    Unexpected,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntryReport {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    pub bytes: u64,
    /// Seconds since the entry last changed.
    pub age_seconds: u64,
    #[serde(flatten)]
    pub state: EntryState,
    /// Cleanup would remove it under the report's grace period.
    pub removable: bool,
    #[serde(skip)]
    entry: Option<NamespaceEntry>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct NamespaceReport {
    pub namespace: &'static str,
    pub entries: Vec<EntryReport>,
    pub referenced_bytes: u64,
    pub unreferenced_bytes: u64,
    pub pending_bytes: u64,
    pub other_bytes: u64,
    pub removable_bytes: u64,
    pub removable_entries: u64,
}

/// A complete account of the package's media namespaces and database.
#[derive(Debug, Clone, Serialize)]
pub struct StorageReport {
    pub schema_version: u32,
    pub grace_seconds: u64,
    pub database_bytes: u64,
    pub namespaces: Vec<NamespaceReport>,
    /// Bytes under `Snapshots`, `Backups`, `Reports` and `Analysis`.
    pub auxiliary_bytes: u64,
    /// Checkpoint databases whose references were included.
    pub checkpoints: Vec<String>,
    /// `Snapshots` entries that could not be read; cleanup refuses while any
    /// exists, because their references are unknown.
    pub unreadable_checkpoints: Vec<String>,
    pub total_bytes: u64,
    pub removable_bytes: u64,
    /// What the retained history occupies inside the database.
    pub history: HistoryUsage,
    /// The [retention policy](crate::generation_retention) for offered,
    /// unaccepted AI variants and its current state.
    pub variant_retention: crate::generation_retention::VariantRetentionReport,
}

/// The stored size of the project's history: every revision stays (see
/// docs/BACKUPS.md, "History limits"), as keyframe documents every
/// `MAX_PATCH_CHAIN` revisions plus one patch per revision.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct HistoryUsage {
    pub revisions: u64,
    pub edits: u64,
    pub keyframes: u64,
    /// Complete documents stored at keyframes.
    pub keyframe_bytes: u64,
    /// Edit history entries (request, forward and inverse patch) and undo or
    /// redo patches.
    pub patch_bytes: u64,
}

fn history_usage(connection: &rusqlite::Connection) -> Result<HistoryUsage, StoreError> {
    let unsigned = |value: i64| u64::try_from(value).unwrap_or(0);
    let (revisions, keyframes, keyframe_bytes): (i64, i64, i64) = connection.query_row(
        "SELECT COUNT(*), COALESCE(SUM(document != 'null'),0),
                COALESCE(SUM(CASE WHEN document != 'null' THEN length(CAST(document AS BLOB)) ELSE 0 END),0)
         FROM revisions",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let (edits, edit_bytes): (i64, i64) = connection.query_row(
        "SELECT COUNT(*), COALESCE(SUM(length(CAST(request AS BLOB)) + length(CAST(edit AS BLOB))),0) FROM history",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let navigation: i64 = connection.query_row(
        "SELECT COALESCE(SUM(length(CAST(patch AS BLOB))),0) FROM revision_patches",
        [],
        |row| row.get(0),
    )?;
    Ok(HistoryUsage {
        revisions: unsigned(revisions),
        edits: unsigned(edits),
        keyframes: unsigned(keyframes),
        keyframe_bytes: unsigned(keyframe_bytes),
        patch_bytes: unsigned(edit_bytes).saturating_add(unsigned(navigation)),
    })
}

impl StorageReport {
    pub fn namespace(&self, name: &str) -> Option<&NamespaceReport> {
        self.namespaces
            .iter()
            .find(|namespace| namespace.namespace == name)
    }
}

/// What explicit cleanup may remove.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CleanupPolicy {
    pub grace: Duration,
    /// Report what would be removed without removing anything.
    pub dry_run: bool,
    /// Unreferenced generated objects: discarded and stale variants, and
    /// objects whose receipt was never committed.
    pub generated: bool,
    /// Render candidates of published jobs and unreferenced render objects.
    pub render_candidates: bool,
    /// Original copies that no inventory record or other row names.
    pub originals: bool,
    /// Abandoned `.pending-*` temporaries.
    pub pending: bool,
}

impl CleanupPolicy {
    pub const fn everything(grace: Duration, dry_run: bool) -> Self {
        Self {
            grace,
            dry_run,
            generated: true,
            render_candidates: true,
            originals: true,
            pending: true,
        }
    }

    /// Only unreferenced objects in `Media/Generated`: what the automatic
    /// retention pass removes. Originals, render candidates and unfinished
    /// writes are left to explicit cleanup.
    pub const fn generated_only(grace: Duration, dry_run: bool) -> Self {
        Self {
            grace,
            dry_run,
            generated: true,
            render_candidates: false,
            originals: false,
            pending: false,
        }
    }

    fn selects(&self, namespace: StorageNamespace, state: &EntryState) -> bool {
        let selected = match namespace {
            StorageNamespace::Generated => self.generated,
            StorageNamespace::Originals => self.originals,
            StorageNamespace::RenderCandidates => self.render_candidates,
        };
        match state {
            EntryState::Unreferenced => selected,
            EntryState::Pending => self.pending && selected,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemovedEntry {
    pub namespace: &'static str,
    pub name: String,
    pub bytes: u64,
    /// Device and inode as listed: a confirmed removal touches only this
    /// exact file, never a later object under the same name.
    #[serde(skip)]
    identity: Option<(i128, i128)>,
}

/// A previewed entry in a form a caller can retain and send back, including
/// the device and inode that bind a removal to exactly the listed file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedRemoval {
    pub namespace: String,
    pub name: String,
    pub bytes: u64,
    /// Decimal device and inode, both or neither.
    pub identity: Option<[String; 2]>,
}

impl RemovedEntry {
    /// This entry as a retained plan line.
    pub fn planned(&self) -> PlannedRemoval {
        PlannedRemoval {
            namespace: self.namespace.to_owned(),
            name: self.name.clone(),
            bytes: self.bytes,
            identity: self
                .identity
                .map(|(device, inode)| [device.to_string(), inode.to_string()]),
        }
    }

    /// A retained plan line, for [`ProjectStore::clean_previewed_storage`],
    /// which removes it only if a fresh scan still finds that exact file
    /// (namespace, name, device and inode) removable.
    pub fn from_planned(planned: &PlannedRemoval) -> Result<Self, StoreError> {
        let invalid = || StoreError::Storage("the cleanup plan names an unknown entry".into());
        let namespace = NAMESPACES
            .iter()
            .map(|(_, name)| *name)
            .find(|name| *name == planned.namespace)
            .ok_or_else(invalid)?;
        let identity = planned
            .identity
            .as_ref()
            .map(|[device, inode]| {
                Ok::<_, StoreError>((
                    device.parse().map_err(|_| invalid())?,
                    inode.parse().map_err(|_| invalid())?,
                ))
            })
            .transpose()?;
        Ok(Self {
            namespace,
            name: planned.name.clone(),
            bytes: planned.bytes,
            identity,
        })
    }

    #[doc(hidden)]
    pub fn for_test(namespace: &'static str, name: &str, bytes: u64) -> Self {
        Self {
            namespace,
            name: name.to_owned(),
            bytes,
            identity: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CleanupOutcome {
    pub dry_run: bool,
    pub removed: Vec<RemovedEntry>,
    pub removed_bytes: u64,
    /// Kept because a reader holds the object.
    pub in_use: Vec<RemovedEntry>,
    /// Kept because the entry changed after it was listed.
    pub changed: Vec<RemovedEntry>,
}

const NAMESPACES: [(StorageNamespace, &str); 3] = [
    (StorageNamespace::Generated, "generated"),
    (StorageNamespace::Originals, "originals"),
    (StorageNamespace::RenderCandidates, "render_candidates"),
];

/// Every hexadecimal digest named by stored rows, by namespace rule.
#[derive(Debug, Default)]
pub(crate) struct References {
    /// Pins generated and original objects.
    pub(crate) media: BTreeMap<String, BTreeSet<ReferenceClass>>,
    /// Pins render candidates.
    pub(crate) render: BTreeMap<String, BTreeSet<ReferenceClass>>,
    /// Objects named in typed form by the rows that pin media: a serialized
    /// object reference (`{"algorithm":"blake3","digest":…},"byte_length":N`)
    /// or a `blake3:<digest>` content identity. These must exist; the length
    /// is known for the first form.
    pub(crate) typed: BTreeMap<String, Option<u64>>,
    /// `Snapshots/` checkpoint databases read.
    pub(crate) checkpoints: Vec<String>,
    /// `Snapshots/` entries that could not be read as a database. Cleanup
    /// refuses to run while any exists.
    pub(crate) unreadable_checkpoints: Vec<String>,
}

/// Every maximal run of exactly 64 lowercase hexadecimal digits.
fn digests(text: &str, mut found: impl FnMut(&str)) {
    let bytes = text.as_bytes();
    let hex = |byte: u8| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte);
    let mut start = 0;
    while start < bytes.len() {
        if !hex(bytes[start]) {
            start += 1;
            continue;
        }
        let mut end = start;
        while end < bytes.len() && hex(bytes[end]) {
            end += 1;
        }
        if end - start == 64 {
            found(&text[start..end]);
        }
        start = end;
    }
}

const TYPED_REFERENCE: &str = "\"algorithm\":\"blake3\",\"digest\":\"";
const TYPED_LENGTH: &str = "\"},\"byte_length\":";
const CONTENT_ID: &str = "blake3:";

/// Typed object references in `text`: serialized references with their
/// byte length, and `blake3:<digest>` identities without one.
fn typed_references(text: &str, mut found: impl FnMut(&str, Option<u64>)) {
    let hex = |digest: &str| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    };
    let mut rest = text;
    while let Some(at) = rest.find(TYPED_REFERENCE) {
        rest = &rest[at + TYPED_REFERENCE.len()..];
        let Some(digest) = rest.get(..64).filter(|digest| hex(digest)) else {
            continue;
        };
        let length = rest[64..].strip_prefix(TYPED_LENGTH).and_then(|tail| {
            let end = tail
                .find(|character: char| !character.is_ascii_digit())
                .unwrap_or(tail.len());
            tail[..end].parse().ok()
        });
        found(digest, length);
    }
    let mut rest = text;
    while let Some(at) = rest.find(CONTENT_ID) {
        rest = &rest[at + CONTENT_ID.len()..];
        if let Some(digest) = rest.get(..64).filter(|digest| hex(digest))
            && !rest[64..]
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_hexdigit())
        {
            found(digest, None);
        }
    }
}

impl References {
    /// Pin media named by `text`, including its typed references.
    fn media(&mut self, text: &str, class: ReferenceClass) {
        digests(text, |digest| {
            self.media
                .entry(digest.to_owned())
                .or_default()
                .insert(class);
        });
        typed_references(text, |digest, length| {
            let known = self.typed.entry(digest.to_owned()).or_insert(length);
            if known.is_none() {
                *known = length;
            }
        });
    }

    fn render(&mut self, text: &str, class: ReferenceClass) {
        digests(text, |digest| {
            self.render
                .entry(digest.to_owned())
                .or_default()
                .insert(class);
        });
    }
}

fn scan_row(
    row: &rusqlite::Row<'_>,
    columns: usize,
    mut text: impl FnMut(&str),
) -> Result<(), StoreError> {
    for index in 0..columns {
        if let ValueRef::Text(bytes) = row.get_ref(index)?
            && let Ok(value) = std::str::from_utf8(bytes)
        {
            text(value);
        }
    }
    Ok(())
}

fn table_names(connection: &rusqlite::Connection) -> Result<Vec<String>, StoreError> {
    Ok(connection
        .prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )?
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?)
}

fn scan_table(
    connection: &rusqlite::Connection,
    table: &str,
    mut text: impl FnMut(&str),
) -> Result<(), StoreError> {
    let mut statement =
        connection.prepare(&format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")))?;
    let columns = statement.column_count();
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        scan_row(row, columns, &mut text)?;
    }
    Ok(())
}

/// Render jobs whose retained candidate is no longer needed: a confirmed
/// published movie, or a latest attempt that ended Failed or Cancelled.
/// Interrupted, Verified and active jobs keep their candidates for the
/// explicit retry and publication paths that resume them.
fn released_render_jobs(
    connection: &rusqlite::Connection,
    tables: &[String],
) -> Result<BTreeSet<String>, StoreError> {
    let mut released = BTreeSet::new();
    if tables.iter().any(|table| table == "render_publications") {
        let mut statement = connection.prepare("SELECT job_id, body FROM render_publications")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let job: String = row.get(0)?;
            let body: String = row.get(1)?;
            let body: serde_json::Value = serde_json::from_str(&body)?;
            if body.get("outcome").and_then(serde_json::Value::as_str) == Some("published") {
                released.insert(job);
            }
        }
    }
    if tables.iter().any(|table| table == "render_job_heads") {
        let mut statement = connection.prepare(
            "SELECT h.job_id FROM render_job_heads h
             JOIN render_attempts a ON a.job_id=h.job_id AND a.attempt_id=h.latest_attempt_id
             WHERE a.state IN ('failed','cancelled')",
        )?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            released.insert(row.get(0)?);
        }
    }
    Ok(released)
}

impl ProjectStore {
    /// The digests every retained row names, read from one consistent
    /// database snapshot, plus every checkpoint database under `Snapshots`
    /// and every published backup under `Backups`.
    pub(crate) fn storage_references(&self) -> Result<References, StoreError> {
        // A later build's tables may name objects this build cannot see.
        if let Some(found) = self.newer_schema {
            return Err(StoreError::NewerSchema {
                found,
                supported: crate::schema::VERSION,
            });
        }
        let mut references = References::default();
        scan_database(&self.connection, &mut references)?;
        self.scan_checkpoints(&mut references);
        Ok(references)
    }

    /// Checkpoints are restorable copies of the database: everything they
    /// mention stays pinned, without liveness exceptions.
    fn scan_checkpoints(&self, references: &mut References) {
        let mut names = Vec::new();
        if let Ok(entries) = std::fs::read_dir(self.package.join("Snapshots")) {
            names.extend(entries.flatten().map(|entry| entry.path()));
        }
        // Published backups pin like checkpoints. Their hidden staging files
        // are skipped: a copy in progress names only objects the live
        // database already pins, and the grace period covers its lifetime.
        // Fail closed: if the backups cannot be listed, what they pin is
        // unknown and cleanup refuses.
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        match crate::backups::list_backups(&self.package) {
            Ok(backups) => names.extend(backups.into_iter().map(|backup| backup.path)),
            Err(_) => references.unreadable_checkpoints.push("Backups".into()),
        }
        names.sort();
        for path in names {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            // SQLite sidecars belong to the database beside them.
            if ["-wal", "-shm", "-journal"]
                .iter()
                .any(|suffix| name.ends_with(suffix))
            {
                continue;
            }
            let scanned = (|| -> Result<(), StoreError> {
                let metadata = std::fs::symlink_metadata(&path)?;
                if !metadata.is_file() {
                    return Err(StoreError::UnsafePath(path.clone()));
                }
                let connection = rusqlite::Connection::open_with_flags(&path, crate::read_flags())?;
                connection.pragma_update(None, "query_only", true)?;
                let transaction = connection.unchecked_transaction()?;
                for table in table_names(&transaction)? {
                    scan_table(&transaction, &table, |text| {
                        references.media(text, ReferenceClass::Checkpoint);
                        references.render(text, ReferenceClass::Checkpoint);
                    })?;
                }
                Ok(())
            })();
            match scanned {
                Ok(()) => references.checkpoints.push(name),
                Err(_) => references.unreadable_checkpoints.push(name),
            }
        }
    }

    /// Account for every media namespace entry and the database. Available
    /// to read-only stores; nothing is hashed or changed.
    pub fn storage_report(&self, grace: Duration) -> Result<StorageReport, StoreError> {
        let references = self.storage_references()?;
        let now = SystemTime::now();
        let mut namespaces = Vec::new();
        for (namespace, name) in NAMESPACES {
            let entries = self.namespace_storage(namespace).list_entries()?;
            let pins = if namespace == StorageNamespace::RenderCandidates {
                &references.render
            } else {
                &references.media
            };
            let mut report = NamespaceReport {
                namespace: name,
                ..NamespaceReport::default()
            };
            for entry in entries {
                let age = now.duration_since(entry.changed).unwrap_or_default();
                let old = age >= grace;
                let (digest, state) = match &entry.kind {
                    EntryKind::Object(digest) => (
                        Some(digest.clone()),
                        match pins.get(digest) {
                            Some(by) => EntryState::Referenced {
                                by: by.iter().copied().collect(),
                            },
                            None => EntryState::Unreferenced,
                        },
                    ),
                    EntryKind::Pending => (None, EntryState::Pending),
                    EntryKind::Damaged => (None, EntryState::Damaged),
                    EntryKind::Unexpected => (None, EntryState::Unexpected),
                };
                let removable =
                    old && matches!(state, EntryState::Unreferenced | EntryState::Pending);
                match state {
                    EntryState::Referenced { .. } => report.referenced_bytes += entry.bytes,
                    EntryState::Unreferenced => report.unreferenced_bytes += entry.bytes,
                    EntryState::Pending => report.pending_bytes += entry.bytes,
                    EntryState::Damaged | EntryState::Unexpected => {
                        report.other_bytes += entry.bytes;
                    }
                }
                if removable {
                    report.removable_bytes += entry.bytes;
                    report.removable_entries += 1;
                }
                report.entries.push(EntryReport {
                    name: entry.name.clone(),
                    digest,
                    bytes: entry.bytes,
                    age_seconds: age.as_secs(),
                    state,
                    removable,
                    entry: Some(entry),
                });
            }
            namespaces.push(report);
        }
        let database_bytes = ["project.sqlite", "project.sqlite-wal", "project.sqlite-shm"]
            .iter()
            .filter_map(|name| std::fs::symlink_metadata(self.package.join(name)).ok())
            .filter(std::fs::Metadata::is_file)
            .map(|metadata| metadata.len())
            .sum::<u64>();
        let auxiliary_bytes = ["Snapshots", "Backups", "Reports", "Analysis"]
            .iter()
            .map(|name| tree_bytes(&self.package.join(name)))
            .sum::<u64>();
        let media: u64 = namespaces
            .iter()
            .map(|namespace| {
                namespace.referenced_bytes
                    + namespace.unreferenced_bytes
                    + namespace.pending_bytes
                    + namespace.other_bytes
            })
            .sum();
        let removable_bytes = namespaces
            .iter()
            .map(|namespace| namespace.removable_bytes)
            .sum();
        let variant_retention = self.variant_retention(&namespaces, now)?;
        Ok(StorageReport {
            schema_version: 1,
            grace_seconds: grace.as_secs(),
            database_bytes,
            namespaces,
            auxiliary_bytes,
            checkpoints: references.checkpoints,
            unreadable_checkpoints: references.unreadable_checkpoints,
            total_bytes: database_bytes + auxiliary_bytes + media,
            removable_bytes,
            history: history_usage(&self.connection)?,
            variant_retention,
        })
    }

    /// The retention policy's counts, plus evicted variants whose own
    /// objects are still listed unreferenced in `Media/Generated`.
    fn variant_retention(
        &self,
        namespaces: &[NamespaceReport],
        now: SystemTime,
    ) -> Result<crate::generation_retention::VariantRetentionReport, StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        let mut report = crate::generation_retention::retention_report(
            &transaction,
            now,
            crate::generation_retention::DEFAULT_VARIANT_RETENTION,
        )?;
        let evicted = crate::generation_retention::evicted_objects(&transaction)?;
        transaction.commit()?;
        let unreferenced: BTreeMap<&str, u64> = namespaces
            .iter()
            .filter(|namespace| namespace.namespace == "generated")
            .flat_map(|namespace| &namespace.entries)
            .filter(|entry| entry.state == EntryState::Unreferenced)
            .filter_map(|entry| Some((entry.digest.as_deref()?, entry.bytes)))
            .collect();
        let mut counted = BTreeSet::new();
        for own in evicted {
            let mut waiting = false;
            for digest in own.iter().flatten() {
                if let Some(bytes) = unreferenced.get(digest.as_str()) {
                    waiting = true;
                    if counted.insert(digest.clone()) {
                        report.evicted_awaiting_cleanup_bytes += bytes;
                    }
                }
            }
            report.evicted_awaiting_cleanup += u64::from(waiting);
        }
        Ok(report)
    }

    /// Remove unreferenced objects and abandoned temporaries that `policy`
    /// selects and that have not changed for its grace period. Requires the
    /// writable store. Objects a reader holds and entries that changed since
    /// they were listed are kept and reported.
    pub fn clean_storage(&mut self, policy: CleanupPolicy) -> Result<CleanupOutcome, StoreError> {
        self.clean_storage_selected(policy, None)
    }

    /// What cleanup would remove under `grace`, from any store, read-only
    /// included, so a preview never waits for the project writer. Pass the
    /// result to [`Self::clean_previewed_storage`] on the writer.
    pub fn preview_storage_cleanup(&self, grace: Duration) -> Result<CleanupOutcome, StoreError> {
        self.preview_storage_cleanup_with(CleanupPolicy::everything(grace, true))
    }

    /// [`Self::preview_storage_cleanup`] limited to what `policy` selects.
    pub fn preview_storage_cleanup_with(
        &self,
        policy: CleanupPolicy,
    ) -> Result<CleanupOutcome, StoreError> {
        let report = self.storage_report(policy.grace)?;
        let mut outcome = CleanupOutcome {
            dry_run: true,
            ..CleanupOutcome::default()
        };
        for (kind, name) in NAMESPACES {
            let Some(namespace) = report.namespace(name) else {
                continue;
            };
            for entry in &namespace.entries {
                if !entry.removable || !policy.selects(kind, &entry.state) {
                    continue;
                }
                outcome.removed_bytes += entry.bytes;
                outcome.removed.push(RemovedEntry {
                    namespace: namespace.namespace,
                    name: entry.name.clone(),
                    bytes: entry.bytes,
                    identity: entry.entry.as_ref().and_then(NamespaceEntry::identity),
                });
            }
        }
        Ok(outcome)
    }

    /// Remove exactly the entries a previous dry run listed, intersected
    /// with what a fresh scan still finds removable under the same grace
    /// period, matched by namespace, name, device and inode. Anything new
    /// since the preview is left alone.
    pub fn clean_previewed_storage(
        &mut self,
        grace: Duration,
        previewed: &[RemovedEntry],
    ) -> Result<CleanupOutcome, StoreError> {
        self.clean_storage_selected(CleanupPolicy::everything(grace, false), Some(previewed))
    }

    /// [`Self::clean_previewed_storage`] limited to what `policy` selects
    /// (its `dry_run` is ignored: this removes).
    pub fn clean_previewed_storage_with(
        &mut self,
        policy: CleanupPolicy,
        previewed: &[RemovedEntry],
    ) -> Result<CleanupOutcome, StoreError> {
        self.clean_storage_selected(
            CleanupPolicy {
                dry_run: false,
                ..policy
            },
            Some(previewed),
        )
    }

    fn clean_storage_selected(
        &mut self,
        policy: CleanupPolicy,
        previewed: Option<&[RemovedEntry]>,
    ) -> Result<CleanupOutcome, StoreError> {
        if self.mode != AccessMode::ReadWrite {
            return Err(StoreError::ReadOnly);
        }
        // Render workers of a closed session publish under this lock; hold it
        // so no candidate appears or is read for verification mid-cleanup.
        // The wait is short: a busy render means "try again later".
        let cancelled = std::sync::atomic::AtomicBool::new(false);
        let deadline = Instant::now() + Duration::from_secs(2);
        let render_guard = if policy.render_candidates && !policy.dry_run {
            Some(
                self.render_storage
                    .lock_render_namespace(ObjectControl::bounded(deadline, &cancelled))
                    .map_err(|error| match error {
                        ObjectStorageError::DeadlineExceeded => StoreError::Storage(
                            "a render is writing its candidate; try cleanup again when it finishes"
                                .into(),
                        ),
                        error => StoreError::Storage(error.to_string()),
                    })?,
            )
        } else {
            None
        };
        // No backup may be copying while references are computed and objects
        // removed: its snapshot may name objects the live database no longer
        // pins. Backups wait for this; cleanup waits at most two seconds.
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        let _backups_quiet = if policy.dry_run {
            None
        } else {
            Some(crate::backups::exclude_backups(
                &self.package,
                Duration::from_secs(2),
            )?)
        };
        let report = self.storage_report(policy.grace)?;
        if !policy.dry_run && !report.unreadable_checkpoints.is_empty() {
            return Err(StoreError::Storage(format!(
                "cleanup cannot read the checkpoint(s) or backups {} under Snapshots or Backups, so it cannot tell what they reference; remove or repair them first",
                report.unreadable_checkpoints.join(", ")
            )));
        }
        let mut outcome = CleanupOutcome {
            dry_run: policy.dry_run,
            ..CleanupOutcome::default()
        };
        for (namespace, name) in NAMESPACES {
            let Some(listed) = report.namespace(name) else {
                continue;
            };
            let storage = self.namespace_storage(namespace);
            for entry in &listed.entries {
                if !entry.removable || !policy.selects(namespace, &entry.state) {
                    continue;
                }
                let Some(listed_entry) = &entry.entry else {
                    continue;
                };
                let record = RemovedEntry {
                    namespace: name,
                    name: entry.name.clone(),
                    bytes: entry.bytes,
                    identity: listed_entry.identity(),
                };
                if let Some(previewed) = previewed
                    && !previewed.iter().any(|previewed| {
                        previewed.namespace == record.namespace
                            && previewed.name == record.name
                            && previewed.identity.is_some()
                            && previewed.identity == record.identity
                    })
                {
                    continue;
                }
                if policy.dry_run {
                    outcome.removed_bytes += entry.bytes;
                    outcome.removed.push(record);
                    continue;
                }
                match storage.remove_entry(listed_entry)? {
                    Removal::Removed => {
                        outcome.removed_bytes += entry.bytes;
                        outcome.removed.push(record);
                    }
                    Removal::InUse => outcome.in_use.push(record),
                    Removal::Changed => outcome.changed.push(record),
                    Removal::Missing => {}
                }
            }
        }
        drop(render_guard);
        Ok(outcome)
    }

    pub(crate) fn namespace_storage(&self, namespace: StorageNamespace) -> NamespaceStorage<'_> {
        match namespace {
            StorageNamespace::Generated => NamespaceStorage::Generated(&self.generated_storage),
            StorageNamespace::Originals => NamespaceStorage::Object(&self.original_storage),
            StorageNamespace::RenderCandidates => NamespaceStorage::Object(&self.render_storage),
        }
    }
}

pub(crate) enum NamespaceStorage<'a> {
    Generated(&'a crate::generated_media::GeneratedStorage),
    Object(&'a crate::object_storage::ObjectStorage),
}

impl NamespaceStorage<'_> {
    pub(crate) fn list_entries(&self) -> Result<Vec<NamespaceEntry>, StoreError> {
        match self {
            Self::Generated(storage) => storage.list_entries(),
            Self::Object(storage) => storage.list_entries(),
        }
        .map_err(|error| StoreError::Storage(error.to_string()))
    }

    fn remove_entry(&self, entry: &NamespaceEntry) -> Result<Removal, StoreError> {
        match self {
            Self::Generated(storage) => storage.remove_entry(entry),
            Self::Object(storage) => storage.remove_entry(entry),
        }
        .map_err(|error| StoreError::Storage(error.to_string()))
    }
}

/// Every digest named by a row of `connection` other than bundle receipts
/// and render rows: the rows whose naming makes a variant *accepted* in
/// [`scan_database`]. Reads within the caller's transaction, if any.
pub(crate) fn non_receipt_media_digests(
    connection: &rusqlite::Connection,
) -> Result<BTreeSet<String>, StoreError> {
    let mut found = BTreeSet::new();
    for table in table_names(connection)? {
        if table == "generation_bundle_receipts" || table.starts_with("render_") {
            continue;
        }
        scan_table(connection, &table, |text| {
            digests(text, |digest| {
                found.insert(digest.to_owned());
            });
        })?;
    }
    Ok(found)
}

/// Scan one database's rows into `references` with the liveness rules.
fn scan_database(
    connection: &rusqlite::Connection,
    references: &mut References,
) -> Result<(), StoreError> {
    let transaction = connection.unchecked_transaction()?;
    let tables = table_names(&transaction)?;
    let released_jobs = released_render_jobs(&transaction, &tables)?;
    for table in &tables {
        if table == "generation_bundle_receipts" || table.starts_with("render_") {
            continue;
        }
        let class = class_of(table);
        scan_table(&transaction, table, |text| references.media(text, class))?;
    }
    // A live bundle receipt pins every output and conditioning object it names.
    if tables
        .iter()
        .any(|table| table == "generation_bundle_receipts")
    {
        let mut statement = transaction.prepare(
            "SELECT b.bundle, b.availability, coalesce(q.relevance,''),
                    json_extract(b.bundle,'$.native_object.content.digest'),
                    json_extract(b.bundle,'$.sampled_object.content.digest'),
                    json_extract(b.bundle,'$.provenance_object.content.digest')
             FROM generation_bundle_receipts b
             LEFT JOIN generation_requests q ON q.request_id=b.request_id",
        )?;
        let mut rows = statement.query([])?;
        let mut live = Vec::new();
        while let Some(row) = rows.next()? {
            let bundle: String = row.get(0)?;
            let availability: String = row.get(1)?;
            let relevance: String = row.get(2)?;
            let own: Vec<Option<String>> = vec![row.get(3)?, row.get(4)?, row.get(5)?];
            let accepted = own
                .iter()
                .flatten()
                .any(|digest| references.media.contains_key(digest));
            if (availability == "present" && relevance == "current") || accepted {
                live.push(bundle);
            }
        }
        for bundle in live {
            references.media(&bundle, ReferenceClass::GenerationReceipt);
        }
    }
    // Render rows pin render candidates until the job is released.
    let render_rows: [(&str, &str); 4] = [
        (
            "render_candidate_checkpoints",
            "SELECT job_id, media FROM render_candidate_checkpoints",
        ),
        (
            "render_attempts",
            "SELECT job_id, body FROM render_attempts",
        ),
        (
            "render_publications",
            "SELECT job_id, body FROM render_publications",
        ),
        (
            "render_publication_operations",
            "SELECT p.job_id, o.body FROM render_publication_operations o JOIN render_publications p ON p.publication_id=o.publication_id",
        ),
    ];
    for (table, query) in render_rows {
        if !tables.iter().any(|name| name == table) {
            continue;
        }
        let mut statement = transaction.prepare(query)?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let job: String = row.get(0)?;
            if released_jobs.contains(&job) {
                continue;
            }
            if let ValueRef::Text(bytes) = row.get_ref(1)?
                && let Ok(text) = std::str::from_utf8(bytes)
            {
                references.render(text, ReferenceClass::RenderJob);
            }
        }
    }
    Ok(())
}

fn tree_bytes(path: &std::path::Path) -> u64 {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.is_file() {
        return metadata.len();
    }
    if !metadata.is_dir() {
        return 0;
    }
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| tree_bytes(&entry.path()))
                .sum()
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_complete_64_digit_lowercase_runs_are_digests() {
        let a = "a".repeat(64);
        let long = "b".repeat(65);
        let upper = "C".repeat(64);
        let text = format!(
            "{{\"digest\":\"{a}\",\"x\":\"{long}\",\"y\":\"{upper}\",\"name\":\"blake3-{}\"}}",
            "0123456789abcdef".repeat(4)
        );
        let mut found = Vec::new();
        digests(&text, |digest| found.push(digest.to_owned()));
        assert_eq!(found, vec![a, "0123456789abcdef".repeat(4)]);
    }

    #[test]
    fn typed_references_carry_lengths_and_content_identities() {
        let a = "a".repeat(64);
        let b = "b".repeat(64);
        let text = format!(
            "{{\"object\":{{\"content\":{{\"algorithm\":\"blake3\",\"digest\":\"{a}\"}},\"byte_length\":42}},\"hash\":\"blake3:{b}\",\"bad\":\"blake3:{a}0\"}}"
        );
        let mut found = Vec::new();
        typed_references(&text, |digest, length| {
            found.push((digest.to_owned(), length))
        });
        assert_eq!(found, vec![(a, Some(42)), (b, None)]);
    }

    /// Candidates stay pinned while their job can still need them: until a
    /// confirmed publication or a terminal Failed/Cancelled latest attempt.
    #[test]
    fn render_candidates_are_released_only_by_publication_or_terminal_failure() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE render_candidate_checkpoints (job_id TEXT, attempt_id TEXT, media TEXT);
                 CREATE TABLE render_attempts (job_id TEXT, attempt_id TEXT, state TEXT, body TEXT);
                 CREATE TABLE render_job_heads (job_id TEXT, latest_attempt_id TEXT);
                 CREATE TABLE render_publications (publication_id TEXT, job_id TEXT, body TEXT);
                 CREATE TABLE render_publication_operations (publication_id TEXT, body TEXT);",
            )
            .unwrap();
        let digest = |job: &str| blake3::hash(job.as_bytes()).to_hex().to_string();
        for (job, state, outcome) in [
            ("published", "verified", Some("published")),
            ("unconfirmed", "verified", Some("published_unconfirmed")),
            ("failed", "failed", None),
            ("cancelled", "cancelled", None),
            ("interrupted", "interrupted", None),
            ("verified", "verified", None),
        ] {
            connection
                .execute(
                    "INSERT INTO render_candidate_checkpoints VALUES (?1, 'encode', ?2)",
                    rusqlite::params![job, format!("{{\"movie\":\"{}\"}}", digest(job))],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO render_attempts VALUES (?1, 'latest', ?2, '{}')",
                    rusqlite::params![job, state],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO render_job_heads VALUES (?1, 'latest')",
                    rusqlite::params![job],
                )
                .unwrap();
            if let Some(outcome) = outcome {
                connection
                    .execute(
                        "INSERT INTO render_publications VALUES (?1, ?1, ?2)",
                        rusqlite::params![job, format!("{{\"outcome\":\"{outcome}\"}}")],
                    )
                    .unwrap();
            }
        }
        let mut references = References::default();
        scan_database(&connection, &mut references).unwrap();
        let pinned: Vec<bool> = [
            "published",
            "unconfirmed",
            "failed",
            "cancelled",
            "interrupted",
            "verified",
        ]
        .iter()
        .map(|job| references.render.contains_key(&digest(job)))
        .collect();
        assert_eq!(pinned, vec![false, true, false, false, true, true]);
    }

    #[test]
    fn tables_map_to_reference_classes() {
        assert_eq!(class_of("revisions"), ReferenceClass::History);
        assert_eq!(class_of("transaction_steps"), ReferenceClass::History);
        assert_eq!(class_of("register_contents"), ReferenceClass::Registers);
        assert_eq!(class_of("original_media"), ReferenceClass::Originals);
        assert_eq!(
            class_of("generation_attempts"),
            ReferenceClass::GenerationRecords
        );
        assert_eq!(class_of("transcripts"), ReferenceClass::OtherRecords);
    }
}
