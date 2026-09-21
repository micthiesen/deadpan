//! Content-addressed storage for project-owned generated media.
//!
//! This module stores already-validated bytes under a fixed BLAKE3-derived
//! name and can return an immutable anonymous snapshot of a stored object. It
//! does not decode media, authorize candidate acceptance, mutate the document
//! or database, or implement eviction. Owner-only namespace mutation is an
//! authority boundary; same-user hostile code can race any mutable POSIX
//! namespace, so callers must re-verify a snapshot at acceptance and use.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::fd::OwnedFd;
use std::path::Path;

use rustix::fs::{
    AtFlags, CWD, FileType, Mode, OFlags, RenameFlags, Stat, fchmod, fstat, fsync, openat,
    renameat_with, statat, unlinkat,
};
use thiserror::Error;

pub use deadpan_core::{GeneratedContentId, GeneratedObjectRef};

const COPY_BUFFER_BYTES: usize = 64 * 1024;
const TEMPORARY_ATTEMPTS: usize = 8;
const FINAL_MODE: Mode = Mode::RUSR.union(Mode::RGRP).union(Mode::ROTH);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeneratedMediaLimits {
    maximum_bytes: u64,
}

impl GeneratedMediaLimits {
    pub fn new(maximum_bytes: u64) -> Result<Self, GeneratedMediaError> {
        if maximum_bytes == 0 {
            return Err(GeneratedMediaError::InvalidBudget);
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
pub struct VerifiedGeneratedObject {
    file: File,
    reference: GeneratedObjectRef,
}

impl VerifiedGeneratedObject {
    pub fn reference(&self) -> &GeneratedObjectRef {
        &self.reference
    }
}

impl Read for VerifiedGeneratedObject {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.file.read(buffer)
    }
}

impl Seek for VerifiedGeneratedObject {
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
}

#[derive(Debug)]
struct MediaDirectories {
    media: OwnedFd,
    generated: OwnedFd,
}

impl GeneratedStorage {
    pub(crate) fn open(package: &Path) -> Result<Self, GeneratedMediaError> {
        let package = openat(
            CWD,
            package,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|source| storage_open_error("package", source))?;
        let metadata = fstat(&package).map_err(|source| GeneratedMediaError::System {
            operation: "inspect package root",
            source,
        })?;
        if !FileType::from_raw_mode(metadata.st_mode).is_dir() {
            return Err(GeneratedMediaError::UnsafeStorageComponent(
                "package".into(),
            ));
        }
        Ok(Self {
            package,
            device: i128::from(metadata.st_dev),
            owner: metadata.st_uid,
        })
    }

    /// Copies already-validated bytes into project-managed generated storage.
    /// This verifies content identity only; it makes no media-validity or
    /// authored-acceptance claim.
    pub(crate) fn promote(
        &self,
        reader: &mut impl Read,
        expected: &GeneratedObjectRef,
        limits: GeneratedMediaLimits,
    ) -> Result<GeneratedObjectRef, GeneratedMediaError> {
        self.promote_with_hooks(
            reader,
            expected,
            limits,
            || {},
            |_| {},
            |directories, file| self.complete_durability(directories, file),
        )
    }

    fn promote_with_hooks(
        &self,
        reader: &mut impl Read,
        expected: &GeneratedObjectRef,
        limits: GeneratedMediaLimits,
        after_directories: impl FnOnce(),
        before_publish: impl FnOnce(&str),
        durability: impl FnOnce(&MediaDirectories, &File) -> Result<(), GeneratedMediaError>,
    ) -> Result<GeneratedObjectRef, GeneratedMediaError> {
        validate_budget(expected, limits)?;
        let directories = self.open_directories()?;
        after_directories();
        let target = object_name(expected.content());
        if let Some(existing) =
            self.open_object_optional(&directories.generated, &target, expected)?
        {
            let existing =
                self.verify_open_object(existing, expected, limits, io::sink(), || {})?;
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
            let read = reader
                .read(&mut buffer)
                .map_err(|source| GeneratedMediaError::SourceRead { source })?;
            if read == 0 {
                break;
            }
            let next = copied
                .checked_add(u64::try_from(read).expect("copy buffer length fits u64"))
                .ok_or(GeneratedMediaError::SourceChanged)?;
            if next > expected.byte_length() {
                return Err(GeneratedMediaError::LengthMismatch {
                    expected: expected.byte_length(),
                    actual: next,
                });
            }
            if next > limits.maximum_bytes {
                return Err(GeneratedMediaError::TooLarge {
                    size: next,
                    maximum: limits.maximum_bytes,
                });
            }
            hasher.update(&buffer[..read]);
            temporary
                .write_all(&buffer[..read])
                .map_err(|source| GeneratedMediaError::Io {
                    operation: "write pending generated object",
                    source,
                })?;
            copied = next;
        }
        if copied != expected.byte_length() {
            return Err(GeneratedMediaError::LengthMismatch {
                expected: expected.byte_length(),
                actual: copied,
            });
        }
        let observed = hasher.finalize().to_hex().to_string();
        if observed != expected.content().digest() {
            return Err(GeneratedMediaError::HashMismatch {
                expected: expected.content().clone(),
                observed,
            });
        }
        fchmod(&temporary, FINAL_MODE).map_err(|source| GeneratedMediaError::System {
            operation: "make generated object read-only",
            source,
        })?;
        sync_file(&temporary, "sync pending generated object")?;
        let metadata = fstat(&temporary).map_err(|source| GeneratedMediaError::System {
            operation: "inspect pending generated object",
            source,
        })?;
        self.validate_object_metadata(&metadata, expected, limits)?;
        before_publish(&temporary_name);
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
                    .ok_or_else(|| {
                        GeneratedMediaError::MissingObject(expected.content().clone())
                    })?;
                let existing =
                    self.verify_open_object(existing, expected, limits, io::sink(), || {})?;
                durability(&directories, &existing)?;
                confirm_named_file(&directories.generated, &target, &existing)?;
                return Ok(expected.clone());
            }
            Err(source) => {
                return Err(GeneratedMediaError::System {
                    operation: "publish generated object",
                    source,
                });
            }
        }

        let published = self
            .open_object_optional(&directories.generated, &target, expected)?
            .ok_or_else(|| GeneratedMediaError::MissingObject(expected.content().clone()))?;
        let published = self.verify_open_object(published, expected, limits, io::sink(), || {})?;
        durability(&directories, &published)?;
        confirm_named_file(&directories.generated, &target, &published)?;
        Ok(expected.clone())
    }

