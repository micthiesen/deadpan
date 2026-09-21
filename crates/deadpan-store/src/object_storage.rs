//! Descriptor-relative content-addressed storage for project-owned byte objects.
//!
//! This module stores already-validated bytes under a fixed BLAKE3-derived
//! name and can return an immutable anonymous snapshot of a stored object. It
//! does not decode media, authorize candidate acceptance, mutate the document
//! or database, or implement eviction. Owner-only namespace mutation is an
//! authority boundary; same-user hostile code can race any mutable POSIX
//! namespace, so callers must re-verify a snapshot at acceptance and use.

#[cfg(target_os = "macos")]
use std::ffi::CString;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use rustix::fs::{
    AtFlags, CWD, FileType, Mode, OFlags, RenameFlags, Stat, fchmod, fstat, fsync, openat,
    renameat_with, statat, unlinkat,
};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub use deadpan_core::{GeneratedContentId, GeneratedObjectRef};

const COPY_BUFFER_BYTES: usize = 64 * 1024;
const TEMPORARY_ATTEMPTS: usize = 8;
const FINAL_MODE: Mode = Mode::RUSR.union(Mode::RGRP).union(Mode::ROTH);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StorageNamespace {
    Generated,
    Originals,
}

impl StorageNamespace {
    const fn component(self) -> &'static str {
        match self {
            Self::Generated => "Generated",
            Self::Originals => "Originals",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ObjectIdentity<'a> {
    digest: &'a str,
    byte_length: u64,
}

impl<'a> ObjectIdentity<'a> {
    pub(crate) fn new(digest: &'a str, byte_length: u64) -> Result<Self, ObjectStorageError> {
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || byte_length == 0
        {
            return Err(ObjectStorageError::InvalidIdentity);
        }
        Ok(Self {
            digest,
            byte_length,
        })
    }

    pub(crate) const fn digest(self) -> &'a str {
        self.digest
    }

    pub(crate) const fn byte_length(self) -> u64 {
        self.byte_length
    }
}

