use super::*;

#[test]
fn headless_delete_previews_and_commits_retained_timing_with_one_undo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = create(scratch.path())?;
    let path = package.to_str().unwrap();
    let input = scratch.path().join("delete.json");
    let file = input.to_str().unwrap();
    let empty = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let mut inserted = request(&empty)?;
    inserted["command"]["subtree"] = json!({
        "root": "group",
        "nodes": {
            "group": BeatNode::sequence("Group", vec![NodeId::new("cut")?, NodeId::new("suffix")?]),
            "cut": BeatNode::hold("Cut", HoldRecipe {
                duration: FrameDuration::new(1)?, picture_context: None,
                video: HoldVideo::Background, audio: HoldAudio::Silence,
            }),
            "suffix": BeatNode::hold("Suffix", HoldRecipe {
                duration: FrameDuration::new(4)?, picture_context: None,
                video: HoldVideo::Background, audio: HoldAudio::Silence,
            }),
        },
    });
    fs::write(&input, inserted.to_string())?;
    success(&["command", path, "--json", file])?;
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    fs::write(
        &input,
        json!({
            "protocol": 1, "project_id": before.project_id(),
            "expected_revision": before.revision_id(), "new_revision": "deleted",
            "command": {"command":"delete", "node":"cut"},
        })
        .to_string(),
    )?;
    let preview = success(&["command", path, "--json", file, "--dry-run"])?;
    assert_eq!(preview["edit"]["duration_delta"], -1);
    assert!(preview["edit"]["forward"]["audio_bindings"].is_object());
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
        before
    );
    let committed = success(&["command", path, "--json", file])?;
    assert_eq!(committed["outcome"]["edit"], preview["edit"]);
    let after = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert_eq!(after.duration()?.frames(), 4);
    assert_eq!(
        after.audio_bindings().bindings()[&NodeId::new("suffix")?]
            .reanchors
            .len(),
        1
    );
    let database = rusqlite::Connection::open(package.join("project.sqlite"))?;
    let encoded: String = database.query_row(
        "SELECT request FROM history WHERE revision_id='deleted'",
        [],
        |row| row.get(0),
    )?;
    let stored: Value = serde_json::from_str(&encoded)?;
    assert_eq!(
        stored["command"],
        json!({"command":"delete_ripple", "node":"cut", "timing":{"allocation":"deleted", "ordinal":0}})
    );
    assert!(!cli(&["command", path, "--json", file])?.status.success());
    success(&["project", "undo", path, "--expected", "deleted"])?;
    let restored = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let mut expected = serde_json::to_value(&before)?;
    expected["revision_id"] = json!(restored.revision_id());
    assert_eq!(serde_json::to_value(restored)?, expected);
    Ok(())
}
