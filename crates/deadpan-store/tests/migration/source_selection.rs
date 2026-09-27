use super::*;
use deadpan_core::*;

fn fixture(root: &Path) -> Result<PathBuf> {
    let package = root.join("source-selection.deadpan");
    fs::create_dir(&package)?;
    fs::create_dir(package.join("Snapshots"))?;
    let db = Connection::open(package.join("project.sqlite"))?;
    db.pragma_update(None, "foreign_keys", false)?;
    db.execute_batch(include_str!("../fixtures/v25-captured-audio-history.sql"))?;
    Ok(package)
}
fn selected() -> SourceAudioMapping {
    SourceAudioMapping::SelectedPlacement {
        start: ExactRatio::new(-2, 3).unwrap(),
        frames: ExactRatio::new(30000, 1001).unwrap(),
        selection: ExactFrameRange::new(ExactRatio::integer(2), ExactRatio::integer(8)).unwrap(),
    }
}

#[test]
fn actual_schema25_binary_preserves_captured_views_audio_history_and_pending_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let db = Connection::open(package.join("project.sqlite"))?;
    let before = contents(&db)?;
    let old_docs = docs(&db)?;
    let old_history = history_json(&db)?;
    let old_metadata = metadata(&db)?;
    let old_operational = operational_metadata(&db)?;
    assert_eq!(old_docs.len(), 19);
    assert_eq!(old_history.len(), 11);
    assert!(matches!(
        ProjectStore::open(&package, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(25))
    ));
    let outcome = ProjectStore::migrate(&package)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (25, DATABASE_SCHEMA_VERSION)
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
        let legacy = legacy_v19::Document::from_json(old)?;
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
            legacy_v19::upgrade_request(old_request)?,
            serde_json::from_str::<CommandRequest>(&new_request)?
        );
        assert!(legacy_v19::matches_edit(
            old_edit,
            &serde_json::from_str::<EditTransaction>(&new_edit)?
        )?);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(old_edit)?,
            serde_json::from_str::<serde_json::Value>(&new_edit)?
        );
    }
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let before_redo = store.snapshot()?;
    store.redo(
        before_redo.revision_id(),
        RevisionId::new("v25-pending-redo")?,
    )?;
    assert_eq!(
        store.snapshot()?.nodes()[&NodeId::new("source")?].label,
        "Retained pending rename"
    );
    store.undo(
        &RevisionId::new("v25-pending-redo")?,
        RevisionId::new("v25-undo-redo")?,
    )?;
    let baseline = store.snapshot()?;
    assert_eq!(baseline.nodes(), before_redo.nodes());
    let target = NodeId::new("source")?;
    let req = CommandRequest {
        project_id: baseline.project_id().clone(),
        expected_revision: baseline.revision_id().clone(),
        new_revision: RevisionId::new("selected")?,
        command: Command::SetSourceAudioMapping {
            node: target.clone(),
            mapping: selected(),
            offset: AudioSample(-31),
        },
    };
    store.commit(&req)?;
    let after = store.snapshot()?;
    let NodeKind::Source { source } = &after.nodes()[&target].kind else {
        unreachable!()
    };
    assert_eq!(source.audio_mapping, selected());
    assert_eq!(source.audio_offset, AudioSample(-31));
    assert_eq!(baseline.duration()?, after.duration()?);
    assert_eq!(baseline.audio_bindings(), after.audio_bindings());
    assert!(after.nodes().values().any(
        |node| matches!(&node.kind,NodeKind::Hold{recipe} if recipe.picture_context.is_some())
    ));
    assert!(after.nodes().values().any(|node|matches!(&node.kind,NodeKind::Repeat{gap:Some(recipe),..} if recipe.picture_context.is_some())));
    store.undo(after.revision_id(), RevisionId::new("undo-selection")?)?;
    assert_eq!(store.snapshot()?.nodes(), baseline.nodes());
    store.redo(
        &RevisionId::new("undo-selection")?,
        RevisionId::new("redo-selection")?,
    )?;
    assert_eq!(store.snapshot()?.nodes(), after.nodes());
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
fn schema25_rejects_new_mapping_variant_or_null_selection_without_touching_original() -> Result {
    for mapping in [
        serde_json::to_value(selected())?,
        serde_json::json!({"type":"duration","frames":{"numerator":"30","denominator":"1"},"selection":null}),
    ] {
        let scratch = tempfile::tempdir()?;
        let package = fixture(scratch.path())?;
        let db = Connection::open(package.join("project.sqlite"))?;
        let (revision, json) = docs(&db)?
            .into_iter()
            .find(|(_, json)| {
                serde_json::from_str::<serde_json::Value>(json).unwrap()["nodes"]["source"]
                    .is_object()
            })
            .unwrap();
        let mut wire: serde_json::Value = serde_json::from_str(&json)?;
        wire["nodes"]["source"]["kind"]["source"]["audio_mapping"] = mapping;
        let encoded = wire
            .to_string()
            .replace("\"selection\":", "\"selec\\u0074ion\":");
        db.execute(
            "UPDATE revisions SET document=?1 WHERE id=?2",
            rusqlite::params![encoded, revision],
        )?;
        let before = contents(&db)?;
        let Err(StoreError::MigrationFailed { backup, .. }) = ProjectStore::migrate(&package)
        else {
            panic!("old source admitted new mapping vocabulary")
        };
        assert_eq!(contents(&db)?, before);
        assert_eq!(contents(&Connection::open(backup)?)?, before);
        assert_eq!(
            db.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
            25
        );
    }
    Ok(())
}
