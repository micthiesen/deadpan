#![cfg(any(target_os = "macos", target_os = "linux"))]

use deadpan_store::{AccessMode, ProjectStore};
use serde_json::Value;
use std::{
    error::Error,
    fs,
    path::Path,
    process::{Command, Output},
};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn cli(arguments: &[&str]) -> Result<Output> {
    Ok(Command::new(env!("CARGO_BIN_EXE_deadpan-cli"))
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
fn error(arguments: &[&str]) -> Result<Value> {
    let output = cli(arguments)?;
    assert!(!output.status.success());
    Ok(serde_json::from_slice(&output.stderr)?)
}
fn create(path: &Path) -> Result<Value> {
    success(&[
        "project",
        "create",
        path.to_str().unwrap(),
        "--fps",
        "30000/1001",
        "--size",
        "320x180",
    ])
}

#[test]
fn original_commands_retain_verify_list_and_relink_without_an_authored_edit() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("cli.deadpan");
    let project = package.to_str().unwrap();
    create(&package)?;
    let before = success(&["project", "dump", project, "--json"])?;
    let source = scratch.path().join("source.mp4");
    fs::write(
        &source,
        include_bytes!("../../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4"),
    )?;
    let retained = success(&[
        "project",
        "retain-original",
        project,
        source.to_str().unwrap(),
        "--linked",
    ])?;
    assert_eq!(retained["authored_asset_registered"], false);
    assert_eq!(retained["retained_original"]["method"], "linked");
    let record = &retained["retained_original"]["record"];
    let digest = record["object"]["content"]["digest"].as_str().unwrap();
    assert_eq!(record["object"]["content"]["algorithm"], "blake3");
    assert_eq!(
        success(&["project", "verify-original", project, digest])?["verification"],
        "complete_bytes_and_identity"
    );
    let moved = scratch.path().join("moved.mp4");
    fs::rename(&source, &moved)?;
    assert_eq!(
        error(&["project", "verify-original", project, digest])?["error"]["code"],
        "OriginalOffline"
    );
    let relinked = success(&[
        "project",
        "relink-original",
        project,
        digest,
        moved.to_str().unwrap(),
        "--expected-version",
        "1",
    ])?;
    assert_eq!(relinked["relinked_original"]["version"], 2);
    assert_eq!(
        error(&[
            "project",
            "relink-original",
            project,
            digest,
            moved.to_str().unwrap(),
            "--expected-version",
            "1"
        ])?["error"]["code"],
        "OriginalLocationConflict"
    );
    let managed = success(&[
        "project",
        "retain-original",
        project,
        moved.to_str().unwrap(),
    ])?;
    assert_eq!(managed["retained_original"]["record"]["managed"], true);
    fs::remove_file(&moved)?;
    success(&["project", "verify-original", project, digest])?;
    let page = success(&["project", "originals", project])?;
    assert_eq!(page["originals"].as_array().unwrap().len(), 1);
    assert!(
        success(&["project", "originals", project, "--after", digest])?["originals"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(success(&["project", "dump", project, "--json"])?, before);
    Ok(())
}

#[test]
fn original_mutations_respect_writer_lock_and_validate_arguments() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("cli.deadpan");
    let project = package.to_str().unwrap();
    create(&package)?;
    let writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let missing = scratch.path().join("missing");
    assert_eq!(
        error(&[
            "project",
            "retain-original",
            project,
            missing.to_str().unwrap()
        ])?["error"]["code"],
        "ProjectAlreadyOpen"
    );
    success(&["project", "originals", project])?;
    drop(writer);
    assert_eq!(
        error(&["project", "verify-original", project, "../bad"])?["error"]["code"],
        "OriginalInvalid"
    );
    assert_eq!(
        error(&["project", "retain-original", project, "relative.mp4"])?["error"]["code"],
        "OriginalInvalid"
    );
    Ok(())
}
