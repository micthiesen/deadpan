//! Real ENOSPC failures for generated media and render candidates on small
//! APFS disk images.
//!
//! Each test attaches its own private image with `hdiutil`, so the failures
//! come from the kernel rather than injected errors. A refused write must be
//! classified `DiskFull`, leave no published object or `.pending-*` temporary,
//! keep the project valid, and the same operation must succeed once space
//! returns. Tests skip with a printed reason when `hdiutil` is unavailable.
#![cfg(target_os = "macos")]

use std::error::Error;
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_core::*;
use deadpan_jobs::render::RenderAttemptIdentity;
use deadpan_jobs::{AttemptId, CancellationToken, RequestId, Sha256};
use deadpan_store::generated_media::{GeneratedMediaLimits, GeneratedReadLimits};
use deadpan_store::render_media::RenderMediaLimits;
use deadpan_store::{AccessMode, ProjectStore};
use sha2::Digest;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

/// Larger than what remains once the volume is nearly full.
const OBJECT_BYTES: usize = 6 << 20;
/// Free space left so the package database itself stays writable.
const RESERVE_BYTES: u64 = 2 << 20;

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
            let refused = fs::File::create(&probe)
                .and_then(|mut file| file.write_all(&[0; 4096]).and_then(|()| file.sync_all()));
            let _ = fs::remove_file(&probe);
            if matches!(&refused, Err(error) if error.kind() == std::io::ErrorKind::StorageFull) {
                return Ok(filler);
            }
        }
        Err("the volume never stayed full".into())
    }

    /// Fills the volume, then rewrites the filler `reserve` bytes smaller.
    /// APFS needs free blocks even to truncate, so never shrink in place.
    fn fill_leaving(&self, reserve: u64) -> Result<PathBuf> {
        let full = fs::metadata(self.fill()?)?.len();
        fs::remove_file(self.mount.join("filler"))?;
        self.fill_with(&[1 << 20], full.saturating_sub(reserve))
    }

    fn fill_with(&self, chunks: &[usize], limit: u64) -> Result<PathBuf> {
        let path = self.mount.join("filler");
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
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

fn document() -> Result<ProjectDocument> {
    Ok(ProjectDocument::new(
        ProjectId::new("project")?,
        RevisionId::new("r0")?,
        PresentationBasis {
            width: 640,
            height: 360,
            frame_rate: FrameRate::new(30, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?)
}

/// Bytes that do not compress or deduplicate into a few APFS blocks.
fn object_bytes(seed: u8) -> Vec<u8> {
    let mut state = 0x9e37_79b9_u32 ^ u32::from(seed);
    (0..OBJECT_BYTES)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state.to_le_bytes()[0]
        })
        .collect()
}

fn reference(bytes: &[u8]) -> Result<GeneratedObjectRef> {
    Ok(GeneratedObjectRef::new(
        GeneratedContentId::new(blake3::hash(bytes).to_hex().to_string())?,
        u64::try_from(bytes.len())?,
    )?)
}

/// Every name in a namespace directory, including `.pending-*` temporaries.
fn entries(directory: &Path) -> Result<Vec<String>> {
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in fs::read_dir(directory)? {
        names.push(entry?.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(names)
}

#[test]
fn a_full_disk_refuses_generated_promotion_without_a_visible_object() -> Result {
    let Some(image) = DiskImage::available(16)? else {
        return Ok(());
    };
    let path = image.mount.join("generated.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    let bytes = object_bytes(1);
    let expected = reference(&bytes)?;
    let limits = GeneratedMediaLimits::new(64 << 20)?;
    let namespace = path.join("Media/Generated");
    let before = entries(&namespace)?;
    let filler = image.fill_leaving(RESERVE_BYTES)?;

    let error = store
        .promote_generated_object(&mut Cursor::new(&bytes), &expected, limits)
        .expect_err("promotion needs more space than remains");
    assert_eq!(error.code(), "DiskFull", "{error}");
    assert_eq!(
        entries(&namespace)?,
        before,
        "no published object and no pending temporary"
    );
    let read = GeneratedReadLimits::new(64 << 20, Duration::from_secs(30))?;
    let missing = store
        .generated_read_handle()
        .snapshot(&expected, read, &AtomicBool::new(false))
        .expect_err("nothing was published");
    assert_eq!(missing.code(), "GeneratedMediaMissing", "{missing}");
    store.validate()?;

    fs::remove_file(&filler)?;
    let promoted = store.promote_generated_object(&mut Cursor::new(&bytes), &expected, limits)?;
    assert_eq!(promoted, expected);
    let snapshot =
        store
            .generated_read_handle()
            .snapshot(&expected, read, &AtomicBool::new(false))?;
    assert_eq!(snapshot.reference(), &expected);
    let published = entries(&namespace)?;
    assert!(
        published.iter().all(|name| !name.starts_with(".pending-")),
        "{published:?}"
    );
    drop(store);
    ProjectStore::open(&path, AccessMode::ReadWrite)?.validate_full()?;
    Ok(())
}

fn sha256(bytes: &[u8]) -> Result<Sha256> {
    let digest = sha2::Sha256::digest(bytes);
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(Sha256::new(hex)?)
}

#[test]
fn a_full_disk_refuses_render_candidate_retention_without_a_visible_object() -> Result {
    let Some(image) = DiskImage::available(16)? else {
        return Ok(());
    };
    let path = image.mount.join("render.deadpan");
    let store = ProjectStore::create(&path, &document()?)?;
    let movie = object_bytes(2);
    let manifest = br#"{"schema":1,"claim":"opaque"}"#.as_slice();
    let identity = RenderAttemptIdentity {
        job_id: RequestId::new("render-job")?,
        attempt_id: AttemptId::new("encoding-attempt")?,
        cancellation_token: CancellationToken::new("render-cancel")?,
        expected_sequence: 1,
    };
    let limits = RenderMediaLimits::new(64 << 20, 4096, (64 << 20) + 4096, 128 << 20, 20)?;
    let writer = store.render_write_handle()?;
    let retain = || {
        writer.prepare_retention(
            &identity,
            &mut Cursor::new(&movie),
            movie.len() as u64,
            &sha256(&movie).expect("digest"),
            manifest,
            limits,
            &AtomicBool::new(false),
            Instant::now() + Duration::from_secs(60),
        )
    };
    let namespace = path.join("Media/RenderCandidates");
    let before = entries(&namespace)?;
    let filler = image.fill_leaving(RESERVE_BYTES)?;

    let Err(error) = retain() else {
        return Err("retention needs more space than remains".into());
    };
    assert_eq!(error.code(), "DiskFull", "{error}");
    assert_eq!(
        entries(&namespace)?,
        before,
        "no published candidate and no pending temporary"
    );
    store.validate()?;

    fs::remove_file(&filler)?;
    let prepared = retain()?;
    let movie_name = format!("blake3-{}", blake3::hash(&movie).to_hex());
    let retained = entries(&namespace)?;
    assert!(retained.contains(&movie_name), "{retained:?}");
    assert!(
        retained.iter().all(|name| !name.starts_with(".pending-")),
        "{retained:?}"
    );
    assert_eq!(prepared.media().movie().byte_length(), movie.len() as u64);
    drop(prepared);
    drop(writer);
    drop(store);
    ProjectStore::open(&path, AccessMode::ReadWrite)?.validate_full()?;
    Ok(())
}
