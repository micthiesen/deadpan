//! Partial build state: the completed ranges of a proxy that is not yet
//! published, kept so a cancelled, paused-then-closed or killed build resumes.
//!
//! ```text
//! Proxies/.partial/v1-<BLAKE3>-s<stream>/   # 0700, one per proxy key
//!   segments.bin   # 0600, encoded range movies one after another; its flock
//!                  # is held exclusively by the build using this state
//!   ranges.json    # 0600, the journal of completed ranges, replaced atomically
//! ```
//!
//! The lock is on the ranges file's open file description, which every
//! worker of the build inherits as its output (or input). A worker that
//! outlives a killed host therefore keeps the state locked until it exits,
//! so a new build never writes beside it.
//!
//! Nothing here is ever served: readers only open published entry names at
//! the cache root, and a partial lives under `.partial`. The journal format
//! and its validation belong to the builder; this module only stores bytes,
//! with the same descriptor-relative, no-follow discipline as the rest of the
//! cache.

use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use rustix::fs::{AtFlags, FlockOperation, Mode, OFlags, flock, fstat, fsync, mkdirat, openat};
use rustix::fs::{renameat, unlinkat};
use rustix::io::Errno;

use super::{
    ProxyCache, ProxyCacheError, ProxyKey, TRASH, directory_flags, list, open_regular, remove_tree,
    touch,
};

pub(super) const PARTIAL: &str = ".partial";
const DATA: &str = "segments.bin";
const JOURNAL: &str = "ranges.json";
const JOURNAL_TEMPORARY: &str = "ranges.json.tmp";
/// Bound of a journal: [`deadpan_media::proxy::MAX_PROXY_SEGMENTS`] records
/// of a few hundred bytes each.
pub const MAX_JOURNAL_BYTES: u64 = 1024 * 1024;

/// The locked partial state of one proxy key. Dropping it releases the lock
/// and keeps the state for a later build; [`Self::remove`] deletes it.
pub struct ProxyPartial {
    cache: ProxyCache,
    name: String,
    directory: OwnedFd,
    /// Locked exclusively for this build.
    data: File,
}

impl ProxyPartial {
    /// The private (0600) file holding the encoded ranges, opened for reading
    /// and writing. The worker appends each range at its current end.
    pub fn data(&self) -> &File {
        &self.data
    }

