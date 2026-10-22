//! Qualified same-Source windows copied once and cut in descending order.
use super::*;

fn same_authored(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = json!(actual.revision_id());
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

pub fn build(directory: &Path) -> Result<Fixture> {
    let mut project = Project::create(directory, "range-sets")?;
    project.shorten()?;
    let before = project.document()?;
    assert_eq!(before.duration()?.frames(), 30);
    let (selected, _) = root_child_at(&before, 1)?;
    let instructions = json!([
        {"type":"select_ranges","ranges":[
            {"start":17,"end":21},{"start":1,"end":5},{"start":9,"end":13},
            {"start":2,"end":4}
        ]},
        {"type":"yank","selector":{"type":"visual_selection"},"register":"a"},
        {"type":"cut","selector":{"type":"visual_selection"},"register":"b"},
        {"type":"clear_selection"},
        {"type":"move_frames","forward":true,"count":100},
        {"type":"paste","register":"b","before":false}
    ]);
    project.save_macro('m', instructions)?;
    let output = project.macro_operation(
        json!({"type":"run","register":"m","parent":before.root(),
            "cursor":1,"selected_child":selected,"count":1}),
        false,
    )?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout)?;
    let expected_ranges = json!([
        {"start":2,"end":6},{"start":10,"end":14},{"start":18,"end":22}
    ]);
    for instruction in ["yank", "cut"] {
        let rows = result["trace"]
            .as_array()
            .ok_or("trace")?
            .iter()
            .filter(|row| row["instruction"]["type"] == instruction)
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["resolved_ranges"], expected_ranges);
        assert_eq!(rows[0]["resolved_nodes"], json!([]));
        assert!(rows[0]["resolved_range"].is_null());
        assert!(rows[0]["resolved_selection"].is_null());
        assert_eq!(rows[0]["capture_set"]["parent"], json!(before.root()));
    }
    let after = project.document()?;
    assert_eq!(after.duration()?.frames(), 30);
    let store = ProjectStore::open(&project.package, AccessMode::ReadOnly)?;
    let bank = store.registers()?;
    let mut captures = Vec::new();
    for name in ['a', 'b'] {
        let RegisterValue::EditedSet { set } = bank.entries[&RegisterName::new(name)?].as_ref()
        else {
            return Err("range union register".into());
        };
        assert_eq!(set.revision_id(), before.revision_id());
        assert_eq!(set.scope(), before.root());
        assert_eq!(set.duration().frames(), 12);
        assert_eq!(set.components().len(), 3);
        let source = store.capture_snapshot_at(set.revision_id())?;
        assert_eq!(source, before);
        set.validate_capture(&source)?;
        let first = serde_json::to_value(&set.components()[0])?;
        for component in set.components() {
            component.validate_capture(&source)?;
            let wire = serde_json::to_value(component)?;
            assert_eq!(
                wire["nodes"], first["nodes"],
                "all windows retain the same complete authored source context"
            );
        }
        captures.push(json!({"register":name.to_string(),"capture":set}));
    }
    let history = store.validate_report(true)?;
    assert_eq!(history.verified_by_receipt, 0);
    assert_eq!(history.replayed + 1, history.revisions);
    drop(store);
    success(&[
        "project",
        "undo",
        project.path(),
        "--expected",
        after.revision_id().as_str(),
    ])?;
    let undone = project.document()?;
    same_authored(&undone, &before)?;
    assert_eq!(
        ProjectStore::open(&project.package, AccessMode::ReadOnly)?.registers()?,
        bank
    );
    success(&[
        "project",
        "redo",
        project.path(),
        "--expected",
        undone.revision_id().as_str(),
    ])?;
    let restored = project.document()?;
    same_authored(&restored, &after)?;
    let store = ProjectStore::open(&project.package, AccessMode::ReadOnly)?;
    assert_eq!(store.registers()?, bank);
    let restored_history = store.validate_report(true)?;
    assert_eq!(restored_history.verified_by_receipt, 0);
    assert_eq!(restored_history.replayed + 1, restored_history.revisions);
    drop(store);
    fs::write(
        project.directory.join("range-sets-macro.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    fs::write(
        project.directory.join("range-sets-captures.json"),
        serde_json::to_vec_pretty(&json!({
            "captures":captures,"before_revision":before.revision_id(),
            "macro_revision":after.revision_id(),"render_revision":restored.revision_id(),
            "history_before_navigation":history,"history_after_redo":restored_history
        }))?,
    )?;
    // Literal recipe oracle, independent of inspection output:
    // remaining Original 12..14,18..22,26..30,34..42, then the copied
    // Original 14..18,22..26,30..34. The 20f hull is never substituted.
    let pictures = [
        (12, 14),
        (18, 22),
        (26, 30),
        (34, 42),
        (14, 18),
        (22, 26),
        (30, 34),
    ]
    .into_iter()
    .flat_map(|(start, end)| (start..end).map(original))
    .collect::<Vec<_>>();
    assert_eq!(pictures.len(), 30);
    project.finish(vec!["Discontiguous interval union"],
        pictures.into_iter().enumerate().map(|(index,picture)|(index as u64,picture)).collect(),
        vec!["Qualified Original cfr-bframes.mp4 A/V at 30000/1001. One macro resolves cursor-relative reversed and overlapping inputs to exact Edit [2,6), [10,14), [18,22) windows of the same retained Source context. One frozen 12f capture, descending partial deletions and paste at the ending seam preserve the other 18f. One Undo/Redo and reopened historical captures pass. All 30 pictures use a literal independent Original-ordinal oracle. Encoded audio compares the complete canonical preview bus; this is not an independent PCM or listening claim.".into()])
}
