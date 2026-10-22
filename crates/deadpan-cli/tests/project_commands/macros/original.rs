//! Original invocation clocks use the saved measured index, never Edit frames.
use super::*;
use deadpan_core::{Anchor, AssetId, SourceMoment, SourceStream};

fn qualified_project(root: &Path) -> Result<(PathBuf, Value)> {
    let package = create(root)?;
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures/vfr.mp4")
        .canonicalize()?;
    let retained = success(&[
        "project",
        "retain-original",
        package.to_str().unwrap(),
        fixture.to_str().unwrap(),
    ])?;
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let path = root.join("source-request.json");
    fs::write(
        &path,
        json!({"protocol":1,"registration":{
        "expected_revision":before.revision_id(),"new_revision":"qualified",
        "original":retained["retained_original"]["record"]["object"]["content"],
        "new_asset_id":"camera","label":"Measured VFR Original",
        "insertion":{"parent":before.root(),"index":0,"node":"camera-source","label":"Original"}
    },"streams":{"type":"video_only"}})
        .to_string(),
    )?;
    success(&[
        "project",
        "register-source",
        package.to_str().unwrap(),
        "--request-json",
        path.to_str().unwrap(),
    ])?;
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let document = store.snapshot()?;
    let qualification = document.assets()[&AssetId::new("camera")?]
        .source_qualification
        .as_ref()
        .unwrap();
    let index = store.source_video_index(document.revision_id(), &AssetId::new("camera")?)?;
    assert!(index.frames().len() >= 4);
    assert!(
        index
            .frames()
            .windows(3)
            .any(|frames| frames[1].pts - frames[0].pts != frames[2].pts - frames[1].pts)
    );
    Ok((
        package,
        json!({"asset":"camera","qualification":qualification,"ordinal":1}),
    ))
}

fn invocation(
    document: &ProjectDocument,
    version: u64,
    original: &Value,
    instructions: Value,
) -> Value {
    envelope(
        document,
        version,
        json!({"type":"apply","program":{"instructions":instructions},
        "parent":document.root(),"cursor":1,"selected_child":"camera-source",
        "visual_selection":{"type":"time","anchor":1,"head":0,"extending":false},
        "original":original,"new_revision":"original-run"}),
    )
}

#[test]
fn original_macro_vfr_marks_dry_run_reopen_undo_redo_and_navigation_only() -> Result {
    let scratch = tempfile::tempdir()?;
    let (package, original) = qualified_project(scratch.path())?;
    let input = scratch.path().join("macro.json");
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let before = store.snapshot()?;
    let index = store.source_video_index(before.revision_id(), &AssetId::new("camera")?)?;
    drop(store);
    let body = json!([
        {"type":"set_mark","letter":"k"},
        {"type":"move_frames","forward":true,"count":2},
        {"type":"jump_mark","letter":"k"}
    ]);
    invoke(&package, &input, &save(&before, 0, "r", body), false)?;
    let mut request = run(&before, 1, "r", 1);
    request["operation"]["original"] = original.clone();
    request["operation"]["selected_child"] = json!("camera-source");
    request["operation"]["visual_selection"] =
        json!({"type":"time","anchor":1,"head":0,"extending":false});
    let unchanged = state(&package)?;
    let preview = invoke(&package, &input, &request, true)?;
    assert_eq!(state(&package)?, unchanged);
    assert_eq!(preview["context"]["original"], original);
    assert_eq!(preview["context"]["cursor"], 1);
    assert_eq!(preview["context"]["selected_child"], "camera-source");
    assert_eq!(
        preview["context"]["visual_selection"],
        request["operation"]["visual_selection"]
    );
    assert!(
        preview["trace"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["before_cursor"] == 1 && row["after_cursor"] == 1)
    );
    assert!(
        preview["trace"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["after_original"]["ordinal"] == 3)
    );
    let result = invoke(&package, &input, &request, false)?;
    assert_eq!(result["context"], preview["context"]);
    assert_eq!(result["mark_changes"], preview["mark_changes"]);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let after = store.snapshot()?;
    let mark = &after.marks()[&deadpan_core::native_mark_id('k')?];
    assert_eq!(mark.owner, *after.root());
    assert!(mark.bindings().all(|binding| matches!(&binding.coordinate,
        Anchor::Source { asset, moment: SourceMoment::Timestamp { stream: SourceStream::Video, timestamp } }
        if asset == &AssetId::new("camera").unwrap() && timestamp.ticks == index.frames()[1].pts && timestamp.time_base == index.time_base())));
    let bank = store.registers()?;
    store.undo(after.revision_id(), RevisionId::new("original-undo")?)?;
    same_document(&store.snapshot()?, &before)?;
    drop(store);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    store.redo(
        &RevisionId::new("original-undo")?,
        RevisionId::new("original-redo")?,
    )?;
    same_document(&store.snapshot()?, &after)?;
    assert_eq!(store.registers()?, bank);
    store.validate()?;
    let redone = store.snapshot()?;
    drop(store);
    let navigation = invocation(
        &redone,
        bank.version,
        &original,
        json!([
            {"type":"move_scope","end":true}, {"type":"jump_mark","letter":"k"}
        ]),
    );
    let unchanged = state(&package)?;
    let result = invoke(&package, &input, &navigation, false)?;
    assert_eq!(result["context"]["original"], original);
    assert_eq!(result["committed"], false);
    assert!(result["committed_revision"].is_null());
    assert!(result["committed_registers"].is_null());
    assert_eq!(state(&package)?, unchanged);
    Ok(())
}

