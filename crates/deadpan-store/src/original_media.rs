//! Durable ownership of complete original bytes, separate from media readiness.
//!
//! These records do not register an authored AssetRecord. A host must separately
//! qualify every selected stream before committing immutable editorial metadata.
//! Locations are operational: relinking identical bytes does not edit history.

use std::fs::{File, Metadata};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{FileExt, MetadataExt};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_core::{GeneratedError, GeneratedObjectRef};
use rusqlite::{Connection, TransactionBehavior, params};
use rustix::fs::{CWD, Mode, OFlags, openat};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::object_storage::{
    ObjectControl, ObjectIdentity, ObjectLimits, ObjectStorageError, PromotionMethod,
    VerifiedObject,
};
use crate::{AccessMode, ProjectStore, StoreError};

// The existing core checksum type has no generation/acceptance authority.
pub use deadpan_core::GeneratedContentId as OriginalContentId;

const MAX_RECORD_BYTES: usize = 128 * 1024;
const MAX_ORIGINALS: i64 = 100_000;
const MAX_PATH_BYTES: usize = 16 * 1024;
const MAX_BOOKMARK_BYTES: usize = 16 * 1024;
const MAX_ORIGINAL_BYTES: u64 = 64 * 1024 * 1024 * 1024;

/// Algorithm-tagged identity of the entire container, not just decoded video.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "GeneratedObjectRef")]
pub struct OriginalObjectRef(GeneratedObjectRef);

impl TryFrom<GeneratedObjectRef> for OriginalObjectRef {
    type Error = OriginalMediaError;
    fn try_from(value: GeneratedObjectRef) -> Result<Self, Self::Error> {
        Self::new(value.content().clone(), value.byte_length())
    }
}

