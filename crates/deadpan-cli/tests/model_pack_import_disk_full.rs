//! Real mid-import ENOSPC through the ordinary CLI installer, without models.
//!
//! A small test manifest supplies byte fixtures, not an inference capability.
//! Its previous verified installed version is prepared through PackStore. It
//! cannot become current() because it has no compiled or signed catalog entry;
//! this test preserves that boundary and checks the selection stays absent.
//! Both sources live outside the private APFS destination, so folder import
//! must copy bytes rather than make a same-volume clone. After the first file
//! is copied and read for verification, its progress callback fills the
//! destination with real writes, after the real free-space preflight. No I/O
//! error or free-space result is injected.
#![cfg(target_os = "macos")]

use std::error::Error;
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_cli::{CliError, models::install_pack_for_selection};
use deadpan_models::packs::{
    ImportSource, PackError, PackFile, PackManifest, PackStore, approved_pack, archive,
    available_space,
};
use sha2::Digest;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

const FIRST_BYTES: usize = 64 << 10;
const PAYLOAD_BYTES: usize = 16 << 20;
const RESERVE_BYTES: usize = 2 << 20;
// nextest uses separate processes; its disk-images group owns serialization
// there. Plain cargo test also needs to keep these two images sequential.
static DISK_IMAGE_TEST: Mutex<()> = Mutex::new(());

fn run(command: &mut Command) -> io::Result<Output> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    deadpan_native_process::spawn(command)?.wait_with_output()
}

struct DiskImage {
    mount: PathBuf,
    attached: bool,
    scratch: tempfile::TempDir,
}

impl DiskImage {
    fn available() -> Result<Option<Self>> {
        match run(Command::new("hdiutil").arg("help")) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                eprintln!("skipping: hdiutil is unavailable ({error})");
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
            Ok(output) if !output.status.success() => {
                return Err(format!(
                    "hdiutil help failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                )
                .into());
            }
            Ok(_) => {}
        }
        // The real import preflight needs its 256 MiB margin plus the files.
        let scratch = tempfile::tempdir()?;
        let image = scratch.path().join("volume.dmg");
        let created = run(Command::new("hdiutil")
            .args([
                "create",
                "-quiet",
                "-size",
                "320m",
                "-fs",
                "APFS",
                "-layout",
                "NONE",
                "-volname",
                "deadpan-import-test",
            ])
            .arg(&image))?;
        if !created.status.success() {
            return Err(format!(
                "hdiutil create failed: {}",
                String::from_utf8_lossy(&created.stderr)
            )
            .into());
        }
        let mount = scratch.path().join("mount");
        fs::create_dir(&mount)?;
        let attached = run(Command::new("hdiutil")
            .args([
                "attach",
                "-quiet",
                "-nobrowse",
                "-noautoopen",
                "-mountpoint",
            ])
            .arg(&mount)
            .arg(&image))?;
        if !attached.status.success() {
            return Err(format!(
                "hdiutil attach failed: {}",
                String::from_utf8_lossy(&attached.stderr)
            )
            .into());
        }
        let image = Self {
            mount,
            attached: true,
            scratch,
        };
        assert_ne!(
            fs::metadata(&image.mount)?.dev(),
            fs::metadata(image.scratch.path())?.dev(),
            "filler must be confined to the attached volume"
        );
        Ok(Some(image))
    }

    fn detach(&mut self) -> Result {
        let output = run(Command::new("hdiutil")
            .args(["detach", "-quiet", "-force"])
            .arg(&self.mount))?;
        if !output.status.success() {
            return Err(format!(
                "hdiutil detach failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        self.attached = false;
        Ok(())
    }
}

impl Drop for DiskImage {
    fn drop(&mut self) {
        if self.attached {
            let _ = self.detach();
        }
    }
}

/// Leave room for metadata, but much less than the next imported file. Prove
/// the kernel refused a write before releasing the explicit reserve.
fn consume_space(mount: &Path) -> Result<PathBuf> {
    let directory = mount.join("filler");
    fs::create_dir(&directory)?;
    let reserve = mount.join("reserve");
    let mut reserved = File::create(&reserve)?;
    reserved.write_all(&vec![0xa5; RESERVE_BYTES])?;
    reserved.sync_all()?;
    drop(reserved);
    let deadline = Instant::now() + Duration::from_secs(30);
    for round in 0..200 {
        if Instant::now() >= deadline {
            break;
        }
        if round > 0 {
            std::thread::sleep(Duration::from_millis(50));
        }
        let mut filler = match File::create(directory.join(round.to_string())) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::StorageFull => continue,
            Err(error) => return Err(error.into()),
        };
        for chunk in [8 << 20, 1 << 20, 64 << 10, 4 << 10, 512] {
            let bytes = vec![0x5a; chunk];
            while Instant::now() < deadline {
                match filler.write_all(&bytes).and_then(|()| filler.sync_data()) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::StorageFull => break,
                    Err(error) => return Err(error.into()),
                }
            }
        }
        let probe = mount.join("full-probe");
        let refused = File::create(&probe)
            .and_then(|mut file| file.write_all(&[0; 4096]).and_then(|()| file.sync_all()));
        let _ = fs::remove_file(&probe);
        if matches!(refused, Err(error) if error.kind() == io::ErrorKind::StorageFull) {
            fs::remove_file(&reserve)?;
            return Ok(directory);
        }
    }
    Err("the private volume did not reach a proven full state within 30 seconds".into())
}

fn bytes(length: usize, seed: u32) -> Vec<u8> {
    let mut state = seed;
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state.to_le_bytes()[0]
        })
        .collect()
}

