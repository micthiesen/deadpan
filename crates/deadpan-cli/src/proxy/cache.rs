//! The per-user, rebuildable preview-proxy cache.
//!
//! Specification §20.1 puts global rebuildable data in
//! `~/Library/Caches/Deadpan`; proxies live in its `Proxies` directory, keyed
//! by the recipe version, the Original object's BLAKE3 content address and
//! the stream, so every package and package copy sharing an Original shares
//! one proxy. Nothing is authoritative: no project, revision or history
//! refers to it, and deleting the directory loses only time.
//!
//! ```text
//! Proxies/
//!   .lock                         # flock: shared for reads, exclusive for changes
//!   v1-<BLAKE3>-s<stream>/        # one published entry (0755)
//!     proxy.mp4  proxy.json used  # movie and sidecar 0444; used's mtime = last use
//!     verified.json               # hash verified for one exact file state
//!   .staging/<uuid>/              # builds in progress (0700)
//!   .trash/<uuid>/                # replaced or evicted entries awaiting removal
//!   .failures/v1-<…>.json         # a remembered failed build
//! ```
//!
//! Every operation is relative to the root descriptor opened once, with
//! `O_NOFOLLOW` on every component. Readers hold a shared `flock` on the
//! movie descriptor for as long as they read it; eviction and removal take an
//! exclusive non-blocking `flock` first and skip entries in use, in this or
//! any other process. Publication swaps a completed, synchronized staging
//! directory into place atomically (`RENAME_SWAP`, or `RENAME_EXCL` for a new
//! entry), under the exclusive cache lock, after checking that the root is
//! still the directory it opened.

use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use deadpan_media::proxy::{PROXY_RECIPE_VERSION, ProxySidecar};
use rustix::fs::{
    AtFlags, Dir, FileType, FlockOperation, Mode, OFlags, RenameFlags, Timestamps, fchmod, flock,
    fstat, fstatvfs, fsync, futimens, mkdirat, openat, renameat, renameat_with, statat, unlinkat,
};
use rustix::io::Errno;
use serde::{Deserialize, Serialize};

pub const PROXY_CACHE_DIRECTORY: &str = "Library/Caches/Deadpan/Proxies";
const LOCK: &str = ".lock";
const ENCODER_LOCK: &str = ".encoder.lock";
const STAGING: &str = ".staging";
const TRASH: &str = ".trash";
const FAILURES: &str = ".failures";
const MOVIE: &str = "proxy.mp4";
const SIDECAR: &str = "proxy.json";
const USED: &str = "used";
const VERIFIED: &str = "verified.json";
/// Default bound on all proxies of one user.
pub const DEFAULT_PROXY_BUDGET_BYTES: u64 = 64 * 1024 * 1024 * 1024;
/// Abandoned staging older than this is removed.
pub const DEFAULT_STAGING_GRACE: Duration = Duration::from_secs(24 * 60 * 60);
/// Entries unused for this long are evicted.
pub const DEFAULT_UNUSED_GRACE: Duration = Duration::from_secs(30 * 24 * 60 * 60);
const MAX_SIDECAR_BYTES: u64 =
    deadpan_media::source_index::MAX_SOURCE_INDEX_JSON_BYTES as u64 + 64 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ProxyCacheError {
    #[error("proxy cache path is unsafe: {0}")]
    Unsafe(String),
    #[error("cached proxy is damaged: {0}")]
    Damaged(String),
    #[error("proxy budget exceeded: {used} of {budget} bytes used")]
    Budget { used: u64, budget: u64 },
    #[error("proxy cache I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Proxy(#[from] deadpan_media::proxy::ProxyError),
}

impl From<Errno> for ProxyCacheError {
    fn from(error: Errno) -> Self {
        Self::Io(error.into())
    }
}

/// One Original stream's proxy slot under the current recipe.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProxyKey {
    original_blake3: String,
    stream_index: u32,
}

