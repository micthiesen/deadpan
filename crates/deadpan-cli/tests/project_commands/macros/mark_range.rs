use super::*;

#[test]
fn named_mark_range_macro_calls_resolve_staged_endpoints_and_fail_atomically() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = create(scratch.path())?;
    let input = scratch.path().join("named-mark-range.json");
    let initial = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    fs::write(&input, request(&initial)?.to_string())?;
    success(&[
        "command",
        package.to_str().unwrap(),
        "--json",
        input.to_str().unwrap(),
    ])?;
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;

    let start = json!({"id":"native-mark-a"});
    let end = json!({"id":"native-mark-b"});
    let staged_body = json!([
        {"type":"move_frames","forward":true,"count":3},
        {"type":"set_mark","letter":"a"},
        {"type":"move_frames","forward":true,"count":7},
        {"type":"set_mark","letter":"b"},
        {"type":"move_frames","forward":false,"count":7},
        {"type":"select_mark_range","start":start.clone(),"end":end.clone()},
        {"type":"yank","selector":{"type":"mark_range","start":start.clone(),"end":end.clone()},"register":"c"}
    ]);
    invoke(&package, &input, &save(&before, 0, "a", staged_body), false)?;
    let after_save_a = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    invoke(
        &package,
        &input,
        &save(
            &after_save_a,
            1,
            "b",
            json!([{"type":"call","register":"a","count":1}]),
        ),
        false,
    )?;

    let bank = ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?;
    let mut invocation = run(&before, bank.version, "b", 0);
    invocation["operation"]["new_revision"] = json!("named-mark-range-edit");
    let unchanged = state(&package)?;
    let preview = invoke(&package, &input, &invocation, true)?;
    assert_eq!(state(&package)?, unchanged);
    assert!(preview["committed_revision"].is_null());
    assert!(preview["committed_registers"].is_null());
    let trace = preview["trace"]
        .as_array()
        .ok_or("Missing named-call trace")?;
    for instruction in ["select_mark_range", "yank"] {
        let rows: Vec<_> = trace
            .iter()
            .filter(|row| row["instruction"]["type"] == instruction)
            .collect();
        assert_eq!(rows.len(), 1, "{instruction}: {trace:?}");
        assert_eq!(rows[0]["resolved_range"], json!({"start":3,"end":10}));
    }
    assert_eq!(preview["context"]["cursor"], 10);
    assert_eq!(preview["context"]["visual_selection"]["type"], "mark_range");

    let result = invoke(&package, &input, &invocation, false)?;
    assert_eq!(result["committed_revision"], "named-mark-range-edit");
    assert_eq!(
        result["committed_registers"]["bank_version"],
        bank.version + 1
    );
    assert_eq!(result["context"]["cursor"], 10);
    assert_eq!(result["context"]["visual_selection"]["type"], "mark_range");
    let writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let after = writer.snapshot()?;
    assert_ne!(after, before);
    let bank = writer.registers()?;
    let RegisterValue::Edited { slice } = bank.entries[&RegisterName::new('c')?].as_ref() else {
        panic!("named mark range yank")
    };
    assert_eq!(
        slice.selection(),
        &deadpan_core::SliceCaptureSelection::Range {
            range: deadpan_core::FrameRange::new(
                deadpan_core::ProjectFrame(3),
                deadpan_core::ProjectFrame(10),
            )?,
        }
    );
    slice.validate_capture(&writer.capture_snapshot_at(slice.revision_id())?)?;
    writer.checkpoint()?;
    drop(writer);

    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    assert_eq!(writer.registers()?, bank);
    assert_eq!(writer.snapshot()?, after);
    writer.undo(
        after.revision_id(),
        RevisionId::new("named-mark-range-undo")?,
    )?;
    same_document(&writer.snapshot()?, &before)?;
    assert_eq!(writer.registers()?, bank);
    writer.checkpoint()?;
    drop(writer);
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    writer.redo(
        &RevisionId::new("named-mark-range-undo")?,
        RevisionId::new("named-mark-range-redo")?,
    )?;
    same_document(&writer.snapshot()?, &after)?;
    assert_eq!(writer.registers()?, bank);

    let after_redo = writer.snapshot()?;
    drop(writer);
    let selection_macro =
        json!([{"type":"select_mark_range","start":start.clone(),"end":end.clone()}]);
    invoke(
        &package,
        &input,
        &save(&after_redo, bank.version, "s", selection_macro),
        false,
    )?;
    let read = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let selection_bank = read.registers()?;
    let mut select_only = run(&after_redo, selection_bank.version, "s", 0);
    select_only["operation"]["new_revision"] = json!("selection-only-should-not-commit");
    let unchanged = state(&package)?;
    let selected = invoke(&package, &input, &select_only, false)?;
    assert_eq!(state(&package)?, unchanged);
    assert!(selected["committed_revision"].is_null());
    assert!(selected["committed_registers"].is_null());
    assert_eq!(selected["context"]["cursor"], 10);
    assert_eq!(
        selected["context"]["visual_selection"]["type"],
        "mark_range"
    );

    let selection_bank = ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?;
    let reverse = json!([{"type":"select_mark_range","start":end.clone(),"end":start.clone()}]);
    invoke(
        &package,
        &input,
        &save(&after_redo, selection_bank.version, "r", reverse),
        false,
    )?;
    let reverse_bank = ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?;
    let reversed = run(&after_redo, reverse_bank.version, "r", 0);
    let error = reject(&package, &input, &reversed)?;
    assert_eq!(error["error"]["code"], "SelectionUnavailable");

    let failing = json!([
        {"type":"call","register":"a","count":1},
        {"type":"call","register":"z","count":1}
    ]);
    invoke(
        &package,
        &input,
        &save(&after_redo, reverse_bank.version, "f", failing),
        false,
    )?;
    let failure_bank = ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?;
    let failure_request = run(&after_redo, failure_bank.version, "f", 0);
    let error = reject(&package, &input, &failure_request)?;
    assert_eq!(error["error"]["code"], "InvalidCommand");
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("called macro register is empty")
    );
    Ok(())
}
