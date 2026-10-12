//! Rebuildable, content-addressed canonical stretch output. Callers derive keys
//! from freshly admitted input; metadata here never grants source authority.
//! Every operation is descriptor-relative. Published reads hold a shared flock,
//! mutations use the cache lock, and cleanup skips active readers/writers.
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rustix::fs::{
    AtFlags, Dir, FileType, FlockOperation, Mode, OFlags, fchmod, flock, fstat, fsync, openat,
    renameat, unlinkat,
};
use rustix::io::Errno;
use serde::Serialize;

pub const ENGINE_PIN: &str = env!("DEADPAN_PCM_ENGINE_PIN");
pub const DEFAULT_BUDGET: u64 = 4 * 1024 * 1024 * 1024;
pub const UNUSED_GRACE: Duration = Duration::from_secs(30 * 24 * 3600);
const HEADER: usize = 80;
const MAGIC: &[u8; 8] = b"DEADPCM1";
const MAX_ENTRIES: usize = 4096;
const MAX_FRAMES: u32 = 86_400_000;
const CHUNK: usize = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheKey(pub(crate) [u8; 32]);
impl CacheKey {
    pub fn display(self) -> String {
        format!("blake3:{}", blake3::Hash::from_bytes(self.0))
    }
    fn filename(self) -> String {
        format!("b3-{}.pcm", blake3::Hash::from_bytes(self.0))
    }
}

struct Inner {
    path: PathBuf,
    root: OwnedFd,
    budget: u64,
}
#[derive(Clone)]
pub struct PcmCache {
    inner: Arc<Inner>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CacheEntry {
    pub key: String,
    pub bytes: u64,
    pub unused_seconds: u64,
    /// Listing does not read every payload. A restore verifies every byte.
    pub validity: &'static str,
    pub origin: &'static str,
    pub staging: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct Cleanup {
    pub removed: Vec<String>,
    pub removed_bytes: u64,
    pub kept_in_use: Vec<String>,
}

pub fn default_root() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .map(|home| home.join("Library/Caches/Deadpan/Audio"))
}

fn invalid(reason: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason)
}
fn directory_flags() -> OFlags {
    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC
}
fn regular(file: &impl AsFd) -> io::Result<()> {
    let stat = fstat(file)?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
        || stat.st_uid != rustix::process::geteuid().as_raw()
        || stat.st_nlink != 1
        || stat.st_mode & 0o022 != 0
    {
        return Err(invalid(
            "PCM cache file has an unsafe type, owner, links or permissions",
        ));
    }
    Ok(())
}
fn entry_name(name: &str) -> bool {
    name.len() == 71
        && name.starts_with("b3-")
        && name.ends_with(".pcm")
        && name.as_bytes()[3..67]
            .iter()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(c))
}
fn staging_name(name: &str) -> bool {
    name.strip_prefix(".tmp-")
        .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
}