impl OriginalObjectRef {
    pub fn new(content: OriginalContentId, byte_length: u64) -> Result<Self, OriginalMediaError> {
        if byte_length > MAX_ORIGINAL_BYTES {
            return Err(OriginalMediaError::InvalidLimits);
        }
        Ok(Self(GeneratedObjectRef::new(content, byte_length)?))
    }
    pub fn content(&self) -> &OriginalContentId {
        self.0.content()
    }
    pub fn byte_length(&self) -> u64 {
        self.0.byte_length()
    }
    fn identity(&self) -> Result<ObjectIdentity<'_>, OriginalMediaError> {
        Ok(ObjectIdentity::new(
            self.content().digest(),
            self.byte_length(),
        )?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkedOriginal {
    path: PathBuf,
    /// Opaque platform bookmark, retained without executing or interpreting it.
    bookmark: Option<Vec<u8>>,
}

impl LinkedOriginal {
    pub fn new(path: PathBuf, bookmark: Option<Vec<u8>>) -> Result<Self, OriginalMediaError> {
        let value = Self { path, bookmark };
        value.validate()?;
        Ok(value)
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn bookmark(&self) -> Option<&[u8]> {
        self.bookmark.as_deref()
    }
    fn validate(&self) -> Result<(), OriginalMediaError> {
        validate_path(&self.path)?;
        if self
            .bookmark
            .as_ref()
            .is_some_and(|b| b.len() > MAX_BOOKMARK_BYTES)
        {
            return Err(OriginalMediaError::InvalidRecord(
                "bookmark exceeds byte limit",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginalMediaRecord {
    object: OriginalObjectRef,
    /// Whole-file checksum retained for source-index identity matching.
    sha256: [u8; 32],
    label: String,
    version: u64,
    managed: bool,
    linked: Option<LinkedOriginal>,
}

impl OriginalMediaRecord {
    pub fn object(&self) -> &OriginalObjectRef {
        &self.object
    }
    pub fn sha256(&self) -> [u8; 32] {
        self.sha256
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn version(&self) -> u64 {
        self.version
    }
    pub fn managed(&self) -> bool {
        self.managed
    }
    pub fn linked(&self) -> Option<&LinkedOriginal> {
        self.linked.as_ref()
    }
    fn validate(&self) -> Result<(), OriginalMediaError> {
        if self.object.byte_length() == 0
            || self.object.byte_length() > MAX_ORIGINAL_BYTES
            || self.version == 0
            || self.version > i64::MAX as u64
            || self.label.is_empty()
            || self.label.len() > 4096
            || self.label.contains('\0')
            || (!self.managed && self.linked.is_none())
        {
            return Err(OriginalMediaError::InvalidRecord(
                "invalid original ownership record",
            ));
        }
        if let Some(link) = &self.linked {
            link.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum OriginalOwnership {
    Managed,
    Linked { bookmark: Option<Vec<u8>> },
}

#[derive(Debug, Clone, Copy)]
pub struct OriginalMediaLimits {
    maximum_bytes: u64,
    timeout: Duration,
}

impl Default for OriginalMediaLimits {
    fn default() -> Self {
        Self {
            maximum_bytes: MAX_ORIGINAL_BYTES,
            timeout: Duration::from_secs(300),
        }
    }
}

impl OriginalMediaLimits {
    pub fn new(maximum_bytes: u64, timeout: Duration) -> Result<Self, OriginalMediaError> {
        if maximum_bytes == 0
            || maximum_bytes > MAX_ORIGINAL_BYTES
            || timeout.is_zero()
            || timeout > Duration::from_secs(3600)
        {
            return Err(OriginalMediaError::InvalidLimits);
        }
        Ok(Self {
            maximum_bytes,
            timeout,
        })
    }
    fn control(self, cancelled: &AtomicBool) -> Result<Control<'_>, OriginalMediaError> {
        Self::new(self.maximum_bytes, self.timeout)?;
        let control = Control {
            deadline: Instant::now() + self.timeout,
            cancelled,
        };
        control.check()?;
        Ok(control)
    }
}

#[derive(Debug, Serialize)]
pub struct OriginalRetentionOutcome {
    pub record: OriginalMediaRecord,
    pub method: OriginalRetentionMethod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OriginalRetentionMethod {
    Existing,
    Cloned,
    Copied,
    Linked,
}

enum SnapshotBytes {
    Managed(VerifiedObject),
    Linked(File),
}

/// Frozen verified full-file bytes. No writable descriptor is exposed.
pub struct VerifiedOriginalObject {
    bytes: SnapshotBytes,
    record: OriginalMediaRecord,
}

impl VerifiedOriginalObject {
    pub fn record(&self) -> &OriginalMediaRecord {
        &self.record
    }
}
impl Read for VerifiedOriginalObject {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match &mut self.bytes {
            SnapshotBytes::Managed(file) => file.read(buffer),
            SnapshotBytes::Linked(file) => file.read(buffer),
        }
    }
}
impl Seek for VerifiedOriginalObject {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        match &mut self.bytes {
            SnapshotBytes::Managed(file) => file.seek(position),
            SnapshotBytes::Linked(file) => file.seek(position),
        }
    }
}

impl ProjectStore {
    /// Retain complete original bytes or an explicit linked location. This does
    /// not assert decodability, audio readiness, or change authored history.
    pub fn retain_original(
        &mut self,
        path: &Path,
        ownership: OriginalOwnership,
        limits: OriginalMediaLimits,
        cancelled: &AtomicBool,
    ) -> Result<OriginalRetentionOutcome, StoreError> {
        require_writer(self)?;
        let control = limits.control(cancelled)?;
        validate_path(path)?;
        let linked = match &ownership {
            OriginalOwnership::Managed => None,
            OriginalOwnership::Linked { bookmark } => {
                Some(LinkedOriginal::new(path.to_owned(), bookmark.clone())?)
            }
        };
        let file = open_source(path)?;
        let InspectedOriginal {
            object,
            sha256,
            state,
        } = inspect_original(&file, limits, &control, io::sink())?;
        let method = match ownership {
            OriginalOwnership::Managed => {
                let method = self
                    .original_storage
                    .promote_file_controlled(
                        &file,
                        object.identity()?,
                        ObjectLimits::new(limits.maximum_bytes)
                            .map_err(OriginalMediaError::from)?,
                        control.object_control(),
                    )
                    .map_err(OriginalMediaError::from)?;
                match method {
                    PromotionMethod::Existing => OriginalRetentionMethod::Existing,
                    PromotionMethod::Cloned => OriginalRetentionMethod::Cloned,
                    PromotionMethod::Copied => OriginalRetentionMethod::Copied,
                }
            }
            OriginalOwnership::Linked { .. } => {
                confirm_source_path(path, &file, &state)?;
                OriginalRetentionMethod::Linked
            }
        };
        control.check()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = read_record(&transaction, object.content())?;
        if let Some(old) = &old
            && (old.object != object || old.sha256 != sha256)
        {
            return Err(OriginalMediaError::IdentityMismatch.into());
        }
        let label = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or(OriginalMediaError::InvalidRecord(
                "source filename is not valid UTF-8",
            ))?
            .to_owned();
        let mut record = old.clone().unwrap_or(OriginalMediaRecord {
            object,
            sha256,
            label,
            version: 1,
            managed: false,
            linked: None,
        });
        record.managed |= !matches!(method, OriginalRetentionMethod::Linked);
        if linked.is_some() {
            record.linked = linked;
        }
        if let Some(old) = &old {
            if record != *old {
                record.version = next_version(old.version)?;
            }
        } else {
            let count: i64 =
                transaction.query_row("SELECT COUNT(*) FROM original_media", [], |r| r.get(0))?;
            if count >= MAX_ORIGINALS {
                return Err(OriginalMediaError::RecordLimit.into());
            }
        }
        write_record(&transaction, &record)?;
        control.check()?;
        transaction.commit()?;
        Ok(OriginalRetentionOutcome { record, method })
    }

    pub fn original_record(
        &self,
        content: &OriginalContentId,
    ) -> Result<Option<OriginalMediaRecord>, StoreError> {
        read_record(&self.connection, content)
    }

    /// Keyset paging bounds UI/headless inventory memory independently of project size.
    pub fn original_records(
        &self,
        after: Option<&OriginalContentId>,
        limit: u32,
    ) -> Result<Vec<OriginalMediaRecord>, StoreError> {
        if limit == 0 || limit > 1000 {
            return Err(OriginalMediaError::InvalidRecord("page size must be 1..=1000").into());
        }
        let mut statement = self.connection.prepare(
            "SELECT content_id, version, record FROM original_media WHERE content_id > ?1 ORDER BY content_id LIMIT ?2")?;
        let mut rows = statement.query(params![
            after.map(ToString::to_string).unwrap_or_default(),
            limit
        ])?;
        let mut records = Vec::new();
        while let Some(row) = rows.next()? {
            records.push(decode_row(row)?);
        }
        Ok(records)
    }

    /// A matching name never authorizes replacement. Verify the complete new
    /// location, then compare the monotonic locator version in one transaction.
    pub fn relink_original(
        &mut self,
        content: &OriginalContentId,
        expected_version: u64,
        location: LinkedOriginal,
        limits: OriginalMediaLimits,
        cancelled: &AtomicBool,
    ) -> Result<OriginalMediaRecord, StoreError> {
        require_writer(self)?;
        location.validate()?;
        let control = limits.control(cancelled)?;
        let old =
            read_record(&self.connection, content)?.ok_or(OriginalMediaError::MissingRecord)?;
        if old.version != expected_version {
            return Err(OriginalMediaError::VersionConflict {
                current: old.version,
            }
            .into());
        }
        let file = open_source(location.path())?;
        let InspectedOriginal {
            object,
            sha256,
            state,
        } = inspect_original(&file, limits, &control, io::sink())?;
        if object != old.object || sha256 != old.sha256 {
            return Err(OriginalMediaError::IdentityMismatch.into());
        }
        confirm_source_path(location.path(), &file, &state)?;
        control.check()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut record =
            read_record(&transaction, content)?.ok_or(OriginalMediaError::MissingRecord)?;
        if record.version != expected_version {
            return Err(OriginalMediaError::VersionConflict {
                current: record.version,
            }
            .into());
        }
        if record.linked.as_ref() != Some(&location) {
            record.linked = Some(location);
            record.version = next_version(record.version)?;
            write_record(&transaction, &record)?;
        }
        control.check()?;
        transaction.commit()?;
        Ok(record)
    }

    /// Rechecks identity before every use and returns an independent snapshot.
    /// Missing/corrupt originals fail explicitly; no offline image is invented.
    pub fn snapshot_original(
        &self,
        content: &OriginalContentId,
        limits: OriginalMediaLimits,
        cancelled: &AtomicBool,
    ) -> Result<VerifiedOriginalObject, StoreError> {
        let control = limits.control(cancelled)?;
        let record =
            read_record(&self.connection, content)?.ok_or(OriginalMediaError::MissingRecord)?;
        if record.object.byte_length() > limits.maximum_bytes {
            return Err(OriginalMediaError::ByteLimit.into());
        }
        let bytes = if record.managed {
            let snapshot = self
                .original_storage
                .snapshot_controlled(
                    record.object.identity()?,
                    ObjectLimits::new(limits.maximum_bytes).map_err(OriginalMediaError::from)?,
                    control.object_control(),
                )
                .map_err(OriginalMediaError::from)?;
            if snapshot.sha256() != record.sha256 {
                return Err(OriginalMediaError::IdentityMismatch.into());
            }
            SnapshotBytes::Managed(snapshot)
        } else {
            let link = record
                .linked
                .as_ref()
                .ok_or(OriginalMediaError::MissingRecord)?;
            let file = open_source(link.path())?;
            let mut copy = tempfile::tempfile()?;
            let InspectedOriginal { object, sha256, .. } =
                inspect_original(&file, limits, &control, &mut copy)?;
            if object != record.object || sha256 != record.sha256 {
                return Err(OriginalMediaError::IdentityMismatch.into());
            }
            copy.seek(SeekFrom::Start(0))?;
            SnapshotBytes::Linked(copy)
        };
        control.check()?;
        Ok(VerifiedOriginalObject { bytes, record })
    }
}

fn require_writer(store: &ProjectStore) -> Result<(), StoreError> {
    if store.mode != AccessMode::ReadWrite {
        Err(StoreError::ReadOnly)
    } else {
        Ok(())
    }
}

fn validate_path(path: &Path) -> Result<(), OriginalMediaError> {
    let text = path.to_str().ok_or(OriginalMediaError::InvalidRecord(
        "source path is not valid UTF-8",
    ))?;
    if !path.is_absolute()
        || text.len() > MAX_PATH_BYTES
        || text.contains('\0')
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(OriginalMediaError::InvalidRecord(
            "source path must be absolute, bounded, and have no parent traversal",
        ));
    }
    Ok(())
}

fn open_source(path: &Path) -> Result<File, OriginalMediaError> {
    let file = File::from(
        openat(
            CWD,
            path,
            OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| OriginalMediaError::Io(e.into()))?,
    );
    if !file.metadata()?.is_file() {
        return Err(OriginalMediaError::NotRegular);
    }
    Ok(file)
}

fn confirm_source_path(
    path: &Path,
    file: &File,
    inspected: &Metadata,
) -> Result<(), OriginalMediaError> {
    let named = open_source(path)?.metadata()?;
    let original = file.metadata()?;
    if !same_source_state(inspected, &named) || !same_source_state(inspected, &original) {
        return Err(OriginalMediaError::SourceChanged);
    }
    Ok(())
}

fn same_source_state(before: &Metadata, after: &Metadata) -> bool {
    before.dev() == after.dev()
        && before.ino() == after.ino()
        && before.len() == after.len()
        && before.mtime() == after.mtime()
        && before.mtime_nsec() == after.mtime_nsec()
        && before.ctime() == after.ctime()
        && before.ctime_nsec() == after.ctime_nsec()
}

struct Control<'a> {
    deadline: Instant,
    cancelled: &'a AtomicBool,
}
impl Control<'_> {
    fn check(&self) -> Result<(), OriginalMediaError> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(OriginalMediaError::Cancelled);
        }
        if Instant::now() >= self.deadline {
            return Err(OriginalMediaError::Deadline);
        }
        Ok(())
    }
    fn object_control(&self) -> ObjectControl<'_> {
        ObjectControl::bounded(self.deadline, self.cancelled)
    }
}

struct InspectedOriginal {
    object: OriginalObjectRef,
    sha256: [u8; 32],
    state: Metadata,
}

fn inspect_original(
    file: &File,
    limits: OriginalMediaLimits,
    control: &Control<'_>,
    mut copy: impl Write,
) -> Result<InspectedOriginal, OriginalMediaError> {
    control.check()?;
    let before = file.metadata()?;
    if !before.is_file() {
        return Err(OriginalMediaError::NotRegular);
    }
    if before.len() == 0 || before.len() > limits.maximum_bytes {
        return Err(OriginalMediaError::ByteLimit);
    }
    let mut blake3 = blake3::Hasher::new();
    let mut sha256 = Sha256::new();
    let mut offset = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        control.check()?;
        let count = file.read_at(&mut buffer, offset)?;
        if count == 0 {
            break;
        }
        offset = offset
            .checked_add(u64::try_from(count).expect("buffer size fits u64"))
            .ok_or(OriginalMediaError::ByteLimit)?;
        if offset > before.len() || offset > limits.maximum_bytes {
            return Err(OriginalMediaError::SourceChanged);
        }
        blake3.update(&buffer[..count]);
        sha256.update(&buffer[..count]);
        copy.write_all(&buffer[..count])?;
    }
    let after = file.metadata()?;
    if offset != before.len() || !same_source_state(&before, &after) {
        return Err(OriginalMediaError::SourceChanged);
    }
    control.check()?;
    Ok(InspectedOriginal {
        object: OriginalObjectRef::new(
            OriginalContentId::new(blake3.finalize().to_hex().to_string())?,
            offset,
        )?,
        sha256: sha256.finalize().into(),
        state: after,
    })
}

fn next_version(version: u64) -> Result<u64, OriginalMediaError> {
    version
        .checked_add(1)
        .filter(|v| *v <= i64::MAX as u64)
        .ok_or(OriginalMediaError::VersionExhausted)
}

pub(crate) fn create_tables(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE original_media (
        content_id TEXT PRIMARY KEY,
        version INTEGER NOT NULL CHECK(version > 0),
        record TEXT NOT NULL CHECK(json_valid(record))
    ) STRICT;",
    )?;
    Ok(())
}

pub(crate) fn check_stored_sizes(connection: &Connection) -> Result<(), StoreError> {
    let oversized: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM original_media WHERE length(CAST(record AS BLOB)) > ?1 OR length(CAST(content_id AS BLOB)) > 71)",
        [i64::try_from(MAX_RECORD_BYTES).expect("record limit fits SQLite")], |row| row.get(0))?;
    let count: i64 =
        connection.query_row("SELECT COUNT(*) FROM original_media", [], |row| row.get(0))?;
    if oversized || count > MAX_ORIGINALS {
        return Err(OriginalMediaError::RecordLimit.into());
    }
    Ok(())
}

