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
                overrides: Default::default(),
            root: node.clone(), nodes: BTreeMap::from([(node, BeatNode::hold("Silence", HoldRecipe {
                duration: FrameDuration::new(45)?, video: HoldVideo::Background, audio: HoldAudio::Silence,
            }))]),
        }},
    }))
}

#[test]
fn headless_migration_and_plan_inspection_are_explicit_and_read_only() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("legacy.deadpan");
    fs::create_dir(&package)?;
    fs::create_dir(package.join("Snapshots"))?;
    let connection = rusqlite::Connection::open(package.join("project.sqlite"))?;
    connection.pragma_update(None, "foreign_keys", false)?;
    connection.execute_batch(include_str!(
        "../../deadpan-store/tests/fixtures/v1-history.sql"
    ))?;
    drop(connection);
    let path = package.to_str().unwrap();
    let old = cli(&["project", "validate", path])?;
    assert!(!old.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&old.stderr)?["error"]["code"],
        "MigrationRequired"
    );
    let outcome = success(&["project", "migrate", path])?;
    assert_eq!(outcome["migration"]["from_schema"], 1);
    assert_eq!(outcome["migration"]["to_schema"], 8);
    assert!(Path::new(outcome["migration"]["backup"].as_str().unwrap()).is_file());
    let writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let before = writer.snapshot()?;
    let plan = success(&["inspect-plan", path])?;
    assert_eq!(
        plan["plan"]["metadata"]["revision_id"],
        before.revision_id().as_str()
    );
    assert_eq!(plan["plan"]["metadata"]["duration"], 26);
    let first = success(&["inspect-plan", path, "--frame", "0"])?;
    assert_eq!(first["sample"]["picture"]["type"], "background");
    assert_eq!(
        first["sample"]["instance"]["repeats"][0]["iteration"]["ordinal"],
        0
    );
    let gap = success(&["inspect-plan", path, "--frame", "12"])?;
    assert_eq!(gap["sample"]["gap_after"]["allocation"], "v1-wrap");
    let second = success(&["inspect-plan", path, "--frame", "14"])?;
    assert_eq!(
        second["sample"]["instance"]["repeats"][0]["iteration"]["ordinal"],
        1
    );
    for boundary in ["-1", "26"] {
        let error = cli(&["inspect-plan", path, "--frame", boundary])?;
        assert!(!error.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&error.stderr)?["error"]["code"],
            "FrameOutOfRange"
        );
    }
    assert_eq!(writer.snapshot()?, before);
    Ok(())
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
fn current_generation_requires_host_reconciliation_but_allows_cli_preview() -> Result {
    use deadpan_store::generation::GenerationRequestInput;

    let scratch = tempfile::tempdir()?;
    let package = create(scratch.path())?;
    let path = package.to_str().unwrap();
    let document = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let input = scratch.path().join("command.json");
    fs::write(&input, serde_json::to_vec(&request(&document)?)?)?;
    success(&["command", path, "--json", input.to_str().unwrap()])?;

    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let before = store.snapshot()?;
    let generation = store.allocate_generation_request(GenerationRequestInput {
        request_id: serde_json::from_value(json!("pending-generation"))?,
        expected_revision: before.revision_id().clone(),
        hold_id: NodeId::new("hold")?,
        context_sha256: serde_json::from_value(json!("a".repeat(64)))?,
        constraints: serde_json::from_value(json!({
            "video": {"frames": 45, "frame_rate": before.presentation_basis().frame_rate,
                      "width": 768, "height": 320},
            "conditioning": "bridge", "motion": "still"
        }))?,
        provider: serde_json::from_value(json!({
            "pack_id": "fixture", "pack_version": "1", "runtime_id": "fixture",
            "runtime_version": "1", "seed": 7
        }))?,
    })?;
    drop(store);
    fs::write(
        &input,
        serde_json::to_vec(&json!({
            "protocol": 1, "project_id": before.project_id(),
            "expected_revision": before.revision_id(), "new_revision": "renamed",
            "command": {"command": "rename", "node": "hold", "label": "Renamed"}
        }))?,
    )?;
    assert_eq!(
        success(&[
            "command",
            path,
            "--json",
            input.to_str().unwrap(),
            "--dry-run"
        ])?["committed"],
        false
    );
    for arguments in [
        vec!["command", path, "--json", input.to_str().unwrap()],
        vec![
            "project",
            "undo",
            path,
            "--expected",
            before.revision_id().as_str(),
        ],
    ] {
        let output = cli(&arguments)?;
        assert!(!output.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stderr)?["error"]["code"],
            "GenerationRelevanceRequired"
        );
    }
    let reader = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_eq!(reader.snapshot()?, before);
    assert_eq!(reader.current_generation_requests()?, vec![generation]);
    Ok(())
}

