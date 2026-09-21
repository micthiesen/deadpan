use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Output};

use deadpan_core::{
    BeatNode, Command, FrameDuration, HoldAudio, HoldRecipe, HoldVideo, NodeId, ProjectDocument,
    Subtree,
};
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::{Value, json};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn cli(arguments: &[&str]) -> Result<Output> {
    Ok(ProcessCommand::new(env!("CARGO_BIN_EXE_deadpan-cli"))
        .args(arguments)
        .output()?)
}

fn success(arguments: &[&str]) -> Result<Value> {
    let output = cli(arguments)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn create(root: &Path) -> Result<PathBuf> {
    let package = root.join("session.deadpan");
    let result = success(&[
        "project",
        "create",
        package.to_str().unwrap(),
        "--fps",
        "30000/1001",
        "--size",
        "1920x1080",
    ])?;
    assert_eq!(result["duration_frames"], 0);
    Ok(package)
}

fn request(document: &ProjectDocument) -> Result<Value> {
    let node = NodeId::new("hold")?;
    Ok(json!({
        "protocol": 1, "project_id": document.project_id(), "expected_revision": document.revision_id(), "new_revision": "after-hold",
        "command": Command::Insert {parent: document.root().clone(), index: 0, subtree: Subtree {
            root: node.clone(), nodes: BTreeMap::from([(node, BeatNode::hold("Silence", HoldRecipe {
                duration: FrameDuration::new(45)?, video: HoldVideo::Background, audio: HoldAudio::Silence,
            }))]),
        }},
    }))
}

#[test]
fn headless_edit_dry_run_conflict_and_durable_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = create(scratch.path())?;
    let path = package.to_str().unwrap();
    let document = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let input = scratch.path().join("command.json");
    fs::write(&input, serde_json::to_vec(&request(&document)?)?)?;
    let input = input.to_str().unwrap();
    let preview = success(&["command", path, "--json", input, "--dry-run"])?;
    assert_eq!(preview["committed"], false);
    assert_eq!(preview["edit"]["duration_delta"], 45);
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
        document
    );

    let committed = success(&["command", path, "--json", input])?;
    assert_eq!(committed["committed"], true);
    assert_eq!(committed["outcome"]["edit"], preview["edit"]);
    let stale = cli(&["command", path, "--json", input])?;
    assert!(!stale.status.success());
    assert!(stale.stdout.is_empty());
    let error: Value = serde_json::from_slice(&stale.stderr)?;
    assert_eq!(error["error"]["code"], "RevisionConflict");
    assert_eq!(error["error"]["current_revision"], "after-hold");

    let undone = success(&["project", "undo", path, "--expected", "after-hold"])?;
    let undo_revision = undone["outcome"]["revision_id"].as_str().unwrap();
    assert_ne!(undo_revision, document.revision_id().as_str());
    let summary = success(&["project", "validate", path])?;
    assert_eq!(summary["duration_frames"], 0);
    success(&["project", "redo", path, "--expected", undo_revision])?;
    assert_eq!(
        success(&["project", "validate", path])?["duration_frames"],
        45
    );
    let dump_a = cli(&["project", "dump", path, "--json"])?;
    let dump_b = cli(&["project", "dump", path, "--json"])?;
    assert!(dump_a.status.success());
    assert_eq!(dump_a.stdout, dump_b.stdout, "dumps are deterministic");
    let checkpoint = success(&["project", "checkpoint", path])?;
    assert!(Path::new(checkpoint["database_checkpoint"].as_str().unwrap()).is_file());
    Ok(())
}

#[test]
fn writer_lock_is_enforced_between_processes_but_readers_and_dry_run_work() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = create(scratch.path())?;
    let writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let document = writer.snapshot()?;
    let input = scratch.path().join("command.json");
    fs::write(&input, serde_json::to_vec(&request(&document)?)?)?;
    let path = package.to_str().unwrap();
    let input = input.to_str().unwrap();
    success(&["project", "validate", path])?;
    success(&["command", path, "--json", input, "--dry-run"])?;
    let output = cli(&["command", path, "--json", input])?;
    assert!(!output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stderr)?["error"]["code"],
        "ProjectAlreadyOpen"
    );
    assert_eq!(writer.snapshot()?, document);
    Ok(())
}

