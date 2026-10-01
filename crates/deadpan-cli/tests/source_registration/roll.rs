use super::*;

use std::collections::BTreeMap;

use deadpan_cli::live_project::{HistoryDirection, ShortOperation, execute_short};
use deadpan_core::{
    BeatNode, CommandRequest, EditTransaction, ProjectDocument, RevisionId, Subtree,
};

pub(super) fn ready(directory: &Path) -> Result<PathBuf> {
    let package = directory.join("roll.deadpan");
    create(&package, "30000/1001")?;
    for (asset, bytes, registered, inserted, slot) in [
        (
            "left",
            include_bytes!("../../../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4")
                .as_slice(),
            "left-registered",
            "left-inserted",
            0,
        ),
        (
            "right",
            include_bytes!("../../../../native/deadpan-source/tests/fixtures/offset-bframes.mp4")
                .as_slice(),
            "right-registered",
            "ready",
            1,
        ),
    ] {
        let original = retain(
            &package,
            &directory.join(format!("{asset}.mp4")),
            bytes,
            false,
        )?;
        let mut input = request(
            &package,
            original,
            json!({"type":"video_and_audio","audio_stream":1}),
            registered,
        )?;
        input["registration"]["new_asset_id"] = json!(asset);
        input["registration"]["insertion"] = Value::Null;
        registration(&package, &save(directory, &input)?, false, true)?;
        let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
        let current = store.snapshot()?;
        let asset_id = AssetId::new(asset)?;
        let receipt = store.registered_source(current.revision_id(), &asset_id)?;
        let timing = deadpan_media::source_import_timing::derive_source_moment(
            receipt.snapshot().video().unwrap().index(),
            receipt.snapshot().audio(),
            10..13,
            current.presentation_basis().frame_rate,
        )?;
        store.commit(&CommandRequest {
            project_id: current.project_id().clone(),
            expected_revision: current.revision_id().clone(),
            new_revision: RevisionId::new(inserted)?,
            command: deadpan_core::Command::Insert {
                parent: current.root().clone(),
                index: slot,
                subtree: Subtree {
                    root: NodeId::new(asset)?,
                    nodes: BTreeMap::from([(
                        NodeId::new(asset)?,
                        BeatNode {
                            label: format!("{asset} measured moment"),
                            framing: None,
                            audio_treatments: Default::default(),
                            audio_editorial_edges: Default::default(),
                            audio_edges: Default::default(),
                            kind: NodeKind::Source {
                                source: timing.source_node(asset_id),
                            },
                        },
                    )]),
                    overrides: Default::default(),
                    gap_overrides: Default::default(),
                },
            },
        })?;
    }
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let current = store.snapshot()?;
    assert_eq!(current.assets().len(), 2);
    assert_ne!(
        current.assets()[&AssetId::new("left")?].source_qualification,
        current.assets()[&AssetId::new("right")?].source_qualification,
    );
    Ok(package)
}

fn envelope(
    before: &ProjectDocument,
    revision: &str,
    right: &str,
    delta: i64,
    wrapper: Option<&str>,
) -> Value {
    json!({
        "protocol":1, "project_id":before.project_id(),
        "expected_revision":before.revision_id(), "new_revision":revision,
        "command": {
            "command":"roll_sources", "parent":before.root(), "left":"left", "right":right,
            "delta_frames":delta, "left_wrapper":null, "right_wrapper":wrapper,
            "timing":{"allocation":revision,"ordinal":0}
        }
    })
}

pub(super) fn typed(envelope: &Value) -> Result<CommandRequest> {
    Ok(CommandRequest {
        project_id: serde_json::from_value(envelope["project_id"].clone())?,
        expected_revision: serde_json::from_value(envelope["expected_revision"].clone())?,
        new_revision: serde_json::from_value(envelope["new_revision"].clone())?,
        command: serde_json::from_value(envelope["command"].clone())?,
    })
}

pub(super) fn hosted(
    store: &mut ProjectStore,
    request: &Value,
    dry_run: bool,
) -> Result<(Value, Option<RevisionId>)> {
    let request = typed(request)?;
    let project = request.project_id.clone();
    Ok(execute_short(
        store,
        &project,
        &ShortOperation::Edit {
            request: Box::new(request),
            dry_run,
        },
    )?)
}