#[test]
fn failed_migration_returns_retained_backup_and_preserves_original() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("damaged.deadpan");
    fs::create_dir(&package)?;
    fs::create_dir(package.join("Snapshots"))?;
    let database = package.join("project.sqlite");
    let connection = rusqlite::Connection::open(&database)?;
    connection.pragma_update(None, "foreign_keys", false)?;
    connection.execute_batch(include_str!(
        "../../deadpan-store/tests/fixtures/v1-history.sql"
    ))?;
    connection.execute_batch(
        "UPDATE history SET edit=json_set(edit,'$.duration_delta',999) WHERE revision_id='v1-wrap'",
    )?;
    drop(connection);
    let before = fs::read(&database)?;
    let output = cli(&["project", "migrate", package.to_str().unwrap()])?;
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let report: Value = serde_json::from_slice(&output.stderr)?;
    assert_eq!(report["error"]["code"], "MigrationFailed");
    let backup = Path::new(report["error"]["recovery_backup"].as_str().unwrap());
    assert!(backup.is_file());
    assert_eq!(fs::read(database)?, before);
    assert_eq!(
        rusqlite::Connection::open(backup)?
            .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
        1
    );
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
        "label": "Overflow", "kind": {"type": "repeat", "child": "hold", "iterations": {"runs": [{"allocation": "overflow", "first": 0, "count": 2}]}, "gap": null}
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

#[test]
fn selection_resolution_is_revision_checked_exact_and_read_only() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = create(scratch.path())?;
    let path = package.to_str().unwrap();
    let document = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let insert_path = scratch.path().join("insert.json");
    fs::write(&insert_path, request(&document)?.to_string())?;
    success(&["command", path, "--json", insert_path.to_str().unwrap()])?;
    let writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let before = writer.snapshot()?;
    let selection_path = scratch.path().join("selection.json");
    let point = |numerator: &str| {
        json!({
            "boundary": { "coordinate": { "space": "local", "node": "hold", "position": { "numerator": numerator, "denominator": "2" } }, "bias": "right" }
        })
    };
    let mut envelope = json!({
        "protocol": 1,
        "request": {
            "project_id": before.project_id(), "expected_revision": before.revision_id(), "role": "audio",
            "selector": { "type": "range", "start": point("5"), "end": point("7") }
        }
    });
    fs::write(&selection_path, envelope.to_string())?;
    let args = [
        "resolve-selection",
        path,
        "--json",
        selection_path.to_str().unwrap(),
    ];
    let output = success(&args)?;
    assert_eq!(
        output["resolved"]["revision_id"],
        before.revision_id().as_str()
    );
    assert_eq!(output["resolved"]["role"], "audio");
    assert_eq!(
        output["resolved"]["selection"]["start"]["exact_frame"]["numerator"],
        "5"
    );
    assert_eq!(
        output["resolved"]["selection"]["frames"],
        json!({ "start": 2, "end": 4 })
    );
    envelope["request"]["expected_revision"] = json!("stale");
    fs::write(&selection_path, envelope.to_string())?;
    let output = cli(&args)?;
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stderr)?;
    assert_eq!(error["error"]["code"], "RevisionConflict");
    assert_eq!(
        error["error"]["current_revision"],
        before.revision_id().as_str()
    );
    envelope["request"]["expected_revision"] = json!(before.revision_id());
    envelope["request"]["selector"]["end"] = point("4");
    fs::write(&selection_path, envelope.to_string())?;
    let output = cli(&args)?;
    assert!(!output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stderr)?["error"]["code"],
        "InvalidRange"
    );
    envelope["protocol"] = json!(2);
    fs::write(&selection_path, envelope.to_string())?;
    let output = cli(&args)?;
    assert!(!output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stderr)?["error"]["code"],
        "ProtocolUnsupported"
    );
    assert_eq!(writer.snapshot()?, before);
    Ok(())
}

