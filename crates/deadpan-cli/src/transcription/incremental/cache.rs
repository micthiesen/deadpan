//! Private rebuildable range records. Atomic descriptor-relative publication;
//! checksummed, bounded reads never follow a record symlink or block on a FIFO.

use std::fs::{DirBuilder, File};
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::fs::DirBuilderExt;
use std::path::Path;

use rustix::fs::{
    AtFlags, Dir, FileType, FlockOperation, Mode, OFlags, flock, fstat, fsync, openat, renameat,
    statat, unlinkat,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};

const MAX_BYTES: u64 = 8 * 1024 * 1024;
const MAX_CACHE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_CACHE_RECORDS: usize = 8_192;

pub(crate) struct RangeCache {
    root: OwnedFd,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    sha256: String,
    record: String,
}

fn error(value: impl std::fmt::Display) -> std::io::Error {
    std::io::Error::other(value.to_string())
}

impl RangeCache {
    pub(crate) fn open(path: &Path) -> std::io::Result<Self> {
        if !path.is_absolute() {
            return Err(error("analysis cache requires an absolute path"));
        }
        DirBuilder::new().recursive(true).mode(0o700).create(path)?;
        let root = rustix::fs::open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        let stat = fstat(&root)?;
        if stat.st_uid != rustix::process::geteuid().as_raw() || stat.st_mode & 0o077 != 0 {
            return Err(error("analysis cache is not private to this user"));
        }
        Ok(Self { root })
    }

    pub(crate) fn name(key: &impl Serialize) -> std::io::Result<String> {
        Ok(format!(
            "blake3-{}.json",
            blake3::hash(&serde_json::to_vec(key).map_err(error)?).to_hex()
        ))
    }

    pub(crate) fn read<T: DeserializeOwned>(&self, name: &str) -> std::io::Result<Option<T>> {
        let fd = match openat(
            &self.root,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let stat = fstat(&fd)?;
        if rustix::fs::FileType::from_raw_mode(stat.st_mode) != rustix::fs::FileType::RegularFile
            || stat.st_uid != rustix::process::geteuid().as_raw()
            || stat.st_mode & 0o077 != 0
            || stat.st_size < 0
            || stat.st_size as u64 > MAX_BYTES
        {
            return Err(error("unsafe or excessive analysis cache record"));
        }
        let mut bytes = Vec::new();
        File::from(fd).take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(error("analysis cache grew beyond its bound"));
        }
        let envelope: Envelope = serde_json::from_slice(&bytes).map_err(error)?;
        if crate::transcription::hex(&Sha256::digest(envelope.record.as_bytes())) != envelope.sha256
        {
            return Err(error("analysis cache checksum differs"));
        }
        serde_json::from_str(&envelope.record)
            .map(Some)
            .map_err(error)
    }