    /// Returns an anonymous verified copy. Callers must still apply media and
    /// authored-acceptance validation appropriate to their operation.
    pub(crate) fn snapshot(
        &self,
        expected: &GeneratedObjectRef,
        limits: GeneratedMediaLimits,
    ) -> Result<VerifiedGeneratedObject, GeneratedMediaError> {
        self.snapshot_after_open(expected, limits, || {})
    }

    fn snapshot_after_open(
        &self,
        expected: &GeneratedObjectRef,
        limits: GeneratedMediaLimits,
        after_open: impl FnOnce(),
    ) -> Result<VerifiedGeneratedObject, GeneratedMediaError> {
        validate_budget(expected, limits)?;
        let directories = self.open_directories()?;
        let target = object_name(expected.content());
        let source = self
            .open_object_optional(&directories.generated, &target, expected)?
            .ok_or_else(|| GeneratedMediaError::MissingObject(expected.content().clone()))?;
        let mut snapshot = tempfile::tempfile().map_err(|source| GeneratedMediaError::Io {
            operation: "create generated-object snapshot",
            source,
        })?;
        self.verify_open_object(source, expected, limits, &mut snapshot, after_open)?;
        snapshot
            .seek(SeekFrom::Start(0))
            .map_err(|source| GeneratedMediaError::Io {
                operation: "rewind generated-object snapshot",
                source,
            })?;
        Ok(VerifiedGeneratedObject {
            file: snapshot,
            reference: expected.clone(),
        })
    }

