use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, Output},
};

use deadpan_core::*;
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

fn ratio(numerator: i64, denominator: i64) -> Value {
    json!({"numerator": numerator.to_string(), "denominator": denominator.to_string()})
}

fn fixture(root: &Path) -> Result<(PathBuf, ProjectStore)> {
    let root_id = NodeId::new("root")?;
    let initial = ProjectDocument::new(
        ProjectId::new("boundary-project")?,
        RevisionId::new("initial")?,
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        root_id.clone(),
    )?;
    let hold = |frames| -> Result<BeatNode> {
        Ok(BeatNode::hold(
            "Hold",
            HoldRecipe {
                duration: FrameDuration::new(frames)?,
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
                picture_context: None,
            },
        ))
    };
    let retime = |child, duration, start, end| -> Result<BeatNode> {
        Ok(BeatNode {
            audio_treatments: Default::default(),
            framing: None,
            label: "Retime".into(),
            audio_editorial_edges: Default::default(),
            audio_edges: Default::default(),
            kind: NodeKind::Retime {
                child: NodeId::new(child)?,
                duration: FrameDuration::new(duration)?,
                mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end))?,
                pitch: PitchPolicy::Preserve,
                purpose: RetimePurpose::Edit,
            },
            cutaways: Vec::new(),
            captions: Vec::new(),
        })
    };
    let repeat = BeatNode {
        audio_treatments: Default::default(),
        framing: None,
        label: "Repeat".into(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: NodeId::new("inner")?,
            iterations: IterationOrder::new(RevisionId::new("plays")?, 2)?,
            gap: Some(HoldRecipe {
                duration: FrameDuration::new(2)?,
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
                picture_context: None,
            }),
            escalation: None,
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    };
    let nodes = BTreeMap::from([
        (
            root_id,
            BeatNode::sequence("Root", vec![NodeId::new("head")?, NodeId::new("outer")?]),
        ),
        (NodeId::new("head")?, hold(3)?),
        (NodeId::new("outer")?, retime("repeat", 7, 0, 14)?),
        (NodeId::new("repeat")?, repeat),
        (NodeId::new("inner")?, retime("leaf", 6, 1, 10)?),
        (NodeId::new("leaf")?, hold(10)?),
    ]);
    let mut wire = serde_json::to_value(initial)?;
    wire["nodes"] = serde_json::to_value(nodes)?;
    let document = ProjectDocument::from_json(&wire.to_string())?;
    let package = root.join("boundary.deadpan");
    // Keep the writable store alive. Queries must coexist with its writer lock.
    let store = ProjectStore::create(&package, &document)?;
    Ok((package, store))
}

fn request(position: Value, bias: &str) -> Value {
    json!({"protocol": 1, "request": {
        "project_id": "boundary-project", "expected_revision": "initial",
        "position": position, "bias": bias
    }})
}