#[test]
fn original_macro_admission_and_late_failure_leave_history_and_registers_untouched() -> Result {
    let scratch = tempfile::tempdir()?;
    let (package, original) = qualified_project(scratch.path())?;
    let input = scratch.path().join("macro.json");
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let set = json!({"type":"set_mark","letter":"k"});
    let mut stale = original.clone();
    stale["qualification"] = json!("0".repeat(64));
    let error = reject(
        &package,
        &input,
        &invocation(&before, 0, &stale, json!([set.clone()])),
    )?;
    assert_eq!(error["error"]["code"], "OriginalUnavailable");
    let mut absent = original.clone();
    absent["asset"] = json!("missing");
    assert_eq!(
        reject(
            &package,
            &input,
            &invocation(&before, 0, &absent, json!([set.clone()]))
        )?["error"]["code"],
        "OriginalUnavailable"
    );
    let failed = invocation(
        &before,
        0,
        &original,
        json!([
            set.clone(), {"type":"move_frames","forward":true,"count":2},
            {"type":"call","register":"z","count":1}
        ]),
    );
    let error = reject(&package, &input, &failed)?;
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("called macro register is empty")
    );
    let mut untrusted_index = original.clone();
    untrusted_index["index"] = json!({"frames":[]});
    assert_eq!(
        reject(
            &package,
            &input,
            &invocation(&before, 0, &untrusted_index, json!([set.clone()]))
        )?["error"]["code"],
        "HostProtocolInvalid"
    );
    // Unsupported edits cannot accidentally operate on the retained Edit context.
    reject(
        &package,
        &input,
        &invocation(
            &before,
            0,
            &original,
            json!([
                set.clone(), {"type":"yank_beat","register":"b"}
            ]),
        ),
    )?;
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let mut forged = serde_json::to_value(&before)?;
    forged["assets"]["camera"]["content_hash"] = json!("e".repeat(64));
    let forged = ProjectDocument::from_json(&forged.to_string())?;
    let forged_context = deadpan_core::SemanticContext {
        hold_duration: None,
        selected_hold_duration: None,
        mixed_hold_duration: None,
        shared_hold_duration: None,
        scoped: None,
        parent: forged.root().clone(),
        cursor: deadpan_core::ProjectFrame(1),
        selected_child: None,
        visual_selection: None,
        original: Some(serde_json::from_value(original.clone())?),
    };
    let body =
        deadpan_core::SemanticProgram::new(vec![deadpan_core::SemanticInstruction::SetMark {
            letter: 'k',
        }])?;
    let unchanged = state(&package)?;
    let error = deadpan_cli::macros::plan_program(
        &store,
        &forged,
        &store.registers()?,
        &forged_context,
        &body,
        RevisionId::new("forged")?,
    )
    .unwrap_err();
    assert!(error.to_string().contains("saved qualification"));
    assert_eq!(state(&package)?, unchanged);
    store.commit(&deadpan_core::CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: RevisionId::new("named-mark")?,
        command: Command::SetMark {
            id: deadpan_core::native_mark_id('k')?,
            owner: before.root().clone(),
            label: "Named mark".into(),
            boundary: deadpan_core::BoundaryAnchor {
                coordinate: Anchor::Sequence {
                    frame: deadpan_core::ProjectFrame(1),
                },
                bias: deadpan_core::InsertionBias::Right,
            },
            loss_policy: deadpan_core::AnchorLossPolicy::KeepUnresolved,
        },
    })?;
    let before = store.snapshot()?;
    drop(store);
    let error = reject(
        &package,
        &input,
        &invocation(&before, 0, &original, json!([set])),
    )?;
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("conflicts")
    );
    Ok(())
}
