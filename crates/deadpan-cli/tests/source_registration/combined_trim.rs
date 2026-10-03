//! The headless protocol and the owned-project dispatch share one complete edit.
use super::roll::{hosted, ready, same_content, typed};
use super::*;
use deadpan_cli::live_project::{HistoryDirection, ShortOperation, execute_short};
use deadpan_core::{
    AudioTimingId, CommandRequest, EditTransaction, FrameRange, ProjectDocument, ProjectFrame,
    RevisionId, SourceTrimCapture, SourceTrimIntent, SourceTrimPolicy, SourceTrimResources,
    SplitIdentities,
};

fn mixed(policy: SourceTrimPolicy) -> SourceTrimIntent {
    SourceTrimIntent {
        in_frames: 1,
        out_frames: 1,
        slip_frames: 1,
        roll_frames: 1,
        policy,
    }
}

fn envelope(before: &ProjectDocument, next: &str, intent: SourceTrimIntent) -> Result<Value> {
    let resolution = before.source_trim_edit(
        before.root(),
        &NodeId::new("left")?,
        Some(&NodeId::new("right")?),
        intent,
    )?;
    Ok(json!({
        "protocol": 1,
        "project_id": before.project_id(),
        "expected_revision": before.revision_id(),
        "new_revision": next,
        "command": deadpan_core::Command::ApplySourceTrim {
            parent: before.root().clone(),
            node: NodeId::new("left")?,
            right: Some(NodeId::new("right")?),
            intent,
            resources: SourceTrimResources {
                target_wrapper: resolution.required_target_wrapper
                    .then(|| NodeId::new("a-crop").unwrap()),
                right_wrapper: resolution.required_right_wrapper
                    .then(|| NodeId::new("b-crop").unwrap()),
                split: SplitIdentities {
                    nodes: (0..resolution.required_split_nodes)
                        .map(|n| NodeId::new(format!("split-{n}")).unwrap()).collect(),
                },
                fillers: (0..resolution.required_filler_nodes)
                    .map(|n| NodeId::new(format!("filler-{n}")).unwrap()).collect(),
                timing: (resolution.capture != SourceTrimCapture::None).then(|| AudioTimingId {
                    allocation: RevisionId::new(next).unwrap(), ordinal: 0,
                }),
            },
        }
    }))
}

// Full authored/admission rows, not just counts: a dry run or refused command
// must preserve redo, evidence and current content as well as history length.
fn stored(database: &Connection) -> Result<String> {
    Ok(database.query_row(
        "SELECT json_array(
         (SELECT json_group_array(json_array(id,parent_id,kind,document)) FROM
             (SELECT * FROM revisions ORDER BY id)),
         (SELECT json_group_array(json_array(id,parent_id,revision_id,request,edit)) FROM
             (SELECT * FROM history ORDER BY id)),
         (SELECT json_group_array(json_array(singleton,head_revision,cursor,workflow)) FROM state),
         (SELECT json_group_array(json_array(position,history_id)) FROM
             (SELECT * FROM redo ORDER BY position)),
         (SELECT json_group_array(json_array(id,original_content_id,original_ref,hex(snapshot))) FROM
             (SELECT * FROM source_qualifications ORDER BY id)),
         (SELECT json_group_array(json_array(content_id,version,record)) FROM
             (SELECT * FROM original_media ORDER BY content_id)))",
        [],
        |row| row.get(0),
    )?)
}

fn literal_range(start: i64, end: i64) -> Result<Value> {
    Ok(serde_json::to_value(FrameRange::new(
        ProjectFrame(start),
        ProjectFrame(end),
    )?)?)
}