#[test]
fn malformed_or_future_protocol_is_rejected_before_mutation() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = create(scratch.path())?;
    let document = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let input = scratch.path().join("command.json");
    let mut request = request(&document)?;
    request["protocol"] = 999.into();
    fs::write(&input, serde_json::to_vec(&request)?)?;
    let output = cli(&[
        "command",
        package.to_str().unwrap(),
        "--json",
        input.to_str().unwrap(),
    ])?;
    assert!(!output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stderr)?["error"]["code"],
        "ProtocolUnsupported"
    );
    request["protocol"] = 1.into();
    request["shell"] = "unexpected".into();
    fs::write(&input, serde_json::to_vec(&request)?)?;
    assert!(
        !cli(&[
            "command",
            package.to_str().unwrap(),
            "--json",
            input.to_str().unwrap()
        ])?
        .status
        .success()
    );
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
        document
    );
    Ok(())
}

#[test]
fn history_dry_runs_work_with_a_writer_and_preserve_the_redo_path() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = create(scratch.path())?;
    let path = package.to_str().unwrap();
    let document = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let input = scratch.path().join("command.json");
    fs::write(&input, serde_json::to_vec(&request(&document)?)?)?;
    success(&["command", path, "--json", input.to_str().unwrap()])?;
    let writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let before = writer.snapshot()?;
    let preview = success(&[
        "project",
        "undo",
        path,
        "--expected",
        "after-hold",
        "--dry-run",
    ])?;
    assert_eq!(preview["committed"], false);
    assert_eq!(preview["outcome"]["edit"]["duration_delta"], -45);
    assert_eq!(writer.snapshot()?, before);
    let stale = cli(&["project", "undo", path, "--expected", "stale", "--dry-run"])?;
    assert!(!stale.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&stale.stderr)?["error"]["code"],
        "RevisionConflict"
    );
    drop(writer);
    let undone = success(&["project", "undo", path, "--expected", "after-hold"])?;
    let revision = undone["outcome"]["revision_id"].as_str().unwrap();
    let writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let before = writer.snapshot()?;
    let preview = success(&["project", "redo", path, "--expected", revision, "--dry-run"])?;
    assert_eq!(preview["committed"], false);
    assert_eq!(preview["outcome"]["edit"]["duration_delta"], 45);
    assert_eq!(writer.snapshot()?, before);
    drop(writer);
    success(&["project", "redo", path, "--expected", revision])?;
    assert_eq!(
        success(&["project", "validate", path])?["duration_frames"],
        45
    );
    Ok(())
}

#[test]
fn dry_run_and_commit_preserve_actionable_domain_errors() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = create(scratch.path())?;
    let document = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let base = request(&document)?;
    let input = scratch.path().join("rejected.json");

    let mut zero = base.clone();
    zero["command"]["subtree"]["nodes"]["hold"]["kind"]["recipe"]["duration"] = json!(0);
    let mut range = base.clone();
    range["command"]["subtree"]["root"] = json!("retime");
    range["command"]["subtree"]["nodes"]["retime"] = json!({
        "label": "Invalid child range", "kind": {"type": "retime", "child": "hold", "duration": 12,
        "mapping": {"start": 0, "end": 46}, "pitch": "preserve"}
    });
    let mut overflow = base.clone();
    overflow["command"]["subtree"]["nodes"]["hold"]["kind"]["recipe"]["duration"] = json!(i64::MAX);
    overflow["command"]["subtree"]["root"] = json!("repeat");
    overflow["command"]["subtree"]["nodes"]["repeat"] = json!({
        "label": "Overflow", "kind": {"type": "repeat", "child": "hold", "plays": 2, "gap": null}
    });
    let mut limit = base;
    limit["command"]["subtree"]["nodes"]["hold"]["label"] = json!("x".repeat(1025));
    for (request, code) in [
        (zero, "InvalidDuration"),
        (range, "SourceRangeInvalid"),
        (overflow, "TimingOverflow"),
        (limit, "LimitExceeded"),
    ] {
        fs::write(&input, serde_json::to_vec(&request)?)?;
        for dry_run in [false, true] {
            let mut args = vec![
                "command",
                package.to_str().unwrap(),
                "--json",
                input.to_str().unwrap(),
            ];
            if dry_run {
                args.push("--dry-run");
            }
            let output = cli(&args)?;
            assert!(!output.status.success());
            assert!(output.stdout.is_empty());
            assert_eq!(
                serde_json::from_slice::<Value>(&output.stderr)?["error"]["code"],
                code
            );
            assert_eq!(
                ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
                document
            );
        }
    }
    Ok(())
}
