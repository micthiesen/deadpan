//! Operational host discovery tied to one live writable store.
//!
//! The host owns the opaque discovery grammar and its secret capability. This
//! module owns only bounded private bytes and descriptor identity checks. It
//! performs no socket I/O and does not advertise merely because a store opens.
//! Same-UID processes are trusted; these checks are not a sandbox against a
//! malicious process replacing mutable POSIX names between system calls.

use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use rustix::fs::{
    AtFlags, CWD, FileType, Mode, OFlags, Stat, fchmod, fstat, fsync, openat, renameat, statat,
    unlinkat,
};
use serde::{Deserialize, Serialize};

use crate::{ProjectStore, StoreError};

pub const MAX_DISCOVERY_BYTES: usize = 8 * 1024;
const DISCOVERY_NAME: &str = ".host.json";
const LOCK_NAME: &str = ".writer.lock";
const PRIVATE_FILE: Mode = Mode::RUSR.union(Mode::WUSR);

/// The actual package directory identity, independent of its spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageIdentity {
    pub device: u64,
    pub inode: u64,
}

/// Opaque capability bytes deliberately have no `Debug` or serialization impl.
pub struct HostDiscovery {
    pub package_identity: PackageIdentity,
    pub bytes: Vec<u8>,
}

struct OwnerToken {
    package_identity: PackageIdentity,
    closed: AtomicBool,
}

/// Revocable proof of one writable store's identity. Clones hold no file, lock,
/// database connection, or discovery registration alive.
#[derive(Clone)]
pub struct WriterOwnerHandle {
    token: Arc<OwnerToken>,
}

impl WriterOwnerHandle {
    pub fn is_closed(&self) -> bool {
        self.token.closed.load(Ordering::Acquire)
    }

    pub fn package_identity(&self) -> PackageIdentity {
        self.token.package_identity
    }
}

pub(crate) struct PackageAnchor {
    directory: OwnedFd,
    identity: PackageIdentity,
    lock_identity: PackageIdentity,
}

impl PackageAnchor {
    pub(crate) fn open(path: &Path, lock: &File) -> Result<Self, StoreError> {
        let directory = open_package(path)?;
        let package = fstat(&directory).map_err(io)?;
        let lock_state = fstat(lock).map_err(io)?;
        validate_lock(&lock_state, &package, true)?;
        let anchor = Self {
            directory,
            identity: identity(&package)?,
            lock_identity: identity(&lock_state)?,
        };
        anchor.check(path, lock)?;
        Ok(anchor)
    }

    fn check(&self, path: &Path, lock: &File) -> Result<(), StoreError> {
        let package = fstat(&self.directory).map_err(io)?;
        validate_package(&package)?;
        if identity(&package)? != self.identity {
            return Err(invalid("package descriptor changed"));
        }
        check_package_name(path, &package)?;
        let held = fstat(lock).map_err(io)?;
        validate_lock(&held, &package, true)?;
        let named = statat(&self.directory, LOCK_NAME, AtFlags::SYMLINK_NOFOLLOW).map_err(io)?;
        validate_lock(&named, &package, true)?;
        if identity(&held)? != self.lock_identity || !same_state(&held, &named) {
            return Err(invalid("writer lock was replaced"));
        }
        Ok(())
    }
}

/// A writer records `.writer.session` once it has opened and removes it when
/// the store closes. Finding one while acquiring the lock means the previous
/// writer ended without closing: a crash, a kill or a power loss.
const SESSION_NAME: &str = ".writer.session";
/// Bound on what an unclean marker may report back.
pub(crate) const MAX_SESSION_MARKER_BYTES: usize = 1024;

/// What a writer found at `.writer.session` before writing its own marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SessionEvidence {
    /// The previous writer closed cleanly.
    Clean,
    /// The previous writer of this package never closed.
    Unclean(String),
    /// A marker written for a different package: this package was copied while
    /// its writer was open. The copy did not crash, so nothing is reported.
    Copied,
}

const SESSION_TEMPORARY_PREFIX: &str = ".writer-session-";

impl PackageAnchor {
    /// The marker text, binding it to this package's device and inode.
    pub(crate) fn session_marker(&self, prefix: &str) -> String {
        format!(
            "{prefix} package={}:{}",
            self.identity.device, self.identity.inode
        )
    }