impl ProxyKey {
    /// `original_blake3` is the retained Original object's content address.
    pub fn new(original_blake3: &str, stream_index: u32) -> Result<Self, ProxyCacheError> {
        if original_blake3.len() != 64
            || !original_blake3
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || stream_index >= 33
        {
            return Err(ProxyCacheError::Unsafe("invalid proxy key".into()));
        }
        Ok(Self {
            original_blake3: original_blake3.to_owned(),
            stream_index,
        })
    }

    pub fn original_blake3(&self) -> &str {
        &self.original_blake3
    }

    pub fn stream_index(&self) -> u32 {
        self.stream_index
    }

    /// The entry directory name, including the recipe version.
    pub fn directory(&self) -> String {
        format!(
            "v{PROXY_RECIPE_VERSION}-{}-s{}",
            self.original_blake3, self.stream_index
        )
    }
}

/// A published entry's sidecar, as read; callers still validate it against
/// the Original with `ProxySidecar::validate_for` before serving pictures.
#[derive(Debug, Clone)]
pub struct ProxyEntry {
    key: ProxyKey,
    sidecar: Arc<ProxySidecar>,
}

impl ProxyEntry {
    pub fn key(&self) -> &ProxyKey {
        &self.key
    }
    pub fn sidecar(&self) -> &Arc<ProxySidecar> {
        &self.sidecar
    }
}

/// An open, verified published movie. The descriptor carries a shared
/// `flock` that keeps eviction away while it (or any duplicate) stays open.
pub struct ProxyRead {
    file: File,
}

impl ProxyRead {
    pub fn into_file(self) -> File {
        self.file
    }
}

/// The exact file state whose bytes were hashed once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileState {
    device: u64,
    inode: u64,
    length: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
}