pub(crate) fn validate_store(connection: &Connection) -> Result<(), StoreError> {
    check_stored_sizes(connection)?;
    let mut statement = connection
        .prepare("SELECT content_id, version, record FROM original_media ORDER BY content_id")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        decode_row(row)?;
    }
    Ok(())
}

fn read_record(
    connection: &Connection,
    content: &OriginalContentId,
) -> Result<Option<OriginalMediaRecord>, StoreError> {
    let mut statement = connection
        .prepare("SELECT content_id, version, record FROM original_media WHERE content_id=?1")?;
    let mut rows = statement.query([content.to_string()])?;
    rows.next()?.map(decode_row).transpose()
}

fn decode_row(row: &rusqlite::Row<'_>) -> Result<OriginalMediaRecord, StoreError> {
    let content: String = row.get(0)?;
    let version = u64::try_from(row.get::<_, i64>(1)?)
        .map_err(|_| OriginalMediaError::InvalidRecord("negative location version"))?;
    let bytes = row
        .get_ref(2)?
        .as_str()
        .map_err(|_| OriginalMediaError::InvalidRecord("ownership record is not text"))?;
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(OriginalMediaError::RecordLimit.into());
    }
    let record: OriginalMediaRecord = serde_json::from_str(bytes)?;
    record.validate()?;
    if record.object.content().to_string() != content || record.version != version {
        return Err(
            OriginalMediaError::InvalidRecord("ownership columns disagree with record").into(),
        );
    }
    Ok(record)
}