    /// Reads a marker left by an earlier writer without changing it. Call only
    /// while holding the writer lock.
    pub(crate) fn read_session(&self) -> Result<SessionEvidence, StoreError> {
        let state = match statat(&self.directory, SESSION_NAME, AtFlags::SYMLINK_NOFOLLOW) {
            Err(rustix::io::Errno::NOENT) => return Ok(SessionEvidence::Clean),
            Err(error) => return Err(io(error)),
            Ok(state) => state,
        };
        if !FileType::from_raw_mode(state.st_mode).is_file() {
            // A link, directory or other entry is reported, never followed.
            return Ok(SessionEvidence::Unclean(
                "unreadable writer marker (not a regular file)".into(),
            ));
        }
        let file = File::from(
            openat(
                &self.directory,
                SESSION_NAME,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(io)?,
        );
        let mut bytes = Vec::new();
        file.take(MAX_SESSION_MARKER_BYTES as u64)
            .read_to_end(&mut bytes)?;
        let marker = String::from_utf8_lossy(&bytes).trim().to_owned();
        let here = format!("package={}:{}", self.identity.device, self.identity.inode);
        Ok(
            match marker
                .split_whitespace()
                .find(|part| part.starts_with("package="))
            {
                Some(package) if package != here => SessionEvidence::Copied,
                _ => SessionEvidence::Unclean(marker),
            },
        )
    }

    /// Atomically replaces any marker with this writer's, after removing
    /// temporary markers an interrupted writer left behind. Call only while
    /// holding the writer lock.
    pub(crate) fn write_session(&self, marker: &str) -> Result<(), StoreError> {
        self.remove_session_temporaries();
        if let Ok(state) = statat(&self.directory, SESSION_NAME, AtFlags::SYMLINK_NOFOLLOW)
            && FileType::from_raw_mode(state.st_mode).is_dir()
        {
            return Err(invalid(
                "a directory occupies the writer session marker name",
            ));
        }
        let temporary_name = format!("{SESSION_TEMPORARY_PREFIX}{}.tmp", uuid::Uuid::new_v4());
        let mut temporary = File::from(
            openat(
                &self.directory,
                temporary_name.as_str(),
                OFlags::WRONLY
                    | OFlags::CREATE
                    | OFlags::EXCL
                    | OFlags::NOFOLLOW
                    | OFlags::NONBLOCK
                    | OFlags::CLOEXEC,
                PRIVATE_FILE,
            )
            .map_err(io)?,
        );
        let written = (|| {
            let bytes = marker.as_bytes();
            temporary.write_all(&bytes[..bytes.len().min(MAX_SESSION_MARKER_BYTES)])?;
            temporary.sync_all()?;
            renameat(
                &self.directory,
                temporary_name.as_str(),
                &self.directory,
                SESSION_NAME,
            )
            .map_err(io)?;
            fsync(&self.directory).map_err(io)
        })();
        if let Err(error) = written {
            let _ = unlinkat(&self.directory, temporary_name.as_str(), AtFlags::empty());
            return Err(error);
        }
        Ok(())
    }

    /// Best effort: only regular files with the temporary marker name.
    fn remove_session_temporaries(&self) {
        let Ok(directory) = rustix::fs::Dir::read_from(&self.directory) else {
            return;
        };
        for entry in directory.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(SESSION_TEMPORARY_PREFIX)
                && name.ends_with(".tmp")
                && statat(&self.directory, name.as_str(), AtFlags::SYMLINK_NOFOLLOW)
                    .is_ok_and(|state| FileType::from_raw_mode(state.st_mode).is_file())
            {
                let _ = unlinkat(&self.directory, name.as_str(), AtFlags::empty());
            }
        }
    }

    /// Removes this writer's marker; the next writer then sees a clean close.
    pub(crate) fn end_session(&self) -> Result<(), StoreError> {
        match unlinkat(&self.directory, SESSION_NAME, AtFlags::empty()) {
            Ok(()) | Err(rustix::io::Errno::NOENT) => {}
            Err(error) => return Err(io(error)),
        }
        fsync(&self.directory).map_err(io)
    }
}

pub(crate) struct OwnerState {
    token: Arc<OwnerToken>,
    // Retaining the descriptor prevents inode reuse from impersonating this
    // registration, even if another same-user process replaces its pathname.
    registration: Option<Registration>,
}

struct Registration {
    file: File,
    state: Stat,
}

impl OwnerState {
    fn new(package_identity: PackageIdentity) -> Self {
        Self {
            token: Arc::new(OwnerToken {
                package_identity,
                closed: AtomicBool::new(false),
            }),
            registration: None,
        }
    }

    fn check(&self, handle: &WriterOwnerHandle) -> Result<(), StoreError> {
        if handle.is_closed() || !Arc::ptr_eq(&self.token, &handle.token) {
            return Err(invalid(
                "writer capability is stale or belongs to another store",
            ));
        }
        Ok(())
    }

