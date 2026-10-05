use super::*;
use std::sync::Mutex;

/// Serves bytes from memory, optionally failing after a byte budget or
/// ignoring range requests, and records every requested offset.
struct Memory {
    bytes: Vec<u8>,
    fail_after: Mutex<Option<usize>>,
    ignore_range: bool,
    offsets: Mutex<Vec<u64>>,
}

impl Memory {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            fail_after: Mutex::new(None),
            ignore_range: false,
            offsets: Mutex::new(Vec::new()),
        }
    }
}

struct Failing {
    inner: io::Cursor<Vec<u8>>,
    remaining: Option<usize>,
}

impl Read for Failing {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match &mut self.remaining {
            Some(0) => Err(io::Error::new(io::ErrorKind::ConnectionReset, "reset")),
            Some(remaining) => {
                let limit = buffer.len().min(*remaining);
                let read = self.inner.read(&mut buffer[..limit])?;
                *remaining -= read;
                Ok(read)
            }
            None => self.inner.read(buffer),
        }
    }
}

impl Transport for Memory {
    fn fetch(&self, _url: &str, offset: u64) -> Result<Download, PackError> {
        self.offsets.lock().unwrap().push(offset);
        let start = if self.ignore_range { 0 } else { offset };
        let remaining = self.fail_after.lock().unwrap().take();
        Ok(Download {
            offset: start,
            body: Box::new(Failing {
                inner: io::Cursor::new(self.bytes[start as usize..].to_vec()),
                remaining,
            }),
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

fn plenty(_: &Path) -> io::Result<u64> {
    Ok(u64::MAX)
}

fn payload() -> Vec<u8> {
    (0..3_000_000_u32).map(|i| (i % 251) as u8).collect()
}

#[test]
fn the_approved_whisper_pack_is_valid_and_pinned() {
    let packs = approved_packs();
    assert_eq!(packs.len(), 2);
    let whisper = &packs[0];
    assert_eq!(whisper.pack_id, "whisper-base-en");
    assert_eq!(whisper.pack_version, "2");
    assert_eq!(
        whisper.operations,
        [Operation::Transcribe, Operation::SpeechActivity]
    );
    assert_eq!(whisper.total_bytes(), 147_964_211 + 885_098);
    assert_eq!(
        whisper.files[0].sha256,
        "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002"
    );
    assert_eq!(whisper.files[1].name, "ggml-silero-v6.2.0.bin");
    assert_eq!(whisper.transcription_file(), Some(&whisper.files[0]));
    assert_eq!(whisper.speech_activity_file(), Some(&whisper.files[1]));
    assert_eq!(
        whisper.files[1].sha256,
        "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987"
    );
    assert!(whisper.licenses[0].redistribution);
    assert!(!whisper.acceptance_required());
    assert!(whisper.check_acceptance(&[]).is_ok());
}

#[test]
fn the_approved_bridge_pack_matches_the_qualified_receipt() {
    let pack = approved_pack("ltx-2.3-q4-bridge").unwrap();
    assert!(pack.supports(Operation::BridgeHold));
    assert_eq!(pack.files.len(), 31);
    assert_eq!(pack.total_bytes(), 36_152_862_913);
    // The worker verifies the same assets from its pinned receipt.
    let receipt: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/model-qualification/evidence/2026-09-20-smoke/download-manifest.json"
    ))
    .unwrap();
    let assets = receipt["assets"].as_array().unwrap();
    assert_eq!(assets.len(), pack.files.len());
    for (asset, file) in assets.iter().zip(&pack.files) {
        assert!(file.name.ends_with(&format!(
            "/{}/{}",
            asset["revision"].as_str().unwrap(),
            asset["path"].as_str().unwrap()
        )));
        assert_eq!(file.sha256, asset["sha256"].as_str().unwrap());
        assert_eq!(file.bytes, asset["size"].as_u64().unwrap());
        assert_eq!(file.url, asset["url"].as_str().unwrap());
    }
    // Two separately accepted license layers with their compiled texts.
    let ids = pack.license_ids();
    assert_eq!(ids, ["ltx-2", "gemma"]);
    assert!(
        pack.licenses
            .iter()
            .all(|license| license.acceptance_required)
    );
    let ltx = license_text(pack.licenses[0].text.as_deref().unwrap()).unwrap();
    assert!(ltx.starts_with("                         LTX-2 Community License Agreement"));
    assert!(
        license_text(pack.licenses[1].text.as_deref().unwrap())
            .unwrap()
            .contains("Gemma Terms of Use")
    );
    assert_eq!(
        pack.license_bytes(&pack.licenses[0]) + pack.license_bytes(&pack.licenses[1]),
        pack.total_bytes()
    );
    assert!(matches!(
        pack.check_acceptance(&["ltx-2".into()]),
        Err(PackError::LicenseNotAccepted { title }) if title == "Gemma Terms of Use"
    ));
    assert!(pack.check_acceptance(&ids).is_ok());
    // The compiled texts are the recorded bytes; the LTX text is the pack's
    // own LICENSE file.
    let digest = |text: &str| -> String {
        sha2::Sha256::digest(text.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    };
    let sources: serde_json::Value =
        serde_json::from_str(include_str!("../../../../models/licenses/sources.json")).unwrap();
    for source in sources["files"].as_array().unwrap() {
        let text = license_text(source["path"].as_str().unwrap()).unwrap();
        assert_eq!(digest(text), source["sha256"].as_str().unwrap());
    }
    let license_file = pack
        .files
        .iter()
        .find(|file| file.name.ends_with("/LICENSE"))
        .unwrap();
    assert_eq!(digest(ltx), license_file.sha256);
}

#[test]
fn manifests_require_https_approved_hosts_safe_names_and_hashes() {
    let base = manifest(b"x");
    type Change = fn(&mut PackManifest);
    let cases: [(&str, Change); 9] = [
        ("http", |m| {
            m.files[0].url = "http://huggingface.co/a".into()
        }),
        ("host", |m| m.files[0].url = "https://example.com/a".into()),
        ("lookalike", |m| {
            m.files[0].url = "https://huggingface.co.evil.test/a".into()
        }),
        ("name", |m| m.files[0].name = "../escape".into()),
        ("hash", |m| m.files[0].sha256 = "ABC".into()),
        ("duplicate", |m| {
            let file = m.files[0].clone();
            m.files.push(file);
        }),
        ("identity", |m| m.pack_version = ".hidden".into()),
        ("traversal", |m| m.files[0].name = "a/../b".into()),
        ("absolute", |m| m.files[0].name = "/etc/passwd".into()),
    ];
    for (label, change) in cases {
        let mut candidate = base.clone();
        change(&mut candidate);
        assert!(candidate.validate().is_err(), "{label}");
    }
    assert!(base.validate().is_ok());
    let mut nested = base.clone();
    nested.files[0].name = "encoder/0123abcd/model.safetensors".into();
    assert!(nested.validate().is_ok());
    // Names that collide with staging files or with another file's directory.
    for name in ["receipt.json", "model.bin.part"] {
        let mut candidate = base.clone();
        candidate.files[0].name = name.into();
        assert!(candidate.validate().is_err(), "{name}");
    }
    let mut shadowed = base.clone();
    let mut inner = shadowed.files[0].clone();
    inner.name = "model.bin/inner.bin".into();
    shadowed.files.push(inner);
    assert!(shadowed.validate().is_err());

    let licensing: [(&str, Change); 5] = [
        ("unknown file license", |m| {
            m.files[0].license = Some("other".into())
        }),
        ("unassigned with several", |m| {
            let mut second = m.licenses[0].clone();
            second.id = "second".into();
            m.licenses.push(second);
        }),
        ("duplicate license", |m| {
            let second = m.licenses[0].clone();
            m.licenses.push(second);
            m.files[0].license = Some(m.licenses[0].id.clone());
        }),
        ("missing text", |m| {
            m.licenses[0].text = Some("absent.txt".into())
        }),
        ("plain http link", |m| {
            m.licenses[0].url = "http://example.com".into()
        }),
    ];
    for (label, change) in licensing {
        let mut candidate = base.clone();
        change(&mut candidate);
        assert!(candidate.validate().is_err(), "{label}");
    }
}

#[test]
fn install_verifies_stages_and_activates_one_complete_version() {
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_path_buf());
    let bytes = payload();
    let manifest = manifest(&bytes);
    assert!(store.installed(&manifest).unwrap().is_none());
    let mut seen = Vec::new();
    let staged = store
        .stage(
            &manifest,
            &[],
            &Memory::new(bytes.clone()),
            plenty,
            &AtomicBool::new(false),
            |p| seen.push(p.completed_bytes),
        )
        .unwrap();
    assert_eq!(seen.last(), Some(&(bytes.len() as u64)));
    assert!(seen.windows(2).all(|pair| pair[0] <= pair[1]));
    // Staged but not yet active until the host's smoke test passes.
    assert!(store.installed(&manifest).unwrap().is_none());
    assert_eq!(
        std::fs::read(staged.file("model.bin").unwrap()).unwrap(),
        bytes
    );
    let installed = store.activate(staged).unwrap();
    assert_eq!(store.installed(&manifest).unwrap(), Some(installed.clone()));
    assert_eq!(
        std::fs::read(installed.file("model.bin").unwrap()).unwrap(),
        bytes
    );
    assert!(installed.file("other.bin").is_none());
    store.remove(&manifest).unwrap();
    assert!(store.installed(&manifest).unwrap().is_none());
    store.remove(&manifest).unwrap();
}

#[test]
fn interrupted_downloads_resume_and_range_ignoring_servers_restart() {
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_path_buf());
    let bytes = payload();
    let manifest = manifest(&bytes);
    let transport = Memory::new(bytes.clone());
    *transport.fail_after.lock().unwrap() = Some(1_000_000);
    let error = store
        .stage(
            &manifest,
            &[],
            &transport,
            plenty,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap_err();
    assert!(matches!(error, PackError::Transport(_)), "{error}");
    let staged = store
        .stage(
            &manifest,
            &[],
            &transport,
            plenty,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    assert_eq!(*transport.offsets.lock().unwrap(), [0, 1_000_000]);
    assert_eq!(
        std::fs::read(staged.file("model.bin").unwrap()).unwrap(),
        bytes
    );
    store.discard(staged).unwrap();

    let restart = Memory {
        ignore_range: true,
        ..Memory::new(bytes.clone())
    };
    *restart.fail_after.lock().unwrap() = Some(500_000);
    assert!(
        store
            .stage(
                &manifest,
                &[],
                &restart,
                plenty,
                &AtomicBool::new(false),
                |_| {}
            )
            .is_err()
    );
    let staged = store
        .stage(
            &manifest,
            &[],
            &restart,
            plenty,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    assert_eq!(
        std::fs::read(staged.file("model.bin").unwrap()).unwrap(),
        bytes
    );
}

#[test]
fn corrupt_space_starved_and_cancelled_installs_never_activate() {
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_path_buf());
    let bytes = payload();
    let manifest = manifest(&bytes);
    let mut corrupt = bytes.clone();
    corrupt[123] ^= 0xff;
    let error = store
        .stage(
            &manifest,
            &[],
            &Memory::new(corrupt),
            plenty,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap_err();
    assert!(matches!(error, PackError::Verification { .. }), "{error}");
    // The corrupt partial file is gone, so a retry downloads from zero.
    let transport = Memory::new(bytes.clone());
    store
        .stage(
            &manifest,
            &[],
            &transport,
            plenty,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    assert_eq!(*transport.offsets.lock().unwrap(), [0]);

    let starved = PackStore::new(root.path().join("starved"));
    let error = starved
        .stage(
            &manifest,
            &[],
            &Memory::new(bytes.clone()),
            |_| Ok(10),
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap_err();
    assert!(
        matches!(error, PackError::Space { available: 10, .. }),
        "{error}"
    );

    let cancelled = PackStore::new(root.path().join("cancelled"));
    let error = cancelled
        .stage(
            &manifest,
            &[],
            &Memory::new(bytes),
            plenty,
            &AtomicBool::new(true),
            |_| {},
        )
        .unwrap_err();
    assert!(matches!(error, PackError::Cancelled), "{error}");
    assert!(cancelled.installed(&manifest).unwrap().is_none());
}

#[test]
fn a_tampered_receipt_or_truncated_file_is_not_installed() {
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_path_buf());
    let bytes = payload();
    let manifest = manifest(&bytes);
    let staged = store
        .stage(
            &manifest,
            &[],
            &Memory::new(bytes),
            plenty,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    let installed = store.activate(staged).unwrap();
    let receipt = installed.directory.join(RECEIPT);
    let original = std::fs::read_to_string(&receipt).unwrap();
    std::fs::write(
        &receipt,
        original.replace(&manifest.files[0].sha256, &"0".repeat(64)),
    )
    .unwrap();
    assert!(store.installed(&manifest).unwrap().is_none());
    std::fs::write(&receipt, &original).unwrap();
    assert!(store.installed(&manifest).unwrap().is_some());
    let model = installed.file("model.bin").unwrap();
    File::options()
        .write(true)
        .open(&model)
        .unwrap()
        .set_len(10)
        .unwrap();
    assert!(store.installed(&manifest).unwrap().is_none());
}

#[test]
fn content_ranges_parse_their_first_byte_and_space_is_measurable() {
    assert_eq!(content_range_start("bytes 1000-1999/2000"), Some(1000));
    assert_eq!(content_range_start("bytes 0-0/1"), Some(0));
    assert_eq!(content_range_start("items 1-2/3"), None);
    assert_eq!(content_range_start("bytes x-1/2"), None);
    let directory = tempfile::tempdir().unwrap();
    assert!(available_space(directory.path()).unwrap() > 0);
}

/// Serves the first bytes, then blocks like a stalled socket until dropped.
struct Stalling(Vec<u8>);

struct Blocking {
    first: Option<Vec<u8>>,
}

impl Read for Blocking {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if let Some(first) = self.first.take() {
            buffer[..first.len()].copy_from_slice(&first);
            return Ok(first.len());
        }
        // Park until the test process ends; the download must not wait here.
        loop {
            std::thread::park();
        }
    }
}

impl Transport for Stalling {
    fn fetch(&self, _url: &str, _offset: u64) -> Result<Download, PackError> {
        Ok(Download {
            offset: 0,
            body: Box::new(Blocking {
                first: Some(self.0[..1000].to_vec()),
            }),
        })
    }
}

#[test]
fn cancelling_a_stalled_download_returns_promptly_and_keeps_its_bytes() {
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_path_buf());
    let bytes = payload();
    let manifest = manifest(&bytes);
    let cancelled = std::sync::Arc::new(AtomicBool::new(false));
    let setter = std::sync::Arc::clone(&cancelled);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        setter.store(true, Ordering::Release);
    });
    let started = Instant::now();
    let error = store
        .stage(&manifest, &[], &Stalling(bytes), plenty, &cancelled, |_| {})
        .unwrap_err();
    assert!(matches!(error, PackError::Cancelled), "{error}");
    assert!(started.elapsed() < Duration::from_secs(5));
    let partial = store.staging(&manifest).join("model.bin.part");
    assert_eq!(std::fs::metadata(partial).unwrap().len(), 1000);
}

#[test]
fn unexpected_resume_offsets_restart_and_installs_are_exclusive() {
    struct Shifted(Vec<u8>, Mutex<Vec<u64>>);
    impl Transport for Shifted {
        fn fetch(&self, _url: &str, offset: u64) -> Result<Download, PackError> {
            self.1.lock().unwrap().push(offset);
            // Answers a resume from the wrong place, and a fresh start fully.
            let start = if offset > 0 { offset / 2 } else { 0 };
            Ok(Download {
                offset: start,
                body: Box::new(io::Cursor::new(self.0[start as usize..].to_vec())),
            })
        }
    }
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_path_buf());
    let bytes = payload();
    let manifest = manifest(&bytes);
    let staging = store.staging(&manifest);
    std::fs::create_dir_all(&staging).unwrap();
    std::fs::write(staging.join("model.bin.part"), &bytes[..1_000_000]).unwrap();
    let transport = Shifted(bytes.clone(), Mutex::new(Vec::new()));
    let error = store
        .stage(
            &manifest,
            &[],
            &transport,
            plenty,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap_err();
    assert!(matches!(error, PackError::Transport(_)), "{error}");
    assert_eq!(
        std::fs::metadata(staging.join("model.bin.part"))
            .unwrap()
            .len(),
        0
    );
    let staged = store
        .stage(
            &manifest,
            &[],
            &transport,
            plenty,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    assert_eq!(*transport.1.lock().unwrap(), [1_000_000, 0]);

    // The staged pack holds the install lock until it is activated.
    let second = store.stage(
        &manifest,
        &[],
        &Memory::new(bytes.clone()),
        plenty,
        &AtomicBool::new(false),
        |_| {},
    );
    assert!(matches!(second, Err(PackError::Busy)));
    store.activate(staged).unwrap();
    assert!(store.installed(&manifest).unwrap().is_some());
}

#[test]
fn a_new_version_copies_identical_files_from_an_installed_one() {
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_path_buf());
    let bytes = payload();
    let mut first = manifest(&bytes);
    first.pack_version = "1".into();
    let staged = store
        .stage(
            &first,
            &[],
            &Memory::new(bytes.clone()),
            plenty,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    store.activate(staged).unwrap();

    // Version 2 keeps model.bin and adds a second file.
    let extra: Vec<u8> = (0..1_000_u32).map(|i| (i % 7) as u8).collect();
    let mut second = first.clone();
    second.pack_version = "2".into();
    second.files.push(PackFile {
        license: None,
        name: "extra.bin".into(),
        url: "https://huggingface.co/example/extra.bin".into(),
        sha256: sha2::Sha256::digest(&extra)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        bytes: extra.len() as u64,
    });
    let transport = Memory::new(extra.clone());
    let staged = store
        .stage(
            &second,
            &[],
            &transport,
            plenty,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    // Only the new file was downloaded.
    assert_eq!(*transport.offsets.lock().unwrap(), [0]);
    let installed = store.activate(staged).unwrap();
    assert_eq!(
        std::fs::read(installed.file("model.bin").unwrap()).unwrap(),
        bytes
    );
    assert_eq!(
        std::fs::read(installed.file("extra.bin").unwrap()).unwrap(),
        extra
    );
    assert!(store.installed(&first).unwrap().is_some());

    // A tampered installed copy is discarded and downloaded instead.
    let mut third = second.clone();
    third.pack_version = "3".into();
    let tampered = root.path().join("test-pack/2/model.bin");
    let mut damaged = bytes.clone();
    damaged[10] ^= 1;
    std::fs::write(&tampered, &damaged).unwrap();
    std::fs::remove_dir_all(root.path().join("test-pack/1")).unwrap();
    let transport = Memory::new(bytes.clone());
    let staged = store.stage(
        &third,
        &[],
        &Memory::new(extra.clone()),
        plenty,
        &AtomicBool::new(false),
        |_| {},
    );
    // The tampered copy fails verification and falls back to the transport,
    // which here serves the wrong bytes.
    assert!(staged.is_err());
    let staged = store
        .stage(
            &third,
            &[],
            &transport,
            plenty,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    assert_eq!(
        std::fs::read(staged.file("model.bin").unwrap()).unwrap(),
        bytes
    );
}

fn accepting(mut manifest: PackManifest) -> PackManifest {
    manifest.licenses[0].acceptance_required = true;
    manifest
}

#[test]
fn a_license_requiring_acceptance_refuses_before_any_byte() {
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_path_buf());
    let bytes = payload();
    let manifest = accepting(manifest(&bytes));
    let transport = Memory::new(bytes.clone());
    let error = store
        .stage(
            &manifest,
            &[],
            &transport,
            plenty,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap_err();
    assert!(
        matches!(error, PackError::LicenseNotAccepted { .. }),
        "{error}"
    );
    assert!(transport.offsets.lock().unwrap().is_empty());
    assert!(!store.staging(&manifest).exists());
    let source = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("model.bin"), &bytes).unwrap();
    let error = store
        .import(
            &manifest,
            &[],
            &ImportSource::Directory(source.path().into()),
            plenty,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap_err();
    assert!(
        matches!(error, PackError::LicenseNotAccepted { .. }),
        "{error}"
    );

    let staged = store
        .stage(
            &manifest,
            &manifest.license_ids(),
            &transport,
            plenty,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    let installed = store.activate(staged).unwrap();
    let receipt: serde_json::Value =
        serde_json::from_slice(&std::fs::read(installed.directory.join(RECEIPT)).unwrap()).unwrap();
    assert_eq!(receipt["accepted_licenses"], serde_json::json!(["mit"]));
    assert_eq!(receipt["origin"], "download");
}

/// A two-file pack whose second file lives in a subdirectory.
fn nested(bytes: &[u8], extra: &[u8]) -> PackManifest {
    let mut manifest = manifest(bytes);
    manifest.files.push(PackFile {
        license: None,
        name: "encoder/rev/extra.bin".into(),
        url: "https://huggingface.co/example/extra.bin".into(),
        sha256: sha2::Sha256::digest(extra)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        bytes: extra.len() as u64,
    });
    manifest
}

#[test]
fn offline_folders_install_verified_copies_and_refuse_tampering() {
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_path_buf());
    let bytes = payload();
    let extra: Vec<u8> = (0..5_000_u32).map(|i| (i % 13) as u8).collect();
    let manifest = nested(&bytes, &extra);
    let source = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("model.bin"), &bytes).unwrap();
    let folder = ImportSource::at(source.path()).unwrap();
    assert_eq!(folder, ImportSource::Directory(source.path().into()));

    // An incomplete folder names what is missing and stages nothing usable.
    let error = store
        .import(
            &manifest,
            &[],
            &folder,
            plenty,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap_err();
    assert!(
        matches!(&error, PackError::ImportIncomplete { missing: 1, example } if example == "encoder/rev/extra.bin"),
        "{error}"
    );

    // A tampered file of the right size is refused and removed from staging.
    std::fs::create_dir_all(source.path().join("encoder/rev")).unwrap();
    let mut tampered = extra.clone();
    tampered[7] ^= 1;
    std::fs::write(source.path().join("encoder/rev/extra.bin"), &tampered).unwrap();
    let error = store
        .import(
            &manifest,
            &[],
            &folder,
            plenty,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap_err();
    assert!(matches!(error, PackError::Verification { .. }), "{error}");
    assert!(store.installed(&manifest).unwrap().is_none());

    // A symbolic link is never followed out of the source folder.
    std::fs::remove_file(source.path().join("encoder/rev/extra.bin")).unwrap();
    let outside = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(outside.path(), &extra).unwrap();
    std::os::unix::fs::symlink(outside.path(), source.path().join("encoder/rev/extra.bin"))
        .unwrap();
    assert!(matches!(
        store.import(
            &manifest,
            &[],
            &folder,
            plenty,
            &AtomicBool::new(false),
            |_| {}
        ),
        Err(PackError::ImportIncomplete { .. })
    ));

    std::fs::remove_file(source.path().join("encoder/rev/extra.bin")).unwrap();
    std::fs::write(source.path().join("encoder/rev/extra.bin"), &extra).unwrap();
    let mut seen = Vec::new();
    let staged = store
        .import(
            &manifest,
            &[],
            &folder,
            plenty,
            &AtomicBool::new(false),
            |p| seen.push(p.completed_bytes),
        )
        .unwrap();
    assert_eq!(seen.last(), Some(&manifest.total_bytes()));
    let installed = store.activate(staged).unwrap();
    assert_eq!(
        std::fs::read(installed.file("encoder/rev/extra.bin").unwrap()).unwrap(),
        extra
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&std::fs::read(installed.directory.join(RECEIPT)).unwrap()).unwrap();
    assert_eq!(receipt["origin"], "import");
    // The source folder is untouched.
    assert_eq!(
        std::fs::read(source.path().join("model.bin")).unwrap(),
        bytes
    );
}

#[test]
fn exported_archives_import_and_hostile_archives_are_refused() {
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_path_buf());
    let bytes = payload();
    let extra: Vec<u8> = (0..5_000_u32).map(|i| (i % 13) as u8).collect();
    let manifest = nested(&bytes, &extra);
    let transport = Memory::new(bytes.clone());
    let source = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(source.path().join("encoder/rev")).unwrap();
    std::fs::write(source.path().join("model.bin"), &bytes).unwrap();
    std::fs::write(source.path().join("encoder/rev/extra.bin"), &extra).unwrap();
    let staged = store
        .import(
            &manifest,
            &[],
            &ImportSource::Directory(source.path().into()),
            plenty,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    store.activate(staged).unwrap();
    drop(transport);

    let archives = tempfile::tempdir().unwrap();
    let archive = archives.path().join("pack.tar");
    store
        .export(&manifest, &archive, &AtomicBool::new(false), |_| {})
        .unwrap();
    // The system tar reads it, so other tools can produce the same layout.
    let listing = std::process::Command::new("tar")
        .arg("-tf")
        .arg(&archive)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(listing.stdout).unwrap(),
        "test-pack/2/model.bin\ntest-pack/2/encoder/rev/extra.bin\n"
    );

    // Export never replaces an existing file.
    let error = store
        .export(&manifest, &archive, &AtomicBool::new(false), |_| {})
        .unwrap_err();
    assert!(matches!(error, PackError::Io(_)), "{error}");

    let other = PackStore::new(archives.path().join("models"));
    let staged = other
        .import(
            &manifest,
            &[],
            &ImportSource::at(&archive).unwrap(),
            plenty,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    let installed = other.activate(staged).unwrap();
    assert_eq!(
        std::fs::read(installed.file("model.bin").unwrap()).unwrap(),
        bytes
    );
    other.remove(&manifest).unwrap();

    // An archive the system tar wrote from the plain folder layout imports too.
    let plain = archives.path().join("plain.tar");
    let status = std::process::Command::new("tar")
        .arg("-cf")
        .arg(&plain)
        .arg("-C")
        .arg(source.path())
        .args(["model.bin", "encoder"])
        .status()
        .unwrap();
    assert!(status.success());
    let staged = other
        .import(
            &manifest,
            &[],
            &ImportSource::Archive(plain),
            plenty,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    other.discard(staged).unwrap();

    // A matching member that is a symbolic link is refused.
    let linked = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(linked.path().join("encoder/rev")).unwrap();
    std::fs::write(linked.path().join("model.bin"), &bytes).unwrap();
    std::os::unix::fs::symlink("/etc/hosts", linked.path().join("encoder/rev/extra.bin")).unwrap();
    let hostile = archives.path().join("hostile.tar");
    assert!(
        std::process::Command::new("tar")
            .arg("-cf")
            .arg(&hostile)
            .arg("-C")
            .arg(linked.path())
            .args(["model.bin", "encoder"])
            .status()
            .unwrap()
            .success()
    );
    let error = other
        .import(
            &manifest,
            &[],
            &ImportSource::Archive(hostile),
            plenty,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap_err();
    assert!(
        matches!(
            error,
            PackError::Verification {
                reason: "archive entry is not a regular file",
                ..
            }
        ),
        "{error}"
    );

    // A flipped byte inside a member fails its hash; a corrupted header fails
    // its checksum. Neither activates anything.
    let mut damaged = std::fs::read(&archive).unwrap();
    let offset = damaged.windows(4).position(|w| w == [0, 1, 2, 3]).unwrap();
    damaged[offset + 100] ^= 1;
    let damaged_path = archives.path().join("damaged.tar");
    std::fs::write(&damaged_path, &damaged).unwrap();
    let error = other
        .import(
            &manifest,
            &[],
            &ImportSource::Archive(damaged_path.clone()),
            plenty,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap_err();
    assert!(matches!(error, PackError::Verification { .. }), "{error}");
    let mut header = std::fs::read(&archive).unwrap();
    header[10] ^= 1;
    std::fs::write(&damaged_path, &header).unwrap();
    let error = other
        .import(
            &manifest,
            &[],
            &ImportSource::Archive(damaged_path),
            plenty,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap_err();
    assert!(
        matches!(
            error,
            PackError::Verification {
                reason: "archive header checksum is wrong",
                ..
            }
        ),
        "{error}"
    );
    // A truncated archive is incomplete.
    let truncated = archives.path().join("truncated.tar");
    std::fs::write(&truncated, &std::fs::read(&archive).unwrap()[..2048]).unwrap();
    assert!(
        other
            .import(
                &manifest,
                &[],
                &ImportSource::Archive(truncated),
                plenty,
                &AtomicBool::new(false),
                |_| {}
            )
            .is_err()
    );
    assert!(other.installed(&manifest).unwrap().is_none());
}

#[test]
fn pack_state_reports_partial_bytes_and_discard_clears_them() {
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().to_path_buf());
    let bytes = payload();
    let manifest = manifest(&bytes);
    assert_eq!(store.state(&manifest).unwrap(), PackState::Absent);
    let transport = Memory::new(bytes.clone());
    *transport.fail_after.lock().unwrap() = Some(1_000_000);
    assert!(
        store
            .stage(
                &manifest,
                &[],
                &transport,
                plenty,
                &AtomicBool::new(false),
                |_| {}
            )
            .is_err()
    );
    assert_eq!(
        store.state(&manifest).unwrap(),
        PackState::Partial { bytes: 1_000_000 }
    );
    assert_eq!(
        store.remaining_bytes(&manifest),
        bytes.len() as u64 - 1_000_000
    );
    store.discard_partial(&manifest).unwrap();
    assert_eq!(store.state(&manifest).unwrap(), PackState::Absent);
}

#[test]
fn archive_numbers_accept_octal_and_base_256() {
    assert_eq!(archive_octal(b"00000001750\0"), 1000);
    let mut big = [0_u8; 12];
    big[4..].copy_from_slice(&12_000_000_000_u64.to_be_bytes());
    big[0] = 0x80;
    assert_eq!(archive_octal(&big), 12_000_000_000);
}

fn archive_octal(field: &[u8]) -> u64 {
    archive::test_octal(field)
}

fn files_under(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if let Ok(entries) = std::fs::read_dir(directory) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let kind = entry.file_type();
            found.push(path.clone());
            if kind.is_ok_and(|kind| kind.is_dir()) {
                found.extend(files_under(&path));
            }
        }
    }
    found
}

/// One ustar header block (with a valid checksum) followed by padded data.
fn ustar(name: &str, kind: u8, link: &str, data: &[u8]) -> Vec<u8> {
    let mut header = [0_u8; 512];
    header[..name.len().min(100)].copy_from_slice(&name.as_bytes()[..name.len().min(100)]);
    header[100..108].copy_from_slice(b"0000644\0");
    header[108..116].copy_from_slice(b"0000000\0");
    header[116..124].copy_from_slice(b"0000000\0");
    header[124..136].copy_from_slice(format!("{:011o}\0", data.len()).as_bytes());
    header[136..148].copy_from_slice(b"00000000000\0");
    header[156] = kind;
    header[157..157 + link.len().min(100)].copy_from_slice(&link.as_bytes()[..link.len().min(100)]);
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    repair_checksum(&mut header);
    let mut bytes = header.to_vec();
    bytes.extend_from_slice(data);
    bytes.resize(bytes.len().div_ceil(512) * 512, 0);
    bytes
}

fn repair_checksum(header: &mut [u8]) {
    header[148..156].copy_from_slice(b"        ");
    let sum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
    header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
}

/// Recomputes every reachable header checksum so mutations reach the member
/// grammar instead of stopping at the checksum.
fn repair_archive(input: &[u8]) -> Vec<u8> {
    let mut bytes = input.to_vec();
    let mut offset = 0;
    while offset + 512 <= bytes.len() {
        let block = &mut bytes[offset..offset + 512];
        if block.iter().all(|byte| *byte == 0) {
            break;
        }
        repair_checksum(block);
        let size = std::str::from_utf8(&block[124..135])
            .ok()
            .and_then(|text| {
                u64::from_str_radix(text.trim_matches(|c: char| c == '\0' || c == ' '), 8).ok()
            })
            .unwrap_or(0);
        offset = offset
            .saturating_add(512)
            .saturating_add(usize::try_from(size.div_ceil(512) * 512).unwrap_or(usize::MAX));
    }
    bytes
}

/// Gate G: hostile offline pack archives. Import must refuse with a typed
/// `PackError`, or stage exactly the manifest's verified bytes; nothing may
/// be written outside the pack store, and no link may be staged.
#[test]
fn adversarial_pack_archives() {
    use deadpan_chaos::{Target, Verdict, fuzz, reject};
    let bytes: Vec<u8> = (0..20_000_u32).map(|i| (i % 251) as u8).collect();
    let extra: Vec<u8> = (0..3_000_u32).map(|i| (i % 13) as u8).collect();
    let manifest = nested(&bytes, &extra);
    let root = tempfile::tempdir().unwrap();
    let store = PackStore::new(root.path().join("installed"));
    let source = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(source.path().join("encoder/rev")).unwrap();
    std::fs::write(source.path().join("model.bin"), &bytes).unwrap();
    std::fs::write(source.path().join("encoder/rev/extra.bin"), &extra).unwrap();
    let staged = store
        .import(
            &manifest,
            &[],
            &ImportSource::Directory(source.path().into()),
            plenty,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    store.activate(staged).unwrap();
    let exported = root.path().join("pack.tar");
    store
        .export(&manifest, &exported, &AtomicBool::new(false), |_| {})
        .unwrap();
    let mut seeds = vec![std::fs::read(&exported).unwrap()];
    let plain = root.path().join("plain.tar");
    let status = std::process::Command::new("tar")
        .arg("-cf")
        .arg(&plain)
        .arg("-C")
        .arg(source.path())
        .args(["model.bin", "encoder"])
        .status()
        .unwrap();
    assert!(status.success());
    seeds.push(std::fs::read(&plain).unwrap());
    // Hostile member shapes: traversal, absolute paths, links, pax and GNU
    // long-name overrides, each followed by the end marker.
    let end = vec![0_u8; 1024];
    for member in [
        ustar("../escape.bin", b'0', "", &bytes[..100]),
        ustar("/tmp/deadpan-absolute.bin", b'0', "", &bytes[..100]),
        ustar("test-pack/2/../../model.bin", b'0', "", &bytes),
        ustar("model.bin", b'2', "/etc/passwd", &[]),
        ustar("model.bin", b'1', "../outside", &[]),
        ustar("encoder", b'5', "", &[]),
        [
            ustar("pax", b'x', "", b"30 path=../../escape/model.bin\n"),
            ustar("model.bin", b'0', "", &bytes),
        ]
        .concat(),
        [
            ustar("././@LongLink", b'L', "", b"../../long/model.bin\0"),
            ustar("model.bin", b'0', "", &bytes),
        ]
        .concat(),
        [
            ustar("model.bin", b'0', "", &bytes),
            ustar("model.bin", b'0', "", &bytes),
        ]
        .concat(),
    ] {
        seeds.push([member, end.clone()].concat());
    }
    let cases = tempfile::tempdir().unwrap();
    let counter = std::cell::Cell::new(0_u32);
    let report = fuzz(
        Target::bytes("models-pack-archive")
            .iterations(300)
            .max_input_bytes(256 * 1024),
        seeds,
        |input| {
            counter.set(counter.get() + 1);
            let case = cases.path().join(format!("case-{}", counter.get()));
            std::fs::create_dir_all(&case).unwrap();
            let archive = case.join("input.tar");
            // Even lengths repair header checksums; odd lengths keep the raw
            // mutation so checksum validation itself stays under test.
            let input = if input.len() % 2 == 0 {
                repair_archive(input)
            } else {
                input.to_vec()
            };
            std::fs::write(&archive, &input).unwrap();
            let target = PackStore::new(case.join("models"));
            let outcome = target.import(
                &manifest,
                &[],
                &ImportSource::Archive(archive.clone()),
                plenty,
                &AtomicBool::new(false),
                |_| {},
            );
            let verdict = match outcome {
                Ok(staged) => {
                    let installed = target
                        .activate(staged)
                        .map_err(|error| format!("activate: {error}"))?;
                    for (name, expected) in
                        [("model.bin", &bytes), ("encoder/rev/extra.bin", &extra)]
                    {
                        let path = installed
                            .file(name)
                            .ok_or("installed pack lacks a manifest file")?;
                        if std::fs::read(&path).map_err(|error| error.to_string())? != *expected {
                            return Err(format!("installed {name} differs from its manifest"));
                        }
                    }
                    Verdict::Accepted
                }
                Err(error) => reject(error)?,
            };
            for path in files_under(&case) {
                if path != archive && !path.starts_with(case.join("models")) {
                    return Err(format!(
                        "import wrote outside the pack store: {}",
                        path.display()
                    ));
                }
                if std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink())
                {
                    return Err(format!("import staged a link: {}", path.display()));
                }
            }
            let _ = std::fs::remove_dir_all(&case);
            Ok(verdict)
        },
    );
    report.assert_clean();
}

/// Gate G: pack manifests are compiled in today, but receipts and future
/// catalogs are parsed from disk; hostile JSON must fail validation cleanly.
#[test]
fn adversarial_pack_manifests() {
    use deadpan_chaos::{Target, Verdict, fuzz, reject};
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models/packs");
    let seeds = deadpan_chaos::seeds_from_dir(&directory);
    assert_eq!(seeds.len(), 2);
    let report = fuzz(
        Target::json("models-pack-manifest").iterations(600),
        seeds,
        |input| match serde_json::from_slice::<PackManifest>(input) {
            Ok(manifest) => match manifest.validate() {
                Ok(()) => Ok(Verdict::Accepted),
                Err(error) => reject(error),
            },
            Err(error) => reject(error),
        },
    );
    report.assert_clean();
}