pub(super) fn same_content(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = json!(actual.revision_id());
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

#[test]
fn headless_roll_cold_and_live_share_preview_one_commit_and_durable_history() -> Result {
    // Exercise both commit hosts independently. Each package receives exactly
    // one Roll transaction, then durable Undo/Redo across reopen boundaries.
    for live_commit in [false, true] {
        let scratch = tempfile::tempdir()?;
        let package = ready(scratch.path())?;
        let path = package.to_str().unwrap();
        let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
        let initial_counts = counts(&package)?;
        let database = Connection::open(package.join("project.sqlite"))?;
        assert_eq!(before.duration()?.frames(), 6);
        let request = envelope(&before, "rolled", "right", 100, Some("right-crop"));
        let file = save(scratch.path(), &request)?;
        let file = file.to_str().unwrap();
        // Cold previews remain available beside the native writer. They use
        // the same receipt admission and report as its live dispatch path.
        let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
        let cold = success(&["command", path, "--json", file, "--dry-run"])?;
        let (live, saved) = hosted(&mut writer, &request, true)?;
        assert_eq!(live, cold);
        assert!(saved.is_none());
        assert_eq!(cold["protocol"], 1);
        assert_eq!(cold["committed"], false);
        let report = &cold["source_roll"];
        assert_eq!(report["requested_delta_frames"], 100);
        assert_eq!(report["applied_delta_frames"], 2);
        assert_eq!(report["minimum_delta_frames"], -2);
        assert_eq!(report["maximum_delta_frames"], 2);
        assert_eq!(report["clamp"]["side"], "right");
        assert_eq!(report["clamp"]["reason"], "minimum_output_duration");
        assert_eq!(report["left"]["asset"], "left");
        assert_eq!(report["right"]["asset"], "right");
        assert_eq!(report["left"]["needs_wrapper"], false);
        assert_eq!(report["right"]["needs_wrapper"], true);
        assert_ne!(
            report["left"]["qualification"],
            report["right"]["qualification"]
        );
        assert_eq!(cold["edit"]["duration_delta"], 0);
        let transaction: EditTransaction = serde_json::from_value(cold["edit"].clone())?;
        assert_eq!(writer.snapshot()?, before);
        assert_eq!(counts(&package)?, initial_counts);
        let committed = if live_commit {
            let (value, saved) = hosted(&mut writer, &request, false)?;
            assert_eq!(saved, Some(RevisionId::new("rolled")?));
            drop(writer);
            value
        } else {
            drop(writer);
            success(&["command", path, "--json", file])?
        };
        assert_eq!(committed["committed"], true);
        assert_eq!(committed["outcome"]["edit"], cold["edit"]);
        assert_eq!(
            counts(&package)?,
            (initial_counts.0 + 1, initial_counts.1 + 1, initial_counts.2)
        );
        let after = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
        assert_eq!(after, transaction.forward.apply(&before)?);
        assert_eq!(after.duration()?.frames(), 6);
        assert_eq!(
            after.nodes()[&NodeId::new("left")?].audio_editorial_edges,
            deadpan_core::AudioEditorialEdges {
                start: false,
                end: true
            }
        );
        assert_eq!(
            after.nodes()[&NodeId::new("right-crop")?].audio_editorial_edges,
            deadpan_core::AudioEditorialEdges {
                start: true,
                end: false
            }
        );
        let (wire, patch): (String, String) = database.query_row(
            "SELECT request,edit FROM history WHERE revision_id='rolled'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(
            serde_json::from_str::<CommandRequest>(&wire)?,
            typed(&request)?
        );
        assert_eq!(
            serde_json::from_str::<EditTransaction>(&patch)?,
            transaction
        );
        // Neither preview nor either committing host may reuse a captured pair
        // from the old revision, even though the physical Source IDs survive.
        for args in [
            vec!["command", path, "--json", file, "--dry-run"],
            vec!["command", path, "--json", file],
        ] {
            assert_eq!(failure(&args)?["error"]["code"], "RevisionConflict");
        }
        let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
        assert!(hosted(&mut writer, &request, true).is_err());
        assert!(hosted(&mut writer, &request, false).is_err());
        drop(writer);
        assert_eq!(
            counts(&package)?,
            (initial_counts.0 + 1, initial_counts.1 + 1, initial_counts.2)
        );
        success(&["project", "undo", path, "--expected", "rolled"])?;
        let undone = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
        same_content(&undone, &before)?;
        assert_ne!(undone.revision_id(), before.revision_id());
        let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
        let (_, saved) = execute_short(
            &mut writer,
            undone.project_id(),
            &ShortOperation::History {
                direction: HistoryDirection::Redo,
                expected_revision: undone.revision_id().clone(),
                new_revision: RevisionId::new("redo-roll")?,
                dry_run: false,
            },
        )?;
        assert_eq!(saved, Some(RevisionId::new("redo-roll")?));
        drop(writer);
        let reopened = ProjectStore::open(&package, AccessMode::ReadOnly)?;
        let redone = reopened.snapshot()?;
        same_content(&redone, &after)?;
        assert_ne!(redone.revision_id(), after.revision_id());
        assert_ne!(redone.revision_id(), undone.revision_id());
        assert_eq!(
            counts(&package)?,
            (initial_counts.0 + 3, initial_counts.1 + 1, initial_counts.2)
        );
        reopened.validate()?;
    }
    Ok(())
}

#[test]
fn headless_roll_zero_and_invalid_metadata_match_live_without_writes() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = ready(scratch.path())?;
    let path = package.to_str().unwrap();
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let initial_counts = counts(&package)?;
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let zero = envelope(&before, "unused", "right", 0, None);
    let file = save(scratch.path(), &zero)?;
    let cold = success(&[
        "command",
        path,
        "--json",
        file.to_str().unwrap(),
        "--dry-run",
    ])?;
    let (live, saved) = hosted(&mut writer, &zero, true)?;
    assert_eq!(cold, live);
    assert!(saved.is_none());
    assert_eq!(cold["source_roll"]["requested_delta_frames"], 0);
    assert_eq!(cold["source_roll"]["applied_delta_frames"], 0);
    assert!(cold["source_roll"]["clamp"].is_null());
    assert!(cold["edit"].is_null());
    assert!(hosted(&mut writer, &zero, false).is_err());
    assert_eq!(writer.snapshot()?, before);
    drop(writer);
    assert_eq!(
        failure(&["command", path, "--json", file.to_str().unwrap()])?["error"]["code"],
        "InvalidCommand"
    );
    assert_eq!(counts(&package)?, initial_counts);
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    for case in [
        "left-wrapper",
        "right-wrapper",
        "timing",
        "reused",
        "reversed",
    ] {
        let mut invalid = zero.clone();
        match case {
            "left-wrapper" => invalid["command"]["left_wrapper"] = json!("unwanted"),
            "right-wrapper" => invalid["command"]["right_wrapper"] = json!("unwanted"),
            "timing" => invalid["command"]["timing"]["allocation"] = json!("wrong"),
            "reused" => {
                invalid["new_revision"] = json!("left-registered");
                invalid["command"]["timing"]["allocation"] = json!("left-registered");
            }
            "reversed" => {
                invalid["command"]["left"] = json!("right");
                invalid["command"]["right"] = json!("left");
            }
            _ => unreachable!(),
        }
        let file = save(scratch.path(), &invalid)?;
        let cold = failure(&[
            "command",
            path,
            "--json",
            file.to_str().unwrap(),
            "--dry-run",
        ])?;
        assert!(cold["error"]["code"].is_string(), "{case}");
        assert!(hosted(&mut writer, &invalid, true).is_err(), "{case}");
        assert!(hosted(&mut writer, &invalid, false).is_err(), "{case}");
        assert_eq!(writer.snapshot()?, before);
        assert_eq!(counts(&package)?, initial_counts);
    }
    // Reuse the same proposed revision after all refused previews, then ask for
    // more Roll at the one-frame right-side limit. The exact clamp survives a
    // zero result in both dispatch paths, with no second history entry.
    let nonzero = envelope(&before, "unused", "right", 100, Some("right-crop"));
    hosted(&mut writer, &nonzero, false)?;
    let after = writer.snapshot()?;
    let clamped = envelope(&after, "still-unused", "right-crop", 100, None);
    let file = save(scratch.path(), &clamped)?;
    let cold = success(&[
        "command",
        path,
        "--json",
        file.to_str().unwrap(),
        "--dry-run",
    ])?;
    let (live, saved) = hosted(&mut writer, &clamped, true)?;
    assert_eq!(cold, live);
    assert!(saved.is_none());
    assert_eq!(cold["source_roll"]["requested_delta_frames"], 100);
    assert_eq!(cold["source_roll"]["applied_delta_frames"], 0);
    assert_eq!(cold["source_roll"]["clamp"]["side"], "right");
    assert_eq!(
        cold["source_roll"]["clamp"]["reason"],
        "minimum_output_duration"
    );
    assert!(cold["edit"].is_null());
    assert!(hosted(&mut writer, &clamped, false).is_err());
    assert_eq!(writer.snapshot()?, after);
    drop(writer);
    assert_eq!(
        failure(&["command", path, "--json", file.to_str().unwrap()])?["error"]["code"],
        "InvalidCommand"
    );
    assert_eq!(
        counts(&package)?,
        (initial_counts.0 + 1, initial_counts.1 + 1, initial_counts.2)
    );
    let reopened = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_eq!(reopened.snapshot()?, after);
    reopened.validate()?;
    Ok(())
}