    fn open_directories(&self) -> Result<MediaDirectories, GeneratedMediaError> {
        let package = fstat(&self.package).map_err(|source| GeneratedMediaError::System {
            operation: "reinspect package root",
            source,
        })?;
        self.validate_contained(&package, "package")?;
        if !FileType::from_raw_mode(package.st_mode).is_dir()
            || package.st_uid != rustix::process::geteuid().as_raw()
            || namespace_is_writable_by_others(&package)
        {
            return Err(GeneratedMediaError::UnsafeStorageComponent(
                "package".into(),
            ));
        }
        let media = self.open_directory(&self.package, "Media")?;
        let generated = self.open_directory(&media, "Generated")?;
        Ok(MediaDirectories { media, generated })
    }

    fn open_directory(
        &self,
        parent: &OwnedFd,
        name: &'static str,
    ) -> Result<OwnedFd, GeneratedMediaError> {
        let directory = openat(
            parent,
            name,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|source| storage_open_error(name, source))?;
        let metadata = fstat(&directory).map_err(|source| GeneratedMediaError::System {
            operation: "inspect generated-media directory",
            source,
        })?;
        self.validate_contained(&metadata, name)?;
        if !FileType::from_raw_mode(metadata.st_mode).is_dir()
            || namespace_is_writable_by_others(&metadata)
        {
            return Err(GeneratedMediaError::UnsafeStorageComponent(name.into()));
        }
        Ok(directory)
    }

