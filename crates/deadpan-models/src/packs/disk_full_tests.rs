//! Model pack downloads on a real full volume.
//!
//! The pack store lives on a private attached APFS image. The downloader's
//! own free-space preflight passes against the real volume; another writer
//! then fills it while the body is streaming, so the kernel refuses one of
//! the downloader's writes. Nothing is injected: the body bytes come from
//! memory (offline, like the other pack tests) and every write is real.
//! A refused download must report the full volume, never activate or leave a
//! finished file, keep its verified-so-far partial bytes, and resume to a
//! complete install once space returns. Skips with a printed reason when
//! `hdiutil` is unavailable.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use sha2::Digest;

use super::*;

/// A private attached APFS image, detached on drop.
struct DiskImage {
    mount: PathBuf,
    _scratch: tempfile::TempDir,
}

impl DiskImage {
    /// `None` when this machine cannot attach disk images.
    fn available(megabytes: u32) -> Option<Self> {
        match ProcessCommand::new("hdiutil").arg("help").output() {
            Ok(_) => Some(Self::new(megabytes)),
            Err(error) => {
                eprintln!("skipping: hdiutil is unavailable ({error})");
                None
            }
        }
    }

    fn new(megabytes: u32) -> Self {
        let scratch = tempfile::tempdir().unwrap();
        let image = scratch.path().join("volume.dmg");
        let created = ProcessCommand::new("hdiutil")
            .args(["create", "-quiet", "-size"])
            .arg(format!("{megabytes}m"))
            .args(["-fs", "APFS", "-layout", "NONE", "-volname", "deadpan-test"])
            .arg(&image)
            .output()
            .unwrap();
        assert!(
            created.status.success(),
            "hdiutil create failed: {}",
            String::from_utf8_lossy(&created.stderr)
        );
        let mount = scratch.path().join("mount");
        fs::create_dir(&mount).unwrap();
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
            .output()
            .unwrap();
        assert!(
            attached.status.success(),
            "hdiutil attach failed: {}",
            String::from_utf8_lossy(&attached.stderr)
        );
        Self {
            mount: mount.canonicalize().unwrap(),
            _scratch: scratch,
        }
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

/// Writes filler files under `mount/fill` until the volume refuses another
/// block, then proves it is full: a new 4 KiB file cannot be written. Each
/// round starts a new file, because APFS can refuse to extend one file while
/// still accepting new ones.
fn fill(mount: &Path) -> io::Result<PathBuf> {
    let directory = mount.join("fill");
    fs::create_dir_all(&directory)?;
    for round in 0..200 {
        if round > 0 {
            std::thread::sleep(Duration::from_millis(50));
        }
        let mut file = match File::create(directory.join(round.to_string())) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::StorageFull => continue,
            Err(error) => return Err(error),
        };
        for chunk in [8 << 20, 1 << 20, 64 << 10, 4 << 10, 512] {
            let bytes = vec![0x5a_u8; chunk];
            loop {
                match file.write_all(&bytes).and_then(|()| file.sync_data()) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::StorageFull => break,
                    Err(error) => return Err(error),
                }
            }
        }
        let probe = mount.join("probe");
        let refused = File::create(&probe)
            .and_then(|mut file| file.write_all(&[0; 4096]).and_then(|()| file.sync_all()));
        let _ = fs::remove_file(&probe);
        if matches!(&refused, Err(error) if error.kind() == io::ErrorKind::StorageFull) {
            return Ok(directory);
        }
    }
    Err(io::Error::other("the volume never stayed full"))
}

const CHUNK: usize = 1 << 20;

/// A body that delivers its first chunk, then lets another writer fill the
/// volume before delivering the rest.
struct FillingBody {
    bytes: io::Cursor<Vec<u8>>,
    mount: PathBuf,
    filled: bool,
}

impl Read for FillingBody {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.bytes.position() >= CHUNK as u64 && !self.filled {
            fill(&self.mount)?;
            self.filled = true;
        }
        let limit = buffer.len().min(CHUNK);
        self.bytes.read(&mut buffer[..limit])
    }
}