    pub(crate) fn write(&self, name: &str, value: &impl Serialize) -> std::io::Result<()> {
        let record = serde_json::to_string(value).map_err(error)?;
        let bytes = serde_json::to_vec(&Envelope {
            sha256: crate::transcription::hex(&Sha256::digest(record.as_bytes())),
            record,
        })
        .map_err(error)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(error("analysis cache record exceeds its bound"));
        }
        // Cache publication never waits behind another process. A busy or
        // full cache loses only resumability; the measured result still works.
        let lock = openat(
            &self.root,
            ".lock",
            OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )?;
        let stat = fstat(&lock)?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
            || stat.st_uid != rustix::process::geteuid().as_raw()
            || stat.st_mode & 0o077 != 0
        {
            return Err(error("analysis cache lock is not a private regular file"));
        }
        flock(&lock, FlockOperation::NonBlockingLockExclusive)?;
        let mut used = 0_u64;
        let mut records = 0_usize;
        for entry in Dir::read_from(&self.root)? {
            let entry = entry?;
            let entry_name = entry.file_name();
            if entry_name.to_bytes() == b"." || entry_name.to_bytes() == b".." {
                continue;
            }
            records += 1;
            if records > MAX_CACHE_RECORDS {
                return Err(error("analysis range cache record limit"));
            }
            let stat = statat(&self.root, entry_name, AtFlags::SYMLINK_NOFOLLOW)?;
            if FileType::from_raw_mode(stat.st_mode) == FileType::RegularFile {
                used = used.saturating_add(u64::try_from(stat.st_size).unwrap_or(u64::MAX));
            }
        }
        if used.saturating_add(bytes.len() as u64) > MAX_CACHE_BYTES {
            return Err(error("analysis range cache byte limit"));
        }
        let temporary = format!(".{}", uuid::Uuid::new_v4().simple());
        let fd = openat(
            &self.root,
            temporary.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )?;
        let result = (|| {
            let mut file = File::from(fd);
            file.write_all(&bytes)?;
            file.sync_all()?;
            renameat(&self.root, temporary.as_str(), &self.root, name)?;
            fsync(&self.root)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = unlinkat(&self.root, temporary.as_str(), AtFlags::empty());
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_address_is_explicit_blake3_of_the_exact_key() {
        let key = ["source", "model", "settings", "version", "owned", "context"];
        let name = RangeCache::name(&key).unwrap();
        let expected =
            blake3::hash(br#"["source","model","settings","version","owned","context"]"#);
        assert_eq!(name, format!("blake3-{}.json", expected.to_hex()));
        assert_eq!(name.len(), "blake3-".len() + 64 + ".json".len());
        assert_eq!(RangeCache::name(&key).unwrap(), name);
        for index in 0..key.len() {
            let mut changed = key;
            changed[index] = "different";
            assert_ne!(RangeCache::name(&changed).unwrap(), name);
        }
    }

    #[test]
    fn range_records_are_private_atomic_and_checksums_refuse_damage() {
        let directory = tempfile::tempdir().unwrap();
        let cache = RangeCache::open(&directory.path().join("ranges")).unwrap();
        let name = RangeCache::name(&["source", "model", "settings", "version"]).unwrap();
        cache.write(&name, &vec![1, 2, 3]).unwrap();
        assert_eq!(cache.read::<Vec<u8>>(&name).unwrap(), Some(vec![1, 2, 3]));
        let other = RangeCache::name(&["source", "new-model", "settings", "version"]).unwrap();
        assert!(cache.read::<Vec<u8>>(&other).unwrap().is_none());
        std::fs::write(
            directory.path().join("ranges").join(&name),
            br#"{"sha256":"bad","record":"[4]"}"#,
        )
        .unwrap();
        assert!(cache.read::<Vec<u8>>(&name).is_err());
    }

    #[test]
    fn cache_refuses_symlinks_and_nonprivate_directories() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("target");
        std::fs::create_dir(&target).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o700)).unwrap();
        let alias = directory.path().join("alias");
        symlink(&target, &alias).unwrap();
        assert!(RangeCache::open(&alias).is_err());
        let cache = RangeCache::open(&target).unwrap();
        symlink("/etc/passwd", target.join("record")).unwrap();
        assert!(cache.read::<Vec<u8>>("record").is_err());
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(RangeCache::open(&target).is_err());
    }

    #[test]
    fn unsafe_and_busy_locks_refuse_without_waiting_and_repaired_lock_writes() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        use std::time::Duration;

        // A regressed FIFO open or blocking flock cannot hang the test
        // process: no peer is created, and the parent has a finite wait.
        let (finished, received) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path().join("ranges");
            let cache = RangeCache::open(&root).unwrap();
            let name = RangeCache::name(&["source", "model"]).unwrap();
            let lock_path = root.join(".lock");
            #[cfg(target_os = "linux")]
            rustix::fs::mkfifoat(rustix::fs::CWD, &lock_path, Mode::from_raw_mode(0o600)).unwrap();
            #[cfg(target_os = "macos")]
            {
                let mut command = std::process::Command::new("mkfifo");
                command.args(["-m", "600"]).arg(&lock_path);
                assert!(
                    deadpan_native_process::spawn(&mut command)
                        .unwrap()
                        .wait()
                        .unwrap()
                        .success()
                );
            }
            assert!(cache.write(&name, &[1_u8]).is_err());
            assert!(cache.read::<Vec<u8>>(&name).unwrap().is_none());
            std::fs::remove_file(&lock_path).unwrap();

            std::fs::write(&lock_path, []).unwrap();
            std::fs::set_permissions(&lock_path, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(cache.write(&name, &[1_u8]).is_err());
            assert!(cache.read::<Vec<u8>>(&name).unwrap().is_none());
            std::fs::remove_file(&lock_path).unwrap();
            symlink("/dev/null", &lock_path).unwrap();
            assert!(cache.write(&name, &[1_u8]).is_err());
            std::fs::remove_file(&lock_path).unwrap();

            let lock = openat(
                &cache.root,
                ".lock",
                OFlags::RDWR
                    | OFlags::CREATE
                    | OFlags::NOFOLLOW
                    | OFlags::NONBLOCK
                    | OFlags::CLOEXEC,
                Mode::from_raw_mode(0o600),
            )
            .unwrap();
            flock(&lock, FlockOperation::NonBlockingLockExclusive).unwrap();
            assert_eq!(
                cache.write(&name, &[1_u8]).unwrap_err().kind(),
                std::io::ErrorKind::WouldBlock
            );
            assert!(cache.read::<Vec<u8>>(&name).unwrap().is_none());
            drop(lock);
            cache.write(&name, &[1_u8]).unwrap();
            assert_eq!(cache.read::<Vec<u8>>(&name).unwrap(), Some(vec![1]));
            finished.send(()).unwrap();
        });
        received
            .recv_timeout(Duration::from_secs(5))
            .expect("cache-lock admission must finish without a FIFO or flock peer");
        worker.join().unwrap();
    }
}