fn history_counts(package: &Path) -> Result<(i64, i64, i64)> {
    let database = rusqlite::Connection::open_with_flags(
        package.join("project.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    Ok(database.query_row(
        "SELECT (SELECT COUNT(*) FROM revisions), (SELECT COUNT(*) FROM history), (SELECT COUNT(*) FROM redo)",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?)
}

fn query(root: &Path, package: &Path, envelope: &Value) -> Result<Output> {
    let input = root.join("boundary.json");
    fs::write(&input, envelope.to_string())?;
    cli(&[
        "locate-boundary",
        package.to_str().unwrap(),
        "--json",
        input.to_str().unwrap(),
    ])
}

fn location(root: &Path, package: &Path, envelope: &Value) -> Result<Value> {
    let output = query(root, package, envelope)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(report["protocol"], 1);
    Ok(report["location"].clone())
}

#[test]
fn exact_nested_retime_location_round_trips_through_existing_occurrence_selection() -> Result {
    let scratch = tempfile::tempdir()?;
    let (package, writer) = fixture(scratch.path())?;
    let before = writer.snapshot()?;
    let initial_history = history_counts(&package)?;
    let result = location(scratch.path(), &package, &request(ratio(33, 4), "right"))?;
    assert_eq!(result["project_id"], "boundary-project");
    assert_eq!(result["revision_id"], "initial");
    assert_eq!(result["position"], ratio(33, 4));
    assert_eq!(result["bias"], "right");
    assert_eq!(result["terminal"], json!({"type": "node"}));
    let scopes = result["scopes"].as_array().unwrap();
    assert_eq!(scopes.len(), 5);
    assert_eq!(
        scopes
            .iter()
            .map(|scope| &scope["entry"])
            .collect::<Vec<_>>(),
        vec![
            &json!({"type": "root"}),
            &json!({"type": "sequence", "index": 1}),
            &json!({"type": "retime"}),
            &json!({"type": "repeat_play"}),
            &json!({"type": "retime"}),
        ]
    );
    for (scope, (node, position, duration)) in scopes.iter().zip([
        ("root", ratio(33, 4), 10),
        ("outer", ratio(21, 4), 7),
        ("repeat", ratio(21, 2), 14),
        ("inner", ratio(5, 2), 6),
        ("leaf", ratio(19, 4), 10),
    ]) {
        assert_eq!(scope["instance"]["node"], node);
        assert_eq!(scope["position"], position);
        assert_eq!(scope["duration"], duration);
    }
    let leaf = scopes.last().unwrap();
    assert_eq!(
        leaf["instance"]["repeats"],
        json!([{
            "node": "repeat", "iteration": {"allocation": "plays", "ordinal": 1}
        }])
    );
    assert!(result["comparisons"].as_u64().unwrap() > 0);
    let selection = json!({"protocol": 1, "request": {
        "project_id": "boundary-project", "expected_revision": "initial", "role": "linked",
        "selector": {"type": "point", "target": {"boundary": {
            "coordinate": {"space": "occurrence", "instance": leaf["instance"], "position": leaf["position"]},
            "bias": "right"
        }}}
    }});
    let input = scratch.path().join("selection.json");
    fs::write(&input, selection.to_string())?;
    let resolved = success(&[
        "resolve-selection",
        package.to_str().unwrap(),
        "--json",
        input.to_str().unwrap(),
    ])?;
    assert_eq!(
        resolved["resolved"]["selection"]["point"]["exact_frame"],
        ratio(33, 4)
    );
    assert_eq!(writer.snapshot()?, before);
    assert_eq!(history_counts(&package)?, initial_history);
    writer.validate()?;
    Ok(())
}

#[test]
fn implicit_gap_keeps_repeat_owner_and_bias_distinguishes_its_start() -> Result {
    let scratch = tempfile::tempdir()?;
    let (package, writer) = fixture(scratch.path())?;
    let result = location(scratch.path(), &package, &request(ratio(13, 2), "right"))?;
    let scopes = result["scopes"].as_array().unwrap();
    assert_eq!(scopes.len(), 3);
    assert_eq!(
        scopes[2]["instance"],
        json!({"node": "repeat", "repeats": []})
    );
    assert_eq!(scopes[2]["position"], ratio(7, 1));
    assert_eq!(scopes[2]["entry"], json!({"type": "retime"}));
    assert_eq!(
        result["terminal"],
        json!({
            "type": "gap", "after": {"allocation": "plays", "ordinal": 0},
            "position": ratio(1, 1), "duration": 2
        })
    );
    let left = location(scratch.path(), &package, &request(ratio(6, 1), "left"))?;
    assert_eq!(left["terminal"]["type"], "node");
    assert_eq!(
        left["scopes"].as_array().unwrap().last().unwrap()["position"],
        ratio(10, 1)
    );
    let right = location(scratch.path(), &package, &request(ratio(6, 1), "right"))?;
    assert_eq!(right["terminal"]["type"], "gap");
    assert_eq!(right["terminal"]["position"], ratio(0, 1));
    assert_eq!(
        writer.snapshot()?.nodes().len(),
        6,
        "query must not materialize a gap Hold"
    );
    Ok(())
}

#[test]
fn owned_gap_entry_is_distinct_from_a_play_and_sequence_indices_keep_empty_slots() -> Result {
    let scratch = tempfile::tempdir()?;
    let (package, mut writer) = fixture(scratch.path())?;
    let empty = NodeId::new("empty")?;
    let gap = NodeId::new("gap-branch")?;
    let hold = NodeId::new("gap-hold")?;
    for (revision, command) in [
        (
            "empty-slot",
            Command::Insert {
                parent: NodeId::new("root")?,
                index: 1,
                subtree: Subtree {
                    root: empty.clone(),
                    nodes: BTreeMap::from([(empty, BeatNode::sequence("Empty", vec![]))]),
                    overrides: Default::default(),
                    gap_overrides: Default::default(),
                },
            },
        ),
        (
            "owned-gap",
            Command::SetGapOverride {
                node: NodeId::new("repeat")?,
                iteration: IterationId {
                    allocation: RevisionId::new("plays")?,
                    ordinal: 0,
                },
                subtree: Subtree {
                    root: gap.clone(),
                    nodes: BTreeMap::from([
                        (gap, BeatNode::sequence("Gap branch", vec![hold.clone()])),
                        (
                            hold,
                            BeatNode::hold(
                                "Gap",
                                HoldRecipe {
                                    duration: FrameDuration::new(2)?,
                                    video: HoldVideo::Background,
                                    audio: HoldAudio::Silence,
                                    picture_context: None,
                                },
                            ),
                        ),
                    ]),
                    overrides: Default::default(),
                    gap_overrides: Default::default(),
                },
            },
        ),
    ] {
        let document = writer.snapshot()?;
        writer.commit(&CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(revision)?,
            command,
        })?;
    }
    let before = writer.snapshot()?;
    let initial_history = history_counts(&package)?;
    let mut envelope = request(ratio(13, 2), "right");
    envelope["request"]["expected_revision"] = json!("owned-gap");
    let result = location(scratch.path(), &package, &envelope)?;
    let scopes = result["scopes"].as_array().unwrap();
    assert_eq!(scopes.len(), 5);
    assert_eq!(scopes[1]["instance"]["node"], "outer");
    assert_eq!(scopes[1]["entry"], json!({"type": "sequence", "index": 2}));
    assert_eq!(scopes[3]["entry"], json!({"type": "repeat_gap"}));
    assert_eq!(
        scopes[3]["instance"],
        json!({
            "node": "gap-branch", "repeats": [{
                "node": "repeat", "iteration": {"allocation": "plays", "ordinal": 0}
            }]
        })
    );
    assert_eq!(scopes[4]["instance"]["node"], "gap-hold");
    assert_eq!(scopes[4]["entry"], json!({"type": "sequence", "index": 0}));
    assert_eq!(scopes[4]["position"], ratio(1, 1));
    assert_eq!(result["terminal"], json!({"type": "node"}));
    assert_eq!(writer.snapshot()?, before);
    assert_eq!(history_counts(&package)?, initial_history);
    Ok(())
}