fn assert_final_children(document: &ProjectDocument, policy: SourceTrimPolicy) -> Result {
    let NodeKind::Sequence { children } = &document.nodes()[document.root()].kind else {
        panic!("ordinary root")
    };
    let expected = if policy == SourceTrimPolicy::Ripple {
        vec!["a-crop", "b-crop"]
    } else {
        vec!["filler-0", "a-crop", "b-crop"]
    };
    assert_eq!(
        children.iter().map(NodeId::as_str).collect::<Vec<_>>(),
        expected
    );
    let durations = document.durations()?;
    let expected = if policy == SourceTrimPolicy::Ripple {
        vec![4, 2]
    } else {
        vec![1, 4, 1]
    };
    assert_eq!(
        children
            .iter()
            .map(|id| durations[id].frames())
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(document.duration()?.frames(), 6);
    Ok(())
}

#[test]
fn combined_trim_cold_and_live_match_exact_geometry_transaction_and_fresh_history() -> Result {
    for policy in [SourceTrimPolicy::Ripple, SourceTrimPolicy::Overwrite] {
        for live_commit in [false, true] {
            let scratch = tempfile::tempdir()?;
            let package = ready(scratch.path())?;
            let path = package.to_str().unwrap();
            let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
            let initial = counts(&package)?;
            let database = Connection::open(package.join("project.sqlite"))?;
            let rows = stored(&database)?;
            let request = envelope(&before, "combined", mixed(policy))?;
            let file = save(scratch.path(), &request)?;
            let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
            let cold = success(&[
                "command",
                path,
                "--json",
                file.to_str().unwrap(),
                "--dry-run",
            ])?;
            let (live, saved) = hosted(&mut writer, &request, true)?;
            assert_eq!(cold, live);
            assert!(saved.is_none());
            assert_eq!(cold["protocol"], 1);
            assert_eq!(cold["committed"], false);
            let report = &cold["source_trim_edit"];
            assert_eq!(
                report["geometry"]["intent"],
                serde_json::to_value(mixed(policy))?
            );
            assert_eq!(
                report["geometry"]["target"]["output_before"],
                literal_range(0, 3)?
            );
            assert_eq!(
                report["geometry"]["right"]["output_before"],
                literal_range(3, 6)?
            );
            assert_eq!(report["geometry"]["target"]["asset"], "left");
            assert_eq!(report["geometry"]["right"]["asset"], "right");
            assert_ne!(
                report["geometry"]["target"]["qualification"],
                report["geometry"]["right"]["qualification"]
            );
            let (a_start, a_end, b_start, b_allocation) = if policy == SourceTrimPolicy::Ripple {
                (0, 4, 4, 1)
            } else {
                (1, 5, 5, 2)
            };
            assert_eq!(
                report["geometry"]["target"]["output_after"],
                literal_range(a_start, a_end)?
            );
            assert_eq!(report["right_after"]["output"], literal_range(b_start, 6)?);
            assert_eq!(
                report["geometry"]["target"]["allocation_after"],
                literal_range(1, 5)?
            );
            assert_eq!(
                report["right_after"]["allocation"],
                literal_range(b_allocation, 3)?
            );
            assert_eq!(report["required_target_wrapper"], true);
            assert_eq!(report["required_right_wrapper"], true);
            assert_eq!(
                report["fillers"],
                if policy == SourceTrimPolicy::Ripple {
                    json!([])
                } else {
                    json!([literal_range(0, 1)?])
                }
            );
            assert_eq!(cold["edit"]["duration_delta"], 0);
            let transaction: EditTransaction = serde_json::from_value(cold["edit"].clone())?;
            assert_eq!(writer.snapshot()?, before);
            assert_eq!(stored(&database)?, rows);
            let committed = if live_commit {
                let (value, saved) = hosted(&mut writer, &request, false)?;
                assert_eq!(saved, Some(RevisionId::new("combined")?));
                drop(writer);
                value
            } else {
                drop(writer);
                success(&["command", path, "--json", file.to_str().unwrap()])?
            };
            assert_eq!(committed["committed"], true);
            assert_eq!(committed["outcome"]["edit"], cold["edit"]);
            assert_eq!(counts(&package)?, (initial.0 + 1, initial.1 + 1, initial.2));
            let after = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
            assert_eq!(after, transaction.forward.apply(&before)?);
            assert_eq!(transaction.inverse.apply(&after)?, before);
            assert_final_children(&after, policy)?;
            let (wire, patch): (String, String) = database.query_row(
                "SELECT request,edit FROM history WHERE revision_id='combined'",
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
            let rows = stored(&database)?;
            assert_eq!(
                failure(&[
                    "command",
                    path,
                    "--json",
                    file.to_str().unwrap(),
                    "--dry-run"
                ])?["error"]["code"],
                "RevisionConflict"
            );
            assert_eq!(
                failure(&["command", path, "--json", file.to_str().unwrap()])?["error"]["code"],
                "RevisionConflict"
            );
            assert_eq!(stored(&database)?, rows);
            success(&["project", "undo", path, "--expected", "combined"])?;
            let undone = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
            same_content(&undone, &before)?;
            assert_ne!(undone.revision_id(), before.revision_id());
            assert_ne!(undone.revision_id(), after.revision_id());
            let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
            let deadpan_cli::macros::Execution {
                committed_revision: saved,
                ..
            } = execute_short(
                &mut writer,
                before.project_id(),
                &ShortOperation::History {
                    direction: HistoryDirection::Redo,
                    expected_revision: undone.revision_id().clone(),
                    new_revision: RevisionId::new("combined-redone")?,
                    dry_run: false,
                },
            )?;
            assert_eq!(saved, Some(RevisionId::new("combined-redone")?));
            drop(writer);
            let reopened = ProjectStore::open(&package, AccessMode::ReadOnly)?;
            let redone = reopened.snapshot()?;
            same_content(&redone, &after)?;
            assert_ne!(redone.revision_id(), after.revision_id());
            assert_ne!(redone.revision_id(), undone.revision_id());
            assert_eq!(counts(&package)?, (initial.0 + 3, initial.1 + 1, initial.2));
            reopened.validate()?;
        }
    }
    Ok(())
}

#[test]
fn combined_zero_and_invalid_resources_preserve_nonempty_redo_and_all_rows() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = ready(scratch.path())?;
    let path = package.to_str().unwrap();
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let initial = writer.snapshot()?;
    writer.commit(&CommandRequest {
        project_id: initial.project_id().clone(),
        expected_revision: initial.revision_id().clone(),
        new_revision: RevisionId::new("renamed")?,
        command: deadpan_core::Command::Rename {
            node: NodeId::new("left")?,
            label: "Saved name".into(),
        },
    })?;
    writer.undo(&RevisionId::new("renamed")?, RevisionId::new("entry")?)?;
    let before = writer.snapshot()?;
    let database = Connection::open(package.join("project.sqlite"))?;
    assert_eq!(
        database.query_row("SELECT count(*) FROM redo", [], |row| row.get::<_, i64>(0))?,
        1
    );
    let rows = stored(&database)?;
    let zero = envelope(&before, "unused", SourceTrimIntent::default())?;
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
    assert_eq!(cold["committed"], false);
    assert!(cold["edit"].is_null());
    assert_eq!(
        cold["source_trim_edit"]["geometry"]["intent"],
        serde_json::to_value(SourceTrimIntent::default())?
    );
    assert_eq!(cold["source_trim_edit"]["capture"], "none");
    assert!(hosted(&mut writer, &zero, false).is_err());
    assert_eq!(stored(&database)?, rows);
    drop(writer);
    assert_eq!(
        failure(&["command", path, "--json", file.to_str().unwrap()])?["error"]["code"],
        "InvalidCommand"
    );
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    for fault in [
        "a-wrapper",
        "b-wrapper",
        "split",
        "filler",
        "timing",
        "stale",
        "reused",
    ] {
        let mut bad = zero.clone();
        match fault {
            "a-wrapper" => bad["command"]["resources"]["target_wrapper"] = json!("unexpected"),
            "b-wrapper" => bad["command"]["resources"]["right_wrapper"] = json!("unexpected"),
            "split" => bad["command"]["resources"]["split"]["nodes"] = json!(["unexpected"]),
            "filler" => bad["command"]["resources"]["fillers"] = json!(["unexpected"]),
            "timing" => {
                bad["command"]["resources"]["timing"] = json!({"allocation":"unused","ordinal":0})
            }
            "stale" => bad["expected_revision"] = json!("ready"),
            "reused" => bad["new_revision"] = json!("renamed"),
            _ => unreachable!(),
        }
        let file = save(scratch.path(), &bad)?;
        let cold = failure(&[
            "command",
            path,
            "--json",
            file.to_str().unwrap(),
            "--dry-run",
        ])?;
        assert!(cold["error"]["code"].is_string(), "{fault}");
        assert!(hosted(&mut writer, &bad, true).is_err(), "{fault}");
        assert!(hosted(&mut writer, &bad, false).is_err(), "{fault}");
        assert_eq!(writer.snapshot()?, before);
        assert_eq!(stored(&database)?, rows, "{fault}");
    }
    let good = envelope(&before, "unused", mixed(SourceTrimPolicy::Overwrite))?;
    for fault in [
        "timing-allocation",
        "wrapper-collision",
        "extra-filler",
        "stale-pair",
    ] {
        // Ripple I=1 necessarily captures a target reanchor. Make the timing
        // case exercise allocation admission rather than an absent timing role
        // or a malformed optional JSON object on a fully bound Overwrite.
        let mut bad = if fault == "timing-allocation" {
            envelope(&before, "unused", mixed(SourceTrimPolicy::Ripple))?
        } else {
            good.clone()
        };
        match fault {
            "timing-allocation" => {
                bad["command"]["resources"]["timing"]["allocation"] = json!("wrong")
            }
            "wrapper-collision" => bad["command"]["resources"]["target_wrapper"] = json!("left"),
            "extra-filler" => bad["command"]["resources"]["fillers"] = json!(["filler-0", "extra"]),
            "stale-pair" => bad["command"]["right"] = json!("left"),
            _ => unreachable!(),
        }
        typed(&bad)?;
        let file = save(scratch.path(), &bad)?;
        assert!(
            failure(&[
                "command",
                path,
                "--json",
                file.to_str().unwrap(),
                "--dry-run"
            ])?["error"]["code"]
                .is_string()
        );
        assert!(hosted(&mut writer, &bad, true).is_err(), "{fault}");
        assert!(hosted(&mut writer, &bad, false).is_err(), "{fault}");
        assert_eq!(stored(&database)?, rows, "{fault}");
    }
    writer.redo(before.revision_id(), RevisionId::new("redo-survives")?)?;
    let redone = writer.snapshot()?;
    assert_ne!(redone.revision_id(), before.revision_id());
    assert_eq!(redone.nodes()[&NodeId::new("left")?].label, "Saved name");
    writer.validate()?;
    Ok(())
}

#[test]
fn used_qualification_refusal_including_retired_b_matches_both_dispatch_paths() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = ready(scratch.path())?;
    let path = package.to_str().unwrap();
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let before = writer.snapshot()?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let clean = stored(&database)?;
    let ripple = envelope(&before, "unused", mixed(SourceTrimPolicy::Ripple))?;
    let retire = envelope(
        &before,
        "unused",
        SourceTrimIntent {
            out_frames: 2,
            roll_frames: 1,
            policy: SourceTrimPolicy::Overwrite,
            ..Default::default()
        },
    )?;
    let typed_retire = typed(&retire)?;
    assert!(
        writer
            .preview_source_trim_edit(&typed_retire)?
            .resolution
            .right_after
            .is_none()
    );
    for (asset, other) in [("left", "right"), ("right", "left")] {
        let qualification = before.assets()[&AssetId::new(asset)?]
            .source_qualification
            .as_ref()
            .unwrap();
        let (content, original_ref, receipt): (String,String,Vec<u8>) = database.query_row(
            "SELECT original_content_id,original_ref,snapshot FROM source_qualifications WHERE id=?1",
            [qualification.as_str()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
        )?;
        let other_receipt =
            writer.registered_source(before.revision_id(), &AssetId::new(other)?)?;
        database.execute(
            "DELETE FROM source_qualifications WHERE id=?1",
            [qualification.as_str()],
        )?;
        let rows = stored(&database)?;
        assert_eq!(
            writer.registered_source(before.revision_id(), &AssetId::new(other)?)?,
            other_receipt
        );
        for request in [&ripple, &retire] {
            let file = save(scratch.path(), request)?;
            let cold = failure(&[
                "command",
                path,
                "--json",
                file.to_str().unwrap(),
                "--dry-run",
            ])?;
            // Cold dispatch refuses at package-open historical validation.
            // The already-open owner below exercises this command's used-source gate.
            assert_eq!(cold["error"]["code"], "SourceRegistrationInvalid");
            let live = hosted(&mut writer, request, true).unwrap_err();
            assert!(live.to_string().contains("qualification is missing"));
            assert!(hosted(&mut writer, request, false).is_err());
            assert_eq!(writer.snapshot()?, before);
            assert_eq!(stored(&database)?, rows);
        }
        database.execute(
            "INSERT INTO source_qualifications(id,original_content_id,original_ref,snapshot) VALUES(?1,?2,?3,?4)",
            rusqlite::params![qualification.as_str(), content, original_ref, receipt],
        )?;
        assert_eq!(stored(&database)?, clean);
    }
    // Refused attempts never reserve their result revision. The same request
    // succeeds after exact evidence restoration, with one durable transaction.
    let file = save(scratch.path(), &retire)?;
    let cold = success(&[
        "command",
        path,
        "--json",
        file.to_str().unwrap(),
        "--dry-run",
    ])?;
    let (committed, saved) = hosted(&mut writer, &retire, false)?;
    assert_eq!(saved, Some(RevisionId::new("unused")?));
    assert_eq!(committed["outcome"]["edit"], cold["edit"]);
    assert!(
        !writer
            .snapshot()?
            .nodes()
            .contains_key(&NodeId::new("right")?)
    );
    writer.validate()?;
    Ok(())
}