    pub(crate) fn close(
        &mut self,
        anchor: Option<&PackageAnchor>,
        package: &Path,
        lock: Option<&File>,
    ) {
        // Revoke before removing discovery and before ProjectStore unlocks.
        self.token.closed.store(true, Ordering::Release);
        if let (Some(anchor), Some(lock), Some(registration)) =
            (anchor, lock, self.registration.take())
            && anchor.check(package, lock).is_ok()
            && registration.check(anchor).is_ok()
        {
            let _ = unlinkat(&anchor.directory, DISCOVERY_NAME, AtFlags::empty());
            let _ = fsync(&anchor.directory);
        }
    }
}

impl Registration {
    fn check(&self, anchor: &PackageAnchor) -> Result<(), StoreError> {
        let held = fstat(&self.file).map_err(io)?;
        let named =
            statat(&anchor.directory, DISCOVERY_NAME, AtFlags::SYMLINK_NOFOLLOW).map_err(io)?;
        validate_discovery(&held, anchor.identity)?;
        validate_discovery(&named, anchor.identity)?;
        if !same_state(&self.state, &held) || !same_state(&held, &named) {
            return Err(invalid("host discovery was changed or replaced"));
        }
        Ok(())
    }
}

impl ProjectStore {
    /// Creates the store's revocable identity lazily, without advertising it.
    pub fn writer_owner_handle(&mut self) -> Result<WriterOwnerHandle, StoreError> {
        self.require_writer()?;
        let anchor = self
            .writer_package
            .as_ref()
            .ok_or_else(|| invalid("writer package is unavailable"))?;
        let lock = self
            ._writer_lock
            .as_ref()
            .ok_or_else(|| invalid("writer lock is unavailable"))?;
        anchor.check(&self.package, lock)?;
        let owner = self
            .writer_owner
            .get_or_insert_with(|| OwnerState::new(anchor.identity));
        let handle = WriterOwnerHandle {
            token: Arc::clone(&owner.token),
        };
        self.check_writer_owner(&handle)?;
        Ok(handle)
    }

    /// Must be checked on the writer immediately before dispatching host work.
    pub fn check_writer_owner(&self, handle: &WriterOwnerHandle) -> Result<(), StoreError> {
        self.require_writer()?;
        let owner = self
            .writer_owner
            .as_ref()
            .ok_or_else(|| invalid("writer capability was not activated"))?;
        owner.check(handle)?;
        let anchor = self
            .writer_package
            .as_ref()
            .ok_or_else(|| invalid("writer package is unavailable"))?;
        let lock = self
            ._writer_lock
            .as_ref()
            .ok_or_else(|| invalid("writer lock is unavailable"))?;
        anchor.check(&self.package, lock)?;
        if let Some(registration) = &owner.registration {
            registration.check(anchor)?;
        }
        Ok(())
    }

    /// Atomically publishes opaque owner-only bytes. One registration may be
    /// active per store; callers must explicitly unpublish before replacing it.
    pub fn publish_host_discovery(
        &mut self,
        handle: &WriterOwnerHandle,
        bytes: &[u8],
    ) -> Result<(), StoreError> {
        self.check_writer_owner(handle)?;
        if bytes.len() > MAX_DISCOVERY_BYTES {
            return Err(invalid("host discovery exceeds its byte limit"));
        }
        if self
            .writer_owner
            .as_ref()
            .is_some_and(|owner| owner.registration.is_some())
        {
            return Err(invalid("this writer already has a host registration"));
        }
        let anchor = self
            .writer_package
            .as_ref()
            .ok_or_else(|| invalid("writer package is unavailable"))?;
        let previous = open_discovery(&anchor.directory, anchor.identity)?;
        let temporary_name = format!(".host-{}.tmp", uuid::Uuid::new_v4());
        let mut temporary = File::from(
            openat(
                &anchor.directory,
                temporary_name.as_str(),
                OFlags::WRONLY
                    | OFlags::CREATE
                    | OFlags::EXCL
                    | OFlags::NOFOLLOW
                    | OFlags::NONBLOCK
                    | OFlags::CLOEXEC,
                PRIVATE_FILE,
            )
            .map_err(io)?,
        );
        let publication = (|| {
            fchmod(&temporary, PRIVATE_FILE).map_err(io)?;
            temporary.write_all(bytes)?;
            temporary.sync_all()?;
            let prepared = fstat(&temporary).map_err(io)?;
            validate_discovery(&prepared, anchor.identity)?;
            let named = statat(
                &anchor.directory,
                temporary_name.as_str(),
                AtFlags::SYMLINK_NOFOLLOW,
            )
            .map_err(io)?;
            if !same_state(&prepared, &named) {
                return Err(invalid("temporary host discovery was replaced"));
            }
            self.check_writer_owner(handle)?;
            check_discovery_name(&anchor.directory, previous.as_ref().map(|(_, state)| state))?;
            renameat(
                &anchor.directory,
                temporary_name.as_str(),
                &anchor.directory,
                DISCOVERY_NAME,
            )
            .map_err(io)?;
            Ok(())
        })();
        if let Err(error) = publication {
            // Never unlink an unfamiliar replacement of our temporary name.
            if let (Ok(held), Ok(named)) = (
                fstat(&temporary),
                statat(
                    &anchor.directory,
                    temporary_name.as_str(),
                    AtFlags::SYMLINK_NOFOLLOW,
                ),
            ) && same_state(&held, &named)
            {
                let _ = unlinkat(&anchor.directory, temporary_name.as_str(), AtFlags::empty());
            }
            return Err(error);
        }
        // Rename changes ctime on some platforms; capture after publication.
        let state = fstat(&temporary).map_err(io)?;
        let registration = Registration {
            file: temporary,
            state,
        };
        self.writer_owner
            .as_mut()
            .ok_or_else(|| invalid("writer capability was not activated"))?
            .registration = Some(registration);
        fsync(&anchor.directory).map_err(io)?;
        self.check_writer_owner(handle)
    }

