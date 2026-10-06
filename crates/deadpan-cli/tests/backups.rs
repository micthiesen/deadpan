//! `project backup`, `backups`, `restore` and `view` through the real CLI.
#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::error::Error;
use std::path::Path;
use std::process::{Command, Output};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn cli(arguments: &[&str]) -> Result<Output> {
    Ok(Command::new(env!("CARGO_BIN_EXE_deadpan-cli"))
        .args(arguments)
        .output()?)
}

fn success(arguments: &[&str]) -> Result<serde_json::Value> {
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

fn failure_code(arguments: &[&str]) -> Result<String> {
    let output = cli(arguments)?;
    if output.status.success() {
        return Err(format!("{arguments:?} unexpectedly succeeded").into());
    }
    let report: serde_json::Value = serde_json::from_slice(&output.stderr)?;
    Ok(report["error"]["code"]
        .as_str()
        .ok_or("missing error code")?
        .to_owned())
}

fn insert_pause(path: &str, revision: &str, new_revision: &str) -> Result<()> {
    use deadpan_core::{
        BeatNode, Command as Edit, FrameDuration, HoldAudio, HoldRecipe, HoldVideo, NodeId, Subtree,
    };
    let view = success(&["project", "view", path])?;
    let document =
        deadpan_store::ProjectStore::open(Path::new(path), deadpan_store::AccessMode::ReadOnly)?
            .snapshot()?;
    let node = NodeId::new(format!("pause-{new_revision}"))?;
    let request = serde_json::json!({
        "protocol": 1,
        "project_id": view["project_id"],
        "expected_revision": revision,
        "new_revision": new_revision,
        "command": Edit::Insert {
            parent: document.root().clone(),
            index: 0,
            subtree: Subtree {
                overrides: Default::default(),
                gap_overrides: Default::default(),
                root: node.clone(),
                nodes: std::collections::BTreeMap::from([(
                    node,
                    BeatNode::hold(
                        "Pause",
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
    let scratch = tempfile::NamedTempFile::new()?;
    std::fs::write(scratch.path(), serde_json::to_vec(&request)?)?;
    success(&[
        "command",
        path,
        "--json",
        scratch.path().to_str().ok_or("path")?,
    ])?;
    Ok(())
}

#[test]
fn backups_restore_and_view_through_the_cli() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("cli.deadpan");
    let path = path.to_str().ok_or("path")?;
    let created = success(&[
        "project", "create", path, "--fps", "30/1", "--size", "1280x720",
    ])?;
    let initial = created["revision_id"]
        .as_str()
        .ok_or("revision")?
        .to_owned();
    insert_pause(path, &initial, "after-first")?;
    let backup = success(&["project", "backup", path])?;
    let id = backup["created"]["backup"]["id"]
        .as_str()
        .ok_or("backup id")?
        .to_owned();
    assert_eq!(backup["created"]["revision_id"], "after-first");
    insert_pause(path, "after-first", "after-second")?;

    let listed = success(&["project", "backups", path, "--verify"])?;
    let entries = listed["backups"].as_array().ok_or("backups")?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["contents"]["revision_id"], "after-first");
    assert_eq!(entries[0]["contents"]["duration_frames"], 12);
    assert_eq!(entries[0]["verified"], true);

    // A dry run verifies and describes, and writes nothing.
    let dry = success(&["project", "restore", path, &id, "--dry-run"])?;
    assert_eq!(dry["would_restore"]["revision_id"], "after-first");
    assert_eq!(dry["current_revision"], "after-second");
    assert_eq!(
        success(&["project", "view", path])?["revision_id"],
        "after-second"
    );
    assert_eq!(
        success(&["project", "backups", path])?["backups"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );

    let restored = success(&["project", "restore", path, &id])?;
    assert_eq!(
        restored["restored"]["restored"]["revision_id"],
        "after-first"
    );
    assert_eq!(restored["restored"]["replaced_revision"], "after-second");
    let safety = restored["restored"]["safety"]["backup"]["id"]
        .as_str()
        .ok_or("safety id")?
        .to_owned();
    assert_eq!(
        success(&["project", "view", path])?["revision_id"],
        "after-first"
    );
    success(&["project", "validate", path])?;
    // The safety backup brings the replaced edit back.
    success(&["project", "restore", path, &safety])?;
    assert_eq!(
        success(&["project", "view", path])?["revision_id"],
        "after-second"
    );
    assert_eq!(
        failure_code(&["project", "restore", path, "no-such-backup"])?,
        "BackupNotFound"
    );
    Ok(())
}

#[test]
fn a_newer_package_is_viewable_but_never_written() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("future.deadpan");
    let path_text = path.to_str().ok_or("path")?;
    success(&[
        "project", "create", path_text, "--fps", "30/1", "--size", "1280x720",
    ])?;
    let newer = make_newer(&path)?;
    let before = std::fs::read(path.join("project.sqlite"))?;
    let viewed = success(&["project", "view", path_text])?;
    assert_eq!(viewed["newer_schema"], newer);
    assert!(
        viewed["read_only_reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("newer Deadpan"))
    );
    success(&["project", "dump", path_text, "--json"])?;
    for arguments in [
        vec!["project", "validate", path_text],
        vec!["project", "migrate", path_text],
        vec!["project", "backup", path_text],
        vec!["project", "checkpoint", path_text],
    ] {
        assert_eq!(failure_code(&arguments)?, "SchemaNewer", "{arguments:?}");
    }
    assert_eq!(std::fs::read(path.join("project.sqlite"))?, before);
    Ok(())
}

fn make_newer(path: &Path) -> Result<u32> {
    let connection = rusqlite::Connection::open(path.join("project.sqlite"))?;
    let current: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    connection.execute_batch("CREATE TABLE future_feature(id INTEGER PRIMARY KEY) STRICT;")?;
    connection.pragma_update(None, "user_version", current + 1)?;
    connection.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
    Ok(current + 1)
}