impl<'a> From<&'a GeneratedObjectRef> for ObjectIdentity<'a> {
    fn from(value: &'a GeneratedObjectRef) -> Self {
        Self {
            digest: value.content().digest(),
            byte_length: value.byte_length(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ObjectControl<'a> {
    deadline: Option<Instant>,
    cancelled: Option<&'a AtomicBool>,
    closed: Option<&'a AtomicBool>,
}

impl<'a> ObjectControl<'a> {
    pub(crate) const fn unbounded() -> Self {
        Self {
            deadline: None,
            cancelled: None,
            closed: None,
        }
    }

    pub(crate) const fn bounded(deadline: Instant, cancelled: &'a AtomicBool) -> Self {
        Self {
            deadline: Some(deadline),
            cancelled: Some(cancelled),
            closed: None,
        }
    }

    pub(crate) const fn with_closed(mut self, closed: &'a AtomicBool) -> Self {
        self.closed = Some(closed);
        self
    }

    fn check(self) -> Result<(), ObjectStorageError> {
        if self
            .closed
            .is_some_and(|closed| closed.load(Ordering::Acquire))
        {
            return Err(ObjectStorageError::SessionClosed);
        }
        if self
            .cancelled
            .is_some_and(|cancelled| cancelled.load(Ordering::Acquire))
        {
            return Err(ObjectStorageError::Cancelled);
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(ObjectStorageError::DeadlineExceeded);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromotionMethod {
    Existing,
    Cloned,
    Copied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectLimits {
    maximum_bytes: u64,
}

impl ObjectLimits {
    pub fn new(maximum_bytes: u64) -> Result<Self, ObjectStorageError> {
        if maximum_bytes == 0 {
            return Err(ObjectStorageError::InvalidBudget);
        }
        Ok(Self { maximum_bytes })
    }

    pub const fn maximum_bytes(self) -> u64 {
        self.maximum_bytes
    }
}

/// Frozen verified bytes. The package object may subsequently move or change
/// without changing this anonymous snapshot.
#[derive(Debug)]
pub struct VerifiedObject {
    file: File,
    reference: GeneratedObjectRef,
    sha256: [u8; 32],
}

impl VerifiedObject {
    pub fn reference(&self) -> &GeneratedObjectRef {
        &self.reference
    }

    pub const fn sha256(&self) -> [u8; 32] {
        self.sha256
    }
}

impl Read for VerifiedObject {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.file.read(buffer)
    }
}

impl Seek for VerifiedObject {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.file.seek(position)
    }
}

/// Package-root authority retained independently of later path replacement.
#[derive(Debug)]
pub(crate) struct GeneratedStorage {
    package: OwnedFd,
    device: i128,
    owner: u32,
    namespace: StorageNamespace,
}

#[derive(Debug)]
struct MediaDirectories {
    media: OwnedFd,
    generated: OwnedFd,
}

impl GeneratedStorage {
    pub(crate) fn open(package: &Path) -> Result<Self, ObjectStorageError> {
        Self::open_namespace(package, StorageNamespace::Generated)
    }

    fn open_namespace(
        package: &Path,
        namespace: StorageNamespace,
    ) -> Result<Self, ObjectStorageError> {
        let package = openat(
            CWD,
            package,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|source| storage_open_error("package", source))?;
        let metadata = fstat(&package).map_err(|source| ObjectStorageError::System {
            operation: "inspect package root",
            source,
        })?;
        if !FileType::from_raw_mode(metadata.st_mode).is_dir() {
            return Err(ObjectStorageError::UnsafeStorageComponent("package".into()));
        }
        Ok(Self {
            package,
            device: i128::from(metadata.st_dev),
            owner: metadata.st_uid,
            namespace,
        })
    }

    /// Copies already-validated bytes into project-managed generated storage.
    /// This verifies content identity only; it makes no media-validity or
    /// authored-acceptance claim.
    pub(crate) fn promote(
        &self,
        reader: &mut impl Read,
        expected: &GeneratedObjectRef,
        limits: ObjectLimits,
    ) -> Result<GeneratedObjectRef, ObjectStorageError> {
        self.promote_with_hooks(
            reader,
            expected,
            limits,
            |_| {},
            |directories, file| self.complete_durability(directories, file),
        )
    }

    fn promote_with_hooks(
        &self,
        reader: &mut impl Read,
        expected: &GeneratedObjectRef,
        limits: ObjectLimits,
        before_publish: impl FnOnce(&str),
        durability: impl FnOnce(&MediaDirectories, &File) -> Result<(), ObjectStorageError>,
    ) -> Result<GeneratedObjectRef, ObjectStorageError> {
        self.promote_with_control_hooks(
            reader,
            expected,
            limits,
            ObjectControl::unbounded(),
            |name| {
                before_publish(name);
                Ok(())
            },
            durability,
        )
    }

    fn promote_with_control_hooks(
        &self,
        reader: &mut impl Read,
        expected: &GeneratedObjectRef,
        limits: ObjectLimits,
        control: ObjectControl<'_>,
        before_publish: impl FnOnce(&str) -> Result<(), ObjectStorageError>,
        durability: impl FnOnce(&MediaDirectories, &File) -> Result<(), ObjectStorageError>,
    ) -> Result<GeneratedObjectRef, ObjectStorageError> {
        control.check()?;
        validate_budget(expected, limits)?;
        let directories = self.open_directories()?;
        let target = object_name(expected.content());
        if let Some(existing) =
            self.open_object_optional(&directories.generated, &target, expected)?
        {
            let existing = self.verify_open_object_controlled(
                existing,
                expected,
                limits,
                io::sink(),
                control,
                || {},
            )?;
            durability(&directories, &existing)?;
            confirm_named_file(&directories.generated, &target, &existing)?;
            return Ok(expected.clone());
        }

        let (temporary_name, temporary, mut pending) =
            self.create_pending(&directories.generated)?;
        let mut temporary = File::from(temporary);
        let mut hasher = blake3::Hasher::new();
        let mut copied = 0_u64;
        let mut buffer = [0_u8; COPY_BUFFER_BYTES];
        loop {
            control.check()?;
            let read = reader
                .read(&mut buffer)
                .map_err(|source| ObjectStorageError::SourceRead { source })?;
            if read == 0 {
                break;
            }
            let next = copied
                .checked_add(u64::try_from(read).expect("copy buffer length fits u64"))
                .ok_or(ObjectStorageError::SourceChanged)?;
            if next > expected.byte_length() {
                return Err(ObjectStorageError::LengthMismatch {
                    expected: expected.byte_length(),
                    actual: next,
                });
            }
            if next > limits.maximum_bytes {
                return Err(ObjectStorageError::TooLarge {
                    size: next,
                    maximum: limits.maximum_bytes,
                });
            }
            hasher.update(&buffer[..read]);
            temporary
                .write_all(&buffer[..read])
                .map_err(|source| ObjectStorageError::Io {
                    operation: "write pending generated object",
                    source,
                })?;
            copied = next;
        }
        control.check()?;
        if copied != expected.byte_length() {
            return Err(ObjectStorageError::LengthMismatch {
                expected: expected.byte_length(),
                actual: copied,
            });
        }
        let observed = hasher.finalize().to_hex().to_string();
        if observed != expected.content().digest() {
            return Err(ObjectStorageError::HashMismatch {
                expected: expected.content().clone(),
                observed,
            });
        }
        fchmod(&temporary, FINAL_MODE).map_err(|source| ObjectStorageError::System {
            operation: "make generated object read-only",
            source,
        })?;
        sync_file(&temporary, "sync pending generated object")?;
        let metadata = fstat(&temporary).map_err(|source| ObjectStorageError::System {
            operation: "inspect pending generated object",
            source,
        })?;
        self.validate_object_metadata(&metadata, expected, limits)?;
        before_publish(&temporary_name)?;
        pending.confirm_path()?;

        match renameat_with(
            &directories.generated,
            temporary_name.as_str(),
            &directories.generated,
            target.as_str(),
            RenameFlags::NOREPLACE,
        ) {
            Ok(()) => pending.published = true,
            Err(rustix::io::Errno::EXIST) => {
                let existing = self
                    .open_object_optional(&directories.generated, &target, expected)?
                    .ok_or_else(|| ObjectStorageError::MissingObject(expected.content().clone()))?;
                let existing = self.verify_open_object_controlled(
                    existing,
                    expected,
                    limits,
                    io::sink(),
                    control,
                    || {},
                )?;
                durability(&directories, &existing)?;
                confirm_named_file(&directories.generated, &target, &existing)?;
                return Ok(expected.clone());
            }
            Err(source) => {
                return Err(ObjectStorageError::System {
                    operation: "publish generated object",
                    source,
                });
            }
        }

        // Publication makes these verified bytes durable even if the caller
        // closes or cancels before the bounded post-publication verification.
        // Retain the descriptor we published so namespace replacement cannot
        // redirect durability onto a different object.
        durability(&directories, &temporary)?;
        let published = self
            .open_object_optional(&directories.generated, &target, expected)?
            .ok_or_else(|| ObjectStorageError::MissingObject(expected.content().clone()))?;
        let published = self.verify_open_object_controlled(
            published,
            expected,
            limits,
            io::sink(),
            control,
            || {},
        )?;
        confirm_named_file(&directories.generated, &target, &published)?;
        confirm_named_file(&directories.generated, &target, &temporary)?;
        Ok(expected.clone())
    }

    /// Returns an anonymous verified copy. Callers must still apply media and
    /// authored-acceptance validation appropriate to their operation.
    pub(crate) fn snapshot(
        &self,
        expected: &GeneratedObjectRef,
        limits: ObjectLimits,
    ) -> Result<VerifiedObject, ObjectStorageError> {
        self.snapshot_after_open(expected, limits, || {})
    }

    fn snapshot_controlled(
        &self,
        expected: &GeneratedObjectRef,
        limits: ObjectLimits,
        control: ObjectControl<'_>,
    ) -> Result<VerifiedObject, ObjectStorageError> {
        self.snapshot_after_open_controlled(expected, limits, control, || {})
    }

    fn snapshot_after_open(
        &self,
        expected: &GeneratedObjectRef,
        limits: ObjectLimits,
        after_open: impl FnOnce(),
    ) -> Result<VerifiedObject, ObjectStorageError> {
        self.snapshot_after_open_controlled(
            expected,
            limits,
            ObjectControl::unbounded(),
            after_open,
        )
    }

    fn snapshot_after_open_controlled(
        &self,
        expected: &GeneratedObjectRef,
        limits: ObjectLimits,
        control: ObjectControl<'_>,
        after_open: impl FnOnce(),
    ) -> Result<VerifiedObject, ObjectStorageError> {
        control.check()?;
        validate_budget(expected, limits)?;
        let directories = self.open_directories()?;
        let target = object_name(expected.content());
        let source = self
            .open_object_optional(&directories.generated, &target, expected)?
            .ok_or_else(|| ObjectStorageError::MissingObject(expected.content().clone()))?;
        let mut snapshot = tempfile::tempfile().map_err(|source| ObjectStorageError::Io {
            operation: "create generated-object snapshot",
            source,
        })?;
        let sha256 = {
            let mut writer = Sha256Writer::new(&mut snapshot);
            self.verify_open_object_controlled(
                source,
                expected,
                limits,
                &mut writer,
                control,
                after_open,
            )?;
            writer.finish()
        };
        snapshot
            .seek(SeekFrom::Start(0))
            .map_err(|source| ObjectStorageError::Io {
                operation: "rewind generated-object snapshot",
                source,
            })?;
        Ok(VerifiedObject {
            file: snapshot,
            reference: expected.clone(),
            sha256,
        })
    }

    fn open_directories(&self) -> Result<MediaDirectories, ObjectStorageError> {
        let package = fstat(&self.package).map_err(|source| ObjectStorageError::System {
            operation: "reinspect package root",
            source,
        })?;
        self.validate_contained(&package, "package")?;
        if !FileType::from_raw_mode(package.st_mode).is_dir()
            || package.st_uid != rustix::process::geteuid().as_raw()
            || namespace_is_writable_by_others(&package)
        {
            return Err(ObjectStorageError::UnsafeStorageComponent("package".into()));
        }
        let media = self.open_directory(&self.package, "Media")?;
        let generated = self.open_directory(&media, self.namespace.component())?;
        Ok(MediaDirectories { media, generated })
    }

    fn open_directory(
        &self,
        parent: &OwnedFd,
        name: &'static str,
    ) -> Result<OwnedFd, ObjectStorageError> {
        let directory = openat(
            parent,
            name,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|source| storage_open_error(name, source))?;
        let metadata = fstat(&directory).map_err(|source| ObjectStorageError::System {
            operation: "inspect generated-media directory",
            source,
        })?;
        self.validate_contained(&metadata, name)?;
        if !FileType::from_raw_mode(metadata.st_mode).is_dir()
            || namespace_is_writable_by_others(&metadata)
        {
            return Err(ObjectStorageError::UnsafeStorageComponent(name.into()));
        }
        Ok(directory)
    }

    fn create_pending<'a>(
        &self,
        directory: &'a OwnedFd,
    ) -> Result<(String, OwnedFd, PendingObject<'a>), ObjectStorageError> {
        for _ in 0..TEMPORARY_ATTEMPTS {
            let name = format!(".pending-{}", uuid::Uuid::new_v4());
            match openat(
                directory,
                name.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR,
            ) {
                Ok(file) => {
                    let metadata = fstat(&file).map_err(|source| ObjectStorageError::System {
                        operation: "inspect pending generated object",
                        source,
                    })?;
                    let pending = PendingObject {
                        directory,
                        name: name.clone(),
                        device: i128::from(metadata.st_dev),
                        inode: i128::from(metadata.st_ino),
                        published: false,
                    };
                    self.validate_contained(&metadata, &name)?;
                    if !FileType::from_raw_mode(metadata.st_mode).is_file()
                        || metadata.st_nlink != 1
                    {
                        return Err(ObjectStorageError::UnsafeObject(name));
                    }
                    return Ok((name, file, pending));
                }
                Err(rustix::io::Errno::EXIST) => continue,
                Err(source) => {
                    return Err(ObjectStorageError::System {
                        operation: "create pending generated object",
                        source,
                    });
                }
            }
        }
        Err(ObjectStorageError::TemporaryNameExhausted)
    }

    fn open_object_optional(
        &self,
        directory: &OwnedFd,
        name: &str,
        expected: &GeneratedObjectRef,
    ) -> Result<Option<OwnedFd>, ObjectStorageError> {
        match openat(
            directory,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(file) => Ok(Some(file)),
            Err(open_error) => match statat(directory, name, AtFlags::SYMLINK_NOFOLLOW) {
                Ok(metadata) => {
                    let file_type = FileType::from_raw_mode(metadata.st_mode);
                    if file_type.is_symlink() {
                        Err(ObjectStorageError::UnsafeObject(name.into()))
                    } else if !file_type.is_file() {
                        Err(ObjectStorageError::NotRegularFile(
                            expected.content().clone(),
                        ))
                    } else {
                        Err(ObjectStorageError::System {
                            operation: "open generated object",
                            source: open_error,
                        })
                    }
                }
                Err(rustix::io::Errno::NOENT) => Ok(None),
                Err(source) => Err(ObjectStorageError::System {
                    operation: "inspect unopened generated object",
                    source,
                }),
            },
        }
    }

    fn verify_open_object_controlled(
        &self,
        source: OwnedFd,
        expected: &GeneratedObjectRef,
        limits: ObjectLimits,
        mut destination: impl Write,
        control: ObjectControl<'_>,
        after_open: impl FnOnce(),
    ) -> Result<File, ObjectStorageError> {
        control.check()?;
        let before = fstat(&source).map_err(|source| ObjectStorageError::System {
            operation: "inspect generated object",
            source,
        })?;
        self.validate_object_metadata(&before, expected, limits)?;
        after_open();
        let mut source = File::from(source);
        let mut hasher = blake3::Hasher::new();
        let mut copied = 0_u64;
        let mut buffer = [0_u8; COPY_BUFFER_BYTES];
        loop {
            control.check()?;
            let read = source
                .read(&mut buffer)
                .map_err(|source| ObjectStorageError::Io {
                    operation: "read generated object",
                    source,
                })?;
            if read == 0 {
                break;
            }
            let next = copied
                .checked_add(u64::try_from(read).expect("copy buffer length fits u64"))
                .ok_or(ObjectStorageError::SourceChanged)?;
            if next > expected.byte_length() {
                return Err(ObjectStorageError::SourceChanged);
            }
            if next > limits.maximum_bytes {
                return Err(ObjectStorageError::TooLarge {
                    size: next,
                    maximum: limits.maximum_bytes,
                });
            }
            hasher.update(&buffer[..read]);
            destination
                .write_all(&buffer[..read])
                .map_err(|source| ObjectStorageError::Io {
                    operation: "write generated-object snapshot",
                    source,
                })?;
            copied = next;
        }
        control.check()?;
        let after = fstat(&source).map_err(|source| ObjectStorageError::System {
            operation: "reinspect generated object",
            source,
        })?;
        self.validate_object_metadata(&after, expected, limits)?;
        if !same_file_state(&before, &after) {
            return Err(ObjectStorageError::SourceChanged);
        }
        if copied != expected.byte_length() {
            return Err(ObjectStorageError::LengthMismatch {
                expected: expected.byte_length(),
                actual: copied,
            });
        }
        let observed = hasher.finalize().to_hex().to_string();
        if observed != expected.content().digest() {
            return Err(ObjectStorageError::HashMismatch {
                expected: expected.content().clone(),
                observed,
            });
        }
        Ok(source)
    }

    fn validate_contained(
        &self,
        metadata: &Stat,
        component: &str,
    ) -> Result<(), ObjectStorageError> {
        if i128::from(metadata.st_dev) != self.device {
            return Err(ObjectStorageError::CrossDevice(component.into()));
        }
        if metadata.st_uid != self.owner {
            return Err(ObjectStorageError::UnexpectedOwner(component.into()));
        }
        Ok(())
    }

    fn complete_durability(
        &self,
        directories: &MediaDirectories,
        file: &File,
    ) -> Result<(), ObjectStorageError> {
        sync_file(file, "sync generated object")?;
        sync_directory(&directories.generated, "sync generated-media directory")?;
        sync_directory(&directories.media, "sync media directory")?;
        sync_directory(&self.package, "sync project package")?;
        full_sync_file(file, "fully sync generated object after namespace sync")
    }

    fn validate_object_metadata(
        &self,
        metadata: &Stat,
        expected: &GeneratedObjectRef,
        limits: ObjectLimits,
    ) -> Result<(), ObjectStorageError> {
        if !FileType::from_raw_mode(metadata.st_mode).is_file() {
            return Err(ObjectStorageError::NotRegularFile(
                expected.content().clone(),
            ));
        }
        self.validate_contained(metadata, expected.content().digest())?;
        if metadata.st_nlink != 1 {
            return Err(ObjectStorageError::MultipleLinks(
                expected.content().clone(),
            ));
        }
        let write_bits = (Mode::WUSR | Mode::WGRP | Mode::WOTH).bits();
        if metadata.st_mode & write_bits != 0 {
            return Err(ObjectStorageError::WritableObject(
                expected.content().clone(),
            ));
        }
        let actual =
            u64::try_from(metadata.st_size).map_err(|_| ObjectStorageError::SourceChanged)?;
        if actual > limits.maximum_bytes {
            return Err(ObjectStorageError::TooLarge {
                size: actual,
                maximum: limits.maximum_bytes,
            });
        }
        if actual != expected.byte_length() {
            return Err(ObjectStorageError::LengthMismatch {
                expected: expected.byte_length(),
                actual,
            });
        }
        Ok(())
    }
}

/// Namespace-scoped storage shared by durable generated and original bytes.
/// Media meaning, source SHA-256, probing, and authored admission stay with the
/// caller; this layer owns only BLAKE3 identity, containment, and durability.
#[derive(Debug)]
pub(crate) struct ObjectStorage {
    inner: GeneratedStorage,
}

/// Verified retained object identity, tied to the still-open source descriptor.
/// This is only a freshness guard; the private snapshot owns the readable bytes.
pub(crate) struct ObjectFreshnessGuard {
    source: File,
    state: Stat,
    reference: GeneratedObjectRef,
    limits: ObjectLimits,
}

impl ObjectStorage {
    pub(crate) fn open(
        package: &Path,
        namespace: StorageNamespace,
    ) -> Result<Self, ObjectStorageError> {
        Ok(Self {
            inner: GeneratedStorage::open_namespace(package, namespace)?,
        })
    }

    pub(crate) fn promote_file_controlled(
        &self,
        source: &File,
        identity: ObjectIdentity<'_>,
        limits: ObjectLimits,
        control: ObjectControl<'_>,
    ) -> Result<PromotionMethod, ObjectStorageError> {
        self.promote_file_controlled_platform(source, identity, limits, control)
    }

    #[cfg(target_os = "macos")]
    fn promote_file_controlled_platform(
        &self,
        source: &File,
        identity: ObjectIdentity<'_>,
        limits: ObjectLimits,
        control: ObjectControl<'_>,
    ) -> Result<PromotionMethod, ObjectStorageError> {
        self.promote_file_controlled_with_clone(
            source,
            identity,
            limits,
            control,
            deadpan_fileclone::try_clone_file,
        )
    }

    #[cfg(not(target_os = "macos"))]
    fn promote_file_controlled_platform(
        &self,
        source: &File,
        identity: ObjectIdentity<'_>,
        limits: ObjectLimits,
        control: ObjectControl<'_>,
    ) -> Result<PromotionMethod, ObjectStorageError> {
        self.promote_file_controlled_by_copy(source, identity, limits, control)
    }

    #[cfg(target_os = "macos")]
    fn promote_file_controlled_with_clone(
        &self,
        source: &File,
        identity: ObjectIdentity<'_>,
        limits: ObjectLimits,
        control: ObjectControl<'_>,
        mut clone_file: impl FnMut(
            &File,
            &File,
            &std::ffi::CStr,
        ) -> io::Result<deadpan_fileclone::CloneOutcome>,
    ) -> Result<PromotionMethod, ObjectStorageError> {
        control.check()?;
        let expected = object_reference(identity)?;
        validate_budget(&expected, limits)?;
        let before = fstat(source).map_err(|source| ObjectStorageError::System {
            operation: "inspect source object",
            source,
        })?;
        validate_source_file(&before, &expected, limits)?;
        if self.has_verified_object(&expected, limits, control)? {
            return Ok(PromotionMethod::Existing);
        }
        if let Some(method) =
            self.try_clone_file_with(source, &expected, limits, control, &before, &mut clone_file)?
        {
            return Ok(method);
        }

        self.copy_file_controlled(source, &expected, limits, control, &before)
    }

    #[cfg(not(target_os = "macos"))]
    fn promote_file_controlled_by_copy(
        &self,
        source: &File,
        identity: ObjectIdentity<'_>,
        limits: ObjectLimits,
        control: ObjectControl<'_>,
    ) -> Result<PromotionMethod, ObjectStorageError> {
        control.check()?;
        let expected = object_reference(identity)?;
        validate_budget(&expected, limits)?;
        let before = fstat(source).map_err(|source| ObjectStorageError::System {
            operation: "inspect source object",
            source,
        })?;
        validate_source_file(&before, &expected, limits)?;
        if self.has_verified_object(&expected, limits, control)? {
            return Ok(PromotionMethod::Existing);
        }
        self.copy_file_controlled(source, &expected, limits, control, &before)
    }

    fn copy_file_controlled(
        &self,
        source: &File,
        expected: &GeneratedObjectRef,
        limits: ObjectLimits,
        control: ObjectControl<'_>,
        source_before: &Stat,
    ) -> Result<PromotionMethod, ObjectStorageError> {
        let mut reader = ReadAtReader {
            file: source,
            offset: 0,
        };
        self.inner.promote_with_control_hooks(
            &mut reader,
            expected,
            limits,
            control,
            |_| {
                let after = fstat(source).map_err(|source| ObjectStorageError::System {
                    operation: "reinspect source object",
                    source,
                })?;
                if same_file_state(source_before, &after) {
                    Ok(())
                } else {
                    Err(ObjectStorageError::SourceChanged)
                }
            },
            |directories, file| self.inner.complete_durability(directories, file),
        )?;
        Ok(PromotionMethod::Copied)
    }

    pub(crate) fn snapshot_controlled(
        &self,
        identity: ObjectIdentity<'_>,
        limits: ObjectLimits,
        control: ObjectControl<'_>,
    ) -> Result<VerifiedObject, ObjectStorageError> {
        self.inner
            .snapshot_controlled(&object_reference(identity)?, limits, control)
    }

    /// Keeps the verified descriptor and its pre-read state for bounded final
    /// admission checks. Generated snapshots retain their existing semantics.
    pub(crate) fn guarded_snapshot_controlled(
        &self,
        identity: ObjectIdentity<'_>,
        limits: ObjectLimits,
        control: ObjectControl<'_>,
    ) -> Result<(VerifiedObject, ObjectFreshnessGuard), ObjectStorageError> {
        let mut snapshot = tempfile::tempfile().map_err(|source| ObjectStorageError::Io {
            operation: "create original-object snapshot",
            source,
        })?;
        let (guard, sha256) = self.verify_guarded(identity, limits, control, &mut snapshot)?;
        snapshot
            .seek(SeekFrom::Start(0))
            .map_err(|source| ObjectStorageError::Io {
                operation: "rewind original-object snapshot",
                source,
            })?;
        let verified = VerifiedObject {
            file: snapshot,
            reference: guard.reference.clone(),
            sha256,
        };
        Ok((verified, guard))
    }

    pub(crate) fn guard_controlled(
        &self,
        identity: ObjectIdentity<'_>,
        limits: ObjectLimits,
        control: ObjectControl<'_>,
    ) -> Result<(ObjectFreshnessGuard, [u8; 32]), ObjectStorageError> {
        self.verify_guarded(identity, limits, control, io::sink())
    }

    fn verify_guarded(
        &self,
        identity: ObjectIdentity<'_>,
        limits: ObjectLimits,
        control: ObjectControl<'_>,
        destination: impl Write,
    ) -> Result<(ObjectFreshnessGuard, [u8; 32]), ObjectStorageError> {
        control.check()?;
        let expected = object_reference(identity)?;
        validate_budget(&expected, limits)?;
        let directories = self.inner.open_directories()?;
        let target = object_name(expected.content());
        let source = self
            .inner
            .open_object_optional(&directories.generated, &target, &expected)?
            .ok_or_else(|| ObjectStorageError::MissingObject(expected.content().clone()))?;
        // Capture before hashing, not after: a change in the gap between the
        // verification loop and creating this guard must invalidate admission.
        let state = fstat(&source).map_err(|source| ObjectStorageError::System {
            operation: "inspect guarded original object",
            source,
        })?;
        let mut writer = Sha256Writer::new(destination);
        let source = self.inner.verify_open_object_controlled(
            source,
            &expected,
            limits,
            &mut writer,
            control,
            || {},
        )?;
        let guard = ObjectFreshnessGuard {
            source,
            state,
            reference: expected,
            limits,
        };
        self.recheck_guard(&guard)?;
        control.check()?;
        Ok((guard, writer.finish()))
    }

    /// Reopens the current package namespace and compares exact source state.
    /// No original bytes are copied or hashed here.
    pub(crate) fn recheck_guard(
        &self,
        guard: &ObjectFreshnessGuard,
    ) -> Result<(), ObjectStorageError> {
        let directories = self.inner.open_directories()?;
        let target = object_name(guard.reference.content());
        let named = self
            .inner
            .open_object_optional(&directories.generated, &target, &guard.reference)?
            .ok_or_else(|| ObjectStorageError::MissingObject(guard.reference.content().clone()))?;
        for source in [named.as_fd(), guard.source.as_fd()] {
            let current = fstat(source).map_err(|source| ObjectStorageError::System {
                operation: "reinspect guarded original object",
                source,
            })?;
            self.inner
                .validate_object_metadata(&current, &guard.reference, guard.limits)?;
            if !same_file_state(&guard.state, &current) {
                return Err(ObjectStorageError::SourceChanged);
            }
        }
        Ok(())
    }

    fn has_verified_object(
        &self,
        expected: &GeneratedObjectRef,
        limits: ObjectLimits,
        control: ObjectControl<'_>,
    ) -> Result<bool, ObjectStorageError> {
        control.check()?;
        let directories = self.inner.open_directories()?;
        let target = object_name(expected.content());
        let Some(existing) =
            self.inner
                .open_object_optional(&directories.generated, &target, expected)?
        else {
            return Ok(false);
        };
        let existing = self.inner.verify_open_object_controlled(
            existing,
            expected,
            limits,
            io::sink(),
            control,
            || {},
        )?;
        self.inner.complete_durability(&directories, &existing)?;
        confirm_named_file(&directories.generated, &target, &existing)?;
        Ok(true)
    }

    #[cfg(target_os = "macos")]
    fn try_clone_file_with(
        &self,
        source: &File,
        expected: &GeneratedObjectRef,
        limits: ObjectLimits,
        control: ObjectControl<'_>,
        source_before: &Stat,
        clone_file: &mut impl FnMut(
            &File,
            &File,
            &std::ffi::CStr,
        ) -> io::Result<deadpan_fileclone::CloneOutcome>,
    ) -> Result<Option<PromotionMethod>, ObjectStorageError> {
        let directories = self.inner.open_directories()?;
        let directory = File::from(directories.generated.try_clone().map_err(|source| {
            ObjectStorageError::Io {
                operation: "clone originals directory descriptor",
                source,
            }
        })?);
        for _ in 0..TEMPORARY_ATTEMPTS {
            control.check()?;
            let temporary_name = format!(".pending-{}", uuid::Uuid::new_v4());
            let c_name = CString::new(temporary_name.as_str())
                .map_err(|_| ObjectStorageError::TemporaryNameExhausted)?;
            match clone_file(source, &directory, &c_name) {
                Ok(deadpan_fileclone::CloneOutcome::Unsupported) => return Ok(None),
                Ok(deadpan_fileclone::CloneOutcome::Cloned) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(source) => {
                    return Err(ObjectStorageError::Io {
                        operation: "clone source object",
                        source,
                    });
                }
            }
            // Capture the new namespace entry before opening it, so an open or
            // validation failure still cleans up exactly the observed inode.
            let metadata = statat(
                &directories.generated,
                temporary_name.as_str(),
                AtFlags::SYMLINK_NOFOLLOW,
            )
            .map_err(|source| ObjectStorageError::System {
                operation: "inspect cloned pending entry",
                source,
            })?;
            let mut pending = PendingObject {
                directory: &directories.generated,
                name: temporary_name.clone(),
                device: i128::from(metadata.st_dev),
                inode: i128::from(metadata.st_ino),
                published: false,
            };
            self.inner.validate_contained(&metadata, &temporary_name)?;
            pending.confirm_path()?;
            control.check()?;
            let cloned = openat(
                &directories.generated,
                temporary_name.as_str(),
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|source| ObjectStorageError::System {
                operation: "open cloned pending object",
                source,
            })?;
            let metadata = fstat(&cloned).map_err(|source| ObjectStorageError::System {
                operation: "inspect cloned pending object",
                source,
            })?;
            self.inner.validate_contained(&metadata, &temporary_name)?;
            if !FileType::from_raw_mode(metadata.st_mode).is_file()
                || metadata.st_nlink != 1
                || i128::from(metadata.st_dev) != pending.device
                || i128::from(metadata.st_ino) != pending.inode
            {
                return Err(ObjectStorageError::UnsafeObject(temporary_name));
            }
            let cloned = File::from(cloned);
            fchmod(&cloned, FINAL_MODE).map_err(|source| ObjectStorageError::System {
                operation: "make cloned object read-only",
                source,
            })?;
            let source_after = fstat(source).map_err(|source| ObjectStorageError::System {
                operation: "reinspect source object after clone",
                source,
            })?;
            if !same_file_state(source_before, &source_after) {
                return Err(ObjectStorageError::SourceChanged);
            }
            let cloned = self.inner.verify_open_object_controlled(
                cloned.into(),
                expected,
                limits,
                io::sink(),
                control,
                || {},
            )?;
            sync_file(&cloned, "sync cloned pending object")?;
            pending.confirm_path()?;
            let target = object_name(expected.content());
            match renameat_with(
                &directories.generated,
                temporary_name.as_str(),
                &directories.generated,
                target.as_str(),
                RenameFlags::NOREPLACE,
            ) {
                Ok(()) => pending.published = true,
                Err(rustix::io::Errno::EXIST) => {
                    return self
                        .has_verified_object(expected, limits, control)
                        .and_then(|exists| {
                            exists
                                .then_some(Some(PromotionMethod::Existing))
                                .ok_or_else(|| {
                                    ObjectStorageError::MissingObject(expected.content().clone())
                                })
                        });
                }
                Err(source) => {
                    return Err(ObjectStorageError::System {
                        operation: "publish cloned object",
                        source,
                    });
                }
            }
            self.inner.complete_durability(&directories, &cloned)?;
            confirm_named_file(&directories.generated, &target, &cloned)?;
            return Ok(Some(PromotionMethod::Cloned));
        }
        Err(ObjectStorageError::TemporaryNameExhausted)
    }
}

fn object_reference(
    identity: ObjectIdentity<'_>,
) -> Result<GeneratedObjectRef, ObjectStorageError> {
    let content = GeneratedContentId::new(identity.digest().to_owned())
        .map_err(|_| ObjectStorageError::InvalidIdentity)?;
    GeneratedObjectRef::new(content, identity.byte_length())
        .map_err(|_| ObjectStorageError::InvalidIdentity)
}

fn validate_source_file(
    metadata: &Stat,
    expected: &GeneratedObjectRef,
    limits: ObjectLimits,
) -> Result<(), ObjectStorageError> {
    if !FileType::from_raw_mode(metadata.st_mode).is_file() {
        return Err(ObjectStorageError::NotRegularFile(
            expected.content().clone(),
        ));
    }
    let actual = u64::try_from(metadata.st_size).map_err(|_| ObjectStorageError::SourceChanged)?;
    if actual > limits.maximum_bytes() {
        return Err(ObjectStorageError::TooLarge {
            size: actual,
            maximum: limits.maximum_bytes(),
        });
    }
    if actual != expected.byte_length() {
        return Err(ObjectStorageError::LengthMismatch {
            expected: expected.byte_length(),
            actual,
        });
    }
    Ok(())
}

struct Sha256Writer<W> {
    destination: W,
    hasher: Sha256,
}

impl<W> Sha256Writer<W> {
    fn new(destination: W) -> Self {
        Self {
            destination,
            hasher: Sha256::new(),
        }
    }

    fn finish(self) -> [u8; 32] {
        self.hasher.finalize().into()
    }
}

impl<W: Write> Write for Sha256Writer<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let written = self.destination.write(bytes)?;
        self.hasher.update(&bytes[..written]);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.destination.flush()
    }
}

struct ReadAtReader<'a> {
    file: &'a File,
    offset: u64,
}