impl FileState {
    fn of(file: &impl AsFd) -> Result<Self, ProxyCacheError> {
        let stat = fstat(file)?;
        Ok(Self {
            device: stat.st_dev as u64,
            inode: stat.st_ino,
            length: u64::try_from(stat.st_size).unwrap_or(0),
            modified_seconds: stat.st_mtime,
            modified_nanoseconds: stat.st_mtime_nsec,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VerifiedMarker {
    state: FileState,
    sha256: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FailureRecord {
    message: String,
    unix_seconds: u64,
}

/// A private staging directory for one proxy being built. Dropping it
/// without publishing removes it.
pub struct ProxyStaging {
    cache: ProxyCache,
    name: String,
    directory: OwnedFd,
    movie: File,
    published: bool,
}

impl ProxyStaging {
    /// The empty private (0600) movie file the worker writes.
    pub fn movie(&self) -> &File {
        &self.movie
    }
}

impl Drop for ProxyStaging {
    fn drop(&mut self) {
        if !self.published
            && let Ok(staging) = self.cache.subdirectory(STAGING)
        {
            let _ = remove_tree(&staging, &self.name);
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProxyCleanupPolicy {
    pub staging_grace: Duration,
    pub unused_grace: Duration,
    pub budget_bytes: u64,
}

impl Default for ProxyCleanupPolicy {
    fn default() -> Self {
        Self {
            staging_grace: DEFAULT_STAGING_GRACE,
            unused_grace: DEFAULT_UNUSED_GRACE,
            budget_bytes: DEFAULT_PROXY_BUDGET_BYTES,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProxyCleanupReport {
    pub removed_entries: Vec<String>,
    pub removed_staging: usize,
    pub kept_in_use: Vec<String>,
    pub retained_bytes: u64,
}

struct Inner {
    path: PathBuf,
    root: OwnedFd,
}

/// Handle to the per-user proxy cache. Clones share one root descriptor.
#[derive(Clone)]
pub struct ProxyCache {
    inner: Arc<Inner>,
}

/// `~/Library/Caches/Deadpan/Proxies`, or None without a home directory.
pub fn default_root() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())
        .map(|home| home.join(PROXY_CACHE_DIRECTORY))
}

/// Cheap identity of a published entry, or of its absence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProxyStamp(Option<(u64, u64, i64, i64)>);

/// The held per-user proxy encoder slot.
pub struct EncoderSlot(#[allow(dead_code)] OwnedFd);

struct CacheLock(#[allow(dead_code)] OwnedFd);

fn directory_flags() -> OFlags {
    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC
}

impl ProxyCache {
    /// Open (creating if needed) the cache at `path`. The directory must be
    /// owned by this user and not writable by others.
    pub fn at(path: &Path) -> Result<Self, ProxyCacheError> {
        if !path.is_absolute() {
            return Err(ProxyCacheError::Unsafe(
                "cache path must be absolute".into(),
            ));
        }
        std::fs::create_dir_all(path)?;
        let root = rustix::fs::open(path, directory_flags(), Mode::empty())?;
        let stat = fstat(&root)?;
        if stat.st_uid != rustix::process::geteuid().as_raw() || stat.st_mode & 0o022 != 0 {
            return Err(ProxyCacheError::Unsafe(format!(
                "{} is not a private directory of this user",
                path.display()
            )));
        }
        let cache = Self {
            inner: Arc::new(Inner {
                path: path.to_owned(),
                root,
            }),
        };
        for name in [STAGING, TRASH, FAILURES] {
            match mkdirat(&cache.inner.root, name, Mode::from_raw_mode(0o700)) {
                Ok(()) | Err(Errno::EXIST) => {}
                Err(error) => return Err(error.into()),
            }
            cache.subdirectory(name)?;
        }
        Ok(cache)
    }

    pub fn path(&self) -> &Path {
        &self.inner.path
    }

    fn subdirectory(&self, name: &str) -> Result<OwnedFd, ProxyCacheError> {
        let directory = openat(&self.inner.root, name, directory_flags(), Mode::empty())?;
        let stat = fstat(&directory)?;
        if stat.st_uid != rustix::process::geteuid().as_raw() {
            return Err(ProxyCacheError::Unsafe(format!("{name} has another owner")));
        }
        Ok(directory)
    }

    fn lock(&self, exclusive: bool) -> Result<CacheLock, ProxyCacheError> {
        let file = openat(
            &self.inner.root,
            LOCK,
            OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )?;
        flock(
            &file,
            if exclusive {
                FlockOperation::LockExclusive
            } else {
                FlockOperation::LockShared
            },
        )?;
        Ok(CacheLock(file))
    }

    /// Wait for the per-user proxy encoder slot: one VideoToolbox proxy
    /// session at a time across processes. Released when dropped.
    pub fn encoder_slot(&self, cancelled: &AtomicBool) -> Result<EncoderSlot, ProxyCacheError> {
        let file = openat(
            &self.inner.root,
            ENCODER_LOCK,
            OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )?;
        loop {
            match flock(&file, FlockOperation::NonBlockingLockExclusive) {
                Ok(()) => return Ok(EncoderSlot(file)),
                Err(Errno::WOULDBLOCK) => {}
                Err(error) => return Err(error.into()),
            }
            if cancelled.load(std::sync::atomic::Ordering::Acquire) {
                return Err(deadpan_media::proxy::ProxyError::Cancelled.into());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// Under the exclusive lock: the root path still names the directory
    /// this handle opened, so a replaced or moved cache is never written.
    fn check_live(&self) -> Result<(), ProxyCacheError> {
        let opened = fstat(&self.inner.root)?;
        let current = rustix::fs::lstat(&self.inner.path)?;
        if opened.st_dev != current.st_dev || opened.st_ino != current.st_ino {
            return Err(ProxyCacheError::Unsafe(
                "the proxy cache directory was replaced".into(),
            ));
        }
        Ok(())
    }

    /// One `lstat` of `key`'s entry: device, inode and change time of the
    /// entry directory, which publication and removal both replace. Lets a
    /// reader remember a missing or unusable entry until it changes.
    pub fn stamp(&self, key: &ProxyKey) -> ProxyStamp {
        ProxyStamp(
            statat(&self.inner.root, key.directory(), AtFlags::SYMLINK_NOFOLLOW)
                .ok()
                .map(|stat| {
                    (
                        stat.st_dev as u64,
                        stat.st_ino,
                        stat.st_ctime,
                        stat.st_ctime_nsec,
                    )
                }),
        )
    }

    /// The published entry for `key`, if any. A missing entry is `Ok(None)`;
    /// an unreadable or malformed one is `Damaged`, which a builder replaces.
    pub fn lookup(&self, key: &ProxyKey) -> Result<Option<ProxyEntry>, ProxyCacheError> {
        let _lock = self.lock(false)?;
        let entry = match openat(
            &self.inner.root,
            key.directory(),
            directory_flags(),
            Mode::empty(),
        ) {
            Ok(entry) => entry,
            Err(Errno::NOENT) => return Ok(None),
            Err(Errno::NOTDIR | Errno::LOOP) => {
                return Err(ProxyCacheError::Damaged("entry is not a directory".into()));
            }
            Err(error) => return Err(error.into()),
        };
        let damaged = |what: &str, error: &dyn std::fmt::Display| {
            ProxyCacheError::Damaged(format!("{what}: {error}"))
        };
        let mut sidecar =
            open_regular(&entry, SIDECAR).map_err(|error| damaged("sidecar", &error))?;
        if sidecar.metadata()?.len() > MAX_SIDECAR_BYTES {
            return Err(ProxyCacheError::Damaged("sidecar is too large".into()));
        }
        let mut bytes = Vec::new();
        (&mut sidecar)
            .take(MAX_SIDECAR_BYTES + 1)
            .read_to_end(&mut bytes)?;
        let sidecar =
            ProxySidecar::from_json(&bytes).map_err(|error| damaged("sidecar", &error))?;
        if sidecar.original.blake3 != key.original_blake3
            || sidecar.original.stream_index != key.stream_index
        {
            return Err(ProxyCacheError::Damaged(
                "sidecar names another Original".into(),
            ));
        }
        let movie = open_regular(&entry, MOVIE).map_err(|error| damaged("movie", &error))?;
        if movie.metadata()?.len() != sidecar.file.byte_length {
            return Err(ProxyCacheError::Damaged(
                "movie length differs from its sidecar".into(),
            ));
        }
        Ok(Some(ProxyEntry {
            key: key.clone(),
            sidecar: Arc::new(sidecar),
        }))
    }

    /// Open a published movie for reading in place. The bytes are hashed
    /// against the sidecar once per exact file state (device, inode, length,
    /// modification time), recorded in the entry; later opens of the same
    /// state only compare that state. The returned descriptor holds a shared
    /// `flock` that protects it from eviction, and the entry is marked used.
    pub fn open(
        &self,
        entry: &ProxyEntry,
        cancelled: &AtomicBool,
    ) -> Result<ProxyRead, ProxyCacheError> {
        let _lock = self.lock(false)?;
        let directory = openat(
            &self.inner.root,
            entry.key.directory(),
            directory_flags(),
            Mode::empty(),
        )?;
        let file = open_regular(&directory, MOVIE)?;
        flock(&file, FlockOperation::LockShared)?;
        let state = FileState::of(&file)?;
        if state.length != entry.sidecar.file.byte_length {
            return Err(ProxyCacheError::Damaged(
                "movie changed after lookup".into(),
            ));
        }
        let verified = read_small(&directory, VERIFIED)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<VerifiedMarker>(&bytes).ok())
            .is_some_and(|marker| {
                marker.state == state && marker.sha256 == entry.sidecar.file.sha256
            });
        if !verified {
            let sha256 = deadpan_media::proxy::sha256_file(&file, cancelled)?;
            if sha256 != entry.sidecar.file.sha256 {
                return Err(ProxyCacheError::Damaged(
                    "movie bytes differ from its sidecar".into(),
                ));
            }
            let marker = serde_json::to_vec(&VerifiedMarker { state, sha256 })
                .map_err(|error| ProxyCacheError::Damaged(error.to_string()))?;
            // Concurrent identical markers are harmless; rename is atomic.
            self.replace_file(&directory, VERIFIED, &marker, Mode::from_raw_mode(0o444))?;
        }
        if let Ok(used) = openat(
            &directory,
            USED,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            let _ = touch(&used);
        }
        Ok(ProxyRead { file })
    }

    /// Write `bytes` to a private staging file and rename it over `name` in
    /// `directory`.
    fn replace_file(
        &self,
        directory: &OwnedFd,
        name: &str,
        bytes: &[u8],
        mode: Mode,
    ) -> Result<(), ProxyCacheError> {
        let staging = self.subdirectory(STAGING)?;
        let temporary = format!("{}.tmp", uuid::Uuid::new_v4().simple());
        let mut file = File::from(openat(
            &staging,
            temporary.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )?);
        let result = (|| -> Result<(), ProxyCacheError> {
            file.write_all(bytes)?;
            file.sync_all()?;
            fchmod(&file, mode)?;
            renameat(&staging, temporary.as_str(), directory, name)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = unlinkat(&staging, temporary.as_str(), AtFlags::empty());
        }
        result
    }

    /// Create a private staging directory with an empty 0600 movie file.
    pub fn stage(&self) -> Result<ProxyStaging, ProxyCacheError> {
        let staging = self.subdirectory(STAGING)?;
        let name = uuid::Uuid::new_v4().simple().to_string();
        mkdirat(&staging, name.as_str(), Mode::from_raw_mode(0o700))?;
        let directory = openat(&staging, name.as_str(), directory_flags(), Mode::empty())?;
        let movie = match openat(
            &directory,
            MOVIE,
            OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        ) {
            Ok(movie) => File::from(movie),
            Err(error) => {
                let _ = remove_tree(&staging, &name);
                return Err(error.into());
            }
        };
        Ok(ProxyStaging {
            cache: self.clone(),
            name,
            directory,
            movie,
            published: false,
        })
    }

    /// Publish a verified proxy atomically, replacing any earlier entry for
    /// the same key. `sidecar` must come from verifying exactly the staged
    /// movie; its length is checked again here.
    pub fn publish(
        &self,
        key: &ProxyKey,
        mut staging: ProxyStaging,
        sidecar: &ProxySidecar,
    ) -> Result<ProxyEntry, ProxyCacheError> {
        if sidecar.original.blake3 != key.original_blake3
            || sidecar.original.stream_index != key.stream_index
            || sidecar.recipe != PROXY_RECIPE_VERSION
        {
            return Err(ProxyCacheError::Damaged(
                "sidecar does not belong to this key".into(),
            ));
        }
        if staging.movie.metadata()?.len() != sidecar.file.byte_length {
            return Err(ProxyCacheError::Damaged(
                "staged movie length differs from its sidecar".into(),
            ));
        }
        let encoded = sidecar.to_json()?;
        staging.movie.sync_all()?;
        fchmod(&staging.movie, Mode::from_raw_mode(0o444))?;
        for (name, bytes) in [(SIDECAR, encoded.as_slice()), (USED, &[][..])] {
            let mut file = File::from(openat(
                &staging.directory,
                name,
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_raw_mode(0o600),
            )?);
            file.write_all(bytes)?;
            file.sync_all()?;
            fchmod(&file, Mode::from_raw_mode(0o444))?;
        }
        fchmod(&staging.directory, Mode::from_raw_mode(0o755))?;
        fsync(&staging.directory)?;
        let _lock = self.lock(true)?;
        self.check_live()?;
        let staging_directory = self.subdirectory(STAGING)?;
        let target = key.directory();
        match renameat_with(
            &staging_directory,
            staging.name.as_str(),
            &self.inner.root,
            target.as_str(),
            RenameFlags::NOREPLACE,
        ) {
            Ok(()) => {}
            Err(Errno::EXIST) => {
                // Swap the complete new entry in; the old one, now under the
                // staging name, goes to the trash. A reader of the old movie
                // keeps its descriptor and lock until it finishes.
                renameat_with(
                    &staging_directory,
                    staging.name.as_str(),
                    &self.inner.root,
                    target.as_str(),
                    RenameFlags::EXCHANGE,
                )?;
                let trash = self.subdirectory(TRASH)?;
                renameat(
                    &staging_directory,
                    staging.name.as_str(),
                    &trash,
                    staging.name.as_str(),
                )?;
                self.empty_trash(&trash);
            }
            Err(error) => return Err(error.into()),
        }
        staging.published = true;
        fsync(&self.inner.root)?;
        let _ = unlinkat(
            &self.subdirectory(FAILURES)?,
            format!("{target}.json").as_str(),
            AtFlags::empty(),
        );
        Ok(ProxyEntry {
            key: key.clone(),
            sidecar: Arc::new(sidecar.clone()),
        })
    }

    /// Remove `key`'s entry unless it is being read. Returns whether an
    /// entry was removed.
    pub fn remove(&self, key: &ProxyKey) -> Result<bool, ProxyCacheError> {
        let _lock = self.lock(true)?;
        self.check_live()?;
        self.evict(&key.directory())
    }

    /// Move one entry to the trash and delete it, unless a reader holds its
    /// movie. Call under the exclusive lock.
    fn evict(&self, name: &str) -> Result<bool, ProxyCacheError> {
        let Some(_exclusive) = self.exclusive_movie(name)? else {
            return Ok(false);
        };
        let trash = self.subdirectory(TRASH)?;
        let trashed = uuid::Uuid::new_v4().simple().to_string();
        match renameat(&self.inner.root, name, &trash, trashed.as_str()) {
            Ok(()) => {}
            Err(Errno::NOENT) => return Ok(false),
            Err(error) => return Err(error.into()),
        }
        remove_tree(&trash, &trashed)?;
        Ok(true)
    }

    /// An exclusive non-blocking lock on `name`'s movie: `None` when a reader
    /// holds it; `Some(None)` when the entry has no readable movie.
    fn exclusive_movie(&self, name: &str) -> Result<Option<Option<File>>, ProxyCacheError> {
        let directory = match openat(&self.inner.root, name, directory_flags(), Mode::empty()) {
            Ok(directory) => directory,
            Err(Errno::NOENT) => return Ok(Some(None)),
            Err(Errno::NOTDIR | Errno::LOOP) => return Ok(Some(None)),
            Err(error) => return Err(error.into()),
        };
        movie_lock(&directory)
    }

    fn empty_trash(&self, trash: &OwnedFd) {
        let Ok(names) = list(trash) else {
            return;
        };
        for name in names {
            let in_use = openat(trash, name.as_str(), directory_flags(), Mode::empty())
                .ok()
                .and_then(|directory| movie_lock(&directory).ok())
                .is_some_and(|lock| lock.is_none());
            if !in_use {
                let _ = remove_tree(trash, &name);
            }
        }
    }

    /// Remember that building `key` failed, so reopening a project does not
    /// repeat a doomed build. Publishing or [`Self::forget_failure`] clears it.
    pub fn record_failure(&self, key: &ProxyKey, message: &str) -> Result<(), ProxyCacheError> {
        let record = FailureRecord {
            message: message.chars().take(1024).collect(),
            unix_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |since| since.as_secs()),
        };
        let bytes = serde_json::to_vec(&record)
            .map_err(|error| ProxyCacheError::Damaged(error.to_string()))?;
        let _lock = self.lock(true)?;
        self.check_live()?;
        self.replace_file(
            &self.subdirectory(FAILURES)?,
            &format!("{}.json", key.directory()),
            &bytes,
            Mode::from_raw_mode(0o600),
        )
    }

    pub fn failure(&self, key: &ProxyKey) -> Option<String> {
        let failures = self.subdirectory(FAILURES).ok()?;
        let bytes = read_small(&failures, &format!("{}.json", key.directory())).ok()?;
        serde_json::from_slice::<FailureRecord>(&bytes)
            .ok()
            .map(|record| record.message)
    }

    pub fn forget_failure(&self, key: &ProxyKey) -> Result<(), ProxyCacheError> {
        let _lock = self.lock(true)?;
        match unlinkat(
            &self.subdirectory(FAILURES)?,
            format!("{}.json", key.directory()).as_str(),
            AtFlags::empty(),
        ) {
            Ok(()) | Err(Errno::NOENT) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    /// Bytes of every published entry.
    pub fn usage(&self) -> Result<u64, ProxyCacheError> {
        let _lock = self.lock(false)?;
        Ok(self.entries()?.iter().map(|entry| entry.bytes).sum())
    }

    /// Bytes available to this user on the cache's volume.
    pub fn available_bytes(&self) -> Result<u64, ProxyCacheError> {
        available(&self.inner.root)
    }

    fn entries(&self) -> Result<Vec<EntryUse>, ProxyCacheError> {
        let mut entries = Vec::new();
        for name in list(&self.inner.root)? {
            if name.starts_with('.') {
                continue;
            }
            let stat = statat(&self.inner.root, name.as_str(), AtFlags::SYMLINK_NOFOLLOW)?;
            let mut used = stat.st_mtime;
            let mut bytes = 0;
            if FileType::from_raw_mode(stat.st_mode) == FileType::Directory {
                let directory = openat(
                    &self.inner.root,
                    name.as_str(),
                    directory_flags(),
                    Mode::empty(),
                )?;
                for child in list(&directory)? {
                    if let Ok(child_stat) =
                        statat(&directory, child.as_str(), AtFlags::SYMLINK_NOFOLLOW)
                    {
                        bytes += u64::try_from(child_stat.st_size).unwrap_or(0);
                        if child == USED {
                            used = child_stat.st_mtime;
                        }
                    }
                }
            } else {
                bytes = u64::try_from(stat.st_size).unwrap_or(0);
            }
            entries.push(EntryUse { name, used, bytes });
        }
        Ok(entries)
    }

    /// Remove abandoned staging and trash, entries of other recipe versions,
    /// entries unused for longer than the unused grace period, then the
    /// least recently used entries while the cache exceeds its budget.
    /// Entries being read and those in `retain` always stay.
    pub fn cleanup(
        &self,
        retain: &[ProxyKey],
        policy: ProxyCleanupPolicy,
    ) -> Result<ProxyCleanupReport, ProxyCacheError> {
        let _lock = self.lock(true)?;
        self.check_live()?;
        let mut report = ProxyCleanupReport::default();
        let now = unix_now();
        let older = |seconds: i64, grace: Duration| {
            now.saturating_sub(seconds) >= i64::try_from(grace.as_secs()).unwrap_or(i64::MAX)
        };
        let staging = self.subdirectory(STAGING)?;
        for name in list(&staging)? {
            if let Ok(stat) = statat(&staging, name.as_str(), AtFlags::SYMLINK_NOFOLLOW)
                && older(stat.st_mtime, policy.staging_grace)
            {
                remove_tree(&staging, &name)?;
                report.removed_staging += 1;
            }
        }
        self.empty_trash(&self.subdirectory(TRASH)?);
        let current = format!("v{PROXY_RECIPE_VERSION}-");
        let retained: Vec<String> = retain.iter().map(ProxyKey::directory).collect();
        let mut entries = self.entries()?;
        let mut total: u64 = entries.iter().map(|entry| entry.bytes).sum();
        // Least recently used first.
        entries.sort_by(|a, b| (a.used, &a.name).cmp(&(b.used, &b.name)));
        for entry in entries {
            if retained.contains(&entry.name) {
                continue;
            }
            let stale = !entry.name.starts_with(&current) || older(entry.used, policy.unused_grace);
            if !(stale || total > policy.budget_bytes) {
                continue;
            }
            if self.evict(&entry.name)? {
                total -= entry.bytes;
                report.removed_entries.push(entry.name);
            } else {
                report.kept_in_use.push(entry.name);
            }
        }
        report.retained_bytes = total;
        Ok(report)
    }
}

struct EntryUse {
    name: String,
    used: i64,
    bytes: u64,
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
        })
}

fn touch(file: &OwnedFd) -> Result<(), ProxyCacheError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let time = rustix::fs::Timespec {
        tv_sec: i64::try_from(now.as_secs()).unwrap_or(i64::MAX),
        tv_nsec: i64::from(now.subsec_nanos()),
    };
    futimens(
        file,
        &Timestamps {
            last_access: time,
            last_modification: time,
        },
    )?;
    Ok(())
}

/// Bytes available to this user on `directory`'s volume.
pub fn available(directory: &impl AsFd) -> Result<u64, ProxyCacheError> {
    let stat = fstatvfs(directory)?;
    Ok(stat.f_bavail.saturating_mul(stat.f_frsize))
}

fn open_regular(directory: &OwnedFd, name: &str) -> Result<File, ProxyCacheError> {
    let file = openat(
        directory,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let stat = fstat(&file)?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile || stat.st_nlink != 1 {
        return Err(ProxyCacheError::Unsafe(format!(
            "{name} is not a single-link regular file"
        )));
    }
    Ok(File::from(file))
}

fn read_small(directory: &OwnedFd, name: &str) -> Result<Vec<u8>, ProxyCacheError> {
    let mut file = open_regular(directory, name)?;
    let mut bytes = Vec::new();
    (&mut file).take(64 * 1024).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// The entry's movie under an exclusive non-blocking `flock`: `None` when a
/// reader holds it, `Some(None)` when there is no movie to protect.
fn movie_lock(directory: &OwnedFd) -> Result<Option<Option<File>>, ProxyCacheError> {
    let movie = match open_regular(directory, MOVIE) {
        Ok(movie) => movie,
        Err(_) => return Ok(Some(None)),
    };
    match flock(&movie, FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => Ok(Some(Some(movie))),
        Err(Errno::WOULDBLOCK) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn list(directory: &OwnedFd) -> Result<Vec<String>, ProxyCacheError> {
    let mut names = Vec::new();
    for entry in Dir::read_from(directory)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name != "." && name != ".." {
            names.push(name);
        }
    }
    Ok(names)
}

/// Remove `name` (a file, or a directory tree without symbolic-link
/// traversal) relative to `parent`. Unlinking needs only the parent's write
/// permission, so read-only published files need no permission changes.
fn remove_tree(parent: &OwnedFd, name: &str) -> Result<(), ProxyCacheError> {
    let stat = match statat(parent, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) => stat,
        Err(Errno::NOENT) => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if FileType::from_raw_mode(stat.st_mode) == FileType::Directory {
        let directory = openat(parent, name, directory_flags(), Mode::empty())?;
        if stat.st_mode & 0o200 == 0 {
            fchmod(&directory, Mode::from_raw_mode(0o700))?;
        }
        for child in list(&directory)? {
            remove_tree(&directory, &child)?;
        }
        match unlinkat(parent, name, AtFlags::REMOVEDIR) {
            Ok(()) | Err(Errno::NOENT) => Ok(()),
            Err(error) => Err(error.into()),
        }
    } else {
        match unlinkat(parent, name, AtFlags::empty()) {
            Ok(()) | Err(Errno::NOENT) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
