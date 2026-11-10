//! Headless cells of the [recovery matrix](../../../docs/qualification/recovery-matrix-2026-11-10.md)
//! that earlier suites covered only through the store or the native app: a
//! second writer, an unreadable database and a real full volume, each through
//! the shipped `deadpan-cli` process. Every refusal must carry its stable code,
//! leave the committed head and history unchanged, and keep the caller's
//! directory out of the message.
#![cfg(target_os = "macos")]

use std::error::Error;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use deadpan_core::{
    BeatNode, Command as Edit, FrameDuration, HoldAudio, HoldRecipe, HoldVideo, NodeId, Subtree,
};
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::Value;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn cli(arguments: &[&str]) -> Result<Output> {
    Ok(Command::new(env!("CARGO_BIN_EXE_deadpan-cli"))
        .args(arguments)
        .output()?)
}

fn success(arguments: &[&str]) -> Result<Value> {
    let output = cli(arguments)?;
    if !output.status.success() {
        return Err(format!(
            "{arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

/// The structured error of a refused invocation, with nothing on stdout.
fn refusal(arguments: &[&str]) -> Result<Value> {
    let output = cli(arguments)?;
    if output.status.success() {
        return Err(format!("{arguments:?} unexpectedly succeeded").into());
    }
    assert!(output.stdout.is_empty(), "a refusal prints no success JSON");
    let report: Value = serde_json::from_slice(&output.stderr)?;
    Ok(report["error"].clone())
}

fn text(path: &Path) -> Result<&str> {
    Ok(path.to_str().ok_or("path")?)
}

fn head(package: &Path) -> Result<String> {
    Ok(
        success(&["project", "view", text(package)?])?["revision_id"]
            .as_str()
            .ok_or("revision")?
            .to_owned(),
    )
}

/// The project identity and root an edit request names, read while the
/// package is still readable.
struct Target {
    project_id: Value,
    root: NodeId,
}

fn target(package: &Path) -> Result<Target> {
    let view = success(&["project", "view", text(package)?])?;
    let document = ProjectStore::open(package, AccessMode::ReadOnly)?.snapshot()?;
    Ok(Target {
        project_id: view["project_id"].clone(),
        root: document.root().clone(),
    })
}

/// A JSON request inserting one silent pause at the root's start.
fn pause_request(
    target: &Target,
    expected: &str,
    new_revision: &str,
) -> Result<tempfile::NamedTempFile> {
    let node = NodeId::new(format!("pause-{new_revision}"))?;
    let request = serde_json::json!({
        "protocol": 1,
        "project_id": target.project_id,
        "expected_revision": expected,
        "new_revision": new_revision,
        "command": Edit::Insert {
            parent: target.root.clone(),
            index: 0,
            subtree: Subtree {
                overrides: Default::default(),
                gap_overrides: Default::default(),
                root: node.clone(),
                nodes: std::collections::BTreeMap::from([(
                    node,
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
    });
    let mut file = tempfile::NamedTempFile::new()?;
    file.write_all(&serde_json::to_vec(&request)?)?;
    Ok(file)
}

fn create(package: &Path) -> Result<String> {
    let created = success(&[
        "project",
        "create",
        text(package)?,
        "--fps",
        "30/1",
        "--size",
        "640x360",
    ])?;
    Ok(created["revision_id"]
        .as_str()
        .ok_or("revision")?
        .to_owned())
}

fn assert_private(error: &Value, private: &Path) {
    let message = error["message"].as_str().unwrap_or_default();
    let private = private.to_string_lossy();
    assert!(
        !message.contains(private.as_ref()),
        "the message names the caller's directory: {message}"
    );
}

#[test]
fn a_second_writer_is_refused_without_writing_and_the_edit_saves_after_it_closes() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("owned.deadpan");
    let initial = create(&package)?;
    let request = pause_request(&target(&package)?, &initial, "after-owner")?;
    let arguments = ["command", text(&package)?, "--json", text(request.path())?];

    // A writer that publishes no live endpoint (a host still starting, or a
    // foreign process) cannot take this edit, and the CLI cannot take the lock.
    let owner = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let error = refusal(&arguments)?;
    assert_eq!(error["code"], "HostOwnerUnavailable", "{error}");
    assert!(
        error["message"].as_str().is_some_and(
            |message| message.contains("writer has no available authenticated endpoint")
        ),
        "{error}"
    );
    assert!(error["current_revision"].is_null(), "{error}");
    assert_private(&error, scratch.path());
    // Read-only inspection coexists with the owner and sees nothing new.
    assert_eq!(head(&package)?, initial);
    drop(owner);

    let saved = success(&arguments)?;
    assert_eq!(saved["outcome"]["revision_id"], "after-owner", "{saved}");
    assert_eq!(head(&package)?, "after-owner");
    Ok(())
}

#[test]
fn an_unreadable_database_is_reported_as_permission_denied_without_its_path() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("denied.deadpan");
    let initial = create(&package)?;
    let database = package.join("project.sqlite");
    let request = pause_request(&target(&package)?, &initial, "after-denied")?;
    let original = fs::metadata(&database)?.permissions();
    let bytes = fs::read(&database)?;
    fs::set_permissions(&database, fs::Permissions::from_mode(0o000))?;
    let restore = Restore(database.clone(), original);

    for arguments in [
        vec!["project", "validate", text(&package)?],
        vec!["project", "view", text(&package)?],
        vec!["command", text(&package)?, "--json", text(request.path())?],
    ] {
        let error = refusal(&arguments)?;
        assert_eq!(error["code"], "PermissionDenied", "{arguments:?}: {error}");
        let message = error["message"].as_str().unwrap_or_default();
        assert!(
            message.contains("project.sqlite") && message.contains("nothing has been changed"),
            "{message}"
        );
        assert_private(&error, scratch.path());
    }
    drop(restore);
    assert_eq!(
        fs::read(&database)?,
        bytes,
        "no refusal touched the database"
    );
    assert_eq!(head(&package)?, initial);
    let valid = success(&["project", "validate", text(&package)?])?;
    assert_eq!(valid["valid"], true, "{valid}");
    Ok(())
}

/// Puts the database's permissions back even when an assertion fails.
struct Restore(PathBuf, fs::Permissions);

impl Drop for Restore {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.0, self.1.clone());
    }
}

/// A private attached APFS image, detached on drop.
struct DiskImage {
    mount: PathBuf,
    _scratch: tempfile::TempDir,
}

impl DiskImage {
    fn new(megabytes: u32) -> Result<Option<Self>> {
        let scratch = tempfile::tempdir()?;
        let image = scratch.path().join("volume.dmg");
        let created = match Command::new("hdiutil")
            .args(["create", "-quiet", "-size"])
            .arg(format!("{megabytes}m"))
            .args(["-fs", "APFS", "-layout", "NONE", "-volname", "deadpan-test"])
            .arg(&image)
            .output()
        {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                eprintln!("skipping: hdiutil is unavailable ({error})");
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        };
        if !created.status.success() {
            return Err(format!(
                "hdiutil create failed: {}",
                String::from_utf8_lossy(&created.stderr)
            )
            .into());
        }
        let mount = scratch.path().join("mount");
        fs::create_dir(&mount)?;
        let attached = Command::new("hdiutil")
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
        Ok(Some(Self {
            mount: mount.canonicalize()?,
            _scratch: scratch,
        }))
    }

    /// Writes a filler until the volume refuses another block, then proves
    /// the volume is full: a new 4 KiB file cannot be written.
    fn fill(&self) -> Result<PathBuf> {
        let path = self.mount.join("filler");
        // APFS can keep releasing space shortly after a refused write.
        for round in 0..64 {
            if round > 0 {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)?;
            for chunk in [1 << 20, 64 << 10, 4 << 10, 512] {
                let bytes = vec![0x5a_u8; chunk];
                loop {
                    match file.write_all(&bytes).and_then(|()| file.sync_data()) {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::StorageFull => break,
                        Err(error) => return Err(error.into()),
                    }
                }
            }
            let probe = self.mount.join("probe");
            let refused = fs::File::create(&probe)
                .and_then(|mut file| file.write_all(&[0; 4096]).and_then(|()| file.sync_all()));
            let _ = fs::remove_file(&probe);
            if matches!(&refused, Err(error) if error.kind() == std::io::ErrorKind::StorageFull) {
                return Ok(path);
            }
        }
        Err("the volume never stayed full".into())
    }
}

impl Drop for DiskImage {
    fn drop(&mut self) {
        let _ = Command::new("hdiutil")
            .args(["detach", "-quiet", "-force"])
            .arg(&self.mount)
            .output();
    }
}

#[test]
fn a_full_volume_refuses_a_headless_edit_as_disk_full_and_saves_it_once_space_returns() -> Result {
    let Some(image) = DiskImage::new(16)? else {
        return Ok(());
    };
    let package = image.mount.join("full.deadpan");
    let initial = create(&package)?;
    let target = target(&package)?;
    let filler = image.fill()?;

    // SQLite may still reuse free pages; keep editing until the volume refuses.
    let mut expected = initial;
    let mut refused = None;
    for ordinal in 0..200 {
        let revision = format!("full-{ordinal}");
        let request = pause_request(&target, &expected, &revision)?;
        let arguments = ["command", text(&package)?, "--json", text(request.path())?];
        let output = cli(&arguments)?;
        if output.status.success() {
            expected = revision;
            continue;
        }
        let report: Value = serde_json::from_slice(&output.stderr)?;
        refused = Some((request, report["error"].clone()));
        break;
    }
    let (request, error) = refused.ok_or("the full volume accepted 200 headless edits")?;
    assert_eq!(error["code"], "DiskFull", "{error}");
    assert_private(&error, &image.mount);
    // Even reading needs SQLite's shared-memory file; say so instead of
    // reporting a generic open failure.
    let error = refusal(&["project", "view", text(&package)?])?;
    assert_eq!(error["code"], "DiskFull", "{error}");
    assert!(
        error["message"]
            .as_str()
            .is_some_and(|message| message.contains("nothing has been changed")),
        "{error}"
    );
    assert_private(&error, &image.mount);

    fs::remove_file(&filler)?;
    assert_eq!(head(&package)?, expected, "a refused edit is never saved");
    let valid = success(&["project", "validate", text(&package)?])?;
    assert_eq!(valid["valid"], true, "{valid}");
    let arguments = ["command", text(&package)?, "--json", text(request.path())?];
    let saved = success(&arguments)?;
    let saved_revision = saved["outcome"]["revision_id"]
        .as_str()
        .ok_or("revision")?
        .to_owned();
    assert_eq!(head(&package)?, saved_revision);
    assert_ne!(saved_revision, expected);
    let valid = success(&["project", "validate", text(&package)?])?;
    assert_eq!(valid["valid"], true, "{valid}");
    Ok(())
}
