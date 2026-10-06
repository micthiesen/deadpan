//! Seek proxy builds on a real full volume.
//!
//! The proxy cache lives on a private attached 16 MB APFS image, so every
//! refusal comes from the kernel: the isolated media worker's own movie
//! writes, the cache's sidecar writes at publication and its verified-marker
//! write when reading. A full volume must be reported as disk-full (an
//! environmental condition, never remembered as this Original's failure),
//! leave no visible or staged proxy, and the same build must publish once
//! space returns. Skips with a printed reason when `hdiutil` or the built
//! `deadpan-media-worker` is unavailable.
#![cfg(target_os = "macos")]

use std::error::Error;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_cli::proxy::cache::{ProxyCache, ProxyCacheError, ProxyKey, ProxyStaging};
use deadpan_cli::proxy::{ProxyBuildError, stage_output};
use deadpan_core::AssetId;
use deadpan_media::proxy::{
    ProxyPlan, ProxyReason, ProxySidecar, VerifyControl, hex, proxy_raster, proxy_request,
    verify_proxy,
};
use deadpan_media::source_index::{SourceContentIdentity, SourceIndexSnapshot};
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use deadpan_media::{ConversionError, ProxyEncodeOptions, encode_proxy_retrying};
use deadpan_source::SourceStreamInfo;
use sha2::{Digest, Sha256};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

/// A private attached APFS image, detached on drop.
struct DiskImage {
    mount: PathBuf,
    _scratch: tempfile::TempDir,
}

impl DiskImage {
    /// `None` when this machine cannot attach disk images.
    fn available(megabytes: u32) -> Result<Option<Self>> {
        match ProcessCommand::new("hdiutil").arg("help").output() {
            Ok(_) => Self::new(megabytes).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                eprintln!("skipping: hdiutil is unavailable ({error})");
                Ok(None)
            }
            Err(error) => Err(error.into()),
        }
    }

    fn new(megabytes: u32) -> Result<Self> {
        let scratch = tempfile::tempdir()?;
        let image = scratch.path().join("volume.dmg");
        let created = ProcessCommand::new("hdiutil")
            .args(["create", "-quiet", "-size"])
            .arg(format!("{megabytes}m"))
            .args(["-fs", "APFS", "-layout", "NONE", "-volname", "deadpan-test"])
            .arg(&image)
            .output()?;
        if !created.status.success() {
            return Err(format!(
                "hdiutil create failed: {}",
                String::from_utf8_lossy(&created.stderr)
            )
            .into());
        }
        let mount = scratch.path().join("mount");
        fs::create_dir(&mount)?;
        let attached = ProcessCommand::new("hdiutil")
            .args([
                "attach",
                "-quiet",
                "-nobrowse",
                "-noautoopen",
                "-mountpoint",
            ])
            .arg(&mount)
            .arg(&image)
            .output()?;
        if !attached.status.success() {
            return Err(format!(
                "hdiutil attach failed: {}",
                String::from_utf8_lossy(&attached.stderr)
            )
            .into());
        }
        Ok(Self {
            mount: mount.canonicalize()?,
            _scratch: scratch,
        })
    }

    /// Writes a filler until the volume refuses another block, then proves
    /// the volume is full: a new 4 KiB file cannot be written.
    fn fill(&self) -> Result<PathBuf> {
        // APFS can keep releasing space shortly after a refused write.
        for round in 0..64 {
            if round > 0 {
                std::thread::sleep(Duration::from_millis(50));
            }
            let filler = self.fill_with(&[1 << 20, 64 << 10, 4 << 10, 512], u64::MAX)?;
            let probe = self.mount.join("probe");
            let refused = File::create(&probe)
                .and_then(|mut file| file.write_all(&[0; 4096]).and_then(|()| file.sync_all()));
            let _ = fs::remove_file(&probe);
            if matches!(&refused, Err(error) if error.kind() == std::io::ErrorKind::StorageFull) {
                return Ok(filler);
            }
        }
        Err("the volume never stayed full".into())
    }

    /// Fills the volume, then frees `reserve` bytes held aside beforehand.
    /// (`statvfs` overstates what APFS will still accept, so free space is
    /// not measured; the reserve file is.)
    fn fill_leaving(&self, reserve: usize) -> Result<PathBuf> {
        let held = self.mount.join("reserve");
        let mut file = File::create(&held)?;
        file.write_all(&vec![0xa5_u8; reserve])?;
        file.sync_all()?;
        drop(file);
        let filler = self.fill()?;
        fs::remove_file(&held)?;
        Ok(filler)
    }

    fn free_bytes(&self) -> Result<u64> {
        let stat = rustix::fs::statvfs(&self.mount)?;
        Ok(stat.f_bavail * stat.f_frsize)
    }

    fn fill_with(&self, chunks: &[usize], limit: u64) -> Result<PathBuf> {
        let path = self.mount.join("filler");
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        let mut written = fs::metadata(&path)?.len();
        for &chunk in chunks {
            let bytes = vec![0x5a_u8; chunk];
            while written + chunk as u64 <= limit {
                match file.write_all(&bytes).and_then(|()| file.sync_data()) {
                    Ok(()) => written += chunk as u64,
                    Err(error) if error.kind() == std::io::ErrorKind::StorageFull => break,
                    Err(error) => return Err(error.into()),
                }
            }
        }
        Ok(path)
    }
}

