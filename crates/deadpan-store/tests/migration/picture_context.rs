use super::*;
use deadpan_core::*;

fn fixture(root: &Path) -> Result<PathBuf> {
    let package = root.join("composition.deadpan");
    fs::create_dir(&package)?;
    fs::create_dir(package.join("Snapshots"))?;
    let db = Connection::open(package.join("project.sqlite"))?;
    db.pragma_update(None, "foreign_keys", false)?;
    db.execute_batch(include_str!("../fixtures/v24-composition-history.sql"))?;
    Ok(package)
}
fn captured() -> CapturedFraming {
    CapturedFraming::capture(
        None,
        CapturedCanvas {
            width: 16,
            height: 16,
            fit: CapturedFit::Fill,
            layers: vec![Some(FramingPose::identity())],
        },
    )
    .unwrap()
}

#[test]
fn actual_schema24_binary_fixture_preserves_every_snapshot_history_backup_and_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let db = Connection::open(package.join("project.sqlite"))?;
    let before = contents(&db)?;
    let old_docs = docs(&db)?;
    let old_history = history_json(&db)?;
    let old_metadata = metadata(&db)?;
    let old_operational = operational_metadata(&db)?;
    assert_eq!(old_docs.len(), 8);
    assert_eq!(old_history.len(), 4);
    assert!(matches!(
        ProjectStore::open(&package, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(24))
    ));
    let outcome = ProjectStore::migrate(&package)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (24, DATABASE_SCHEMA_VERSION)
    );
    assert_eq!(
        contents(&Connection::open(outcome.backup.unwrap())?)?,
        before
    );
    assert_eq!(metadata(&db)?, old_metadata);
    assert_eq!(operational_metadata(&db)?, old_operational);
    for ((old_id, old), (new_id, new)) in old_docs.iter().zip(docs(&db)?) {
        assert_eq!(old_id, &new_id);
        let modern = ProjectDocument::from_json(&new)?;
        let legacy = legacy_v18::Document::from_json(old)?;
        assert!(legacy.matches(&modern));
        assert_eq!(legacy.upgrade()?, modern);
        let mut expected: serde_json::Value = serde_json::from_str(old)?;
        expected["schema_version"] = serde_json::json!(DOCUMENT_SCHEMA_VERSION);
        assert_eq!(serde_json::to_value(modern)?, expected);
    }
    for ((old_request, old_edit), (new_request, new_edit)) in
        old_history.iter().zip(history_json(&db)?)
    {
        assert_eq!(
            legacy_v18::upgrade_request(old_request)?,
            serde_json::from_str::<CommandRequest>(&new_request)?
        );
        assert!(legacy_v18::matches_edit(
            old_edit,
            &serde_json::from_str::<EditTransaction>(&new_edit)?
        )?);
    }
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let head = store.snapshot()?;
    assert!(head.nodes().values().any(|node| node.framing.is_some()));
    store.redo(head.revision_id(), RevisionId::new("v24-pending-redo")?)?;
    let redone = store.snapshot()?;
    store.undo(redone.revision_id(), RevisionId::new("v24-undo-redo")?)?;
    let before_capture = store.snapshot()?;
    let target = before_capture
        .nodes()
        .iter()
        .find(|(_, node)| matches!(node.kind, NodeKind::Hold { .. }))
        .unwrap()
        .0
        .clone();
    let req = CommandRequest {
        project_id: before_capture.project_id().clone(),
        expected_revision: before_capture.revision_id().clone(),
        new_revision: RevisionId::new("capture")?,
        command: Command::SetHoldPictureContext {
            node: target.clone(),
            context: Some(captured()),
        },
    };
    store.commit(&req)?;
    let after = store.snapshot()?;
    let NodeKind::Hold { recipe } = &after.nodes()[&target].kind else {
        unreachable!()
    };
    assert_eq!(recipe.picture_context, Some(captured()));
    assert_eq!(after.duration()?, before_capture.duration()?);
    assert_eq!(after.audio_bindings(), before_capture.audio_bindings());
    store.undo(after.revision_id(), RevisionId::new("undo-capture")?)?;
    store.redo(
        &RevisionId::new("undo-capture")?,
        RevisionId::new("redo-capture")?,
    )?;
    store.validate()?;
    let expected = store.snapshot()?;
    drop(store);
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
        expected
    );
    Ok(())
}

#[test]
fn schema24_rejects_injected_null_context_without_touching_original_or_backup() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let db = Connection::open(package.join("project.sqlite"))?;
    let (revision, json) = docs(&db)?
        .into_iter()
        .find(|(_, json)| {
            let wire: serde_json::Value = serde_json::from_str(json).unwrap();
            wire["nodes"]
                .as_object()
                .unwrap()
                .values()
                .any(|n| n["kind"]["type"] == "hold")
        })
        .unwrap();
    let mut wire: serde_json::Value = serde_json::from_str(&json)?;
    let node = wire["nodes"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .find(|n| n["kind"]["type"] == "hold")
        .unwrap();
    node["kind"]["recipe"]["picture_context"] = serde_json::Value::Null;
    db.execute(
        "UPDATE revisions SET document=?1 WHERE id=?2",
        rusqlite::params![wire.to_string(), revision],
    )?;
    let before = contents(&db)?;
    let Err(StoreError::MigrationFailed { backup, .. }) = ProjectStore::migrate(&package) else {
        panic!("old recipe admitted new field")
    };
    assert_eq!(contents(&db)?, before);
    assert_eq!(contents(&Connection::open(backup)?)?, before);
    assert_eq!(
        db.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
        24
    );
    Ok(())
}