struct Served {
    bytes: Vec<u8>,
    /// While set, the body fills this volume after its first chunk.
    filling: Option<PathBuf>,
    offsets: Mutex<Vec<u64>>,
}

impl Transport for Served {
    fn fetch(&self, _url: &str, offset: u64) -> Result<Download, PackError> {
        self.offsets.lock().unwrap().push(offset);
        let rest = self.bytes[offset as usize..].to_vec();
        Ok(Download {
            offset,
            body: match &self.filling {
                Some(mount) => Box::new(FillingBody {
                    bytes: io::Cursor::new(rest),
                    mount: mount.clone(),
                    filled: false,
                }),
                None => Box::new(io::Cursor::new(rest)),
            },
        })
    }
}

fn manifest(bytes: &[u8]) -> PackManifest {
    let mut manifest = approved_packs().remove(0);
    manifest.pack_id = "test-pack".into();
    manifest.files = vec![PackFile {
        license: None,
        name: "model.bin".into(),
        url: "https://huggingface.co/example/model.bin".into(),
        sha256: sha2::Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        bytes: bytes.len() as u64,
    }];
    manifest
}

#[test]
fn a_volume_that_fills_during_a_download_fails_truthfully_and_resumes() {
    // Room for the 256 MiB preflight margin plus the pack.
    let Some(image) = DiskImage::available(320) else {
        return;
    };
    let store = PackStore::new(image.mount.join("Models"));
    let bytes: Vec<u8> = (0..(8 * CHUNK) as u32).map(|i| (i % 251) as u8).collect();
    let manifest = manifest(&bytes);
    let cancelled = AtomicBool::new(false);
    let filling = Served {
        bytes: bytes.clone(),
        filling: Some(image.mount.clone()),
        offsets: Mutex::default(),
    };

    let error = store
        .stage(
            &manifest,
            &[],
            &filling,
            available_space,
            &cancelled,
            |_| {},
        )
        .expect_err("the volume filled during the download");
    assert!(
        matches!(&error, PackError::Io(io) if io.kind() == io::ErrorKind::StorageFull),
        "{error:?}"
    );
    assert!(
        error.to_string().contains("No space left on device"),
        "{error}"
    );
    let staging = store.staging(&manifest);
    assert!(!staging.join("model.bin").exists(), "no finished file");
    assert!(!staging.join(RECEIPT).exists(), "no receipt");
    assert!(store.installed(&manifest).unwrap().is_none());
    assert!(!store.active(&manifest).exists(), "nothing activated");
    let kept = fs::metadata(staging.join(part("model.bin"))).map_or(0, |metadata| metadata.len());
    assert!(kept < bytes.len() as u64);
    assert_eq!(
        &fs::read(staging.join(part("model.bin"))).unwrap_or_default()[..],
        &bytes[..kept as usize],
        "the kept partial is an exact prefix"
    );

    // While the volume stays full, the preflight refuses before any byte.
    let refused = store
        .stage(
            &manifest,
            &[],
            &filling,
            available_space,
            &cancelled,
            |_| {},
        )
        .expect_err("still full");
    assert!(matches!(refused, PackError::Space { .. }), "{refused:?}");
    assert_eq!(filling.offsets.lock().unwrap().len(), 1);

    // Space returns: the same install resumes from the kept bytes.
    fs::remove_dir_all(image.mount.join("fill")).unwrap();
    let served = Served {
        bytes: bytes.clone(),
        filling: None,
        offsets: Mutex::default(),
    };
    let staged = store
        .stage(&manifest, &[], &served, available_space, &cancelled, |_| {})
        .unwrap();
    assert_eq!(*served.offsets.lock().unwrap(), vec![kept]);
    let installed = store.activate(staged).unwrap();
    assert_eq!(
        fs::read(installed.file("model.bin").unwrap()).unwrap(),
        bytes
    );
    assert_eq!(store.installed(&manifest).unwrap(), Some(installed));
}