#[test]
fn persistent_mark_queries_and_loss_states_share_the_headless_command_path() -> Result {
    use deadpan_core::{
        Anchor, AnchorLossPolicy, BoundaryAnchor, ExactRatio, InsertionBias, MarkId, ProjectFrame,
    };
    let scratch = tempfile::tempdir()?;
    let package = create(scratch.path())?;
    let path = package.to_str().unwrap();
    let initial = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let command_file = scratch.path().join("command.json");
    let command_path = command_file.to_str().unwrap();
    fs::write(&command_file, request(&initial)?.to_string())?;
    success(&["command", path, "--json", command_path])?;
    let write_command = |command: Command, revision: &str| -> Result {
        let document = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
        fs::write(&command_file,json!({"protocol":1,"project_id":document.project_id(),"expected_revision":document.revision_id(),"new_revision":revision,"command":command}).to_string())?;
        Ok(())
    };
    let writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let before = writer.snapshot()?;
    write_command(
        Command::SetMark {
            id: MarkId::new("a")?,
            owner: before.root().clone(),
            label: "Start of the pause".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: NodeId::new("hold")?,
                    position: ExactRatio::integer(12),
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
        "mark-a",
    )?;
    let dry = success(&["command", path, "--json", command_path, "--dry-run"])?;
    assert_eq!(
        dry["edit"]["forward"]["marks"]["a"]["after"]["state"]["type"],
        "bound"
    );
    assert_eq!(writer.snapshot()?, before);
    drop(writer);
    success(&["command", path, "--json", command_path])?;
    write_command(
        Command::SetMark {
            id: MarkId::new("b")?,
            owner: before.root().clone(),
            label: "Pinned end".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Sequence {
                    frame: ProjectFrame(20),
                },
                bias: InsertionBias::Left,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
        "mark-b",
    )?;
    success(&["command", path, "--json", command_path])?;
    let selection_file = scratch.path().join("selection.json");
    let query = |selector: Value| -> Result<Output> {
        let document = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
        fs::write(&selection_file,json!({"protocol":1,"request":{"project_id":document.project_id(),"expected_revision":document.revision_id(),"role":"linked","selector":selector}}).to_string())?;
        cli(&[
            "resolve-selection",
            path,
            "--json",
            selection_file.to_str().unwrap(),
        ])
    };
    let writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let output = query(json!({"type":"mark_range","start":{"id":"a"},"end":{"id":"b"}}))?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout)?["resolved"]["selection"]["frames"],
        json!({"start":12,"end":20})
    );
    drop(writer);
    write_command(
        Command::Delete {
            node: NodeId::new("hold")?,
        },
        "delete-hold",
    )?;
    success(&["command", path, "--json", command_path])?;
    let output = query(json!({"type":"mark","target":{"id":"a"}}))?;
    assert!(!output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stderr)?["error"]["code"],
        "MarkUnresolved"
    );
    let document = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    success(&[
        "project",
        "undo",
        path,
        "--expected",
        document.revision_id().as_str(),
    ])?;
    let output = query(json!({"type":"mark","target":{"id":"a"}}))?;
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout)?["resolved"]["selection"]["point"]["frame"],
        12
    );
    let output = query(json!({"type":"mark","target":{"id":"missing"}}))?;
    assert!(!output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stderr)?["error"]["code"],
        "MarkMissing"
    );
    Ok(())
}

#[test]
fn sparse_override_commands_and_inspection_share_revision_and_dry_run_guards() -> Result {
    use deadpan_core::{NodeKind, WrapAnchorPolicy};
    let scratch = tempfile::tempdir()?;
    let package = create(scratch.path())?;
    let path = package.to_str().unwrap();
    let file = scratch.path().join("command.json");
    let file_path = file.to_str().unwrap();
    let snapshot = || -> Result<ProjectDocument> {
        Ok(ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?)
    };
    fs::write(&file, request(&snapshot()?)?.to_string())?;
    success(&["command", path, "--json", file_path])?;
    let write = |command: Command| -> Result {
        let current = snapshot()?;
        fs::write(&file, json!({
            "protocol":1,"project_id":current.project_id(),"expected_revision":current.revision_id(),"command":command
        }).to_string())?;
        Ok(())
    };
    write(Command::WrapRepeat {
        node: NodeId::new("hold")?,
        id: NodeId::new("repeat")?,
        plays: 3,
        gap: None,
        anchor_policy: WrapAnchorPolicy::First,
    })?;
    success(&["command", path, "--json", file_path])?;
    let before = snapshot()?;
    let NodeKind::Repeat { iterations, .. } = &before.nodes()[&NodeId::new("repeat")?].kind else {
        panic!()
    };
    let selected = iterations.at(1).unwrap();
    write(Command::SetPlayOverride {
        node: NodeId::new("repeat")?,
        iteration: selected.clone(),
        subtree: Subtree {
            root: NodeId::new("custom")?,
            overrides: BTreeMap::new(),
            nodes: BTreeMap::from([(
                NodeId::new("custom")?,
                BeatNode::hold(
                    "Longer pause",
                    HoldRecipe {
                        duration: FrameDuration::new(60)?,
                        video: HoldVideo::Background,
                        audio: HoldAudio::Silence,
                    },
                ),
            )]),
        },
    })?;
    let writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let preview = success(&["command", path, "--json", file_path, "--dry-run"])?;
    assert_eq!(preview["edit"]["duration_delta"], 15);
    assert_eq!(
        preview["edit"]["forward"]["overrides"]["repeat"]["after"][0]["iteration"],
        serde_json::to_value(&selected)?
    );
    assert_eq!(writer.snapshot()?, before);
    drop(writer);
    success(&["command", path, "--json", file_path])?;
    // Replaying an already committed request must fail before allocating another subtree.
    let stale = cli(&["command", path, "--json", file_path])?;
    assert!(!stale.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&stale.stderr)?["error"]["code"],
        "RevisionConflict"
    );
    let writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let after = writer.snapshot()?;
    assert_eq!(after.duration()?.frames(), 150);
    let inspected = success(&["inspect-plan", path, "--frame", "46"])?;
    assert_eq!(inspected["sample"]["instance"]["node"], "custom");
    assert_eq!(
        inspected["sample"]["instance"]["repeats"][0]["iteration"],
        serde_json::to_value(&selected)?
    );
    assert_eq!(writer.snapshot()?, after);
    drop(writer);
    write(Command::ClearPlayOverride {
        node: NodeId::new("repeat")?,
        iteration: selected,
    })?;
    success(&["command", path, "--json", file_path])?;
    let cleared = snapshot()?;
    assert_eq!(cleared.duration()?.frames(), 135);
    assert!(cleared.overrides().is_empty());
    success(&[
        "project",
        "undo",
        path,
        "--expected",
        cleared.revision_id().as_str(),
    ])?;
    assert_eq!(snapshot()?.overrides(), after.overrides());
    Ok(())
}