    /// The journal's bytes, or `None` when there is none. Unreadable,
    /// oversized or unsafe journals are `None` too: the builder starts over.
    pub fn journal(&self) -> Option<Vec<u8>> {
        let mut file = open_regular(&self.directory, JOURNAL).ok()?;
        if file.metadata().ok()?.len() > MAX_JOURNAL_BYTES {
            return None;
        }
        let mut bytes = Vec::new();
        (&mut file)
            .take(MAX_JOURNAL_BYTES + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        Some(bytes)
    }

    /// Durably record `journal` in place of the previous one: write a
    /// temporary file, synchronize it, rename it over the journal and
    /// synchronize the directory. A crash leaves either journal, never a
    /// torn one. Call after synchronizing the ranges it names.
    pub fn write_journal(&self, journal: &[u8]) -> Result<(), ProxyCacheError> {
        if journal.len() as u64 > MAX_JOURNAL_BYTES {
            return Err(ProxyCacheError::Damaged(
                "proxy journal is too large".into(),
            ));
        }
        match unlinkat(&self.directory, JOURNAL_TEMPORARY, AtFlags::empty()) {
            Ok(()) | Err(Errno::NOENT) => {}
            Err(error) => return Err(error.into()),
        }
        let mut file = File::from(openat(
            &self.directory,
            JOURNAL_TEMPORARY,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )?);
        file.write_all(journal)?;
        file.sync_all()?;
        drop(file);
        renameat(&self.directory, JOURNAL_TEMPORARY, &self.directory, JOURNAL)?;
        fsync(&self.directory)?;
        Ok(())
    }

    /// Truncate the ranges file to `length`, dropping bytes no journal names
    /// (an interrupted range's tail), and synchronize it.
    pub fn truncate(&self, length: u64) -> Result<(), ProxyCacheError> {
        self.data.set_len(length)?;
        self.data.sync_all()?;
        Ok(())
    }

    /// Delete this partial state: after publication, or when the build
    /// failed for good.
    pub fn remove(self) -> Result<(), ProxyCacheError> {
        let _lock = self.cache.lock(true)?;
        self.cache.check_live()?;
        let partials = self.cache.subdirectory(PARTIAL)?;
        let trash = self.cache.subdirectory(TRASH)?;
        let trashed = uuid::Uuid::new_v4().simple().to_string();
        match renameat(&partials, self.name.as_str(), &trash, trashed.as_str()) {
            Ok(()) => remove_tree(&trash, &trashed),
            Err(Errno::NOENT) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

impl ProxyCache {
    /// Open, creating if needed, `key`'s partial build state and lock it for
    /// this build. Waits while another build (in any process) holds it.
    pub fn partial(
        &self,
        key: &ProxyKey,
        cancelled: &AtomicBool,
    ) -> Result<ProxyPartial, ProxyCacheError> {
        let name = key.directory();
        loop {
            {
                // Under the shared cache lock cleanup cannot move the
                // directory between its creation and our lock. The partial
                // lock is only tried here: waiting for it while holding the
                // cache lock could deadlock with a publishing build.
                let _cache = self.lock(false)?;
                let partials = self.subdirectory(PARTIAL)?;
                match mkdirat(&partials, name.as_str(), Mode::from_raw_mode(0o700)) {
                    Ok(()) | Err(Errno::EXIST) => {}
                    Err(error) => return Err(error.into()),
                }
                let directory = openat(&partials, name.as_str(), directory_flags(), Mode::empty())
                    .map_err(|error| match error {
                        Errno::NOTDIR | Errno::LOOP => {
                            ProxyCacheError::Unsafe(format!("partial {name} is not a directory"))
                        }
                        other => other.into(),
                    })?;
                let stat = fstat(&directory)?;
                if stat.st_uid != rustix::process::geteuid().as_raw() || stat.st_mode & 0o077 != 0 {
                    return Err(ProxyCacheError::Unsafe(format!(
                        "partial {name} is not a private directory of this user"
                    )));
                }
                let data = open_data(&directory)?;
                match flock(&data, FlockOperation::NonBlockingLockExclusive) {
                    Ok(()) => {
                        // Its modification time marks the last use.
                        let _ = touch(&data);
                        return Ok(ProxyPartial {
                            cache: self.clone(),
                            name,
                            directory,
                            data,
                        });
                    }
                    Err(Errno::WOULDBLOCK) => {}
                    Err(error) => return Err(error.into()),
                }
            }
            if cancelled.load(std::sync::atomic::Ordering::Acquire) {
                return Err(deadpan_media::proxy::ProxyError::Cancelled.into());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// Remove `key`'s partial state unless a build holds it. Returns whether
    /// it was removed.
    pub fn remove_partial(&self, key: &ProxyKey) -> Result<bool, ProxyCacheError> {
        let _lock = self.lock(true)?;
        self.check_live()?;
        let partials = self.subdirectory(PARTIAL)?;
        self.remove_partial_locked(&partials, &key.directory())
    }

    /// Bytes of the partial state of `key`, if any; for tests and reports.
    pub fn partial_bytes(&self, key: &ProxyKey) -> Option<u64> {
        let _lock = self.lock(false).ok()?;
        let partials = self.subdirectory(PARTIAL).ok()?;
        partial_use(&partials, &key.directory()).map(|(_, bytes)| bytes)
    }

    /// Under the exclusive cache lock: trash and delete one partial unless
    /// its lock is held.
    pub(super) fn remove_partial_locked(
        &self,
        partials: &OwnedFd,
        name: &str,
    ) -> Result<bool, ProxyCacheError> {
        let directory = match openat(partials, name, directory_flags(), Mode::empty()) {
            Ok(directory) => Some(directory),
            Err(Errno::NOENT) => return Ok(false),
            // Not a directory: remove the name itself, never its target.
            Err(Errno::NOTDIR | Errno::LOOP) => None,
            Err(error) => return Err(error.into()),
        };
        let _held = match directory {
            Some(directory) => match partial_lock(&directory)? {
                Some(lock) => lock,
                None => return Ok(false),
            },
            None => None,
        };
        let trash = self.subdirectory(TRASH)?;
        let trashed = uuid::Uuid::new_v4().simple().to_string();
        match renameat(partials, name, &trash, trashed.as_str()) {
            Ok(()) => {}
            Err(Errno::NOENT) => return Ok(false),
            Err(error) => return Err(error.into()),
        }
        remove_tree(&trash, &trashed)?;
        Ok(true)
    }
}

fn open_data(directory: &OwnedFd) -> Result<File, ProxyCacheError> {
    let file = openat(
        directory,
        DATA,
        OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o600),
    )?;
    let stat = fstat(&file)?;
    if rustix::fs::FileType::from_raw_mode(stat.st_mode) != rustix::fs::FileType::RegularFile
        || stat.st_nlink != 1
        || stat.st_uid != rustix::process::geteuid().as_raw()
        || stat.st_mode & 0o077 != 0
    {
        return Err(ProxyCacheError::Unsafe(
            "partial ranges are not a private single-link regular file".into(),
        ));
    }
    Ok(File::from(file))
}

/// The partial's ranges file under a non-blocking exclusive `flock`: `None`
/// when a build (or a worker it started) holds it, `Some(None)` when there
/// is no regular ranges file.
fn partial_lock(directory: &OwnedFd) -> Result<Option<Option<File>>, ProxyCacheError> {
    let lock = match open_regular(directory, DATA) {
        Ok(lock) => lock,
        Err(_) => return Ok(Some(None)),
    };
    match flock(&lock, FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => Ok(Some(Some(lock))),
        Err(Errno::WOULDBLOCK) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Last use (newest modification of the partial and its files, in Unix
/// seconds) and total bytes of one partial, without following links.
pub(super) fn partial_use(partials: &OwnedFd, name: &str) -> Option<(i64, u64)> {
    let stat = rustix::fs::statat(partials, name, AtFlags::SYMLINK_NOFOLLOW).ok()?;
    let mut used = stat.st_mtime;
    let mut bytes = u64::try_from(stat.st_size).unwrap_or(0);
    if rustix::fs::FileType::from_raw_mode(stat.st_mode) == rustix::fs::FileType::Directory {
        bytes = 0;
        let directory = openat(partials, name, directory_flags(), Mode::empty()).ok()?;
        for child in list(&directory).ok()? {
            if let Ok(child) =
                rustix::fs::statat(&directory, child.as_str(), AtFlags::SYMLINK_NOFOLLOW)
            {
                bytes += u64::try_from(child.st_size).unwrap_or(0);
                used = used.max(child.st_mtime);
            }
        }
    }
    Some((used, bytes))
}

/// Whether a partial's lock is held by a build.
pub(super) fn partial_in_use(partials: &OwnedFd, name: &str) -> bool {
    openat(partials, name, directory_flags(), Mode::empty())
        .ok()
        .and_then(|directory| partial_lock(&directory).ok())
        .is_some_and(|lock| lock.is_none())
}
