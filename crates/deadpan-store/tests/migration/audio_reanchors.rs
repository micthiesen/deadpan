use super::*;
use deadpan_core::*;
use serde_json::{Value, json};

fn fixture(root: &Path) -> Result<PathBuf> {
    let package = root.join("audio-binding-history.deadpan");
    fs::create_dir(&package)?;
    fs::create_dir(package.join("Snapshots"))?;
    let database = Connection::open(package.join("project.sqlite"))?;
    database.pragma_update(None, "foreign_keys", false)?;
    database.execute_batch(include_str!("../fixtures/v26-audio-binding-history.sql"))?;
    Ok(package)
}

#[test]
fn actual_schema26_binary_retains_selected_audio_phase_terms_and_pending_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_documents = docs(&database)?;
    let old_history = history_json(&database)?;
    let old_metadata = metadata(&database)?;
    let old_operational = operational_metadata(&database)?;
    assert_eq!(old_documents.len(), 27);
    assert_eq!(old_history.len(), 15);
    assert!(matches!(
        ProjectStore::open(&package, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(26))
    ));
    assert_eq!(contents(&database)?, before);
    let outcome = ProjectStore::migrate(&package)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (26, DATABASE_SCHEMA_VERSION)
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
        let legacy = legacy_v20::Document::from_json(old)?;
        assert!(legacy.matches(&modern));
        assert_eq!(legacy.upgrade()?, modern);
        let mut expected: Value = serde_json::from_str(old)?;
        expected["schema_version"] = json!(DOCUMENT_SCHEMA_VERSION);
        assert_eq!(serde_json::to_value(modern)?, expected);
    }
    for ((old_request, old_edit), (new_request, new_edit)) in
        old_history.iter().zip(history_json(&database)?)
    {
        assert_eq!(
            legacy_v20::upgrade_request(old_request)?,
            serde_json::from_str::<CommandRequest>(&new_request)?
        );
        assert!(legacy_v20::matches_edit(
            old_edit,
            &serde_json::from_str::<EditTransaction>(&new_edit)?
        )?);
        assert_eq!(
            serde_json::from_str::<Value>(old_edit)?,
            serde_json::from_str::<Value>(&new_edit)?
        );
    }
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let baseline = store.snapshot()?;
    assert!(baseline.nodes().values().any(|node| matches!(&node.kind,
        NodeKind::Source { source } if matches!(source.audio_mapping, SourceAudioMapping::SelectedPlacement { .. }))));
    assert!(
        baseline
            .audio_bindings()
            .bindings()
            .values()
            .any(|binding| binding
                .resume
                .as_ref()
                .is_some_and(|resume| !resume.phase.terms.is_empty()))
    );
    assert!(
        baseline
            .audio_bindings()
            .bindings()
            .values()
            .all(|binding| binding.reanchors.is_empty())
    );
    store.redo(baseline.revision_id(), RevisionId::new("v26-pending-redo")?)?;
    let redone = store.snapshot()?;
    assert_eq!(
        redone.nodes()[redone.root()].label,
        "Retained selected-audio redo"
    );
    assert_eq!(redone.audio_bindings(), baseline.audio_bindings());
    store.undo(redone.revision_id(), RevisionId::new("v26-undo-redo")?)?;
    let undone = store.snapshot()?;
    assert_eq!(undone.nodes(), baseline.nodes());
    assert_eq!(undone.audio_bindings(), baseline.audio_bindings());
    assert_ne!(undone.revision_id(), baseline.revision_id());
    let request = CommandRequest {
        project_id: undone.project_id().clone(),
        expected_revision: undone.revision_id().clone(),
        new_revision: RevisionId::new("fresh-after-migration")?,
        command: Command::Rename {
            node: undone.root().clone(),
            label: "Current revision".into(),
        },
    };
    store.commit(&request)?;
    assert!(store.commit(&request).is_err());
    store.validate()?;
    let expected = store.snapshot()?;
    drop(store);
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
        expected
    );
    assert!(ProjectStore::migrate(&package)?.backup.is_none());
    Ok(())
}