    /// Removes only this store's unchanged registered discovery file.
    pub fn unpublish_host_discovery(
        &mut self,
        handle: &WriterOwnerHandle,
    ) -> Result<(), StoreError> {
        self.check_writer_owner(handle)?;
        let owner = self
            .writer_owner
            .as_mut()
            .ok_or_else(|| invalid("writer capability was not activated"))?;
        if owner.registration.is_none() {
            return Ok(());
        }
        let anchor = self
            .writer_package
            .as_ref()
            .ok_or_else(|| invalid("writer package is unavailable"))?;
        unlinkat(&anchor.directory, DISCOVERY_NAME, AtFlags::empty()).map_err(io)?;
        owner.registration = None;
        fsync(&anchor.directory).map_err(io)?;
        self.check_writer_owner(handle)
    }
}

/// Reads a bounded stable private record without interpreting or logging it.
/// A record alone proves neither that a live host exists nor authentication.
pub fn read_discovery(package: &Path) -> Result<Option<HostDiscovery>, StoreError> {
    let directory = open_package(package)?;
    let package_state = fstat(&directory).map_err(io)?;
    let package_identity = identity(&package_state)?;
    let Some((mut file, before)) = open_discovery(&directory, package_identity)? else {
        check_package_name(package, &package_state)?;
        return Ok(None);
    };
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(u64::try_from(MAX_DISCOVERY_BYTES).map_err(|_| invalid("invalid byte limit"))? + 1)
        .read_to_end(&mut bytes)?;
    let after = fstat(&file).map_err(io)?;
    validate_discovery(&after, package_identity)?;
    let named = statat(&directory, DISCOVERY_NAME, AtFlags::SYMLINK_NOFOLLOW).map_err(io)?;
    if bytes.len() > MAX_DISCOVERY_BYTES
        || u64::try_from(bytes.len()).ok() != u64::try_from(before.st_size).ok()
        || !same_state(&before, &after)
        || !same_state(&after, &named)
    {
        return Err(invalid("host discovery changed while reading"));
    }
    check_package_name(package, &package_state)?;
    Ok(Some(HostDiscovery {
        package_identity,
        bytes,
    }))
}

pub(crate) fn acquire_lock(package: &Path) -> Result<File, StoreError> {
    let directory = open_package(package)?;
    let package_state = fstat(&directory).map_err(io)?;
    let lock = File::from(
        openat(
            &directory,
            LOCK_NAME,
            OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            PRIVATE_FILE,
        )
        .map_err(io)?,
    );
    let state = fstat(&lock).map_err(io)?;
    // Existing versions created 0644 lock files. They remain compatible, but
    // cannot be writable by anyone other than the current UID.
    validate_lock(&state, &package_state, false)?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => return Err(StoreError::AlreadyOpen),
        Err(std::fs::TryLockError::Error(error)) => return Err(StoreError::Io(error)),
    }
    let validation = (|| {
        let named = statat(&directory, LOCK_NAME, AtFlags::SYMLINK_NOFOLLOW).map_err(io)?;
        if !same_state(&state, &named) {
            return Err(invalid("writer lock was replaced during acquisition"));
        }
        fchmod(&lock, PRIVATE_FILE).map_err(io)?;
        check_package_name(package, &package_state)?;
        let held = fstat(&lock).map_err(io)?;
        let named = statat(&directory, LOCK_NAME, AtFlags::SYMLINK_NOFOLLOW).map_err(io)?;
        validate_lock(&held, &package_state, true)?;
        if !same_state(&held, &named) {
            return Err(invalid("writer lock was replaced during acquisition"));
        }
        Ok(())
    })();
    if let Err(error) = validation {
        let _ = lock.unlock();
        return Err(error);
    }
    Ok(lock)
}

