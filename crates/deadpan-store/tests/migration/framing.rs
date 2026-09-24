use super::*;
use deadpan_core::*;

fn request(doc: &ProjectDocument, revision: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: doc.project_id().clone(),
        expected_revision: doc.revision_id().clone(),
        new_revision: RevisionId::new(revision).unwrap(),
        command,
    }
}

fn fixture(path: &Path) -> Result<ProjectDocument> {
    let initial = ProjectDocument::new(
        ProjectId::new("framing-store")?,
        RevisionId::new("initial")?,
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    let mut store = ProjectStore::create(path, &initial)?;
    for (revision, at, length, name) in
        [("pause-one", 0, 8, "hold"), ("pause-two", 3, 2, "inserted")]
    {
        let before = store.snapshot()?;
        store.commit(&request(
            &before,
            revision,
            Command::InsertTime {
                at: ProjectFrame(at),
                hold: HoldRecipe {
                    duration: FrameDuration::new(length)?,
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                },
                id: NodeId::new(name)?,
                identities: SplitIdentities {
                    nodes: (0..before.nodes().len() + 4)
                        .map(|i| NodeId::new(format!("{revision}-copy-{i}")).unwrap())
                        .collect(),
                },
                timing: AudioTimingId {
                    allocation: RevisionId::new(revision)?,
                    ordinal: 0,
                },
            },
        ))?;
    }
    store.commit(&request(
        &store.snapshot()?,
        "rename",
        Command::Rename {
            node: NodeId::new("inserted")?,
            label: "Retained pause".into(),
        },
    ))?;
    store.undo(&RevisionId::new("rename")?, RevisionId::new("undo")?)?;
    store.redo(&RevisionId::new("undo")?, RevisionId::new("redo")?)?;
    store.undo(&RevisionId::new("redo")?, RevisionId::new("pending")?)?;
    let current = store.snapshot()?;
    store.validate()?;
    drop(store);
    // Generated using only core17 operations. This unit fixture proves frozen
    // grammar/replay, not provenance from an independently built old binary.
    let db = Connection::open(path.join("project.sqlite"))?;
    db.execute(
        "UPDATE revisions SET document=json_set(document,'$.schema_version',17)",
        [],
    )?;
    db.pragma_update(None, "user_version", 23)?;
    for (_, json) in docs(&db)? {
        legacy_v17::Document::from_json(&json)?;
    }
    for (request, _) in history_json(&db)? {
        legacy_v17::upgrade_request(&request)?;
    }
    Ok(current)
}

fn framing() -> Framing {
    Framing::creep(
        FramingPose::identity(),
        FramingPose::new(
            ExactRatio::new(1, 3).unwrap(),
            ExactRatio::new(2, 3).unwrap(),
            ExactRatio::new(27, 20).unwrap(),
        )
        .unwrap(),
        FramingCurve::Smoothstep,
    )
    .unwrap()
}

#[test]
fn schema23_keeps_insert_time_history_and_redo_then_persists_framing() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("framing.deadpan");
    let expected = fixture(&path)?;
    let db = Connection::open(path.join("project.sqlite"))?;
    let before = contents(&db)?;
    let old_documents = docs(&db)?;
    let old_history = history_json(&db)?;
    let old_metadata = metadata(&db)?;
    let old_operational = operational_metadata(&db)?;
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(23))
    ));
    let outcome = ProjectStore::migrate(&path)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (23, DATABASE_SCHEMA_VERSION)
    );
    assert_eq!(
        contents(&Connection::open(outcome.backup.unwrap())?)?,
        before
    );
    assert_eq!(metadata(&db)?, old_metadata);
    assert_eq!(operational_metadata(&db)?, old_operational);
    for ((old_id, old), (new_id, new)) in old_documents.iter().zip(docs(&db)?) {
        assert_eq!(old_id, &new_id);
        let modern = ProjectDocument::from_json(&new)?;
        let frozen = legacy_v17::Document::from_json(old)?;
        assert!(frozen.matches(&modern));
        assert_eq!(frozen.upgrade()?, modern);
        let mut expected: serde_json::Value = serde_json::from_str(old)?;
        expected["schema_version"] = serde_json::json!(DOCUMENT_SCHEMA_VERSION);
        assert_eq!(serde_json::to_value(modern)?, expected);
    }
    for ((old_request, old_edit), (new_request, new_edit)) in
        old_history.iter().zip(history_json(&db)?)
    {
        let modern: EditTransaction = serde_json::from_str(&new_edit)?;
        assert!(legacy_v17::matches_edit(old_edit, &modern)?);
        assert_eq!(
            legacy_v17::upgrade_request(old_request)?,
            serde_json::from_str::<CommandRequest>(&new_request)?
        );
    }
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, expected);
    store.redo(expected.revision_id(), RevisionId::new("redo-migrated")?)?;
    let base = store.snapshot()?;
    let change = request(
        &base,
        "framing",
        Command::SetFraming {
            node: NodeId::new("inserted")?,
            framing: Some(framing()),
        },
    );
    store.commit(&change)?;
    let framed = store.snapshot()?;
    assert_eq!(framed.audio_bindings(), base.audio_bindings());
    assert_eq!(
        framed.nodes()[&NodeId::new("inserted")?].framing,
        Some(framing())
    );
    let count = docs(&db)?.len();
    assert!(store.commit(&change).is_err());
    assert_eq!(docs(&db)?.len(), count);
    store.undo(framed.revision_id(), RevisionId::new("undo-framing")?)?;
    assert_eq!(store.snapshot()?.nodes(), base.nodes());
    store.redo(
        &RevisionId::new("undo-framing")?,
        RevisionId::new("redo-framing")?,
    )?;
    store.validate()?;
    let final_doc = store.snapshot()?;
    drop(store);
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)?.snapshot()?,
        final_doc
    );
    Ok(())
}