fn manifest(version: &str, files: &[(&str, &[u8])]) -> PackManifest {
    let mut manifest = approved_pack("whisper-base-en").unwrap();
    manifest.pack_id = "offline-storage-fixture".into();
    manifest.pack_version = version.into();
    manifest.files = files
        .iter()
        .map(|(name, bytes)| PackFile {
            name: (*name).into(),
            url: format!("https://huggingface.co/example/{name}"),
            sha256: sha2::Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
            bytes: bytes.len() as u64,
            license: None,
        })
        .collect();
    manifest.validate().unwrap();
    manifest
}

fn import_failure(from_archive: bool) -> Result {
    let _guard = DISK_IMAGE_TEST
        .lock()
        .expect("a previous disk-image test panicked while holding the test lock");
    let Some(mut image) = DiskImage::available()? else {
        return Ok(());
    };
    let source = tempfile::tempdir()?;
    assert_ne!(
        fs::metadata(source.path())?.dev(),
        fs::metadata(&image.mount)?.dev()
    );
    let store = PackStore::new(image.mount.join("Models"));
    let cancelled = AtomicBool::new(false);

    let previous_bytes = bytes(4096, 17);
    fs::write(source.path().join("previous.bin"), &previous_bytes)?;
    let previous = manifest("1", &[("previous.bin", &previous_bytes)]);
    let staged = store.import(
        &previous,
        &[],
        &ImportSource::Directory(source.path().into()),
        available_space,
        &cancelled,
        |_| {},
    )?;
    // Storage fixture only: byte verification is genuine; model inference and
    // catalog admission are deliberately not claimed for these random bytes.
    let installed = store.activate(staged)?;
    let receipt = fs::read(installed.directory.join("receipt.json"))?;
    assert!(store.pointer(&previous.pack_id)?.is_none());

    let first = bytes(FIRST_BYTES, 31);
    let payload = bytes(PAYLOAD_BYTES, 43);
    fs::write(source.path().join("first.bin"), &first)?;
    fs::write(source.path().join("payload.bin"), &payload)?;
    let replacement = manifest("2", &[("first.bin", &first), ("payload.bin", &payload)]);
    let input = if from_archive {
        let path = source.path().join("replacement.tar");
        let entries = replacement
            .files
            .iter()
            .map(|file| {
                (
                    file.name.clone(),
                    source.path().join(&file.name),
                    file.bytes,
                )
            })
            .collect::<Vec<_>>();
        archive::write(&path, &entries, &cancelled, |_| {})?;
        ImportSource::Archive(path)
    } else {
        ImportSource::Directory(source.path().into())
    };
    let mut filler = None;
    let error = install_pack_for_selection(
        &store,
        &replacement,
        &[],
        Some(&input),
        &cancelled,
        |progress| {
            if progress.completed_bytes >= FIRST_BYTES as u64 && filler.is_none() {
                filler =
                    Some(consume_space(&image.mount).expect("fill the private APFS destination"));
            }
        },
        |phase| panic!("failed import unexpectedly reached {phase}; fixture bytes must never reach inference"),
    )
    .expect_err("second file cannot fit after the first file is copied and read for verification");
    assert!(
        filler.is_some(),
        "real preflight passed and first file was copied and read for verification"
    );
    assert!(
        matches!(&error, CliError::ModelPack(PackError::Io(io)) if io.kind() == io::ErrorKind::StorageFull),
        "{error:?}"
    );
    assert!(
        error.to_string().contains("No space left on device"),
        "{error}"
    );

    let staging = store.root().join(".staging").join(format!(
        "{}-{}",
        replacement.pack_id, replacement.pack_version
    ));
    assert_eq!(fs::read(staging.join("first.bin"))?, first);
    assert!(
        !staging.join("payload.bin").exists(),
        "failed file was marked finished"
    );
    assert!(
        !staging.join("receipt.json").exists(),
        "failed import acquired a complete receipt"
    );
    assert!(
        !store
            .root()
            .join(&replacement.pack_id)
            .join(&replacement.pack_version)
            .exists(),
        "partial replacement activated"
    );
    assert!(store.installed(&replacement)?.is_none());
    assert_eq!(store.installed(&previous)?, Some(installed.clone()));
    assert_eq!(
        fs::read(installed.directory.join("previous.bin"))?,
        previous_bytes
    );
    assert_eq!(fs::read(installed.directory.join("receipt.json"))?, receipt);
    assert!(
        store.pointer(&previous.pack_id)?.is_none(),
        "import changed selection"
    );

    // Releasing space permits the same source to stage and verify completely.
    // Do not run the fixture bytes through a real inference smoke test.
    fs::remove_dir_all(filler.unwrap())?;
    let retry = store.import(
        &replacement,
        &[],
        &input,
        available_space,
        &cancelled,
        |_| {},
    )?;
    assert_eq!(fs::read(retry.directory().join("payload.bin"))?, payload);
    assert!(retry.directory().join("receipt.json").is_file());
    store.discard(retry)?;
    assert_eq!(store.installed(&previous)?, Some(installed));
    image.detach()?;
    Ok(())
}

#[test]
fn offline_folder_enospc_preserves_the_previous_verified_installed_version() -> Result {
    import_failure(false)
}

#[test]
fn offline_archive_enospc_preserves_the_previous_verified_installed_version() -> Result {
    import_failure(true)
}