#[test]
fn endpoint_bias_and_query_budgets_are_explicit() -> Result {
    let scratch = tempfile::tempdir()?;
    let (package, _writer) = fixture(scratch.path())?;
    for (at, bias, terminal) in [(0, "left", "project_start"), (10, "right", "project_end")] {
        let mut envelope = request(ratio(at, 1), bias);
        envelope["limits"] = json!({"max_scopes": 1, "max_comparisons": 0});
        let result = location(scratch.path(), &package, &envelope)?;
        assert_eq!(result["terminal"], json!({"type": terminal}));
        assert_eq!(result["scopes"].as_array().unwrap().len(), 1);
        assert_eq!(result["scopes"][0]["entry"], json!({"type": "root"}));
        assert_eq!(result["comparisons"], 0);
    }
    for (at, bias) in [(0, "right"), (10, "left")] {
        let result = location(scratch.path(), &package, &request(ratio(at, 1), bias))?;
        assert_eq!(result["terminal"]["type"], "node");
    }
    for limits in [
        json!({"max_scopes": 0, "max_comparisons": 8192}),
        json!({"max_scopes": 4, "max_comparisons": 8192}),
        json!({"max_scopes": 257, "max_comparisons": 0}),
    ] {
        let mut envelope = request(ratio(33, 4), "right");
        envelope["limits"] = limits;
        let output = query(scratch.path(), &package, &envelope)?;
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let error: Value = serde_json::from_slice(&output.stderr)?;
        assert_eq!(error["error"]["code"], "BoundaryQueryLimit");
    }
    Ok(())
}

#[test]
fn stale_foreign_and_malformed_requests_fail_without_authored_changes() -> Result {
    let scratch = tempfile::tempdir()?;
    let (package, writer) = fixture(scratch.path())?;
    let before = writer.snapshot()?;
    let initial_history = history_counts(&package)?;
    let mut cases = Vec::new();
    let base = request(ratio(33, 4), "right");
    for (field, value, code) in [
        ("expected_revision", json!("stale"), "RevisionConflict"),
        ("project_id", json!("different-project"), "ProjectConflict"),
        ("future", Value::Null, "InvalidInput"),
        ("position", ratio(1, 0), "InvalidInput"),
    ] {
        let mut envelope = base.clone();
        envelope["request"][field] = value;
        cases.push((envelope, code));
    }
    let mut unsupported = base.clone();
    unsupported["protocol"] = json!(2);
    cases.push((unsupported, "ProtocolUnsupported"));
    let mut unknown = base.clone();
    unknown["future"] = Value::Null;
    cases.push((unknown, "InvalidInput"));
    let mut unknown_limits = base.clone();
    unknown_limits["limits"] = json!({"max_scopes": 257, "max_comparisons": 8192, "future": null});
    cases.push((unknown_limits, "InvalidInput"));
    let mut partial_limits = base;
    partial_limits["limits"] = json!({"max_scopes": 257});
    cases.push((partial_limits, "InvalidInput"));
    for (envelope, code) in cases {
        let output = query(scratch.path(), &package, &envelope)?;
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let error: Value = serde_json::from_slice(&output.stderr)?;
        assert_eq!(error["error"]["code"], code, "{envelope}");
        if code == "RevisionConflict" {
            assert_eq!(error["error"]["current_revision"], "initial");
        }
        assert_eq!(writer.snapshot()?, before);
        assert_eq!(history_counts(&package)?, initial_history);
    }
    writer.validate()?;
    drop(writer);
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
        before
    );
    Ok(())
}