impl Drop for DiskImage {
    fn drop(&mut self) {
        let _ = ProcessCommand::new("hdiutil")
            .args(["detach", "-quiet", "-force"])
            .arg(&self.mount)
            .output();
    }
}

/// The isolated media worker of this checkout's debug build.
fn media_worker() -> Option<PathBuf> {
    let worker = std::env::var_os("DEADPAN_MEDIA_WORKER")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/deadpan-media-worker")
        });
    worker.is_file().then_some(worker)
}

/// One VideoToolbox proxy session at a time across test processes, shared
/// with the media worker's real-media proxy suite.
fn vt_slot() -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(std::env::temp_dir().join("deadpan-proxy-tests-videotoolbox.lock"))?;
    rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive)?;
    Ok(file)
}

struct Original {
    input: VerifiedSourceInput,
    index: Arc<SourceIndexSnapshot>,
    info: SourceStreamInfo,
    /// Stands in for the Original object's BLAKE3: the cache only needs a
    /// stable 64-digit identity shared by the key and the sidecar.
    identity: String,
}

fn original() -> Result<Original> {
    let bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4"),
    )?;
    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    let content = SourceContentIdentity::new(digest, bytes.len() as u64)?;
    let cancelled = AtomicBool::new(false);
    let input = VerifiedSourceInput::copy_verified(
        &mut bytes.as_slice(),
        content,
        1 << 30,
        Duration::from_secs(60),
        &cancelled,
    )?;
    let session = SourceSession::open_input(
        input.clone(),
        AssetId::new("original")?,
        SourceSessionLimits::default(),
        &cancelled,
    )?;
    Ok(Original {
        input,
        index: Arc::new(session.index().clone()),
        info: session.info().clone(),
        identity: hex(&digest),
    })
}

fn plan(original: &Original) -> ProxyPlan {
    let (width, height) = proxy_raster(original.info.width, original.info.height);
    ProxyPlan {
        width,
        height,
        reason: ProxyReason::Requested,
    }
}

/// The production attempt: a fresh cache staging movie written by the
/// isolated worker.
fn encode(
    cache: &ProxyCache,
    worker: &Path,
    original: &Original,
) -> std::result::Result<ProxyStaging, ConversionError> {
    let request = proxy_request(
        original.input.identity().byte_length(),
        &original.info,
        original.index.index().frames().len() as u64,
        &plan(original),
        Duration::from_secs(120),
    )
    .expect("SDR proxy request");
    let (staging, _) = encode_proxy_retrying(
        worker,
        &original.input,
        &request,
        || stage_output(cache),
        &AtomicBool::new(false),
        ProxyEncodeOptions {
            stall: Duration::from_secs(20),
            pause: None,
        },
    )?;
    Ok(staging)
}

fn verify(staging: &ProxyStaging, original: &Original) -> Result<ProxySidecar> {
    Ok(verify_proxy(
        staging.movie(),
        &original.input,
        &original.identity,
        Arc::clone(&original.index),
        &original.info,
        ProxyReason::Requested,
        VerifyControl {
            timeout: Duration::from_secs(120),
            cancelled: &AtomicBool::new(false),
            pause: None,
        },
    )?)
}