    fn create_pending<'a>(
        &self,
        directory: &'a OwnedFd,
    ) -> Result<(String, OwnedFd, PendingObject<'a>), GeneratedMediaError> {
        for _ in 0..TEMPORARY_ATTEMPTS {
            let name = format!(".pending-{}", uuid::Uuid::new_v4());
            match openat(
                directory,
                name.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR,
            ) {
                Ok(file) => {
                    let metadata = fstat(&file).map_err(|source| GeneratedMediaError::System {
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
                        return Err(GeneratedMediaError::UnsafeObject(name));
                    }
                    return Ok((name, file, pending));
                }
                Err(rustix::io::Errno::EXIST) => continue,
                Err(source) => {
                    return Err(GeneratedMediaError::System {
                        operation: "create pending generated object",
                        source,
                    });
                }
            }
        }
        Err(GeneratedMediaError::TemporaryNameExhausted)
    }

    fn open_object_optional(
        &self,
        directory: &OwnedFd,
        name: &str,
        expected: &GeneratedObjectRef,
    ) -> Result<Option<OwnedFd>, GeneratedMediaError> {
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
                        Err(GeneratedMediaError::UnsafeObject(name.into()))
                    } else if !file_type.is_file() {
                        Err(GeneratedMediaError::NotRegularFile(
                            expected.content().clone(),
                        ))
                    } else {
                        Err(GeneratedMediaError::System {
                            operation: "open generated object",
                            source: open_error,
                        })
                    }
                }
                Err(rustix::io::Errno::NOENT) => Ok(None),
                Err(source) => Err(GeneratedMediaError::System {
                    operation: "inspect unopened generated object",
                    source,
                }),
            },
        }
    }

    fn verify_open_object(
        &self,
        source: OwnedFd,
        expected: &GeneratedObjectRef,
        limits: GeneratedMediaLimits,
        mut destination: impl Write,
        after_open: impl FnOnce(),
    ) -> Result<File, GeneratedMediaError> {
        let before = fstat(&source).map_err(|source| GeneratedMediaError::System {
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
            let read = source
                .read(&mut buffer)
                .map_err(|source| GeneratedMediaError::Io {
                    operation: "read generated object",
                    source,
                })?;
            if read == 0 {
                break;
            }
            let next = copied
                .checked_add(u64::try_from(read).expect("copy buffer length fits u64"))
                .ok_or(GeneratedMediaError::SourceChanged)?;
            if next > expected.byte_length() {
                return Err(GeneratedMediaError::SourceChanged);
            }
            if next > limits.maximum_bytes {
                return Err(GeneratedMediaError::TooLarge {
                    size: next,
                    maximum: limits.maximum_bytes,
                });
            }
            hasher.update(&buffer[..read]);
            destination
                .write_all(&buffer[..read])
                .map_err(|source| GeneratedMediaError::Io {
                    operation: "write generated-object snapshot",
                    source,
                })?;
            copied = next;
        }
        let after = fstat(&source).map_err(|source| GeneratedMediaError::System {
            operation: "reinspect generated object",
            source,
        })?;
        self.validate_object_metadata(&after, expected, limits)?;
        if !same_file_state(&before, &after) {
            return Err(GeneratedMediaError::SourceChanged);
        }
        if copied != expected.byte_length() {
            return Err(GeneratedMediaError::LengthMismatch {
                expected: expected.byte_length(),
                actual: copied,
            });
        }
        let observed = hasher.finalize().to_hex().to_string();
        if observed != expected.content().digest() {
            return Err(GeneratedMediaError::HashMismatch {
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
    ) -> Result<(), GeneratedMediaError> {
        if i128::from(metadata.st_dev) != self.device {
            return Err(GeneratedMediaError::CrossDevice(component.into()));
        }
        if metadata.st_uid != self.owner {
            return Err(GeneratedMediaError::UnexpectedOwner(component.into()));
        }
        Ok(())
    }

    fn complete_durability(
        &self,
        directories: &MediaDirectories,
        file: &File,
    ) -> Result<(), GeneratedMediaError> {
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
        limits: GeneratedMediaLimits,
    ) -> Result<(), GeneratedMediaError> {
        if !FileType::from_raw_mode(metadata.st_mode).is_file() {
            return Err(GeneratedMediaError::NotRegularFile(
                expected.content().clone(),
            ));
        }
        self.validate_contained(metadata, expected.content().digest())?;
        if metadata.st_nlink != 1 {
            return Err(GeneratedMediaError::MultipleLinks(
                expected.content().clone(),
            ));
        }
        let write_bits = (Mode::WUSR | Mode::WGRP | Mode::WOTH).bits();
        if metadata.st_mode & write_bits != 0 {
            return Err(GeneratedMediaError::WritableObject(
                expected.content().clone(),
            ));
        }
        let actual =
            u64::try_from(metadata.st_size).map_err(|_| GeneratedMediaError::SourceChanged)?;
        if actual > limits.maximum_bytes {
            return Err(GeneratedMediaError::TooLarge {
                size: actual,
                maximum: limits.maximum_bytes,
            });
        }
        if actual != expected.byte_length() {
            return Err(GeneratedMediaError::LengthMismatch {
                expected: expected.byte_length(),
                actual,
            });
        }
        Ok(())
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
    fn confirm_path(&self) -> Result<(), GeneratedMediaError> {
        let metadata = statat(
            self.directory,
            self.name.as_str(),
            AtFlags::SYMLINK_NOFOLLOW,
        )
        .map_err(|source| GeneratedMediaError::System {
            operation: "reinspect pending generated-object path",
            source,
        })?;
        if i128::from(metadata.st_dev) != self.device
            || i128::from(metadata.st_ino) != self.inode
            || !FileType::from_raw_mode(metadata.st_mode).is_file()
            || metadata.st_nlink != 1
        {
            return Err(GeneratedMediaError::SourceChanged);
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
    limits: GeneratedMediaLimits,
) -> Result<(), GeneratedMediaError> {
    if expected.byte_length() > limits.maximum_bytes {
        return Err(GeneratedMediaError::TooLarge {
            size: expected.byte_length(),
            maximum: limits.maximum_bytes,
        });
    }
    Ok(())
}

fn object_name(content: &GeneratedContentId) -> String {
    format!("blake3-{}", content.digest())
}

fn storage_open_error(component: &str, source: rustix::io::Errno) -> GeneratedMediaError {
    match source {
        rustix::io::Errno::NOENT => GeneratedMediaError::MissingStorageComponent(component.into()),
        rustix::io::Errno::LOOP | rustix::io::Errno::NOTDIR => {
            GeneratedMediaError::UnsafeStorageComponent(component.into())
        }
        _ => GeneratedMediaError::System {
            operation: "open generated-media storage",
            source,
        },
    }
}

fn sync_directory(directory: &OwnedFd, operation: &'static str) -> Result<(), GeneratedMediaError> {
    fsync(directory).map_err(|source| GeneratedMediaError::System { operation, source })
}

#[cfg(target_os = "macos")]
fn sync_file(file: &File, operation: &'static str) -> Result<(), GeneratedMediaError> {
    rustix::fs::fcntl_fullfsync(file)
        .map_err(|source| GeneratedMediaError::System { operation, source })
}

#[cfg(target_os = "linux")]
fn sync_file(file: &File, operation: &'static str) -> Result<(), GeneratedMediaError> {
    fsync(file).map_err(|source| GeneratedMediaError::System { operation, source })
}

#[cfg(target_os = "macos")]
fn full_sync_file(file: &File, operation: &'static str) -> Result<(), GeneratedMediaError> {
    rustix::fs::fcntl_fullfsync(file)
        .map_err(|source| GeneratedMediaError::System { operation, source })
}

#[cfg(target_os = "linux")]
fn full_sync_file(_: &File, _: &'static str) -> Result<(), GeneratedMediaError> {
    Ok(())
}

fn namespace_is_writable_by_others(metadata: &Stat) -> bool {
    metadata.st_mode & (Mode::WGRP | Mode::WOTH).bits() != 0
}

fn confirm_named_file(
    directory: &OwnedFd,
    name: &str,
    file: &File,
) -> Result<(), GeneratedMediaError> {
    let descriptor = fstat(file).map_err(|source| GeneratedMediaError::System {
        operation: "inspect durable generated object",
        source,
    })?;
    let named = statat(directory, name, AtFlags::SYMLINK_NOFOLLOW).map_err(|source| {
        GeneratedMediaError::System {
            operation: "reinspect durable generated-object path",
            source,
        }
    })?;
    if descriptor.st_dev != named.st_dev
        || descriptor.st_ino != named.st_ino
        || !FileType::from_raw_mode(named.st_mode).is_file()
    {
        return Err(GeneratedMediaError::SourceChanged);
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
pub enum GeneratedMediaError {
    #[error("generated-media byte budget must be positive")]
    InvalidBudget,
    #[error("generated-media storage component is missing: {0}")]
    MissingStorageComponent(String),
    #[error("generated-media storage component is unsafe: {0}")]
    UnsafeStorageComponent(String),
    #[error("generated object does not exist: {0}")]
    MissingObject(GeneratedContentId),
    #[error("generated object is unsafe: {0}")]
    UnsafeObject(String),
    #[error("generated object is not a regular file: {0}")]
    NotRegularFile(GeneratedContentId),
    #[error("generated-media path crossed onto another filesystem at {0}")]
    CrossDevice(String),
    #[error("generated-media path has an unexpected owner at {0}")]
    UnexpectedOwner(String),
    #[error("generated object has another hard link: {0}")]
    MultipleLinks(GeneratedContentId),
    #[error("generated object is writable: {0}")]
    WritableObject(GeneratedContentId),
    #[error("generated object size {size} exceeds host limit {maximum}")]
    TooLarge { size: u64, maximum: u64 },
    #[error("generated object length is {actual}, expected {expected}")]
    LengthMismatch { expected: u64, actual: u64 },
    #[error("generated object BLAKE3 mismatch for {expected}: observed {observed}")]
    HashMismatch {
        expected: GeneratedContentId,
        observed: String,
    },
    #[error("generated object changed during verification")]
    SourceChanged,
    #[error("could not allocate a unique pending generated-object name")]
    TemporaryNameExhausted,
    #[error("generated input read failed")]
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

impl GeneratedMediaError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidBudget => "GeneratedMediaBudgetInvalid",
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

    fn limits() -> GeneratedMediaLimits {
        GeneratedMediaLimits::new(1024 * 1024).unwrap()
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
        assert!(GeneratedMediaLimits::new(0).is_err());
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
            storage.promote(
                &mut untouched,
                &expected,
                GeneratedMediaLimits::new(2).unwrap()
            ),
            Err(GeneratedMediaError::TooLarge { .. })
        ));
        assert_eq!(untouched.position(), 0);

        assert!(matches!(
            storage.promote(&mut Cursor::new(b"abcd"), &expected, limits()),
            Err(GeneratedMediaError::LengthMismatch {
                expected: 3,
                actual: 4
            })
        ));
        assert!(matches!(
            storage.promote(&mut Cursor::new(b"ab"), &expected, limits()),
            Err(GeneratedMediaError::LengthMismatch {
                expected: 3,
                actual: 2
            })
        ));
        let wrong_hash =
            GeneratedObjectRef::new(GeneratedContentId::new("a".repeat(64)).unwrap(), 3).unwrap();
        assert!(matches!(
            storage.promote(&mut Cursor::new(b"abc"), &wrong_hash, limits()),
            Err(GeneratedMediaError::HashMismatch { .. })
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
            Err(GeneratedMediaError::SourceRead { .. })
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
            Err(GeneratedMediaError::HashMismatch { .. })
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
            Err(GeneratedMediaError::MissingStorageComponent(component)) if component == "Media"
        ));

        fs::create_dir(package.path().join("elsewhere")).unwrap();
        symlink("elsewhere", package.path().join("Media")).unwrap();
        assert!(matches!(
            storage.snapshot(&expected, limits()),
            Err(GeneratedMediaError::UnsafeStorageComponent(component)) if component == "Media"
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
            Err(GeneratedMediaError::UnsafeObject(_))
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
            Err(GeneratedMediaError::WritableObject(_))
        ));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
        fs::hard_link(&path, package.path().join("Media/Generated/alias")).unwrap();
        assert!(matches!(
            storage.snapshot(&expected, limits()),
            Err(GeneratedMediaError::MultipleLinks(_))
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
            Err(GeneratedMediaError::SourceChanged | GeneratedMediaError::HashMismatch { .. })
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
            || {},
            |name| {
                let path = directory.join(name);
                fs::remove_file(&path).unwrap();
                fs::write(&path, b"unrelated replacement").unwrap();
                *replacement_name.borrow_mut() = Some(name.to_owned());
            },
            |_, _| Ok(()),
        );
        assert!(matches!(result, Err(GeneratedMediaError::SourceChanged)));
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
            || {},
            |_| {},
            |_, _| {
                Err(GeneratedMediaError::Io {
                    operation: "injected first durability failure",
                    source: io::Error::other("injected"),
                })
            },
        );
        assert!(matches!(result, Err(GeneratedMediaError::Io { .. })));
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
            || {},
            |_| {},
            |_, _| {
                Err(GeneratedMediaError::Io {
                    operation: "injected retry durability failure",
                    source: io::Error::other("injected"),
                })
            },
        );
        assert!(matches!(retry, Err(GeneratedMediaError::Io { .. })));
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
                    || {},
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
            Err(GeneratedMediaError::UnsafeStorageComponent(component)) if component == "package"
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
            Err(GeneratedMediaError::CrossDevice(component)) if component == "mounted"
        ));
        let mut metadata = fstat(&storage.package).unwrap();
        metadata.st_uid = metadata.st_uid.wrapping_add(1);
        assert!(matches!(
            storage.validate_contained(&metadata, "foreign"),
            Err(GeneratedMediaError::UnexpectedOwner(component)) if component == "foreign"
        ));

        let expected = object(b"directory");
        fs::create_dir(object_path(package.path(), &expected)).unwrap();
        assert!(matches!(
            storage.snapshot(&expected, limits()),
            Err(GeneratedMediaError::NotRegularFile(_))
        ));
    }
}
