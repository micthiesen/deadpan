//! `project retain-original --linked` records a bookmark, and
//! `project relink-moved` follows it after the file moves, relinking only
//! after the moved file's bytes match.
#![cfg(target_os = "macos")]

use std::error::Error;
use std::process::Command;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn success(arguments: &[&str]) -> Result<serde_json::Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_deadpan-cli"))
        .args(arguments)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "{arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[test]
fn a_moved_linked_file_is_found_by_its_bookmark_and_verified() -> Result {
    let scratch = tempfile::tempdir()?;
    let root = scratch.path().canonicalize()?;
    let project = root.join("moved.deadpan");
    let project = project.to_str().ok_or("path")?;
    success(&[
        "project", "create", project, "--fps", "30/1", "--size", "640x360",
    ])?;
    let source = root.join("sound.wav");
    std::fs::write(&source, b"linked sound bytes")?;
    let retained = success(&[
        "project",
        "retain-original",
        project,
        source.to_str().ok_or("path")?,
        "--linked",
    ])?;
    assert!(
        retained["retained_original"]["record"]["linked"]["bookmark"]
            .as_array()
            .is_some_and(|bookmark| !bookmark.is_empty()),
        "{retained}"
    );
    // Nothing moved: nothing to do.
    let none = success(&["project", "relink-moved", project])?;
    assert_eq!(none["relinked_originals"], serde_json::json!([]));
    std::fs::create_dir(root.join("Library"))?;
    let moved = root.join("Library/renamed.wav");
    std::fs::rename(&source, &moved)?;
    let relinked = success(&["project", "relink-moved", project])?;
    let records = relinked["relinked_originals"].as_array().ok_or("records")?;
    assert_eq!(records.len(), 1, "{relinked}");
    assert_eq!(records[0]["linked"]["path"], moved.to_str().ok_or("path")?);
    assert_eq!(records[0]["version"], 2);
    assert_eq!(relinked["refused_candidates"], serde_json::json!([]));
    Ok(())
}