impl Read for ReadAtReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        use std::os::unix::fs::FileExt;
        let read = self.file.read_at(buffer, self.offset)?;
        self.offset = self
            .offset
            .checked_add(u64::try_from(read).expect("read length fits u64"))
            .ok_or_else(|| io::Error::other("source object offset overflow"))?;
        Ok(read)
    }
}

struct PendingObject<'a> {
    directory: &'a OwnedFd,
    name: String,
    device: i128,
    inode: i128,
    published: bool,
}

impl PendingObject<'_> {
    fn confirm_path(&self) -> Result<(), ObjectStorageError> {
        let metadata = statat(
            self.directory,
            self.name.as_str(),
            AtFlags::SYMLINK_NOFOLLOW,
        )
        .map_err(|source| ObjectStorageError::System {
            operation: "reinspect pending generated-object path",
            source,
        })?;
        if i128::from(metadata.st_dev) != self.device
            || i128::from(metadata.st_ino) != self.inode
            || !FileType::from_raw_mode(metadata.st_mode).is_file()
            || metadata.st_nlink != 1
        {
            return Err(ObjectStorageError::SourceChanged);
        }
        Ok(())
    }
}

impl Drop for PendingObject<'_> {
    fn drop(&mut self) {
        if self.published {
            return;
        }
        let Ok(metadata) = statat(
            self.directory,
            self.name.as_str(),
            AtFlags::SYMLINK_NOFOLLOW,
        ) else {
            return;
        };
        if i128::from(metadata.st_dev) == self.device && i128::from(metadata.st_ino) == self.inode {
            let _ = unlinkat(self.directory, self.name.as_str(), AtFlags::empty());
        }
    }
}

