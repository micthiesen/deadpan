use super::*;
use deadpan_cli::macros::{MAX_REQUEST_BYTES, Request};
use deadpan_core::{RegisterName, RegisterValue, RevisionId};

fn state(package: &Path) -> Result<Vec<String>> {
    let database = rusqlite::Connection::open(package.join("project.sqlite"))?;
    let mut rows = Vec::new();
    for query in [
        "SELECT json_array(id,parent_id,kind,document) FROM revisions ORDER BY id",
        "SELECT json_array(id,parent_id,revision_id,request,edit) FROM history ORDER BY id",
        "SELECT json_array(singleton,head_revision,cursor,workflow) FROM state",
        "SELECT json_array(position,history_id) FROM redo ORDER BY position",
        "SELECT json_array(owner_revision,ordinal,step_revision,document) FROM transaction_steps ORDER BY owner_revision,ordinal",
        "SELECT json_array(singleton,version) FROM register_state",
        "SELECT json_array(name,content_id) FROM registers ORDER BY name",
        "SELECT json_array(id,capture_revision,capture_step,value) FROM register_contents ORDER BY id",
    ] {
        rows.extend(
            database
                .prepare(query)?
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?,
        );
    }
    Ok(rows)
}

fn envelope(document: &ProjectDocument, version: u64, operation: Value) -> Value {
    json!({"protocol":1,"project_id":document.project_id(),
        "expected_revision":document.revision_id(),"expected_bank_version":version,
        "operation":operation})
}

fn save(document: &ProjectDocument, version: u64, register: &str, instructions: Value) -> Value {
    envelope(
        document,
        version,
        json!({"type":"save","register":register,
        "program":{"instructions":instructions}}),
    )
}

fn run(document: &ProjectDocument, version: u64, register: &str, cursor: i64) -> Value {
    envelope(
        document,
        version,
        json!({"type":"run","register":register,
        "parent":document.root(),"cursor":cursor,"count":1,"new_revision":"macro-cut"}),
    )
}

fn invoke(package: &Path, input: &Path, request: &Value, dry_run: bool) -> Result<Value> {
    fs::write(input, request.to_string())?;
    let mut arguments = vec![
        "macro",
        package.to_str().unwrap(),
        "--json",
        input.to_str().unwrap(),
    ];
    if dry_run {
        arguments.push("--dry-run");
    }
    success(&arguments)
}

