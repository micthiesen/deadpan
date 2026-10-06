//! Inline `apply` programs: the native single-action Apply path, headless.
use super::*;

fn apply(document: &ProjectDocument, version: u64, instructions: Value) -> Value {
    envelope(
        document,
        version,
        json!({"type":"apply","program":{"instructions":instructions},
        "parent":document.root(),"cursor":4,
        "visual_selection":{"type":"time","anchor":18,"head":4,"extending":true},
        "new_revision":"applied"}),
    )
}

#[test]
fn inline_apply_previews_refuses_stale_revisions_and_commits_one_undo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = create(scratch.path())?;
    let input = scratch.path().join("apply.json");
    let initial = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    fs::write(&input, request(&initial)?.to_string())?;
    success(&[
        "command",
        package.to_str().unwrap(),
        "--json",
        input.to_str().unwrap(),
    ])?;
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let repeat = json!([{"type":"repeat","selector":{"type":"visual_selection"},"plays":3}]);
    let invocation = apply(&before, 0, repeat.clone());

    let unchanged = state(&package)?;
    let preview = invoke(&package, &input, &invocation, true)?;
    assert_eq!(state(&package)?, unchanged);
    assert_eq!(preview["operation"], "apply");
    assert_eq!(preview["instruction_count"], 1);
    assert_eq!(preview["committed"], false);
    assert!(preview["committed_revision"].is_null());
    assert_eq!(preview["edit"]["new_revision"], "applied");
    assert_eq!(preview["edit"]["duration_delta"], 28);

    // A request naming an older revision is refused without writing.
    let mut stale = invocation.clone();
    stale["expected_revision"] = json!(initial.revision_id());
    let error = reject(&package, &input, &stale)?;
    assert_eq!(error["error"]["code"], "RevisionConflict");
    assert_eq!(
        error["error"]["current_revision"],
        json!(before.revision_id())
    );
    // An empty program is not a valid request.
    let error = reject(&package, &input, &apply(&before, 0, json!([])))?;
    assert!(error["error"]["code"].is_string(), "{error}");

    let result = invoke(&package, &input, &invocation, false)?;
    assert_eq!(result["committed"], true);
    assert_eq!(result["committed_revision"], "applied");
    assert!(result["committed_registers"].is_null());
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let after = writer.snapshot()?;
    assert_eq!(after.duration()?.frames(), before.duration()?.frames() + 28);
    assert_eq!(writer.registers()?.version, 0);
    writer.undo(after.revision_id(), RevisionId::new("apply-undo")?)?;
    same_document(&writer.snapshot()?, &before)?;
    Ok(())
}