fn validate_budget(
    expected: &GeneratedObjectRef,
    limits: ObjectLimits,
) -> Result<(), ObjectStorageError> {
    if expected.byte_length() > limits.maximum_bytes {
        return Err(ObjectStorageError::TooLarge {
            size: expected.byte_length(),
            maximum: limits.maximum_bytes,
        });
    }
    Ok(())
}

fn object_name(content: &GeneratedContentId) -> String {
    format!("blake3-{}", content.digest())
}

fn storage_open_error(component: &str, source: rustix::io::Errno) -> ObjectStorageError {
    match source {
        rustix::io::Errno::NOENT => ObjectStorageError::MissingStorageComponent(component.into()),
        rustix::io::Errno::LOOP | rustix::io::Errno::NOTDIR => {
            ObjectStorageError::UnsafeStorageComponent(component.into())
        }
        _ => ObjectStorageError::System {
            operation: "open generated-media storage",
            source,
        },
    }
}

fn sync_directory(directory: &OwnedFd, operation: &'static str) -> Result<(), ObjectStorageError> {
    fsync(directory).map_err(|source| ObjectStorageError::System { operation, source })
}

#[cfg(target_os = "macos")]
fn sync_file(file: &File, operation: &'static str) -> Result<(), ObjectStorageError> {
    rustix::fs::fcntl_fullfsync(file)
        .map_err(|source| ObjectStorageError::System { operation, source })
}

