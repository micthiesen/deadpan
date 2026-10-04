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
    assert_eq!(packs.len(), 1);
    let whisper = &packs[0];
    assert_eq!(whisper.pack_id, "whisper-base-en");
    assert_eq!(whisper.operations, [Operation::Transcribe]);
    assert_eq!(whisper.total_bytes(), 147_964_211);
    assert_eq!(
        whisper.files[0].sha256,
        "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002"
    );
    assert!(whisper.license.redistribution);
}

#[test]
fn manifests_require_https_approved_hosts_safe_names_and_hashes() {
    let base = manifest(b"x");
    type Change = fn(&mut PackManifest);
    let cases: [(&str, Change); 7] = [
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
    ];
    for (label, change) in cases {
        let mut candidate = base.clone();
        change(&mut candidate);
        assert!(candidate.validate().is_err(), "{label}");
    }
    assert!(base.validate().is_ok());
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
            .stage(&manifest, &restart, plenty, &AtomicBool::new(false), |_| {})
            .is_err()
    );
    let staged = store
        .stage(&manifest, &restart, plenty, &AtomicBool::new(false), |_| {})
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