/// Published entries and staged attempts; dot files are the cache's own.
fn visible(root: &Path) -> Result<(Vec<String>, Vec<String>)> {
    let names = |directory: &Path| -> Result<Vec<String>> {
        let mut names = Vec::new();
        if directory.exists() {
            for entry in fs::read_dir(directory)? {
                names.push(entry?.file_name().to_string_lossy().into_owned());
            }
        }
        names.sort();
        Ok(names)
    };
    let published = names(root)?
        .into_iter()
        .filter(|name| !name.starts_with('.'))
        .collect();
    Ok((published, names(&root.join(".staging"))?))
}

#[test]
fn a_full_cache_volume_fails_proxy_builds_truthfully_and_recovers() -> Result {
    let Some(worker) = media_worker() else {
        eprintln!(
            "skipping: needs a built deadpan-media-worker (cargo build -p deadpan-media-worker)"
        );
        return Ok(());
    };
    let Some(image) = DiskImage::available(16)? else {
        return Ok(());
    };
    let _slot = vt_slot()?;
    let original = original()?;
    let root = image.mount.join("Proxies");
    let cache = ProxyCache::at(&root)?;
    let key = ProxyKey::new(&original.identity, original.index.stream_index())?;
    let nothing = (Vec::new(), Vec::new());

    // 1. Too little room for the worker's movie (about 67 KB; 48 KB stay free): the worker's
    // own write is refused by the kernel.
    let filler = image.fill_leaving(48 << 10)?;
    let error = ProxyBuildError::from(match encode(&cache, &worker, &original) {
        Err(error) => error,
        Ok(staging) => {
            return Err(format!(
                "the worker encoded {} bytes; {} free",
                staging.movie().metadata()?.len(),
                image.free_bytes()?
            )
            .into());
        }
    });
    assert!(
        matches!(
            &error,
            ProxyBuildError::Conversion(ConversionError::Worker { .. })
        ),
        "the worker's own write is refused: {error}"
    );
    assert!(error.is_disk_full(), "{error}");
    assert!(error.is_environmental(), "never remembered: {error}");
    assert!(cache.lookup(&key)?.is_none());
    assert_eq!(visible(&root)?, nothing, "failed attempts are removed");
    // Remembering a failure on the full volume fails truthfully too, which
    // the builder no longer lets replace the build's own error.
    if let Err(record) = cache.record_failure(&key, "probe") {
        assert!(ProxyBuildError::from(record).is_disk_full());
    }
    cache.forget_failure(&key)?;
    fs::remove_file(&filler)?;

    // 2. The movie fits but the volume fills before publication: writing
    // the sidecar is refused and nothing becomes visible.
    let staging = encode(&cache, &worker, &original)?;
    let sidecar = verify(&staging, &original)?;
    let filler = image.fill()?;
    let error = cache
        .publish(&key, staging, &sidecar)
        .err()
        .ok_or("publication on a full volume")?;
    assert!(matches!(error, ProxyCacheError::Io(_)), "{error}");
    let error = ProxyBuildError::from(error);
    assert!(error.is_disk_full() && error.is_environmental(), "{error}");
    assert!(cache.lookup(&key)?.is_none());
    assert_eq!(visible(&root)?, nothing, "the staged entry is removed");
    fs::remove_file(&filler)?;

    // 3. Once space returns, the same build publishes.
    let staging = encode(&cache, &worker, &original)?;
    let sidecar = verify(&staging, &original)?;
    let entry = cache.publish(&key, staging, &sidecar)?;
    assert_eq!(visible(&root)?, (vec![key.directory()], Vec::new()));

    // 4. A verified proxy stays readable on a full volume: its verified
    // marker is only a cache of the hash, so failing to write it is no
    // reason to refuse the picture.
    let filler = image.fill()?;
    let cancelled = AtomicBool::new(false);
    let read = cache.open(&entry, &cancelled)?;
    assert_eq!(read.into_file().metadata()?.len(), sidecar.file.byte_length);
    fs::remove_file(&filler)?;
    cache.open(&entry, &cancelled)?;
    Ok(())
}