#[cfg(target_os = "linux")]
fn sync_file(file: &File, operation: &'static str) -> Result<(), ObjectStorageError> {
    fsync(file).map_err(|source| ObjectStorageError::System { operation, source })
}

#[cfg(target_os = "macos")]
fn full_sync_file(file: &File, operation: &'static str) -> Result<(), ObjectStorageError> {
    rustix::fs::fcntl_fullfsync(file)
        .map_err(|source| ObjectStorageError::System { operation, source })
}

#[cfg(target_os = "linux")]
fn full_sync_file(_: &File, _: &'static str) -> Result<(), ObjectStorageError> {
    Ok(())
}

fn namespace_is_writable_by_others(metadata: &Stat) -> bool {
    metadata.st_mode & (Mode::WGRP | Mode::WOTH).bits() != 0
}

fn confirm_named_file(
    directory: &OwnedFd,
    name: &str,
    file: &File,
) -> Result<(), ObjectStorageError> {
    let descriptor = fstat(file).map_err(|source| ObjectStorageError::System {
        operation: "inspect durable generated object",
        source,
    })?;
    let named = statat(directory, name, AtFlags::SYMLINK_NOFOLLOW).map_err(|source| {
        ObjectStorageError::System {
            operation: "reinspect durable generated-object path",
            source,
        }
    })?;
    if descriptor.st_dev != named.st_dev
        || descriptor.st_ino != named.st_ino
        || !FileType::from_raw_mode(named.st_mode).is_file()
    {
        return Err(ObjectStorageError::SourceChanged);
    }
    Ok(())
}

fn same_file_state(before: &Stat, after: &Stat) -> bool {
    before.st_dev == after.st_dev
        && before.st_ino == after.st_ino
        && before.st_mode == after.st_mode
        && before.st_uid == after.st_uid
        && before.st_size == after.st_size
        && before.st_nlink == after.st_nlink
        && before.st_mtime == after.st_mtime
        && before.st_mtime_nsec == after.st_mtime_nsec
        && before.st_ctime == after.st_ctime
        && before.st_ctime_nsec == after.st_ctime_nsec
}

#[derive(Debug, Error)]
pub enum ObjectStorageError {
    #[error("object identity must contain a lowercase 64-digit BLAKE3 digest and positive length")]
    InvalidIdentity,
    #[error("media byte budget must be positive")]
    InvalidBudget,
    #[error("object storage operation was cancelled")]
    Cancelled,
    #[error("object storage session is closed")]
    SessionClosed,
    #[error("object storage operation exceeded its deadline")]
    DeadlineExceeded,
    #[error("media storage component is missing: {0}")]
    MissingStorageComponent(String),
    #[error("media storage component is unsafe: {0}")]
    UnsafeStorageComponent(String),
    #[error("media object does not exist: {0}")]
    MissingObject(GeneratedContentId),
    #[error("media object is unsafe: {0}")]
    UnsafeObject(String),
    #[error("media object is not a regular file: {0}")]
    NotRegularFile(GeneratedContentId),
    #[error("media path crossed onto another filesystem at {0}")]
    CrossDevice(String),
    #[error("media path has an unexpected owner at {0}")]
    UnexpectedOwner(String),
    #[error("media object has another hard link: {0}")]
    MultipleLinks(GeneratedContentId),
    #[error("media object is writable: {0}")]
    WritableObject(GeneratedContentId),
    #[error("media object size {size} exceeds host limit {maximum}")]
    TooLarge { size: u64, maximum: u64 },
    #[error("media object length is {actual}, expected {expected}")]
    LengthMismatch { expected: u64, actual: u64 },
    #[error("media object BLAKE3 mismatch for {expected}: observed {observed}")]
    HashMismatch {
        expected: GeneratedContentId,
        observed: String,
    },
    #[error("media object changed during verification")]
    SourceChanged,
    #[error("could not allocate a unique pending media-object name")]
    TemporaryNameExhausted,
    #[error("media input read failed")]
    SourceRead {
        #[source]
        source: io::Error,
    },
    #[error("{operation} failed")]
    Io {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("{operation} failed")]
    System {
        operation: &'static str,
        #[source]
        source: rustix::io::Errno,
    },
}

impl ObjectStorageError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidIdentity => "GeneratedMediaIdentityInvalid",
            Self::InvalidBudget => "GeneratedMediaBudgetInvalid",
            Self::Cancelled => "OperationCancelled",
            Self::SessionClosed => "StorageSessionClosed",
            Self::DeadlineExceeded => "DeadlineExceeded",
            Self::MissingStorageComponent(_) => "GeneratedMediaStorageMissing",
            Self::UnsafeStorageComponent(_) | Self::UnsafeObject(_) => "GeneratedMediaPathUnsafe",
            Self::MissingObject(_) => "GeneratedMediaMissing",
            Self::NotRegularFile(_) => "GeneratedMediaNotRegular",
            Self::CrossDevice(_) => "GeneratedMediaCrossDevice",
            Self::UnexpectedOwner(_) => "GeneratedMediaOwnerMismatch",
            Self::MultipleLinks(_) => "GeneratedMediaMultipleLinks",
            Self::WritableObject(_) => "GeneratedMediaWritable",
            Self::TooLarge { .. } => "GeneratedMediaTooLarge",
            Self::LengthMismatch { .. } => "GeneratedMediaLengthMismatch",
            Self::HashMismatch { .. } => "GeneratedMediaHashMismatch",
            Self::SourceChanged => "GeneratedMediaSourceChanged",
            Self::TemporaryNameExhausted => "GeneratedMediaTemporaryNameExhausted",
            Self::SourceRead { source } | Self::Io { source, .. } => io_error_code(source),
            Self::System { source, .. } => errno_code(*source),
        }
    }
}

fn io_error_code(error: &io::Error) -> &'static str {
    match error.kind() {
        io::ErrorKind::StorageFull => "DiskFull",
        io::ErrorKind::ReadOnlyFilesystem => "ProjectReadOnly",
        io::ErrorKind::PermissionDenied => "PermissionDenied",
        _ => "IoFailure",
    }
}

