//! Public CLI union selection, inspection and atomic durable editing.
use deadpan_core::{
    BeatNode, Command, FrameDuration, HoldAudio, HoldRecipe, HoldVideo, NodeId, NodeKind,
    ProjectDocument, RegisterName, RegisterValue, SliceCaptureSelection, Subtree,
};
use deadpan_store::{AccessMode, ProjectStore};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command as Process, Output},
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn name(value: char) -> RegisterName {
    RegisterName::new(value).unwrap()
}
fn hold(label: &str, frames: i64) -> BeatNode {
    BeatNode::hold(
        label,
        HoldRecipe {
            picture_context: None,
            duration: FrameDuration::new(frames).unwrap(),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}
fn cli(arguments: &[&str]) -> Result<Output> {
    let mut command = Process::new(env!("CARGO_BIN_EXE_deadpan-cli"));
    command
        .args(arguments)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    Ok(deadpan_native_process::spawn(&mut command)?.wait_with_output()?)
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
fn snapshot(package: &Path) -> Result<ProjectDocument> {
    Ok(ProjectStore::open(package, AccessMode::ReadOnly)?.snapshot()?)
}
fn command(package: &Path, input: &Path, id: &str, command: Command) -> Result<Value> {
    let before = snapshot(package)?;
    fs::write(
        input,
        serde_json::to_vec(&json!({"protocol":1,"project_id":before.project_id(),
        "expected_revision":before.revision_id(),"new_revision":id,"command":command}))?,
    )?;
    success(&[
        "command",
        package.to_str().unwrap(),
        "--json",
        input.to_str().unwrap(),
    ])
}
fn seed(package: &Path, input: &Path) -> Result<ProjectDocument> {
    success(&[
        "project",
        "create",
        package.to_str().unwrap(),
        "--fps",
        "30000/1001",
        "--size",
        "16x16",
    ])?;
    let empty = snapshot(package)?;
    command(
        package,
        input,
        "seed-node-set",
        Command::Insert {
            parent: empty.root().clone(),
            index: 0,
            subtree: Subtree {
                root: node("scope"),
                nodes: BTreeMap::from([
                    (
                        node("scope"),
                        BeatNode::sequence(
                            "Retained navigation group",
                            vec![
                                node("lead"),
                                node("group-a"),
                                node("gap"),
                                node("group-b"),
                                node("tail"),
                            ],
                        ),
                    ),
                    (node("lead"), hold("Lead", 3)),
                    (
                        node("group-a"),
                        BeatNode::sequence("First actual parent", vec![node("empty-a"), node("a")]),
                    ),
                    (node("empty-a"), BeatNode::sequence("Empty A", vec![])),
                    (node("a"), hold("Whole A", 4)),
                    (node("gap"), hold("Unselected middle", 2)),
                    (
                        node("group-b"),
                        BeatNode::sequence(
                            "Second actual parent",
                            vec![node("b"), node("empty-b")],
                        ),
                    ),
                    (node("b"), hold("Whole B", 6)),
                    (node("empty-b"), BeatNode::sequence("Empty B", vec![])),
                    (node("tail"), hold("Tail", 3)),
                ]),
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        },
    )?;
    let document = snapshot(package)?;
    assert_eq!(document.duration()?.frames(), 18);
    Ok(document)
}
fn cells(package: &Path) -> Result<Vec<String>> {
    let connection = Connection::open(package.join("project.sqlite"))?;
    let mut result = Vec::new();
    for query in [
        "SELECT json_array(id,parent_id,kind,document) FROM revisions ORDER BY id",
        "SELECT json_array(id,parent_id,revision_id,request,edit) FROM history ORDER BY id",
        "SELECT json_array(singleton,head_revision,cursor,workflow) FROM state",
        "SELECT json_array(position,history_id) FROM redo ORDER BY position",
        "SELECT json_array(owner_revision,ordinal,step_revision,document) FROM transaction_steps ORDER BY owner_revision,ordinal",
        "SELECT json_array(singleton,version,bank_digest) FROM register_state",
        "SELECT json_array(name,content_id) FROM registers ORDER BY name",
        "SELECT json_array(id,capture_revision,capture_step,value) FROM register_contents ORDER BY id",
    ] {
        result.extend(
            connection
                .prepare(query)?
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?,
        );
    }
    Ok(result)
}
fn invocation(package: &Path, instructions: Value, id: &str) -> Result<Value> {
    let store = ProjectStore::open(package, AccessMode::ReadOnly)?;
    let document = store.snapshot()?;
    let version = store.registers()?.version;
    Ok(
        json!({"protocol":1,"project_id":document.project_id(),"expected_revision":document.revision_id(),
        "expected_bank_version":version,"operation":{"type":"apply","program":{"instructions":instructions},
        "parent":document.root(),"cursor":17,"selected_child":"scope","new_revision":id}}),
    )
}
fn invoke(package: &Path, input: &Path, request: &Value, dry_run: bool) -> Result<Value> {
    fs::write(input, serde_json::to_vec(request)?)?;
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
fn rejected(package: &Path, input: &Path, request: &Value) -> Result<Value> {
    fs::write(input, serde_json::to_vec(request)?)?;
    let unchanged = cells(package)?;
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
    assert_eq!(cells(package)?, unchanged);
    Ok(serde_json::from_slice(&output.stderr)?)
}
fn selector() -> Value {
    json!({"type":"nodes","nodes":["b","empty-b","a","empty-a"]})
}
fn assert_trace(row: &Value, parent: &NodeId) {
    assert!(
        row["resolved_range"].is_null(),
        "a union has no contiguous envelope authority"
    );
    assert!(row["resolved_selection"].is_null());
    assert_eq!(row["capture_set"]["parent"], json!(parent));
    let members = row["resolved_nodes"].as_array().unwrap();
    assert_eq!(
        members
            .iter()
            .map(|member| member["node"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["empty-a", "a", "b", "empty-b"]
    );
    assert_eq!(
        members
            .iter()
            .map(|member| member["parent"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["group-a", "group-a", "group-b", "group-b"]
    );
    assert_eq!(
        row["resolved_ranges"],
        json!([
            {"start":3,"end":3},{"start":3,"end":7},{"start":9,"end":15},{"start":15,"end":15},
        ])
    );
}
fn same_authored(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = json!(actual.revision_id());
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

#[test]
fn nodes_cli_reports_the_real_union_and_round_trips_one_cut_paste_transaction() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("nodes.deadpan");
    let input = scratch.path().join("nodes.json");
    let initial = seed(&package, &input)?;
    command(
        &package,
        &input,
        "temporary-tail",
        Command::Rename {
            node: node("tail"),
            label: "Temporary".into(),
        },
    )?;
    success(&[
        "project",
        "undo",
        package.to_str().unwrap(),
        "--expected",
        "temporary-tail",
    ])?;
    let before = snapshot(&package)?;
    same_authored(&before, &initial)?;
    assert!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?
            .history_availability()?
            .1
    );
    let mut copy = invocation(
        &package,
        json!([{"type":"yank","selector":selector(),"register":"a"}]),
        "copy-no-history",
    )?;
    copy["operation"]["visual_selection"] =
        json!({"type":"time","anchor":0,"head":1,"extending":false});
    let unchanged = cells(&package)?;
    let preview = invoke(&package, &input, &copy, true)?;
    assert_eq!(cells(&package)?, unchanged);
    assert_trace(&preview["trace"][0], before.root());
    let copied = invoke(&package, &input, &copy, false)?;
    assert!(copied["committed_revision"].is_null());
    assert_eq!(snapshot(&package)?, before);
    assert_eq!(copied["committed_registers"]["bank_version"], 1);
    let inspected = success(&[
        "macro",
        "inspect",
        package.to_str().unwrap(),
        "--register",
        "a",
    ])?;
    let entry = &inspected["registers"][0];
    assert_eq!(entry["type"], "edited_set");
    assert_eq!(entry["capture_revision"], json!(before.revision_id()));
    assert_eq!(entry["scope"], json!(before.root()));
    assert_eq!(entry["duration"], 10);
    assert_eq!(entry["components"].as_array().unwrap().len(), 4);
    assert_eq!(
        entry["components"][0]["selection"],
        json!({"type":"child","node":"empty-a"})
    );
    let bank = ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?;
    assert_eq!(
        bank.entries[&name('a')],
        bank.entries[&RegisterName::unnamed()]
    );
    let original_copy = bank.entries[&name('a')].clone();
    success(&[
        "project",
        "redo",
        package.to_str().unwrap(),
        "--expected",
        before.revision_id().as_str(),
    ])?;
    let redone = snapshot(&package)?;
    success(&[
        "project",
        "undo",
        package.to_str().unwrap(),
        "--expected",
        redone.revision_id().as_str(),
    ])?;
    let before_edit = snapshot(&package)?;
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?,
        bank
    );
    let edit = invocation(
        &package,
        json!([
            {"type":"cut","selector":selector(),"register":"b"},
            {"type":"paste","register":"b","before":true}
        ]),
        "node-union-cut-paste",
    )?;
    let unchanged = cells(&package)?;
    let preview = invoke(&package, &input, &edit, true)?;
    assert_eq!(cells(&package)?, unchanged);
    assert_trace(&preview["trace"][0], before_edit.root());
    assert_eq!(preview["edit"]["duration_delta"], 0);
    let result = invoke(&package, &input, &edit, false)?;
    assert_eq!(result["committed_revision"], "node-union-cut-paste");
    let after = snapshot(&package)?;
    assert_eq!(after.duration()?.frames(), 18);
    for id in ["empty-a", "a", "b", "empty-b"] {
        assert!(!after.nodes().contains_key(&node(id)));
    }
    assert_eq!(
        after.nodes()[&node("gap")],
        before_edit.nodes()[&node("gap")]
    );
    let NodeKind::Sequence { children } = &after.nodes()[after.root()].kind else {
        panic!("root")
    };
    assert_eq!(children.len(), 2);
    assert_eq!(children[1], node("scope"));
    let NodeKind::Sequence {
        children: copied_children,
    } = &after.nodes()[&children[0]].kind
    else {
        panic!("copied union group")
    };
    assert_eq!(copied_children.len(), 4);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let bank = store.registers()?;
    let RegisterValue::EditedSet { set } = bank.entries[&name('b')].as_ref() else {
        panic!("union register")
    };
    assert_eq!(set.components().len(), 4);
    assert_eq!(set.revision_id(), before_edit.revision_id());
    for (component, id) in set
        .components()
        .iter()
        .zip(["empty-a", "a", "b", "empty-b"])
    {
        assert_eq!(
            component.selection(),
            &SliceCaptureSelection::Child { node: node(id) }
        );
        component.validate_capture(&store.capture_snapshot_at(component.revision_id())?)?;
    }
    assert_eq!(bank.entries[&name('a')], original_copy);
    let history = store.validate_report(true)?;
    assert_eq!(history.verified_by_receipt, 0);
    assert_eq!(history.replayed + 1, history.revisions);
    drop(store);
    success(&[
        "project",
        "undo",
        package.to_str().unwrap(),
        "--expected",
        after.revision_id().as_str(),
    ])?;
    let undone = snapshot(&package)?;
    same_authored(&undone, &before_edit)?;
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?,
        bank
    );
    success(&[
        "project",
        "redo",
        package.to_str().unwrap(),
        "--expected",
        undone.revision_id().as_str(),
    ])?;
    same_authored(&snapshot(&package)?, &after)?;
    let reopened = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_eq!(reopened.registers()?, bank);
    let history = reopened.validate_report(true)?;
    assert_eq!(history.verified_by_receipt, 0);
    assert_eq!(history.replayed + 1, history.revisions);
    Ok(())
}

#[test]
fn nodes_cli_rejects_bad_members_and_late_failures_without_any_sql_publication() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("node-refusals.deadpan");
    let input = scratch.path().join("node-refusals.json");
    seed(&package, &input)?;
    let before = cells(&package)?;
    for nodes in [
        json!(["a", "missing"]),
        json!(["a", "a"]),
        json!(["group-a", "a"]),
        json!(["a", "root"]),
    ] {
        let request = invocation(
            &package,
            json!([
                {"type":"yank","selector":{"type":"nodes","nodes":["a"]},"register":"c"},
                {"type":"cut","selector":{"type":"nodes","nodes":nodes},"register":"b"}
            ]),
            "refused-members",
        )?;
        rejected(&package, &input, &request)?;
        assert_eq!(cells(&package)?, before);
    }
    let mut outside = invocation(
        &package,
        json!([
            {"type":"yank","selector":{"type":"nodes","nodes":["a"]},"register":"c"},
            {"type":"cut","selector":{"type":"nodes","nodes":["b"]},"register":"b"}
        ]),
        "refused-outside",
    )?;
    outside["operation"]["parent"] = json!("group-a");
    outside["operation"]["selected_child"] = json!("a");
    outside["operation"]["cursor"] = json!(6);
    rejected(&package, &input, &outside)?;
    let late = invocation(
        &package,
        json!([
            {"type":"cut","selector":selector(),"register":"b"},
            {"type":"paste","register":"b","before":true},
            {"type":"call","register":"z","count":1}
        ]),
        "refused-late-call",
    )?;
    let error = rejected(&package, &input, &late)?;
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("called macro register is empty")
    );
    assert_eq!(cells(&package)?, before);
    assert!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?
            .registers()?
            .entries
            .is_empty()
    );
    Ok(())
}

#[test]
fn ranges_cli_normalizes_relative_union_and_round_trips_one_cut_paste_transaction() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("ranges.deadpan");
    let input = scratch.path().join("ranges.json");
    seed(&package, &input)?;
    // Partial endpoints retain the existing direct-child boundary contract.
    // Publicly enter that flat editing structure before selecting intervals.
    for id in ["group-a", "group-b", "scope"] {
        command(
            &package,
            &input,
            &format!("ungroup-{id}"),
            Command::Ungroup { node: node(id) },
        )?;
    }
    let before = snapshot(&package)?;
    let ranges = json!({"type":"ranges","ranges":[
        {"start":8,"end":10},{"start":0,"end":2},{"start":1,"end":2}
    ]});
    let mut copy = invocation(
        &package,
        json!([{"type":"yank","selector":ranges,"register":"a"}]),
        "copy-ranges-no-history",
    )?;
    copy["operation"]["cursor"] = json!(3);
    copy["operation"]["selected_child"] = json!("a");
    copy["operation"]["visual_selection"] =
        json!({"type":"time","anchor":0,"head":1,"extending":false});
    let unchanged = cells(&package)?;
    let preview = invoke(&package, &input, &copy, true)?;
    assert_eq!(cells(&package)?, unchanged);
    let trace = &preview["trace"][0];
    assert!(trace["resolved_range"].is_null());
    assert!(trace["resolved_selection"].is_null());
    assert_eq!(trace["resolved_nodes"], json!([]));
    assert_eq!(
        trace["resolved_ranges"],
        json!([
            {"start":3,"end":5},{"start":11,"end":13}
        ])
    );
    assert_eq!(trace["capture_set"]["parent"], json!(before.root()));
    let copied = invoke(&package, &input, &copy, false)?;
    assert!(copied["committed_revision"].is_null());
    assert_eq!(snapshot(&package)?, before);
    let inspected = success(&[
        "macro",
        "inspect",
        package.to_str().unwrap(),
        "--register",
        "a",
    ])?;
    let entry = &inspected["registers"][0];
    assert_eq!(entry["type"], "edited_set");
    assert_eq!(entry["duration"], 4);
    assert_eq!(entry["components"].as_array().unwrap().len(), 2);
    assert_eq!(
        entry["components"][0]["selection"],
        json!({"type":"range","range":{"start":3,"end":5}})
    );
    assert_eq!(
        entry["components"][1]["selection"],
        json!({"type":"range","range":{"start":11,"end":13}})
    );
    let mut edit = invocation(
        &package,
        json!([
            {"type":"cut","selector":ranges,"register":"b"},
            {"type":"move_frames","forward":true,"count":100},
            {"type":"paste","register":"b","before":false}
        ]),
        "range-union-cut-paste",
    )?;
    edit["operation"]["cursor"] = json!(3);
    edit["operation"]["selected_child"] = json!("a");
    let unchanged = cells(&package)?;
    let dry = invoke(&package, &input, &edit, true)?;
    assert_eq!(cells(&package)?, unchanged);
    assert_eq!(dry["edit"]["duration_delta"], 0);
    let result = invoke(&package, &input, &edit, false)?;
    assert_eq!(result["committed_revision"], "range-union-cut-paste");
    let after = snapshot(&package)?;
    assert_eq!(after.duration()?.frames(), 18);
    assert_eq!(after.nodes()[&node("gap")], before.nodes()[&node("gap")]);
    let NodeKind::Sequence { children } = &after.nodes()[after.root()].kind else {
        panic!("ordinary root")
    };
    let copied_group = children.last().ok_or("pasted union")?;
    let NodeKind::Sequence {
        children: components,
    } = &after.nodes()[copied_group].kind
    else {
        panic!("copied union group")
    };
    assert_eq!(components.len(), 2);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let bank = store.registers()?;
    let RegisterValue::EditedSet { set } = bank.entries[&name('b')].as_ref() else {
        panic!("range union register")
    };
    assert_eq!(set.revision_id(), before.revision_id());
    assert_eq!(set.duration().frames(), 4);
    assert_eq!(set.components().len(), 2);
    set.validate_capture(&store.capture_snapshot_at(set.revision_id())?)?;
    let validation = store.validate_report(true)?;
    assert_eq!(validation.verified_by_receipt, 0);
    assert_eq!(validation.replayed + 1, validation.revisions);
    drop(store);
    success(&[
        "project",
        "undo",
        package.to_str().unwrap(),
        "--expected",
        after.revision_id().as_str(),
    ])?;
    let undone = snapshot(&package)?;
    same_authored(&undone, &before)?;
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?,
        bank
    );
    success(&[
        "project",
        "redo",
        package.to_str().unwrap(),
        "--expected",
        undone.revision_id().as_str(),
    ])?;
    same_authored(&snapshot(&package)?, &after)?;
    let reopened = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_eq!(reopened.registers()?, bank);
    assert_eq!(reopened.validate_report(true)?.verified_by_receipt, 0);
    Ok(())
}

#[test]
fn ranges_cli_rejects_invalid_offsets_and_late_failures_without_sql_publication() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("range-refusals.deadpan");
    let input = scratch.path().join("range-refusals.json");
    seed(&package, &input)?;
    let before = cells(&package)?;
    for ranges in [
        json!([]),
        json!([{"start":0,"end":0}]),
        json!([{"start":3,"end":2}]),
        json!([{"start":-4,"end":1}]),
        json!([{"start":0,"end":16}]),
        json!([{"start":0,"end":i64::MAX}]),
        json!([{"start":0,"end":1,"unexpected":true}]),
        json!(vec![json!({"start":0,"end":1}); 65]),
    ] {
        let mut request = invocation(
            &package,
            json!([
                {"type":"yank","selector":{"type":"ranges","ranges":[{"start":0,"end":1}]},"register":"c"},
                {"type":"cut","selector":{"type":"ranges","ranges":ranges},"register":"b"}
            ]),
            "refused-range-offsets",
        )?;
        request["operation"]["cursor"] = json!(3);
        request["operation"]["parent"] = json!("group-a");
        request["operation"]["selected_child"] = json!("a");
        rejected(&package, &input, &request)?;
        assert_eq!(cells(&package)?, before);
    }
    let mut outside = invocation(
        &package,
        json!([
            {"type":"yank","selector":{"type":"ranges","ranges":[{"start":0,"end":1}]},"register":"c"},
            {"type":"cut","selector":{"type":"ranges","ranges":[{"start":0,"end":1},{"start":8,"end":9}]},"register":"b"}
        ]),
        "refused-range-scope",
    )?;
    outside["operation"]["parent"] = json!("group-a");
    outside["operation"]["selected_child"] = json!("a");
    outside["operation"]["cursor"] = json!(3);
    rejected(&package, &input, &outside)?;
    assert_eq!(cells(&package)?, before);
    let mut no_scalar = invocation(
        &package,
        json!([
            {"type":"select_ranges","ranges":[{"start":0,"end":1},{"start":3,"end":4}]},
            {"type":"group","selector":{"type":"visual_selection"},"label":"Refused envelope"}
        ]),
        "refused-range-envelope",
    )?;
    no_scalar["operation"]["parent"] = json!("group-b");
    no_scalar["operation"]["selected_child"] = json!("b");
    no_scalar["operation"]["cursor"] = json!(9);
    let error = rejected(&package, &input, &no_scalar)?;
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("does not accept a discontiguous set")
    );
    assert_eq!(cells(&package)?, before);
    Ok(())
}