#[test]
fn schema26_rejects_reanchors_in_snapshots_and_both_nested_patch_directions() -> Result {
    for location in [
        "snapshot",
        "forward-before",
        "forward-after",
        "inverse-before",
        "inverse-after",
    ] {
        for value in [Value::Null, json!([])] {
            let scratch = tempfile::tempdir()?;
            let package = fixture(scratch.path())?;
            let database = Connection::open(package.join("project.sqlite"))?;
            let (table, column, identity, mut wire) = if location == "snapshot" {
                let (revision, encoded) = docs(&database)?
                    .into_iter()
                    .find(|(_, encoded)| {
                        let wire: Value = serde_json::from_str(encoded).unwrap();
                        wire["audio_bindings"]["bindings"]
                            .as_object()
                            .is_some_and(|bindings| !bindings.is_empty())
                    })
                    .unwrap();
                (
                    "revisions",
                    "document",
                    revision,
                    serde_json::from_str::<Value>(&encoded)?,
                )
            } else {
                let encoded: String = database.query_row(
                    "SELECT edit FROM history WHERE revision_id='selected-pause'",
                    [],
                    |row| row.get(0),
                )?;
                (
                    "history",
                    "edit",
                    "selected-pause".to_owned(),
                    serde_json::from_str::<Value>(&encoded)?,
                )
            };
            let bindings = if location == "snapshot" {
                &mut wire["audio_bindings"]["bindings"]
            } else {
                let (direction, side) = location.split_once('-').unwrap();
                &mut wire[direction]["audio_bindings"][side]["bindings"]
            };
            let (_, binding) = bindings.as_object_mut().unwrap().iter_mut().next().unwrap();
            binding["reanchors"] = value;
            // Escaping prevents an implementation based on a raw substring
            // check from satisfying the frozen grammar contract.
            let encoded = wire
                .to_string()
                .replace("\"reanchors\":", "\"reanc\\u0068ors\":");
            let identity_column = if table == "revisions" {
                "id"
            } else {
                "revision_id"
            };
            database.execute(
                &format!("UPDATE {table} SET {column}=?1 WHERE {identity_column}=?2"),
                rusqlite::params![encoded, identity],
            )?;
            let before = contents(&database)?;
            let Err(StoreError::MigrationFailed { backup, .. }) = ProjectStore::migrate(&package)
            else {
                panic!("schema26 admitted reanchors at {location}")
            };
            assert_eq!(contents(&database)?, before);
            assert_eq!(contents(&Connection::open(backup)?)?, before);
            assert_eq!(
                database.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
                26
            );
        }
    }
    Ok(())
}

#[test]
fn modern_reanchor_intent_survives_durable_history_undo_redo_and_reopen() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    ProjectStore::migrate(&package)?;
    let original = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let (owner, binding) = original.audio_bindings().bindings().iter().next().unwrap();
    let mut binding = binding.clone();
    binding.reanchors.push(AudioReanchorStep {
        placement: binding.lattice.clone(),
        window: None,
    });
    let mut wire = serde_json::to_value(&original)?;
    wire["audio_bindings"]["bindings"][owner.as_str()] = serde_json::to_value(&binding)?;
    let initial = ProjectDocument::from_json(&wire.to_string())?;
    let modern_path = scratch.path().join("modern-reanchors.deadpan");
    let mut store = ProjectStore::create(&modern_path, &initial)?;
    let request = CommandRequest {
        project_id: initial.project_id().clone(),
        expected_revision: initial.revision_id().clone(),
        new_revision: RevisionId::new("reanchor-rename")?,
        command: Command::Rename {
            node: initial.root().clone(),
            label: "Retained reanchors".into(),
        },
    };
    store.commit(&request)?;
    let renamed = store.snapshot()?;
    assert_eq!(renamed.audio_bindings(), initial.audio_bindings());
    store.undo(renamed.revision_id(), RevisionId::new("reanchor-undo")?)?;
    let undone = store.snapshot()?;
    assert_eq!(undone.nodes(), initial.nodes());
    assert_eq!(undone.audio_bindings().bindings()[owner], binding);
    assert_ne!(undone.revision_id(), initial.revision_id());
    drop(store);
    let mut store = ProjectStore::open(&modern_path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, undone);
    store.redo(undone.revision_id(), RevisionId::new("reanchor-redo")?)?;
    let redone = store.snapshot()?;
    assert_eq!(redone.nodes(), renamed.nodes());
    assert_eq!(redone.audio_bindings(), initial.audio_bindings());
    assert_ne!(redone.revision_id(), renamed.revision_id());
    store.validate()?;
    drop(store);
    assert_eq!(
        ProjectStore::open(&modern_path, AccessMode::ReadOnly)?.snapshot()?,
        redone
    );
    Ok(())
}