impl PcmCache {
    /// Worker-only: opening and listing perform filesystem I/O.
    pub fn at(path: &Path) -> io::Result<Self> {
        Self::with_budget(path, DEFAULT_BUDGET)
    }
    pub fn with_budget(path: &Path, budget: u64) -> io::Result<Self> {
        if !path.is_absolute() || budget == 0 || budget > DEFAULT_BUDGET {
            return Err(invalid("invalid PCM cache path or budget"));
        }
        std::fs::create_dir_all(path)?;
        let root = rustix::fs::open(path, directory_flags(), Mode::empty())?;
        let stat = fstat(&root)?;
        if stat.st_uid != rustix::process::geteuid().as_raw() || stat.st_mode & 0o022 != 0 {
            return Err(invalid("PCM cache root is not private to this user"));
        }
        let cache = Self {
            inner: Arc::new(Inner {
                path: path.to_owned(),
                root,
                budget,
            }),
        };
        // Contention is evaluated per operation. A busy cache during admission
        // must not disable reuse for this entire retained playback session.
        Ok(cache)
    }
    pub fn path(&self) -> &Path {
        &self.inner.path
    }
    fn live(&self) -> io::Result<()> {
        let opened = fstat(&self.inner.root)?;
        let current = rustix::fs::lstat(&self.inner.path)?;
        if opened.st_dev != current.st_dev || opened.st_ino != current.st_ino {
            return Err(invalid("PCM cache root was replaced"));
        }
        Ok(())
    }
    fn lock(&self) -> io::Result<OwnedFd> {
        self.live()?;
        let file = openat(
            &self.inner.root,
            ".lock",
            OFlags::RDWR | OFlags::CREATE | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )?;
        regular(&file)?;
        // This optional cache never blocks playback behind another process.
        flock(&file, FlockOperation::NonBlockingLockExclusive)?;
        Ok(file)
    }
    fn open_file(&self, name: &str) -> io::Result<File> {
        let file = File::from(openat(
            &self.inner.root,
            name,
            OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?);
        regular(&file)?;
        Ok(file)
    }
    fn entries_locked(&self) -> io::Result<Vec<CacheEntry>> {
        let mut entries = Vec::new();
        let mut visited = 0;
        for entry in Dir::read_from(&self.inner.root)? {
            let entry = entry?;
            visited += 1;
            if visited > MAX_ENTRIES + 3 {
                return Err(invalid("PCM cache entry limit exceeded"));
            }
            let Ok(name) = entry.file_name().to_str() else {
                continue;
            };
            let staging = staging_name(name);
            if !staging && !entry_name(name) {
                continue;
            }
            let file = self.open_file(name)?;
            let stat = fstat(&file)?;
            let modified = u64::try_from(stat.st_mtime)
                .ok()
                .and_then(|s| UNIX_EPOCH.checked_add(Duration::from_secs(s)));
            let age = modified
                .and_then(|m| SystemTime::now().duration_since(m).ok())
                .unwrap_or_default();
            entries.push(CacheEntry {
                key: name.to_owned(),
                bytes: file.metadata()?.len(),
                unused_seconds: age.as_secs(),
                validity: if staging {
                    "unpublished"
                } else {
                    "payload checked on restore"
                },
                origin: "canonical stereo f32 stretch output",
                staging,
            });
        }
        entries.sort_by(|a, b| {
            b.unused_seconds
                .cmp(&a.unused_seconds)
                .then_with(|| a.key.cmp(&b.key))
        });
        Ok(entries)
    }
    pub fn entries(&self) -> io::Result<Vec<CacheEntry>> {
        let _lock = self.lock()?;
        self.entries_locked()
    }
    fn clean_locked(&self, grace: Duration, dry_run: bool) -> io::Result<Cleanup> {
        let mut result = Cleanup::default();
        for entry in self.entries_locked()? {
            if entry.unused_seconds < grace.as_secs() {
                continue;
            }
            let file = self.open_file(&entry.key)?;
            match flock(&file, FlockOperation::NonBlockingLockExclusive) {
                Ok(()) => {}
                Err(Errno::WOULDBLOCK) => {
                    result.kept_in_use.push(entry.key);
                    continue;
                }
                Err(error) => return Err(error.into()),
            }
            if !dry_run {
                unlinkat(&self.inner.root, &entry.key, AtFlags::empty())?;
            }
            result.removed_bytes = result
                .removed_bytes
                .checked_add(entry.bytes)
                .ok_or_else(|| invalid("PCM cache accounting overflow"))?;
            result.removed.push(entry.key);
        }
        Ok(result)
    }
    pub fn cleanup(&self, grace: Duration, dry_run: bool) -> io::Result<Cleanup> {
        let _lock = self.lock()?;
        let result = self.clean_locked(grace, dry_run)?;
        if !dry_run {
            fsync(&self.inner.root)?;
        }
        Ok(result)
    }

    /// Copy into private caller-owned PCM while hashing the complete payload.
    /// On any error the caller must discard its incomplete private destination.
    pub(crate) fn restore(
        &self,
        key: CacheKey,
        frames: u32,
        mut consume: impl FnMut(&[[f32; 2]]) -> io::Result<()>,
    ) -> io::Result<bool> {
        let lock = self.lock()?;
        let mut file = match self.open_file(&key.filename()) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        flock(&file, FlockOperation::NonBlockingLockShared)?;
        drop(lock);
        let bytes = HEADER as u64 + u64::from(frames) * 8;
        if frames == 0 || frames > MAX_FRAMES || file.metadata()?.len() != bytes {
            return Err(invalid("PCM cache length mismatch"));
        }
        let mut header = [0; HEADER];
        file.read_exact(&mut header)?;
        if &header[..8] != MAGIC
            || header[8..40] != key.0
            || header[40..48] != u64::from(frames).to_le_bytes()
        {
            return Err(invalid("PCM cache header mismatch"));
        }
        let mut hash = blake3::Hasher::new();
        let mut buffer = [0; CHUNK * 8];
        let mut samples = [[0.0; 2]; CHUNK];
        let mut cursor = 0;
        while cursor < frames as usize {
            let count = (frames as usize - cursor).min(CHUNK);
            file.read_exact(&mut buffer[..count * 8])?;
            deadpan_diagnostics::IO
                .pcm_cache_file
                .read((count * 8) as u64);
            hash.update(&buffer[..count * 8]);
            for (sample, bytes) in samples[..count].iter_mut().zip(buffer.chunks_exact(8)) {
                sample[0] = f32::from_le_bytes(bytes[..4].try_into().expect("four bytes"));
                sample[1] = f32::from_le_bytes(bytes[4..].try_into().expect("four bytes"));
                if sample.iter().any(|v| !v.is_finite()) {
                    return Err(invalid("nonfinite cached PCM"));
                }
            }
            consume(&samples[..count])?;
            cursor += count;
        }
        if hash.finalize().as_bytes() != &header[48..80] {
            return Err(invalid("PCM cache checksum mismatch"));
        }
        // Last-use metadata is optional on a read-only volume; bytes are immutable.
        let _ = file.set_modified(SystemTime::now());
        Ok(true)
    }

    pub(crate) fn store(
        &self,
        key: CacheKey,
        frames: u32,
        mut read: impl FnMut(std::ops::Range<usize>) -> io::Result<Vec<f32>>,
    ) -> io::Result<()> {
        if frames == 0 || frames > MAX_FRAMES {
            return Err(invalid("invalid PCM cache extent"));
        }
        let length = HEADER as u64 + u64::from(frames) * 8;
        let lock = self.lock()?;
        self.clean_locked(UNUSED_GRACE, false)?;
        let entries = self.entries_locked()?;
        let used = entries
            .iter()
            .try_fold(0u64, |sum, e| sum.checked_add(e.bytes))
            .ok_or_else(|| invalid("PCM cache accounting overflow"))?;
        if entries.len() >= MAX_ENTRIES
            || used
                .checked_add(length)
                .is_none_or(|n| n > self.inner.budget)
        {
            return Err(io::Error::new(
                io::ErrorKind::StorageFull,
                "saved audio cache is full; keeping private PCM",
            ));
        }
        let name = format!(".tmp-{}", uuid::Uuid::new_v4());
        let mut file = File::from(openat(
            &self.inner.root,
            &name,
            OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )?);
        let staging = Staging {
            cache: self.clone(),
            name,
        };
        flock(&file, FlockOperation::NonBlockingLockExclusive)?;
        // Reserve the complete logical size under the cache lock, including
        // concurrent in-progress output in the shared budget.
        file.set_len(length)?;
        file.seek(SeekFrom::Start(HEADER as u64))?;
        drop(lock);
        let mut hash = blake3::Hasher::new();
        let mut buffer = [0; CHUNK * 8];
        let mut cursor = 0;
        while cursor < frames as usize {
            let end = (cursor + CHUNK).min(frames as usize);
            let samples = read(cursor..end)?;
            if samples.len() != (end - cursor) * 2 || samples.iter().any(|v| !v.is_finite()) {
                return Err(invalid("invalid canonical cache output"));
            }
            for (value, bytes) in samples.iter().zip(buffer.chunks_exact_mut(4)) {
                bytes.copy_from_slice(&value.to_le_bytes());
            }
            let bytes = &buffer[..samples.len() * 4];
            hash.update(bytes);
            file.write_all(bytes)?;
            deadpan_diagnostics::IO
                .pcm_cache_file
                .write(bytes.len() as u64);
            cursor = end;
        }
        let mut header = [0; HEADER];
        header[..8].copy_from_slice(MAGIC);
        header[8..40].copy_from_slice(&key.0);
        header[40..48].copy_from_slice(&u64::from(frames).to_le_bytes());
        header[48..].copy_from_slice(hash.finalize().as_bytes());
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&header)?;
        fchmod(&file, Mode::from_raw_mode(0o400))?;
        file.sync_all()?;
        let _lock = self.lock()?;
        let old = match self.open_file(&key.filename()) {
            Ok(file) => {
                flock(&file, FlockOperation::NonBlockingLockExclusive)?;
                Some(file)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        renameat(
            &self.inner.root,
            &staging.name,
            &self.inner.root,
            key.filename(),
        )?;
        fsync(&self.inner.root)?;
        drop(old);
        Ok(())
    }
}

struct Staging {
    cache: PcmCache,
    name: String,
}
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = unlinkat(&self.cache.inner.root, &self.name, AtFlags::empty());
    }
}

#[cfg(test)]
#[path = "persistent_pcm_tests.rs"]
mod tests;