#[test]
fn nested_occurrence_command_is_atomic_and_uses_the_same_headless_plan() -> Result {
    use deadpan_core::{NodeKind, WrapAnchorPolicy};
    let scratch = tempfile::tempdir()?;
    let package = create(scratch.path())?;
    let path = package.to_str().unwrap();
    let file = scratch.path().join("command.json");
    let file_path = file.to_str().unwrap();
    let snapshot = || -> Result<ProjectDocument> {
        Ok(ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?)
    };
    fs::write(&file, request(&snapshot()?)?.to_string())?;
    success(&["command", path, "--json", file_path])?;
    for (child, id) in [("hold", "inner"), ("inner", "outer")] {
        let current = snapshot()?;
        fs::write(&file,json!({"protocol":1,"project_id":current.project_id(),"expected_revision":current.revision_id(),
            "command":Command::WrapRepeat {node:NodeId::new(child)?,id:NodeId::new(id)?,plays:2,gap:None,anchor_policy:WrapAnchorPolicy::First}
        }).to_string())?;
        success(&["command", path, "--json", file_path])?;
    }
    let before = snapshot()?;
    let play = |name: &str| -> Result<Value> {
        let NodeKind::Repeat { iterations, .. } = &before.nodes()[&NodeId::new(name)?].kind else {
            panic!()
        };
        Ok(serde_json::to_value(iterations.at(1).unwrap())?)
    };
    fs::write(&file,json!({"protocol":1,"project_id":before.project_id(),"expected_revision":before.revision_id(),
        "new_revision":"occurrence-change",
        "command":{"command":"edit_occurrence",
            "instance":{"node":"hold","repeats":[{"node":"outer","iteration":play("outer")?},{"node":"inner","iteration":play("inner")?}]},
            "edit":{"type":"set_hold_duration","duration":60},
            "identities":{"nodes":["outer-inner","outer-default","selected-hold"],"marks":[]}
        }
    }).to_string())?;
    let writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let preview = success(&["command", path, "--json", file_path, "--dry-run"])?;
    assert_eq!(preview["edit"]["duration_delta"], 15);
    assert_eq!(writer.snapshot()?, before);
    drop(writer);
    let committed = success(&["command", path, "--json", file_path])?;
    assert_eq!(committed["outcome"]["edit"], preview["edit"]);
    let after = snapshot()?;
    assert_eq!(after.duration()?.frames(), 195);
    assert_eq!(after.nodes().len(), before.nodes().len() + 3);
    let selected = success(&["inspect-plan", path, "--frame", "136"])?;
    assert_eq!(selected["sample"]["instance"]["node"], "selected-hold");
    assert_eq!(
        selected["sample"]["instance"]["repeats"][1]["node"],
        "outer-inner"
    );
    let other = success(&["inspect-plan", path, "--frame", "46"])?;
    assert_eq!(other["sample"]["instance"]["node"], "hold");
    success(&[
        "project",
        "undo",
        path,
        "--expected",
        after.revision_id().as_str(),
    ])?;
    let undone = snapshot()?;
    assert_eq!(undone.nodes(), before.nodes());
    assert_eq!(undone.overrides(), before.overrides());
    success(&[
        "project",
        "redo",
        path,
        "--expected",
        undone.revision_id().as_str(),
    ])?;
    let redone = snapshot()?;
    assert_eq!(redone.nodes(), after.nodes());
    assert_eq!(redone.overrides(), after.overrides());
    Ok(())
}