fn reject(package: &Path, input: &Path, request: &Value) -> Result<Value> {
    fs::write(input, request.to_string())?;
    let before = state(package)?;
    let output = cli(&[
        "macro",
        package.to_str().unwrap(),
        "--json",
        input.to_str().unwrap(),
    ])?;
    assert!(
        !output.status.success(),
        "unexpected success: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(state(package)?, before);
    Ok(serde_json::from_slice(&output.stderr)?)
}

fn same_document(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = json!(actual.revision_id());
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

#[test]
fn macro_cli_saves_previews_moves_and_cuts_as_one_durable_undo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = create(scratch.path())?;
    let input = scratch.path().join("macro.json");
    let initial = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    fs::write(&input, request(&initial)?.to_string())?;
    success(&[
        "command",
        package.to_str().unwrap(),
        "--json",
        input.to_str().unwrap(),
    ])?;
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let original = state(&package)?;
    let motion = save(
        &before,
        0,
        "h",
        json!([{"type":"move_frames","forward":true,"count":100}]),
    );
    let preview = invoke(&package, &input, &motion, true)?;
    assert_eq!(preview["bank_version"], 1);
    assert_eq!(preview["committed"], false);
    assert!(preview["committed_registers"].is_null());
    assert_eq!(state(&package)?, original);
    let saved = invoke(&package, &input, &motion, false)?;
    assert_eq!(saved["committed"], true);
    assert!(saved["committed_revision"].is_null());
    assert_eq!(
        saved["committed_registers"],
        json!({"project_id":before.project_id(),
        "revision_id":before.revision_id(),"bank_version":1})
    );
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
        before
    );
    let motion_state = state(&package)?;
    let moved = invoke(&package, &input, &run(&before, 1, "h", 4), false)?;
    assert_eq!(moved["context"]["cursor"], 45);
    assert_eq!(moved["context"]["selected_child"], "hold");
    assert_eq!(moved["trace"].as_array().unwrap().len(), 2);
    assert_eq!(
        moved["trace"][1]["before_scope"],
        json!({"start":0,"end":45})
    );
    assert_eq!(moved["committed"], false);
    assert!(moved["edit"].is_null());
    assert!(moved["committed_registers"].is_null());
    assert_eq!(state(&package)?, motion_state);

    let cuts = save(
        &before,
        1,
        "a",
        json!([
            {"type":"cut_frames","operation":{"count":5},"register":"b"},
            {"type":"move_frames","forward":true,"count":2},
            {"type":"cut_frames","operation":{"count":3},"register":"c"},
        ]),
    );
    invoke(&package, &input, &cuts, false)?;
    let cut_request = run(&before, 2, "a", 4);
    let pre_cut = state(&package)?;
    let preview = invoke(&package, &input, &cut_request, true)?;
    assert_eq!(preview["edit"]["duration_delta"], -8);
    assert_eq!(preview["context"]["cursor"], 6);
    assert_eq!(
        preview["trace"][1]["resolved_range"],
        json!({"start":4,"end":9})
    );
    assert_eq!(
        preview["trace"][3]["resolved_range"],
        json!({"start":6,"end":9})
    );
    assert_eq!(
        preview["trace"][3]["before_scope"],
        json!({"start":0,"end":40})
    );
    assert_eq!(state(&package)?, pre_cut);
    let result = invoke(&package, &input, &cut_request, false)?;
    assert_eq!(
        result["edit"]["duration_delta"],
        preview["edit"]["duration_delta"]
    );
    assert_eq!(result["context"]["cursor"], preview["context"]["cursor"]);
    assert_eq!(result["context"]["parent"], preview["context"]["parent"]);
    assert_eq!(result["committed_revision"], "macro-cut");
    assert_eq!(result["committed_registers"]["revision_id"], "macro-cut");
    assert_eq!(result["committed_registers"]["bank_version"], 3);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let after = store.snapshot()?;
    assert_eq!(after.duration()?.frames(), 37);
    let selected: NodeId = serde_json::from_value(result["context"]["selected_child"].clone())?;
    let deadpan_core::NodeKind::Sequence { children } = &after.nodes()[after.root()].kind else {
        panic!("root must stay a Sequence")
    };
    let index = children
        .iter()
        .position(|child| child == &selected)
        .unwrap();
    let cursor = result["context"]["cursor"].as_i64().unwrap();
    assert!(after.source_splice_boundary(after.root(), index)?.0 <= cursor);
    assert!(cursor < after.source_splice_boundary(after.root(), index + 1)?.0);
    let bank = store.registers()?;
    assert_eq!(bank.version, 3);
    assert_eq!(bank.entries.len(), 5);
    let RegisterValue::Edited { slice: first } = bank.entries[&RegisterName::new('b')?].as_ref()
    else {
        panic!("first cut missing")
    };
    let RegisterValue::Edited { slice: second } = bank.entries[&RegisterName::new('c')?].as_ref()
    else {
        panic!("second cut missing")
    };
    assert_eq!(first.revision_id(), before.revision_id());
    assert_ne!(second.revision_id(), before.revision_id());
    assert_eq!(
        bank.entries[&RegisterName::unnamed()],
        bank.entries[&RegisterName::new('c')?]
    );
    first.validate_capture(&store.capture_snapshot_at(first.revision_id())?)?;
    second.validate_capture(&store.capture_snapshot_at(second.revision_id())?)?;
    store.validate()?;
    drop(store);
    let inspected = success(&["macro", "inspect", package.to_str().unwrap()])?;
    assert_eq!(inspected["revision_id"], "macro-cut");
    assert_eq!(inspected["bank_version"], 3);
    let selected = success(&[
        "macro",
        "inspect",
        package.to_str().unwrap(),
        "--register",
        "a",
    ])?;
    assert_eq!(
        selected["registers"][0]["program"],
        cuts["operation"]["program"]
    );
    assert!(selected["registers"][0].get("capture_revision").is_none());
    let database = rusqlite::Connection::open(package.join("project.sqlite"))?;
    let counts: (i64, i64) = database.query_row(
        "SELECT (SELECT count(*) FROM history), (SELECT count(*) FROM transaction_steps)",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(counts, (2, 2));
    reject(&package, &input, &cut_request)?;
    success(&[
        "project",
        "undo",
        package.to_str().unwrap(),
        "--expected",
        "macro-cut",
    ])?;
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let undone = store.snapshot()?;
    same_document(&undone, &before)?;
    assert_eq!(store.registers()?, bank);
    drop(store);
    success(&[
        "project",
        "redo",
        package.to_str().unwrap(),
        "--expected",
        undone.revision_id().as_str(),
    ])?;
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    same_document(&store.snapshot()?, &after)?;
    assert_eq!(store.registers()?, bank);
    store.validate()?;
    Ok(())
}

#[test]
fn macro_cli_late_failures_guards_and_request_limits_leave_no_writes() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = create(scratch.path())?;
    let input = scratch.path().join("macro.json");
    let initial = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    fs::write(&input, request(&initial)?.to_string())?;
    success(&[
        "command",
        package.to_str().unwrap(),
        "--json",
        input.to_str().unwrap(),
    ])?;
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let late_failure = save(
        &before,
        0,
        "a",
        json!([
            {"type":"cut_frames","operation":{"count":2},"register":"b"},
            {"type":"call","register":"z","count":1},
        ]),
    );
    // Saving validates the body but does not execute an unavailable call.
    invoke(&package, &input, &late_failure, true)?;
    invoke(&package, &input, &late_failure, false)?;
    let error = reject(&package, &input, &run(&before, 1, "a", 0))?;
    assert_eq!(error["error"]["code"], "InvalidCommand");
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("called macro register is empty")
    );
    let valid_body = save(
        &before,
        1,
        "a",
        json!([{"type":"move_frames","forward":true,"count":2}]),
    );
    invoke(&package, &input, &valid_body, false)?;
    let valid = run(&before, 2, "a", 0);
    assert_eq!(
        invoke(&package, &input, &valid, true)?["context"]["cursor"],
        2
    );
    for (field, value, code, message) in [
        (
            "protocol",
            json!(2),
            "ProtocolUnsupported",
            "Macro protocol must be 1",
        ),
        (
            "expected_revision",
            json!("stale"),
            "RevisionConflict",
            "expected revision is no longer current",
        ),
        (
            "expected_bank_version",
            json!(0),
            "RegisterInvalid",
            "Register bank version changed",
        ),
        (
            "project_id",
            json!("foreign"),
            "HostProjectChanged",
            "different project",
        ),
        (
            "unknown",
            json!(true),
            "HostProtocolInvalid",
            "unknown field",
        ),
    ] {
        let mut request = valid.clone();
        request[field] = value;
        let error = reject(&package, &input, &request)?;
        assert_eq!(error["error"]["code"], code, "{field}");
        assert!(
            error["error"]["message"]
                .as_str()
                .unwrap()
                .contains(message),
            "{error}"
        );
    }
    for (field, value, code) in [
        ("register", json!("\""), "InvalidCommand"),
        ("register", json!("A"), "HostProtocolInvalid"),
        ("register", json!("ab"), "HostProtocolInvalid"),
        ("count", json!(0), "HostProtocolInvalid"),
        ("count", json!(u32::MAX), "LimitExceeded"),
        ("parent", json!("hold"), "InvalidCommand"),
        ("cursor", json!(46), "SelectionUnavailable"),
        ("unknown", json!(true), "HostProtocolInvalid"),
    ] {
        let mut request = valid.clone();
        request["operation"][field] = value;
        assert_eq!(
            reject(&package, &input, &request)?["error"]["code"],
            code,
            "{field}"
        );
    }
    let mut omitted = valid.clone();
    omitted["operation"]
        .as_object_mut()
        .unwrap()
        .remove("cursor");
    assert_eq!(
        reject(&package, &input, &omitted)?["error"]["code"],
        "HostProtocolInvalid"
    );
    for instructions in [
        json!([]),
        json!([{"type":"move_frames","forward":true,"count":0}]),
        json!([{"type":"call","register":"\"","count":1}]),
        json!([{"type":"move_frames","forward":true,"count":1,"unknown":true}]),
        json!(vec![
            json!({"type":"move_frames","forward":true,"count":1});
            1025
        ]),
    ] {
        assert_eq!(
            reject(&package, &input, &save(&before, 2, "b", instructions))?["error"]["code"],
            "HostProtocolInvalid"
        );
    }
    let mut oversized = vec![b' '; MAX_REQUEST_BYTES + 1];
    oversized[..2].copy_from_slice(b"{}");
    fs::write(&input, oversized)?;
    let rows = state(&package)?;
    let result = cli(&[
        "macro",
        package.to_str().unwrap(),
        "--json",
        input.to_str().unwrap(),
    ])?;
    assert!(!result.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&result.stderr)?["error"]["code"],
        "LimitExceeded"
    );
    assert_eq!(state(&package)?, rows);

    // The request's dry-run bit remains authoritative without a CLI flag.
    let mut dry = save(
        &before,
        2,
        "h",
        json!([{"type":"move_frames","forward":false,"count":2}]),
    );
    dry["dry_run"] = json!(true);
    assert_eq!(invoke(&package, &input, &dry, false)?["committed"], false);
    assert_eq!(state(&package)?, rows);

    // Prepared execution rechecks the live bank at commit, including pure motion.
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let motion: Request = serde_json::from_value(save(
        &before,
        2,
        "h",
        json!([{"type":"move_frames","forward":true,"count":2}]),
    ))?;
    let prepared = deadpan_cli::macros::prepare(&writer, &motion)?;
    let saved = deadpan_cli::macros::commit(&mut writer, &prepared)?;
    assert_eq!(saved.committed_registers.unwrap().bank_version, 3);
    let motion_run: Request = serde_json::from_value(run(&before, 3, "h", 0))?;
    let prepared = deadpan_cli::macros::prepare(&writer, &motion_run)?;
    let another: Request = serde_json::from_value(save(
        &before,
        3,
        "i",
        json!([{"type":"move_frames","forward":false,"count":2}]),
    ))?;
    let next = deadpan_cli::macros::prepare(&writer, &another)?;
    deadpan_cli::macros::commit(&mut writer, &next)?;
    let rows = state(&package)?;
    assert!(deadpan_cli::macros::commit(&mut writer, &prepared).is_err());
    assert_eq!(state(&package)?, rows);
    assert_eq!(
        writer.snapshot()?.revision_id(),
        &RevisionId::new("after-hold")?
    );
    Ok(())
}