fn errno_code(error: rustix::io::Errno) -> &'static str {
    match error {
        rustix::io::Errno::NOSPC => "DiskFull",
        rustix::io::Errno::ROFS => "ProjectReadOnly",
        rustix::io::Errno::ACCESS | rustix::io::Errno::PERM => "PermissionDenied",
        _ => "IoFailure",
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, OpenOptions};
    use std::io::Cursor;
    use std::os::unix::fs::{PermissionsExt, symlink};

    use super::*;

    fn package() -> tempfile::TempDir {
        let package = tempfile::tempdir().unwrap();
        fs::create_dir(package.path().join("Media")).unwrap();
        fs::create_dir(package.path().join("Media/Generated")).unwrap();
        package
    }

    fn object(bytes: &[u8]) -> GeneratedObjectRef {
        GeneratedObjectRef::new(
            GeneratedContentId::new(blake3::hash(bytes).to_hex().to_string()).unwrap(),
            u64::try_from(bytes.len()).unwrap(),
        )
        .unwrap()
    }

    fn limits() -> ObjectLimits {
        ObjectLimits::new(1024 * 1024).unwrap()
    }

    fn object_path(package: &Path, reference: &GeneratedObjectRef) -> std::path::PathBuf {
        package
            .join("Media/Generated")
            .join(object_name(reference.content()))
    }

    fn pending_entries(package: &Path) -> Vec<String> {
        fs::read_dir(package.join("Media/Generated"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".pending-"))
            .collect()
    }

    fn originals_object_path(package: &Path, reference: &GeneratedObjectRef) -> std::path::PathBuf {
        package
            .join("Media/Originals")
            .join(object_name(reference.content()))
    }

    fn originals_pending_entries(package: &Path) -> Vec<String> {
        fs::read_dir(package.join("Media/Originals"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".pending-"))
            .collect()
    }

    #[test]
    fn identifiers_and_references_have_a_strict_wire_shape() {
        let reference = object(b"abc");
        let json = serde_json::to_string(&reference).unwrap();
        assert_eq!(
            json,
            format!(
                "{{\"content\":{{\"algorithm\":\"blake3\",\"digest\":\"{}\"}},\"byte_length\":3}}",
                reference.content().digest()
            )
        );
        assert_eq!(
            serde_json::from_str::<GeneratedObjectRef>(&json).unwrap(),
            reference
        );
        for invalid in [
            r#"{"content":{"algorithm":"sha256","digest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"byte_length":1}"#,
            r#"{"content":{"algorithm":"blake3","digest":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"},"byte_length":1}"#,
            r#"{"content":{"algorithm":"blake3","digest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","extra":1},"byte_length":1}"#,
            r#"{"content":{"algorithm":"blake3","digest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"byte_length":0}"#,
            r#"{"content":{"algorithm":"blake3","digest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"byte_length":1,"extra":1}"#,
        ] {
            assert!(serde_json::from_str::<GeneratedObjectRef>(invalid).is_err());
        }
        assert!(ObjectLimits::new(0).is_err());
    }

    #[test]
    fn promotion_is_read_only_deduplicated_and_snapshot_is_immutable() {
        let package = package();
        let storage = GeneratedStorage::open(package.path()).unwrap();
        let expected = object(b"generated bytes");
        assert_eq!(
            storage
                .promote(&mut Cursor::new(b"generated bytes"), &expected, limits())
                .unwrap(),
            expected
        );
        let path = object_path(package.path(), &expected);
        assert_eq!(fs::read(&path).unwrap(), b"generated bytes");
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o222, 0);

        struct PanicReader;
        impl Read for PanicReader {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                panic!("deduplication must not consume another source")
            }
        }
        storage
            .promote(&mut PanicReader, &expected, limits())
            .unwrap();

        let mut snapshot = storage.snapshot(&expected, limits()).unwrap();
        assert_eq!(snapshot.reference(), &expected);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(&path, b"changed later!!!").unwrap();
        let mut frozen = Vec::new();
        snapshot.read_to_end(&mut frozen).unwrap();
        assert_eq!(frozen, b"generated bytes");
    }

    #[test]
    fn bounds_mismatches_and_source_errors_remove_only_pending_files() {
        let package = package();
        let storage = GeneratedStorage::open(package.path()).unwrap();
        let expected = object(b"abc");
        let mut untouched = Cursor::new(b"abc");
        assert!(matches!(
            storage.promote(&mut untouched, &expected, ObjectLimits::new(2).unwrap()),
            Err(ObjectStorageError::TooLarge { .. })
        ));
        assert_eq!(untouched.position(), 0);

        assert!(matches!(
            storage.promote(&mut Cursor::new(b"abcd"), &expected, limits()),
            Err(ObjectStorageError::LengthMismatch {
                expected: 3,
                actual: 4
            })
        ));
        assert!(matches!(
            storage.promote(&mut Cursor::new(b"ab"), &expected, limits()),
            Err(ObjectStorageError::LengthMismatch {
                expected: 3,
                actual: 2
            })
        ));
        let wrong_hash =
            GeneratedObjectRef::new(GeneratedContentId::new("a".repeat(64)).unwrap(), 3).unwrap();
        assert!(matches!(
            storage.promote(&mut Cursor::new(b"abc"), &wrong_hash, limits()),
            Err(ObjectStorageError::HashMismatch { .. })
        ));

        struct FailingReader {
            first: bool,
        }
        impl Read for FailingReader {
            fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
                if self.first {
                    self.first = false;
                    buffer[..2].copy_from_slice(b"ab");
                    Ok(2)
                } else {
                    Err(io::Error::other("injected read failure"))
                }
            }
        }
        assert!(matches!(
            storage.promote(&mut FailingReader { first: true }, &expected, limits()),
            Err(ObjectStorageError::SourceRead { .. })
        ));
        assert!(pending_entries(package.path()).is_empty());
        assert!(!object_path(package.path(), &expected).exists());
        assert!(!object_path(package.path(), &wrong_hash).exists());
    }

    #[test]
    fn an_existing_corrupt_object_is_never_overwritten() {
        let package = package();
        let storage = GeneratedStorage::open(package.path()).unwrap();
        let expected = object(b"right");
        let path = object_path(package.path(), &expected);
        fs::write(&path, b"wrong").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
        assert!(matches!(
            storage.promote(&mut Cursor::new(b"right"), &expected, limits()),
            Err(ObjectStorageError::HashMismatch { .. })
        ));
        assert_eq!(fs::read(&path).unwrap(), b"wrong");
        assert!(pending_entries(package.path()).is_empty());
    }

    #[test]
    fn missing_and_symbolic_directories_fail_only_media_operations() {
        let package = tempfile::tempdir().unwrap();
        let storage = GeneratedStorage::open(package.path()).unwrap();
        let expected = object(b"abc");
        assert!(matches!(
            storage.snapshot(&expected, limits()),
            Err(ObjectStorageError::MissingStorageComponent(component)) if component == "Media"
        ));

        fs::create_dir(package.path().join("elsewhere")).unwrap();
        symlink("elsewhere", package.path().join("Media")).unwrap();
        assert!(matches!(
            storage.snapshot(&expected, limits()),
            Err(ObjectStorageError::UnsafeStorageComponent(component)) if component == "Media"
        ));
    }

    #[test]
    fn held_descriptors_defeat_directory_swaps_and_final_symlinks() {
        let package_root = package();
        let storage = GeneratedStorage::open(package_root.path()).unwrap();
        let expected = object(b"original");
        storage
            .promote(&mut Cursor::new(b"original"), &expected, limits())
            .unwrap();

        let package_path = package_root.path().to_path_buf();
        let mut snapshot = storage
            .snapshot_after_open(&expected, limits(), || {
                fs::rename(package_path.join("Media"), package_path.join("PinnedMedia")).unwrap();
                fs::create_dir(package_path.join("Media")).unwrap();
                fs::create_dir(package_path.join("Media/Generated")).unwrap();
                fs::write(
                    package_path.join("Media/Generated/attacker"),
                    b"replacement",
                )
                .unwrap();
            })
            .unwrap();
        let mut bytes = Vec::new();
        snapshot.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"original");

        let symlink_package = package();
        let storage = GeneratedStorage::open(symlink_package.path()).unwrap();
        let target = object(b"link target");
        symlink("/dev/null", object_path(symlink_package.path(), &target)).unwrap();
        assert!(matches!(
            storage.snapshot(&target, limits()),
            Err(ObjectStorageError::UnsafeObject(_))
        ));
    }

    #[test]
    fn hardlinks_writable_files_and_mutation_are_rejected() {
        let package = package();
        let storage = GeneratedStorage::open(package.path()).unwrap();
        let expected = object(b"same");
        let path = object_path(package.path(), &expected);
        fs::write(&path, b"same").unwrap();
        assert!(matches!(
            storage.snapshot(&expected, limits()),
            Err(ObjectStorageError::WritableObject(_))
        ));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
        fs::hard_link(&path, package.path().join("Media/Generated/alias")).unwrap();
        assert!(matches!(
            storage.snapshot(&expected, limits()),
            Err(ObjectStorageError::MultipleLinks(_))
        ));
        fs::remove_file(package.path().join("Media/Generated/alias")).unwrap();

        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let mut writer = OpenOptions::new().write(true).open(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
        let result = storage.snapshot_after_open(&expected, limits(), || {
            writer.write_all(b"else").unwrap();
            writer.sync_all().unwrap();
        });
        assert!(matches!(
            result,
            Err(ObjectStorageError::SourceChanged | ObjectStorageError::HashMismatch { .. })
        ));
    }

    #[test]
    fn replaced_pending_entry_is_not_published_or_deleted() {
        use std::cell::RefCell;

        let package = package();
        let storage = GeneratedStorage::open(package.path()).unwrap();
        let expected = object(b"publish me");
        let replacement_name = RefCell::new(None);
        let directory = package.path().join("Media/Generated");
        let result = storage.promote_with_hooks(
            &mut Cursor::new(b"publish me"),
            &expected,
            limits(),
            |name| {
                let path = directory.join(name);
                fs::remove_file(&path).unwrap();
                fs::write(&path, b"unrelated replacement").unwrap();
                *replacement_name.borrow_mut() = Some(name.to_owned());
            },
            |_, _| Ok(()),
        );
        assert!(matches!(result, Err(ObjectStorageError::SourceChanged)));
        let replacement = directory.join(replacement_name.into_inner().unwrap());
        assert_eq!(fs::read(replacement).unwrap(), b"unrelated replacement");
        assert!(!object_path(package.path(), &expected).exists());
    }

    #[test]
    fn retry_after_post_publish_failure_finishes_existing_object_durability() {
        let package = package();
        let storage = GeneratedStorage::open(package.path()).unwrap();
        let expected = object(b"durable retry");
        let result = storage.promote_with_hooks(
            &mut Cursor::new(b"durable retry"),
            &expected,
            limits(),
            |_| {},
            |_, _| {
                Err(ObjectStorageError::Io {
                    operation: "injected first durability failure",
                    source: io::Error::other("injected"),
                })
            },
        );
        assert!(matches!(result, Err(ObjectStorageError::Io { .. })));
        assert_eq!(
            fs::read(object_path(package.path(), &expected)).unwrap(),
            b"durable retry"
        );

        struct PanicReader;
        impl Read for PanicReader {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                panic!("retry must verify the published object without consuming input")
            }
        }
        let retry = storage.promote_with_hooks(
            &mut PanicReader,
            &expected,
            limits(),
            |_| {},
            |_, _| {
                Err(ObjectStorageError::Io {
                    operation: "injected retry durability failure",
                    source: io::Error::other("injected"),
                })
            },
        );
        assert!(matches!(retry, Err(ObjectStorageError::Io { .. })));
        assert_eq!(
            storage
                .promote(&mut PanicReader, &expected, limits())
                .unwrap(),
            expected
        );
    }

    #[test]
    fn no_replace_race_verifies_and_durably_accepts_the_winner() {
        use std::cell::Cell;

        let package = package();
        let storage = GeneratedStorage::open(package.path()).unwrap();
        let expected = object(b"race winner");
        let target = object_path(package.path(), &expected);
        let durability_called = Cell::new(false);
        assert_eq!(
            storage
                .promote_with_hooks(
                    &mut Cursor::new(b"race winner"),
                    &expected,
                    limits(),
                    |_| {
                        fs::write(&target, b"race winner").unwrap();
                        fs::set_permissions(&target, fs::Permissions::from_mode(0o444)).unwrap();
                    },
                    |_, _| {
                        durability_called.set(true);
                        Ok(())
                    },
                )
                .unwrap(),
            expected
        );
        assert!(durability_called.get());
        assert!(pending_entries(package.path()).is_empty());
    }

    #[test]
    fn namespace_authority_is_checked_at_each_media_operation() {
        let package = package();
        let storage = GeneratedStorage::open(package.path()).unwrap();
        fs::set_permissions(package.path(), fs::Permissions::from_mode(0o770)).unwrap();
        let expected = object(b"bytes");
        assert!(matches!(
            storage.snapshot(&expected, limits()),
            Err(ObjectStorageError::UnsafeStorageComponent(component)) if component == "package"
        ));
    }

    #[test]
    fn device_owner_and_type_checks_are_independent_of_schema_constraints() {
        let package = package();
        let storage = GeneratedStorage::open(package.path()).unwrap();
        let mut metadata = fstat(&storage.package).unwrap();
        metadata.st_dev = metadata.st_dev.wrapping_add(1);
        assert!(matches!(
            storage.validate_contained(&metadata, "mounted"),
            Err(ObjectStorageError::CrossDevice(component)) if component == "mounted"
        ));
        let mut metadata = fstat(&storage.package).unwrap();
        metadata.st_uid = metadata.st_uid.wrapping_add(1);
        assert!(matches!(
            storage.validate_contained(&metadata, "foreign"),
            Err(ObjectStorageError::UnexpectedOwner(component)) if component == "foreign"
        ));

        let expected = object(b"directory");
        fs::create_dir(object_path(package.path(), &expected)).unwrap();
        assert!(matches!(
            storage.snapshot(&expected, limits()),
            Err(ObjectStorageError::NotRegularFile(_))
        ));
    }

    #[test]
    fn shared_original_storage_clones_or_copies_without_moving_the_source_cursor() {
        let package = package();
        fs::create_dir(package.path().join("Media/Originals")).unwrap();
        let storage = ObjectStorage::open(package.path(), StorageNamespace::Originals).unwrap();
        let source_path = package.path().join("source.mov");
        let bytes = b"immutable original source bytes";
        fs::write(&source_path, bytes).unwrap();
        let mut source = OpenOptions::new().read(true).open(&source_path).unwrap();
        source.seek(SeekFrom::Start(7)).unwrap();
        let digest = blake3::hash(bytes).to_hex().to_string();
        let identity = ObjectIdentity::new(&digest, bytes.len() as u64).unwrap();
        let cancelled = AtomicBool::new(false);
        let control = ObjectControl::bounded(
            Instant::now() + std::time::Duration::from_secs(5),
            &cancelled,
        );
        assert!(matches!(
            storage
                .promote_file_controlled(&source, identity, limits(), control)
                .unwrap(),
            PromotionMethod::Cloned | PromotionMethod::Copied
        ));
        assert_eq!(source.stream_position().unwrap(), 7);
        assert_eq!(
            storage
                .promote_file_controlled(&source, identity, limits(), control)
                .unwrap(),
            PromotionMethod::Existing
        );
        assert_eq!(source.stream_position().unwrap(), 7);

        let mut snapshot = storage
            .snapshot_controlled(identity, limits(), control)
            .unwrap();
        let expected_sha256: [u8; 32] = Sha256::digest(bytes).into();
        assert_eq!(snapshot.sha256(), expected_sha256);
        let mut observed = Vec::new();
        snapshot.read_to_end(&mut observed).unwrap();
        assert_eq!(observed, bytes);
        assert!(
            package
                .path()
                .join("Media/Originals")
                .join(format!("blake3-{digest}"))
                .is_file()
        );
        assert!(
            package
                .path()
                .join("Media/Generated")
                .read_dir()
                .unwrap()
                .next()
                .is_none()
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn clone_validation_failure_removes_only_its_observed_pending_entry() {
        let package = package();
        let originals = package.path().join("Media/Originals");
        fs::create_dir(&originals).unwrap();
        let storage = ObjectStorage::open(package.path(), StorageNamespace::Originals).unwrap();
        let bytes = b"source for invalid cloned entry";
        let source_path = package.path().join("source.mov");
        fs::write(&source_path, bytes).unwrap();
        let source = File::open(&source_path).unwrap();
        let expected = object(bytes);
        let other_name = originals.join("retained-link");

        let result = storage.promote_file_controlled_with_clone(
            &source,
            ObjectIdentity::from(&expected),
            limits(),
            ObjectControl::unbounded(),
            |source, directory, name| {
                let result = deadpan_fileclone::try_clone_file(source, directory, name)?;
                assert_eq!(result, deadpan_fileclone::CloneOutcome::Cloned);
                // A second link makes validation fail before the cloned file
                // is opened. The unrelated name must remain intact.
                fs::hard_link(originals.join(name.to_str().unwrap()), &other_name)?;
                Ok(result)
            },
        );
        assert!(matches!(
            result,
            Err(ObjectStorageError::SourceChanged | ObjectStorageError::UnsafeObject(_))
        ));
        assert!(originals_pending_entries(package.path()).is_empty());
        assert!(!originals_object_path(package.path(), &expected).exists());
        assert_eq!(fs::read(other_name).unwrap(), bytes);
        assert_eq!(fs::read(source_path).unwrap(), bytes);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn unsupported_clone_uses_verified_copy_without_moving_the_source_cursor() {
        use std::cell::Cell;

        let package = package();
        fs::create_dir(package.path().join("Media/Originals")).unwrap();
        let storage = ObjectStorage::open(package.path(), StorageNamespace::Originals).unwrap();
        let bytes = vec![0x5a; COPY_BUFFER_BYTES + 17];
        let source_path = package.path().join("copy-source.mov");
        fs::write(&source_path, &bytes).unwrap();
        let mut source = File::open(source_path).unwrap();
        source.seek(SeekFrom::Start(11)).unwrap();
        let expected = object(&bytes);
        let identity = ObjectIdentity::from(&expected);
        let clone_calls = Cell::new(0);

        assert_eq!(
            storage
                .promote_file_controlled_with_clone(
                    &source,
                    identity,
                    limits(),
                    ObjectControl::unbounded(),
                    |_, _, _| {
                        clone_calls.set(clone_calls.get() + 1);
                        Ok(deadpan_fileclone::CloneOutcome::Unsupported)
                    },
                )
                .unwrap(),
            PromotionMethod::Copied
        );
        assert_eq!(clone_calls.get(), 1);
        assert_eq!(source.stream_position().unwrap(), 11);
        assert_eq!(
            fs::read(originals_object_path(package.path(), &expected)).unwrap(),
            bytes
        );
        assert!(originals_pending_entries(package.path()).is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn copy_fallback_rejects_a_source_changed_after_clone_attempt() {
        let package = package();
        fs::create_dir(package.path().join("Media/Originals")).unwrap();
        let storage = ObjectStorage::open(package.path(), StorageNamespace::Originals).unwrap();
        let bytes = vec![0x31; COPY_BUFFER_BYTES + 3];
        let source_path = package.path().join("changed-source.mov");
        fs::write(&source_path, &bytes).unwrap();
        let mut source = File::open(source_path).unwrap();
        source.seek(SeekFrom::Start(13)).unwrap();
        let initial_mode = source.metadata().unwrap().permissions().mode();
        let expected = object(&bytes);
        let identity = ObjectIdentity::from(&expected);

        let result = storage.promote_file_controlled_with_clone(
            &source,
            identity,
            limits(),
            ObjectControl::unbounded(),
            |source, _, _| {
                source
                    .set_permissions(fs::Permissions::from_mode(initial_mode ^ 0o100))
                    .unwrap();
                Ok(deadpan_fileclone::CloneOutcome::Unsupported)
            },
        );

        assert!(matches!(result, Err(ObjectStorageError::SourceChanged)));
        assert_eq!(source.stream_position().unwrap(), 13);
        assert!(!originals_object_path(package.path(), &expected).exists());
        assert!(originals_pending_entries(package.path()).is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn copy_fallback_never_overwrites_a_competing_object() {
        use std::cell::Cell;

        let package = package();
        fs::create_dir(package.path().join("Media/Originals")).unwrap();
        let storage = ObjectStorage::open(package.path(), StorageNamespace::Originals).unwrap();
        let bytes = b"expected original bytes";
        let source_path = package.path().join("raced-source.mov");
        fs::write(&source_path, bytes).unwrap();
        let source = File::open(source_path).unwrap();
        let expected = object(bytes);
        let identity = ObjectIdentity::from(&expected);
        let target = originals_object_path(package.path(), &expected);
        let competing = vec![b'x'; bytes.len()];
        let clone_calls = Cell::new(0);

        let result = storage.promote_file_controlled_with_clone(
            &source,
            identity,
            limits(),
            ObjectControl::unbounded(),
            |_, _, _| {
                clone_calls.set(clone_calls.get() + 1);
                fs::write(&target, &competing).unwrap();
                fs::set_permissions(&target, fs::Permissions::from_mode(0o444)).unwrap();
                Ok(deadpan_fileclone::CloneOutcome::Unsupported)
            },
        );

        assert!(matches!(
            result,
            Err(ObjectStorageError::HashMismatch { .. })
        ));
        assert_eq!(clone_calls.get(), 1);
        assert_eq!(fs::read(target).unwrap(), competing);
        assert!(originals_pending_entries(package.path()).is_empty());
    }

    #[test]
    fn controlled_publication_cancels_and_cleans_its_pending_object() {
        let package = package();
        fs::create_dir(package.path().join("Media/Originals")).unwrap();
        let storage = ObjectStorage::open(package.path(), StorageNamespace::Originals).unwrap();
        let digest = blake3::hash(b"abcdef").to_hex().to_string();
        let identity = ObjectIdentity::new(&digest, 6).unwrap();
        let source_path = package.path().join("cancelled-source");
        fs::write(&source_path, b"abcdef").unwrap();
        let source = File::open(source_path).unwrap();
        let cancelled = AtomicBool::new(true);
        let result = storage.promote_file_controlled(
            &source,
            identity,
            limits(),
            ObjectControl::bounded(
                Instant::now() + std::time::Duration::from_secs(5),
                &cancelled,
            ),
        );
        assert!(matches!(result, Err(ObjectStorageError::Cancelled)));
        assert!(
            fs::read_dir(package.path().join("Media/Originals"))
                .unwrap()
                .next()
                .is_none()
        );

        let expired = storage.snapshot_controlled(
            identity,
            limits(),
            ObjectControl::bounded(
                Instant::now() - std::time::Duration::from_millis(1),
                &AtomicBool::new(false),
            ),
        );
        assert!(matches!(expired, Err(ObjectStorageError::DeadlineExceeded)));
    }

    #[test]
    fn publication_finishes_durability_before_close_or_cancel_stops_verification() {
        use std::cell::Cell;

        for closing in [true, false] {
            let package = package();
            fs::create_dir(package.path().join("Media/Originals")).unwrap();
            let storage = ObjectStorage::open(package.path(), StorageNamespace::Originals).unwrap();
            let bytes = b"original survives interrupted publication";
            let expected = object(bytes);
            let closed = AtomicBool::new(false);
            let cancelled = AtomicBool::new(false);
            let durable = Cell::new(false);
            let result = storage.inner.promote_with_control_hooks(
                &mut Cursor::new(bytes),
                &expected,
                limits(),
                ObjectControl::bounded(
                    Instant::now() + std::time::Duration::from_secs(5),
                    &cancelled,
                )
                .with_closed(&closed),
                |_| {
                    // Raise the stop signal after verification and immediately
                    // before rename, so the next controlled read must stop.
                    if closing { &closed } else { &cancelled }.store(true, Ordering::Release);
                    Ok(())
                },
                |directories, file| {
                    confirm_named_file(
                        &directories.generated,
                        &object_name(expected.content()),
                        file,
                    )?;
                    storage.inner.complete_durability(directories, file)?;
                    durable.set(true);
                    Ok(())
                },
            );
            assert!(
                durable.get(),
                "published bytes must be durable before stopping"
            );
            let error = result.unwrap_err();
            assert_eq!(
                error.code(),
                if closing {
                    "StorageSessionClosed"
                } else {
                    "OperationCancelled"
                }
            );
            assert_eq!(
                crate::original_media::OriginalMediaError::from(error).code(),
                if closing {
                    "OriginalImportClosed"
                } else {
                    "OriginalCancelled"
                }
            );
            assert_eq!(
                fs::read(originals_object_path(package.path(), &expected)).unwrap(),
                bytes
            );
            assert!(originals_pending_entries(package.path()).is_empty());
        }
    }

    #[test]
    fn publication_durability_failure_is_not_hidden_by_close_or_cancel() {
        for closing in [true, false] {
            let package = package();
            let storage = GeneratedStorage::open(package.path()).unwrap();
            let bytes = b"durability failure remains an error";
            let expected = object(bytes);
            let closed = AtomicBool::new(false);
            let cancelled = AtomicBool::new(false);
            let result = storage.promote_with_control_hooks(
                &mut Cursor::new(bytes),
                &expected,
                limits(),
                ObjectControl::bounded(
                    Instant::now() + std::time::Duration::from_secs(5),
                    &cancelled,
                )
                .with_closed(&closed),
                |_| {
                    if closing { &closed } else { &cancelled }.store(true, Ordering::Release);
                    Ok(())
                },
                |_, _| {
                    Err(ObjectStorageError::Io {
                        operation: "injected interrupted publication durability failure",
                        source: io::Error::other("injected"),
                    })
                },
            );
            assert!(matches!(
                result,
                Err(ObjectStorageError::Io {
                    operation: "injected interrupted publication durability failure",
                    ..
                })
            ));
            assert_eq!(
                fs::read(object_path(package.path(), &expected)).unwrap(),
                bytes
            );
            assert!(pending_entries(package.path()).is_empty());
        }
    }

    #[test]
    fn publication_rejects_namespace_replacement_during_durability() {
        let package = package();
        let storage = GeneratedStorage::open(package.path()).unwrap();
        let bytes = b"same bytes in a different file";
        let expected = object(bytes);
        let target = object_path(package.path(), &expected);
        let result = storage.promote_with_hooks(
            &mut Cursor::new(bytes),
            &expected,
            limits(),
            |_| {},
            |_, _| {
                fs::remove_file(&target).unwrap();
                fs::write(&target, bytes).unwrap();
                fs::set_permissions(&target, fs::Permissions::from_mode(0o444)).unwrap();
                Ok(())
            },
        );
        assert!(matches!(result, Err(ObjectStorageError::SourceChanged)));
        assert_eq!(fs::read(target).unwrap(), bytes);
        assert!(pending_entries(package.path()).is_empty());
    }

    #[test]
    fn object_control_distinguishes_session_close_from_user_cancellation() {
        let closed = AtomicBool::new(true);
        let cancelled = AtomicBool::new(false);
        let control = ObjectControl::bounded(
            Instant::now() + std::time::Duration::from_secs(5),
            &cancelled,
        )
        .with_closed(&closed);
        assert_eq!(control.check().unwrap_err().code(), "StorageSessionClosed");
        cancelled.store(true, Ordering::Release);
        assert_eq!(control.check().unwrap_err().code(), "StorageSessionClosed");
        closed.store(false, Ordering::Release);
        assert_eq!(control.check().unwrap_err().code(), "OperationCancelled");
    }
}