fn write_record(connection: &Connection, record: &OriginalMediaRecord) -> Result<(), StoreError> {
    record.validate()?;
    let json = serde_json::to_string(record)?;
    if json.len() > MAX_RECORD_BYTES {
        return Err(OriginalMediaError::RecordLimit.into());
    }
    connection.execute(
        "INSERT INTO original_media(content_id,version,record) VALUES(?1,?2,?3)
        ON CONFLICT(content_id) DO UPDATE SET version=excluded.version,record=excluded.record",
        params![
            record.object.content().to_string(),
            i64::try_from(record.version).map_err(|_| OriginalMediaError::VersionExhausted)?,
            json
        ],
    )?;
    Ok(())
}

#[derive(Debug, Error)]
pub enum OriginalMediaError {
    #[error("original-media byte or time limits are invalid")]
    InvalidLimits,
    #[error("original-media record is invalid: {0}")]
    InvalidRecord(&'static str),
    #[error("original-media record or inventory exceeds its limit")]
    RecordLimit,
    #[error("original content is not registered")]
    MissingRecord,
    #[error("original source is not a regular file")]
    NotRegular,
    #[error("original source is empty or exceeds the byte budget")]
    ByteLimit,
    #[error("original source changed during verification")]
    SourceChanged,
    #[error("source content does not match the registered original")]
    IdentityMismatch,
    #[error("original location version changed; current version is {current}")]
    VersionConflict { current: u64 },
    #[error("original location versions are exhausted")]
    VersionExhausted,
    #[error("original-media operation cancelled")]
    Cancelled,
    #[error("original-media operation exceeded its deadline")]
    Deadline,
    #[error(transparent)]
    Identity(#[from] GeneratedError),
    #[error(transparent)]
    Storage(#[from] ObjectStorageError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl OriginalMediaError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::VersionConflict { .. } => "OriginalLocationConflict",
            Self::MissingRecord => "OriginalNotFound",
            Self::IdentityMismatch | Self::SourceChanged => "OriginalContentMismatch",
            Self::Cancelled => "OriginalCancelled",
            Self::Deadline => "OriginalDeadline",
            Self::Io(e) if e.kind() == io::ErrorKind::NotFound => "OriginalOffline",
            Self::Io(e) if e.kind() == io::ErrorKind::PermissionDenied => "PermissionDenied",
            Self::Io(e) if e.kind() == io::ErrorKind::StorageFull => "DiskFull",
            Self::Io(_) => "IoFailure",
            Self::Storage(e) => match e {
                ObjectStorageError::Cancelled => "OriginalCancelled",
                ObjectStorageError::DeadlineExceeded => "OriginalDeadline",
                ObjectStorageError::MissingObject(_)
                | ObjectStorageError::MissingStorageComponent(_) => "OriginalOffline",
                ObjectStorageError::HashMismatch { .. }
                | ObjectStorageError::LengthMismatch { .. }
                | ObjectStorageError::SourceChanged => "OriginalContentMismatch",
                ObjectStorageError::Io { .. }
                | ObjectStorageError::System { .. }
                | ObjectStorageError::SourceRead { .. } => e.code(),
                _ => "OriginalStorageInvalid",
            },
            _ => "OriginalInvalid",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, FileTimes};

    #[test]
    fn final_link_confirmation_rejects_same_inode_changes_after_hashing() {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("linked.mov");
        fs::write(&path, b"original").unwrap();
        let file = open_source(&path).unwrap();
        let limits = OriginalMediaLimits::default();
        let cancelled = AtomicBool::new(false);
        let control = limits.control(&cancelled).unwrap();
        let inspected = inspect_original(&file, limits, &control, io::sink()).unwrap();
        confirm_source_path(&path, &file, &inspected.state).unwrap();

        fs::write(&path, b"modified").unwrap();
        // Force a distinct timestamp even on a filesystem with coarse clocks.
        file.set_times(
            FileTimes::new()
                .set_modified(inspected.state.modified().unwrap() + Duration::from_secs(2)),
        )
        .unwrap();
        assert_eq!(file.metadata().unwrap().ino(), inspected.state.ino());
        assert_eq!(file.metadata().unwrap().len(), inspected.state.len());
        assert!(matches!(
            confirm_source_path(&path, &file, &inspected.state),
            Err(OriginalMediaError::SourceChanged)
        ));
    }
}