fn open_package(path: &Path) -> Result<OwnedFd, StoreError> {
    let directory = openat(
        CWD,
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(io)?;
    let state = fstat(&directory).map_err(io)?;
    validate_package(&state)?;
    check_package_name(path, &state)?;
    Ok(directory)
}

fn check_package_name(path: &Path, held: &Stat) -> Result<(), StoreError> {
    let named = statat(CWD, path, AtFlags::SYMLINK_NOFOLLOW).map_err(io)?;
    validate_package(&named)?;
    if identity(held)? != identity(&named)? {
        return Err(invalid("package directory was replaced"));
    }
    Ok(())
}

fn open_discovery(
    directory: &OwnedFd,
    package: PackageIdentity,
) -> Result<Option<(File, Stat)>, StoreError> {
    let file = match openat(
        directory,
        DISCOVERY_NAME,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(file) => File::from(file),
        Err(rustix::io::Errno::NOENT) => return Ok(None),
        Err(error) => return Err(io(error)),
    };
    let state = fstat(&file).map_err(io)?;
    validate_discovery(&state, package)?;
    check_discovery_name(directory, Some(&state))?;
    Ok(Some((file, state)))
}

fn check_discovery_name(directory: &OwnedFd, previous: Option<&Stat>) -> Result<(), StoreError> {
    let named = statat(directory, DISCOVERY_NAME, AtFlags::SYMLINK_NOFOLLOW);
    match (previous, named) {
        (None, Err(rustix::io::Errno::NOENT)) => Ok(()),
        (Some(expected), Ok(named)) if same_state(expected, &named) => Ok(()),
        (_, Err(error)) if error != rustix::io::Errno::NOENT => Err(io(error)),
        _ => Err(invalid("host discovery was changed or replaced")),
    }
}

fn validate_package(state: &Stat) -> Result<(), StoreError> {
    if !FileType::from_raw_mode(state.st_mode).is_dir()
        || state.st_uid != rustix::process::geteuid().as_raw()
        || state.st_mode & 0o022 != 0
    {
        return Err(invalid(
            "package must be a current-user directory without shared write access",
        ));
    }
    Ok(())
}

fn validate_lock(state: &Stat, package: &Stat, private: bool) -> Result<(), StoreError> {
    if !FileType::from_raw_mode(state.st_mode).is_file()
        || state.st_uid != rustix::process::geteuid().as_raw()
        || state.st_dev != package.st_dev
        || state.st_nlink != 1
        || state.st_size != 0
        || state.st_mode & 0o022 != 0
        || (private && state.st_mode & 0o7777 != 0o600)
    {
        return Err(invalid(
            "writer lock is not a private regular single-link file",
        ));
    }
    Ok(())
}

fn validate_discovery(state: &Stat, package: PackageIdentity) -> Result<(), StoreError> {
    if !FileType::from_raw_mode(state.st_mode).is_file()
        || state.st_uid != rustix::process::geteuid().as_raw()
        || identity(state)?.device != package.device
        || state.st_nlink != 1
        || state.st_mode & 0o7777 != 0o600
        || usize::try_from(state.st_size)
            .ok()
            .is_none_or(|size| size > MAX_DISCOVERY_BYTES)
    {
        return Err(invalid(
            "host discovery must be a bounded private regular single-link file",
        ));
    }
    Ok(())
}

fn identity(state: &Stat) -> Result<PackageIdentity, StoreError> {
    Ok(PackageIdentity {
        device: u64::try_from(i128::from(state.st_dev))
            .map_err(|_| invalid("package device identity is out of range"))?,
        inode: u64::try_from(i128::from(state.st_ino))
            .map_err(|_| invalid("package inode identity is out of range"))?,
    })
}

fn same_state(before: &Stat, after: &Stat) -> bool {
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

fn invalid(reason: &'static str) -> StoreError {
    StoreError::HostOwner(reason)
}

fn io(error: rustix::io::Errno) -> StoreError {
    StoreError::Io(error.into())
}
