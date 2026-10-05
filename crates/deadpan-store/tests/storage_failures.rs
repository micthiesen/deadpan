//! Real ENOSPC, EROFS and EACCES failures on small APFS disk images.
//!
//! Each test attaches its own private image with `hdiutil`, so the failures
//! come from the kernel and SQLite rather than injected errors. After every
//! failure the project must keep its last committed state, validate completely
//! and accept the same edit once space or access returns.
#![cfg(target_os = "macos")]

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_core::*;
use deadpan_store::original_media::{OriginalMediaLimits, OriginalOwnership};
use deadpan_store::{AccessMode, ProjectStore, StoreError};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

/// A private attached APFS image, detached on drop.
struct DiskImage {
    mount: PathBuf,
    _scratch: tempfile::TempDir,
}

impl DiskImage {
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
        let image = Self {
            mount: mount.canonicalize()?,
            _scratch: scratch,
        };
        image.attach(false)?;
        Ok(image)
    }

    fn attach(&self, read_only: bool) -> Result {
        let mut command = ProcessCommand::new("hdiutil");
        command.args(["attach", "-quiet", "-nobrowse", "-noautoopen"]);
        if read_only {
            command.arg("-readonly");
        }
        let attached = command
            .arg("-mountpoint")
            .arg(&self.mount)
            .arg(self._scratch.path().join("volume.dmg"))
            .output()?;
        if !attached.status.success() {
            return Err(format!(
                "hdiutil attach failed: {}",
                String::from_utf8_lossy(&attached.stderr)
            )
            .into());
        }
        Ok(())
    }

    fn detach(&self) -> Result {
        let detached = ProcessCommand::new("hdiutil")
            .args(["detach", "-quiet"])
            .arg(&self.mount)
            .output()?;
        if !detached.status.success() {
            return Err(format!(
                "hdiutil detach failed: {}",
                String::from_utf8_lossy(&detached.stderr)
            )
            .into());
        }
        Ok(())
    }

    /// Writes a filler until the volume refuses another block, then proves
    /// the volume is full: a new 4 KiB file cannot be written.
    fn fill(&self) -> Result<PathBuf> {
        // APFS can release space shortly after a write fails; keep filling
        // (appending) until the refusal holds.
        // APFS can keep releasing space under load; give it time to settle.
        for round in 0..64 {
            if round > 0 {
                std::thread::sleep(std::time::Duration::from_millis(50));
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

fn insert(current: &ProjectDocument, revision: &str, node: &str) -> Result<CommandRequest> {
    let id = NodeId::new(node)?;
    Ok(CommandRequest {
        project_id: current.project_id().clone(),
        expected_revision: current.revision_id().clone(),
        new_revision: RevisionId::new(revision)?,
        command: Command::Insert {
            parent: current.root().clone(),
            index: 0,
            subtree: Subtree {
                overrides: Default::default(),
                gap_overrides: Default::default(),
                root: id.clone(),
                nodes: BTreeMap::from([(
                    id,
                    BeatNode::hold(
                        "A pause long enough to need a new database page",
                        HoldRecipe {
                            picture_context: None,
                            duration: FrameDuration::new(12)?,
                            video: HoldVideo::Background,
                            audio: HoldAudio::Silence,
                        },
                    ),
                )]),
            },
        },
    })
}

/// Commits until the full volume refuses one; returns that request.
fn first_refused_commit(store: &mut ProjectStore) -> Result<(CommandRequest, StoreError)> {
    for ordinal in 0..2_000 {
        let current = store.snapshot()?;
        let request = insert(&current, &format!("full-{ordinal}"), &format!("n{ordinal}"))?;
        match store.commit(&request) {
            Ok(_) => continue,
            Err(error) => {
                assert_eq!(
                    store.snapshot()?,
                    current,
                    "a refused commit keeps the last committed document"
                );
                assert_eq!(store.head_revision()?, *current.revision_id());
                return Ok((request, error));
            }
        }
    }
    Err("the full volume accepted 2,000 commits".into())
}

#[test]
fn a_full_disk_refuses_the_commit_and_the_same_edit_saves_once_space_returns() -> Result {
    let image = DiskImage::new(16)?;
    let path = image.mount.join("full.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    let first = insert(&store.snapshot()?, "r1", "first")?;
    store.commit(&first)?;
    let filler = image.fill()?;

    let (request, error) = first_refused_commit(&mut store)?;
    assert_eq!(error.code(), "DiskFull", "{error}");
    // A full checkpoint also fails truthfully and leaves no partial copy.
    let checkpoint = store
        .checkpoint()
        .expect_err("no checkpoint on a full disk");
    assert_eq!(checkpoint.code(), "DiskFull", "{checkpoint}");
    assert_eq!(fs::read_dir(path.join("Snapshots"))?.count(), 0);
    store.validate()?;

    fs::remove_file(&filler)?;
    let saved = store.commit(&request)?;
    assert_eq!(saved.revision_id, request.new_revision);
    store.checkpoint()?;
    drop(store);

    let reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert!(reopened.open_recovery().is_clean());
    assert_eq!(reopened.head_revision()?, request.new_revision);
    reopened.validate_full()?;
    Ok(())
}

#[test]
fn a_full_disk_refuses_original_retention_without_a_partial_object() -> Result {
    let image = DiskImage::new(16)?;
    let scratch = tempfile::tempdir()?;
    let source = scratch.path().join("original.mp4");
    // Larger than what remains once the volume is nearly full.
    fs::write(&source, vec![0x42_u8; 6 << 20])?;
    let source = source.canonicalize()?;
    let path = image.mount.join("retain.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    // Leave a little room so the package itself stays writable.
    let filler = image.fill_leaving(2 << 20)?;
    let limits = OriginalMediaLimits::new(64 << 20, Duration::from_secs(60))?;
    let cancelled = AtomicBool::new(false);
    let error = store
        .retain_original(&source, OriginalOwnership::Managed, limits, &cancelled)
        .expect_err("retention needs more space than remains");
    assert_eq!(error.code(), "DiskFull", "{error}");
    let leftovers: Vec<_> = fs::read_dir(path.join("Media/Originals"))?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::result::Result<_, _>>()?;
    assert!(leftovers.is_empty(), "no partial object: {leftovers:?}");
    assert!(store.original_records(None, 10)?.is_empty());

    fs::remove_file(&filler)?;
    let retained =
        store.retain_original(&source, OriginalOwnership::Managed, limits, &cancelled)?;
    assert_eq!(retained.record.object().byte_length(), 6 << 20);
    drop(store);
    ProjectStore::open(&path, AccessMode::ReadWrite)?.validate_full()?;
    Ok(())
}

#[test]
fn a_read_only_volume_refuses_writing_without_changing_the_project() -> Result {
    let image = DiskImage::new(16)?;
    let path = image.mount.join("readonly.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    store.commit(&insert(&store.snapshot()?, "r1", "first")?)?;
    drop(store);
    let database = fs::read(path.join("project.sqlite"))?;
    image.detach()?;
    image.attach(true)?;

    for mode in [AccessMode::ReadWrite, AccessMode::ReadOnly] {
        // A WAL database needs writable shared memory even for readers.
        let error = ProjectStore::open(&path, mode)
            .err()
            .expect("a read-only volume cannot host this project");
        assert!(
            matches!(error, StoreError::ReadOnlyLocation(_)),
            "{mode:?}: {error}"
        );
        assert_eq!(error.code(), "ProjectReadOnly");
        assert!(error.to_string().contains("copy it to a writable folder"));
    }
    assert_eq!(fs::read(path.join("project.sqlite"))?, database);
    assert!(!path.join(".writer.session").exists());
    Ok(())
}

#[test]
fn permission_denied_refuses_writing_without_changing_the_project() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("denied.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    store.commit(&insert(&store.snapshot()?, "r1", "first")?)?;
    drop(store);
    let database = path.join("project.sqlite");
    let before = fs::read(&database)?;
    fs::set_permissions(&database, fs::Permissions::from_mode(0o400))?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o500))?;
    let result = ProjectStore::open(&path, AccessMode::ReadWrite);
    // Restore before asserting so the temporary directory can be removed.
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    fs::set_permissions(&database, fs::Permissions::from_mode(0o644))?;
    let error = result
        .err()
        .expect("a read-only package cannot host a writer");
    // SQLite reports the unwritable database file as SQLITE_READONLY.
    assert_eq!(error.code(), "ProjectReadOnly", "{error}");
    assert_eq!(fs::read(&database)?, before);
    assert!(!path.join(".writer.session").exists());
    let reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(reopened.head_revision()?.as_str(), "r1");
    reopened.validate_full()?;
    Ok(())
}
