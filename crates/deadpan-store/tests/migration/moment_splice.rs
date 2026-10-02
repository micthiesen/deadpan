use super::*;
use deadpan_core::*;
use serde_json::{Value, json};

fn fixture(root: &Path) -> Result<PathBuf> {
    let package = root.join("moment-splice-history.deadpan");
    fs::create_dir(&package)?;
    fs::create_dir(package.join("Snapshots"))?;
    let database = Connection::open(package.join("project.sqlite"))?;
    database.pragma_update(None, "foreign_keys", false)?;
    database.execute_batch(include_str!("../fixtures/v32-moment-splice-history.sql"))?;
    Ok(package)
}

#[test]
fn actual_schema32_history_retains_nested_interiors_gap_clocks_and_pending_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_documents = docs(&database)?;
    let old_history = history_json(&database)?;
    let old_metadata = metadata(&database)?;
    let old_operational = operational_metadata(&database)?;
    assert_eq!((old_documents.len(), old_history.len()), (32, 15));
    assert!(matches!(
        ProjectStore::open(&package, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(32))
    ));
    let outcome = ProjectStore::migrate(&package)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (32, DATABASE_SCHEMA_VERSION)
    );
    assert_eq!(
        contents(&Connection::open(outcome.backup.unwrap())?)?,
        before
    );
    assert_eq!(metadata(&database)?, old_metadata);
    assert_eq!(operational_metadata(&database)?, old_operational);
    for ((old_id, old), (new_id, new)) in old_documents.iter().zip(docs(&database)?) {
        assert_eq!(old_id, &new_id);
        let modern = ProjectDocument::from_json(&new)?;
        let legacy = legacy_v26::Document::from_json(old)?;
        assert!(legacy.matches(&modern));
        assert_eq!(legacy.upgrade()?, modern);
        let mut expected: Value = serde_json::from_str(old)?;
        expected["schema_version"] = json!(DOCUMENT_SCHEMA_VERSION);
        assert_eq!(serde_json::to_value(modern)?, expected);
    }
    for ((old_request, old_edit), (new_request, new_edit)) in
        old_history.iter().zip(history_json(&database)?)
    {
        let request: CommandRequest = serde_json::from_str(&new_request)?;
        let edit: EditTransaction = serde_json::from_str(&new_edit)?;
        assert_eq!(legacy_v26::upgrade_request(old_request)?, request);
        assert!(legacy_v26::matches_edit(old_edit, &edit)?);
        // This semantic boundary adds no wire fields and must not recapture or
        // normalize any admitted old binding, command or patch.
        assert_eq!(old_request, &new_request);
        assert_eq!(old_edit, &new_edit);
        assert_eq!(
            serde_json::from_str::<Value>(old_request)?,
            serde_json::to_value(&request)?
        );
        assert_eq!(
            serde_json::from_str::<Value>(old_edit)?,
            serde_json::to_value(&edit)?
        );
        let prior = snapshot(&database, request.expected_revision.as_str())?;
        let after = snapshot(&database, request.new_revision.as_str())?;
        assert_eq!(edit.forward.apply(&prior)?, after);
        assert_eq!(edit.inverse.apply(&after)?, prior);
    }
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let baseline = store.snapshot()?;
    let gap = NodeId::new("core23-detached-gap")?;
    let repeat = NodeId::new("old-gap-owner")?;
    assert_eq!(baseline.gap_overrides()[&repeat].len(), 3);
    assert_eq!(
        baseline.audio_bindings().bindings()[&gap]
            .lattice
            .reference
            .recipe,
        AudioRecipeKind::RepeatGap
    );
    assert!(baseline.nodes().contains_key(&NodeId::new("core23-pause")?));
    let pause = NodeId::new("core26-pause")?;
    let second_pause = NodeId::new("core26-second-pause")?;
    assert!(baseline.nodes().contains_key(&pause));
    assert!(!baseline.nodes().contains_key(&second_pause));
    store.redo(baseline.revision_id(), RevisionId::new("v32-pending-redo")?)?;
    let redone = store.snapshot()?;
    assert!(redone.nodes().contains_key(&second_pause));
    assert_eq!(
        redone.duration()?.frames(),
        baseline.duration()?.frames() + 3
    );
    store.undo(redone.revision_id(), RevisionId::new("v32-undo-redo")?)?;
    assert_eq!(store.snapshot()?.nodes(), baseline.nodes());
    assert_eq!(
        store.snapshot()?.audio_bindings(),
        baseline.audio_bindings()
    );
    assert_eq!(store.snapshot()?.gap_overrides(), baseline.gap_overrides());
    store.validate()?;
    drop(store);
    ProjectStore::open(&package, AccessMode::ReadOnly)?.validate()?;
    assert!(ProjectStore::migrate(&package)?.backup.is_none());
    Ok(())
}