#[test]
fn schema23_cannot_smuggle_new_node_fields_and_failed_migration_keeps_original() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("invalid-framing.deadpan");
    fixture(&path)?;
    let db = Connection::open(path.join("project.sqlite"))?;
    db.execute("UPDATE revisions SET document=json_set(document,'$.nodes.root.framing',NULL) WHERE kind='initial'",[])?;
    let before = contents(&db)?;
    let Err(StoreError::MigrationFailed { backup, .. }) = ProjectStore::migrate(&path) else {
        panic!("new field must fail frozen grammar")
    };
    assert_eq!(contents(&db)?, before);
    assert_eq!(contents(&Connection::open(backup)?)?, before);
    assert_eq!(
        db.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
        23
    );
    Ok(())
}

#[test]
fn frozen_v17_rejects_framing_in_commands_subtrees_occurrences_and_patches() -> Result {
    let doc = ProjectDocument::new(
        ProjectId::new("p")?,
        RevisionId::new("r")?,
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    let change = request(
        &doc,
        "new",
        Command::SetFraming {
            node: NodeId::new("root")?,
            framing: Some(framing()),
        },
    );
    assert!(legacy_v17::upgrade_request(&serde_json::to_string(&change)?).is_err());
    let tx = deadpan_core::apply(&doc, &change)?;
    assert!(legacy_v17::matches_edit(&serde_json::to_string(&tx)?, &tx).is_err());
    let mut wire = serde_json::to_value(&change)?;
    wire["command"] = serde_json::json!({"command":"edit_occurrence","instance":{"node":"root","repeats":[]},"edit":{"type":"set_framing","framing":null},"identities":{"nodes":[],"marks":[]}});
    assert!(legacy_v17::upgrade_request(&wire.to_string()).is_err());
    wire["command"] = serde_json::json!({"command":"insert","parent":"root","index":0,"subtree":{"root":"copy","nodes":{"copy":{"label":"copy","kind":{"type":"sequence","children":[]},"framing":null}},"overrides":{}}});
    assert!(legacy_v17::upgrade_request(&wire.to_string()).is_err());
    Ok(())
}

#[test]
fn actual_old_binary_schema23_fixture_replays_every_snapshot_and_pending_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("old-binary.deadpan");
    fs::create_dir(&package)?;
    fs::create_dir(package.join("Snapshots"))?;
    let db = Connection::open(package.join("project.sqlite"))?;
    db.pragma_update(None, "foreign_keys", false)?;
    db.execute_batch(include_str!("../fixtures/v23-framing-history.sql"))?;
    let before = contents(&db)?;
    let old_docs = docs(&db)?;
    let old_history = history_json(&db)?;
    let old_metadata = metadata(&db)?;
    let old_operational = operational_metadata(&db)?;
    assert_eq!(old_docs.len(), 7);
    assert_eq!(old_history.len(), 3);
    let outcome = ProjectStore::migrate(&package)?;
    assert_eq!(
        contents(&Connection::open(outcome.backup.unwrap())?)?,
        before
    );
    assert_eq!(metadata(&db)?, old_metadata);
    assert_eq!(operational_metadata(&db)?, old_operational);
    for ((old_id, old), (new_id, new)) in old_docs.iter().zip(docs(&db)?) {
        assert_eq!(old_id, &new_id);
        let modern = ProjectDocument::from_json(&new)?;
        assert!(legacy_v17::Document::from_json(old)?.matches(&modern));
        let mut expected: serde_json::Value = serde_json::from_str(old)?;
        expected["schema_version"] = serde_json::json!(DOCUMENT_SCHEMA_VERSION);
        assert_eq!(serde_json::to_value(modern)?, expected);
    }
    for ((old_request, old_edit), (new_request, new_edit)) in
        old_history.iter().zip(history_json(&db)?)
    {
        assert_eq!(
            legacy_v17::upgrade_request(old_request)?,
            serde_json::from_str::<CommandRequest>(&new_request)?
        );
        assert!(legacy_v17::matches_edit(
            old_edit,
            &serde_json::from_str(&new_edit)?
        )?);
    }
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let before = store.snapshot()?;
    assert_eq!(before.nodes()[&NodeId::new("inserted")?].label, "Pause");
    store.redo(before.revision_id(), RevisionId::new("current-redo")?)?;
    assert_eq!(
        store.snapshot()?.nodes()[&NodeId::new("inserted")?].label,
        "Old binary pause"
    );
    store.validate()?;
    Ok(())
}
