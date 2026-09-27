use super::*;
use deadpan_core::*;
use serde_json::{Value, json};

fn fixture(root: &Path) -> Result<PathBuf> {
    let package = root.join("audio-binding-history.deadpan");
    fs::create_dir(&package)?;
    fs::create_dir(package.join("Snapshots"))?;
    let database = Connection::open(package.join("project.sqlite"))?;
    database.pragma_update(None, "foreign_keys", false)?;
    database.execute_batch(include_str!("../fixtures/v27-audio-reanchor-history.sql"))?;
    Ok(package)
}

#[test]
fn actual_schema27_binary_retains_reanchors_phase_terms_and_pending_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_documents = docs(&database)?;
    let old_history = history_json(&database)?;
    let old_metadata = metadata(&database)?;
    let old_operational = operational_metadata(&database)?;
    assert_eq!(old_documents.len(), 9);
    assert_eq!(old_history.len(), 5);
    assert!(matches!(
        ProjectStore::open(&package, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(27))
    ));
    assert_eq!(contents(&database)?, before);
    let outcome = ProjectStore::migrate(&package)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (27, DATABASE_SCHEMA_VERSION)
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
        let legacy = legacy_v21::Document::from_json(old)?;
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
            legacy_v21::upgrade_request(old_request)?,
            serde_json::from_str::<CommandRequest>(&new_request)?
        );
        assert!(legacy_v21::matches_edit(
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
            .any(|binding| binding.reanchors.len() > 1)
    );
    assert!(baseline.audio_bindings().gap_bindings().is_empty());
    store.redo(baseline.revision_id(), RevisionId::new("v27-pending-redo")?)?;
    let redone = store.snapshot()?;
    assert_eq!(
        redone.nodes()[redone.root()].label,
        "Retained reanchor redo"
    );
    assert_eq!(redone.audio_bindings(), baseline.audio_bindings());
    store.undo(redone.revision_id(), RevisionId::new("v27-undo-redo")?)?;
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
fn schema27_rejects_new_gap_intent_in_snapshots_and_both_patch_directions() -> Result {
    for location in [
        "snapshot",
        "forward-before",
        "forward-after",
        "inverse-before",
        "inverse-after",
    ] {
        for nested in ["state", "lattice", "phase", "reanchor"] {
            for field in ["gap_bindings", "gap_after", "recipe", "root"] {
                if (nested == "state") != (field == "gap_bindings") {
                    continue;
                }
                for null in [false, true] {
                    let scratch = tempfile::tempdir()?;
                    let package = fixture(scratch.path())?;
                    let database = Connection::open(package.join("project.sqlite"))?;
                    let (table, column, identity_column, identity, encoded) =
                        if location == "snapshot" {
                            let (revision, encoded) = docs(&database)?.into_iter().next().unwrap();
                            ("revisions", "document", "id", revision, encoded)
                        } else {
                            let encoded: String = database.query_row(
                                "SELECT edit FROM history WHERE revision_id='append-reanchor'",
                                [],
                                |row| row.get(0),
                            )?;
                            (
                                "history",
                                "edit",
                                "revision_id",
                                "append-reanchor".into(),
                                encoded,
                            )
                        };
                    let mut wire: Value = serde_json::from_str(&encoded)?;
                    let state = if location == "snapshot" {
                        &mut wire["audio_bindings"]
                    } else {
                        let (direction, side) = location.split_once('-').unwrap();
                        &mut wire[direction]["audio_bindings"][side]
                    };
                    let parent = if nested == "state" {
                        state
                    } else {
                        let binding = state["bindings"]
                            .as_object_mut()
                            .unwrap()
                            .values_mut()
                            .find(|binding| {
                                binding.pointer("/resume/phase/terms/0/placement").is_some()
                                    && binding.pointer("/reanchors/0/placement").is_some()
                            })
                            .unwrap();
                        let placement = match nested {
                            "lattice" => &mut binding["lattice"],
                            "phase" => &mut binding["resume"]["phase"]["terms"][0]["placement"],
                            _ => &mut binding["reanchors"][0]["placement"],
                        };
                        if field == "gap_after" {
                            placement
                        } else {
                            &mut placement["reference"]
                        }
                    };
                    parent[field] = if null {
                        Value::Null
                    } else {
                        match field {
                            "gap_bindings" | "gap_after" => json!({}),
                            "recipe" => json!("node"),
                            _ => json!({"type":"gap_definition_point_ceil", "repeat":"repeat"}),
                        }
                    };
                    let escaped =
                        format!("\\u{:04x}{}", u32::from(field.as_bytes()[0]), &field[1..]);
                    let encoded = wire
                        .to_string()
                        .replace(&format!("\"{field}\":"), &format!("\"{escaped}\":"));
                    database.execute(
                        &format!("UPDATE {table} SET {column}=?1 WHERE {identity_column}=?2"),
                        rusqlite::params![encoded, identity],
                    )?;
                    let before = contents(&database)?;
                    let Err(StoreError::MigrationFailed { backup, .. }) =
                        ProjectStore::migrate(&package)
                    else {
                        panic!("schema27 admitted {field} at {location}/{nested}")
                    };
                    assert_eq!(contents(&database)?, before);
                    assert_eq!(contents(&Connection::open(backup)?)?, before);
                    assert_eq!(
                        database
                            .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
                        27
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn gap_bindings_survive_durable_commands_undo_redo_and_reopen() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    ProjectStore::migrate(&package)?;
    let mut document = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert!(document.audio_bindings().gap_bindings().is_empty());
    let state = capture_unbound_audio_bindings(
        &document,
        AudioTimingId {
            allocation: RevisionId::new("gap-fixture-capture")?,
            ordinal: 0,
        },
    )?;
    assert_eq!(state.gap_bindings().len(), 1);
    let mut wire = serde_json::to_value(&document)?;
    wire["audio_bindings"] = serde_json::to_value(&state)?;
    document = ProjectDocument::from_json(&wire.to_string())?;
    let modern_path = scratch.path().join("modern-gaps.deadpan");
    let mut store = ProjectStore::create(&modern_path, &document)?;
    let gap = match &document.nodes()[&NodeId::new("old-gap-owner")?].kind {
        NodeKind::Repeat { gap, .. } => gap.clone(),
        _ => unreachable!(),
    };
    store.commit(&CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new("gap-growth")?,
        command: Command::SetRepeat {
            node: NodeId::new("old-gap-owner")?,
            plays: 4,
            gap,
        },
    })?;
    let grown = store.snapshot()?;
    assert_eq!(
        grown.audio_bindings().gap_bindings(),
        document.audio_bindings().gap_bindings()
    );
    store.undo(grown.revision_id(), RevisionId::new("gap-undo")?)?;
    let undone = store.snapshot()?;
    assert_eq!(undone.nodes(), document.nodes());
    assert_eq!(undone.audio_bindings(), document.audio_bindings());
    assert_ne!(undone.revision_id(), document.revision_id());
    drop(store);
    let mut store = ProjectStore::open(&modern_path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, undone);
    store.redo(undone.revision_id(), RevisionId::new("gap-redo")?)?;
    let redone = store.snapshot()?;
    assert_eq!(redone.nodes(), grown.nodes());
    assert_eq!(redone.audio_bindings(), grown.audio_bindings());
    assert_ne!(redone.revision_id(), grown.revision_id());
    store.validate()?;
    drop(store);
    assert_eq!(
        ProjectStore::open(&modern_path, AccessMode::ReadOnly)?.snapshot()?,
        redone
    );
    Ok(())
}