#[test]
fn schema26_rejects_new_source_splice_without_promoting_valid_modern_history() -> Result {
    let initial = super::sequence_insert::nested_initial(true)?;
    let NodeKind::Source { source } = &initial.nodes()[&NodeId::new("first")?].kind else {
        panic!("fixture first node is a Source");
    };
    let next = RevisionId::new("modern-paste")?;
    let request = CommandRequest {
        project_id: initial.project_id().clone(),
        expected_revision: initial.revision_id().clone(),
        new_revision: next.clone(),
        command: Command::SpliceSource {
            parent: NodeId::new("inner")?,
            index: 0,
            source: source.clone(),
            id: NodeId::new("pasted")?,
            label: "Pasted".into(),
            timing: AudioTimingId {
                allocation: next,
                ordinal: 0,
            },
        },
    };
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("forged.deadpan");
    let mut store = ProjectStore::create(&package, &initial)?;
    store.commit(&request)?;
    store.validate()?;
    drop(store);
    let database = Connection::open(package.join("project.sqlite"))?;
    // Only this rejection test relabels modern content. The positive fixture
    // above is generated by the unmodified preserved core26/database32 CLI.
    database.execute(
        "UPDATE revisions SET document=json_set(document,'$.schema_version',26)",
        [],
    )?;
    remove_empty_render_tables(&database)?;
    development_break::remove_empty_register_tables(&database)?;
    database.pragma_update(None, "user_version", 32)?;
    for (_, wire) in docs(&database)? {
        legacy_v26::Document::from_json(&wire)?;
    }
    let history = history_json(&database)?;
    assert!(legacy_v26::upgrade_request(&history[0].0).is_err());
    let before = contents(&database)?;
    let StoreError::MigrationFailed { backup, .. } = ProjectStore::migrate(&package).unwrap_err()
    else {
        panic!("schema 32 accepted a later source-splice command");
    };
    assert_eq!(contents(&database)?, before);
    assert_eq!(contents(&Connection::open(backup)?)?, before);
    assert_eq!(
        database.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
        32
    );
    Ok(())
}

#[test]
fn schema26_snapshot_request_and_patch_fields_remain_closed_without_promotion() -> Result {
    for position in ["initial", "later", "command", "forward", "inverse"] {
        let scratch = tempfile::tempdir()?;
        let package = fixture(scratch.path())?;
        let database = Connection::open(package.join("project.sqlite"))?;
        if matches!(position, "initial" | "later") {
            let selector = if position == "initial" {
                "parent_id IS NULL"
            } else {
                "parent_id IS NOT NULL"
            };
            database.execute(&format!("UPDATE revisions SET document=json_set(document,'$.future',null) WHERE id=(SELECT id FROM revisions WHERE {selector} LIMIT 1)"), [])?;
        } else if position == "command" {
            database.execute("UPDATE history SET request=json_set(request,'$.command.future',null) WHERE id=(SELECT MIN(id) FROM history)", [])?;
        } else {
            database.execute("UPDATE history SET edit=json_set(edit,?1,null) WHERE id=(SELECT MIN(id) FROM history)", [format!("$.{position}.future")])?;
        }
        let before = contents(&database)?;
        let StoreError::MigrationFailed { backup, .. } =
            ProjectStore::migrate(&package).unwrap_err()
        else {
            panic!("schema 32 accepted unknown {position} field");
        };
        assert_eq!(contents(&database)?, before);
        assert_eq!(contents(&Connection::open(backup)?)?, before);
        assert_eq!(
            database.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
            32
        );
    }
    Ok(())
}
