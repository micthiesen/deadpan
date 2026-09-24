use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use deadpan_core::{
    IterationOrder, NodeId, NodeKind, ProjectDocument, RevisionId, legacy_v1, legacy_v2, legacy_v3,
    legacy_v4, legacy_v5, legacy_v6, legacy_v7, legacy_v8, legacy_v9, legacy_v10, legacy_v11,
    legacy_v12, legacy_v13, legacy_v14, legacy_v15,
};
use deadpan_jobs::{Relevance, RequestId};
use deadpan_store::generation::{
    ContextObservation, GenerationRequestInput, RelevanceObservation, RelevancePlan,
};
use deadpan_store::{AccessMode, DATABASE_SCHEMA_VERSION, ProjectStore, StoreError};
use rusqlite::Connection;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

#[path = "migration/insert_time.rs"]
mod insert_time;

#[test]
fn schema_twenty_one_retains_exact_lineage_and_does_not_invent_audio_bindings() -> Result {
    use deadpan_core::{CommandRequest, EditTransaction};
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 21)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_docs = docs(&database)?;
    let old_history = history_json(&database)?;
    let old_metadata = metadata(&database)?;
    let old_operational = operational_metadata(&database)?;
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(21))
    ));
    let migration = ProjectStore::migrate(&path)?;
    assert_eq!(
        (migration.from_schema, migration.to_schema),
        (21, DATABASE_SCHEMA_VERSION)
    );
    assert_eq!(
        contents(&Connection::open(migration.backup.unwrap())?)?,
        before
    );
    assert_eq!(metadata(&database)?, old_metadata);
    assert_eq!(operational_metadata(&database)?, old_operational);
    let mut saw_lineage = false;
    for ((old_id, old_json), (new_id, new_json)) in old_docs.iter().zip(docs(&database)?) {
        assert_eq!(old_id, &new_id);
        let current = ProjectDocument::from_json(&new_json)?;
        assert!(legacy_v15::Document::from_json(old_json)?.matches(&current));
        assert!(current.audio_bindings().is_empty());
        saw_lineage |= !current.audio_lineage().is_empty();
        let mut expected: serde_json::Value = serde_json::from_str(old_json)?;
        expected["schema_version"] = serde_json::json!(deadpan_core::DOCUMENT_SCHEMA_VERSION);
        assert_eq!(serde_json::to_value(current)?, expected);
    }
    assert!(saw_lineage, "fixture must retain authored audio lineage");
    for ((old_request, old_edit), (new_request, new_edit)) in
        old_history.iter().zip(history_json(&database)?)
    {
        let request: CommandRequest = serde_json::from_str(&new_request)?;
        let edit: EditTransaction = serde_json::from_str(&new_edit)?;
        assert_eq!(legacy_v15::upgrade_request(old_request)?, request);
        assert!(legacy_v15::matches_edit(old_edit, &edit)?);
        assert!(edit.forward.audio_bindings.is_none());
        assert!(edit.inverse.audio_bindings.is_none());
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(old_edit)?,
            serde_json::to_value(&edit)?
        );
        let prior = snapshot(&database, request.expected_revision.as_str())?;
        let after = snapshot(&database, request.new_revision.as_str())?;
        assert_eq!(edit.forward.apply(&prior)?, after);
        assert_eq!(edit.inverse.apply(&after)?, prior);
    }
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.single_source_state()?, None);
    let baseline = store.snapshot()?;
    let next = RevisionId::new("binding-schema-redo")?;
    store.redo(baseline.revision_id(), next.clone())?;
    store.undo(&next, RevisionId::new("binding-schema-undo")?)?;
    assert_eq!(store.snapshot()?.audio_lineage(), baseline.audio_lineage());
    assert_eq!(store.snapshot()?.nodes(), baseline.nodes());
    assert!(store.snapshot()?.audio_bindings().is_empty());
    store.validate()?;
    drop(store);
    assert!(
        ProjectStore::open(&path, AccessMode::ReadOnly)?
            .snapshot()?
            .audio_bindings()
            .is_empty()
    );
    assert!(ProjectStore::migrate(&path)?.backup.is_none());
    Ok(())
}

#[test]
fn all_legacy_databases_reject_audio_binding_ingress_without_promotion() -> Result {
    for version in 1..=21 {
        for position in ["initial", "later", "forward", "inverse", "command"] {
            for value in ["null", "{}"] {
                let scratch = tempfile::tempdir()?;
                let path = fixture_version(scratch.path(), version)?;
                let database = Connection::open(path.join("project.sqlite"))?;
                if matches!(position, "initial" | "later") {
                    let selector = if position == "initial" {
                        "parent_id IS NULL"
                    } else {
                        "parent_id IS NOT NULL"
                    };
                    database.execute(&format!("UPDATE revisions SET document=json_set(document,'$.audio_bindings',json(?1)) WHERE id=(SELECT id FROM revisions WHERE {selector} LIMIT 1)"), [value])?;
                } else if position == "command" {
                    database.execute("UPDATE history SET request=json_set(request,'$.command.audio_bindings',json(?1)) WHERE id=(SELECT MIN(id) FROM history)", [value])?;
                } else {
                    let pointer = format!("$.{position}.audio_bindings");
                    database.execute("UPDATE history SET edit=json_set(edit,?1,json(?2)) WHERE id=(SELECT MIN(id) FROM history)", [&pointer, value])?;
                }
                let before = contents(&database)?;
                let StoreError::MigrationFailed { backup, .. } =
                    ProjectStore::migrate(&path).unwrap_err()
                else {
                    panic!(
                        "schema {version}, {position}={value}: expected retained failed migration"
                    );
                };
                assert_eq!(contents(&database)?, before);
                assert_eq!(contents(&Connection::open(backup)?)?, before);
                assert_eq!(
                    database
                        .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
                    version
                );
            }
        }
    }
    Ok(())
}

#[test]
fn schema_twenty_one_rejects_changed_lineage_without_promotion() -> Result {
    for tamper in [
        "UPDATE revisions SET document=json_set(document,'$.audio_lineage',json('{}')) WHERE id='split'",
        "UPDATE history SET edit=json_set(edit,'$.forward.audio_lineage',json('{}')) WHERE revision_id='split'",
        "UPDATE history SET edit=json_set(edit,'$.inverse.audio_lineage',json('{}')) WHERE revision_id='split'",
    ] {
        let scratch = tempfile::tempdir()?;
        let path = fixture_version(scratch.path(), 21)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        assert_eq!(database.execute(tamper, [])?, 1);
        let before = contents(&database)?;
        let StoreError::MigrationFailed { backup, .. } = ProjectStore::migrate(&path).unwrap_err()
        else {
            panic!("expected retained failed migration");
        };
        assert_eq!(contents(&database)?, before);
        assert_eq!(contents(&Connection::open(backup)?)?, before);
        assert_eq!(
            database.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
            21
        );
    }
    Ok(())
}

#[test]
fn schema_twenty_replays_lineage_from_copy_history_and_preserves_navigation() -> Result {
    use deadpan_core::{CommandRequest, EditTransaction};
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 20)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_docs = docs(&database)?;
    let old_history = history_json(&database)?;
    let old_metadata = metadata(&database)?;
    let old_operational = operational_metadata(&database)?;
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(20))
    ));
    let migration = ProjectStore::migrate(&path)?;
    assert_eq!(
        (migration.from_schema, migration.to_schema),
        (20, DATABASE_SCHEMA_VERSION)
    );
    assert_eq!(
        contents(&Connection::open(migration.backup.unwrap())?)?,
        before
    );
    assert_eq!(metadata(&database)?, old_metadata);
    assert_eq!(operational_metadata(&database)?, old_operational);
    for ((old_id, old_json), (new_id, new_json)) in old_docs.iter().zip(docs(&database)?) {
        assert_eq!(old_id, &new_id);
        let current = ProjectDocument::from_json(&new_json)?;
        assert!(legacy_v14::Document::from_json(old_json)?.matches(&current));
        let mut projected = serde_json::to_value(&current)?;
        projected.as_object_mut().unwrap().remove("audio_lineage");
        projected["schema_version"] = serde_json::json!(14);
        assert_eq!(
            projected,
            serde_json::from_str::<serde_json::Value>(old_json)?
        );
        if new_id == "insert" {
            assert!(current.audio_lineage().is_empty());
        }
        if ["split", "occurrence-rename", "occurrence-split"].contains(&new_id.as_str()) {
            assert!(!current.audio_lineage().is_empty(), "{new_id}");
        }
    }
    for ((old_request, old_edit), (new_request, new_edit)) in
        old_history.iter().zip(history_json(&database)?)
    {
        let request: CommandRequest = serde_json::from_str(&new_request)?;
        let edit: EditTransaction = serde_json::from_str(&new_edit)?;
        assert_eq!(legacy_v14::upgrade_request(old_request)?, request);
        assert!(legacy_v14::matches_edit(old_edit, &edit)?);
        let before = snapshot(&database, request.expected_revision.as_str())?;
        let after = snapshot(&database, request.new_revision.as_str())?;
        assert_eq!(edit.forward.apply(&before)?, after);
        assert_eq!(edit.inverse.apply(&after)?, before);
        let mut wrong_summary: serde_json::Value = serde_json::from_str(old_edit)?;
        wrong_summary["changed_ids"] = serde_json::json!(["not-a-changed-node"]);
        assert!(!legacy_v14::matches_edit(
            &wrong_summary.to_string(),
            &edit
        )?);
    }
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.single_source_state()?, None);
    let baseline = store.snapshot()?;
    let redo = RevisionId::new("lineage-schema-redo")?;
    store.redo(baseline.revision_id(), redo.clone())?;
    assert!(
        !store
            .snapshot()?
            .nodes()
            .contains_key(&NodeId::new("left")?)
    );
    store.undo(&redo, RevisionId::new("lineage-schema-undo")?)?;
    assert_eq!(store.snapshot()?.audio_lineage(), baseline.audio_lineage());
    assert_eq!(store.snapshot()?.nodes(), baseline.nodes());
    store.validate()?;
    drop(store);
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)?
            .snapshot()?
            .audio_lineage(),
        baseline.audio_lineage()
    );
    assert!(ProjectStore::migrate(&path)?.backup.is_none());
    Ok(())
}

#[test]
fn all_legacy_databases_reject_lineage_ingress_without_promotion() -> Result {
    for version in 1..=20 {
        for position in ["initial", "later", "forward", "inverse", "command"] {
            for value in ["null", "{}"] {
                let scratch = tempfile::tempdir()?;
                let path = fixture_version(scratch.path(), version)?;
                let database = Connection::open(path.join("project.sqlite"))?;
                if matches!(position, "initial" | "later") {
                    let selector = if position == "initial" {
                        "parent_id IS NULL"
                    } else {
                        "parent_id IS NOT NULL"
                    };
                    database.execute(&format!("UPDATE revisions SET document=json_set(document,'$.audio_lineage',json(?1)) WHERE id=(SELECT id FROM revisions WHERE {selector} LIMIT 1)"), [value])?;
                } else if position == "command" {
                    database.execute("UPDATE history SET request=json_set(request,'$.command.audio_lineage',json(?1)) WHERE id=(SELECT MIN(id) FROM history)", [value])?;
                } else {
                    let pointer = format!("$.{position}.audio_lineage");
                    database.execute("UPDATE history SET edit=json_set(edit,?1,json(?2)) WHERE id=(SELECT MIN(id) FROM history)", [&pointer, value])?;
                }
                let before = contents(&database)?;
                let StoreError::MigrationFailed { backup, .. } =
                    ProjectStore::migrate(&path).unwrap_err()
                else {
                    panic!(
                        "schema {version}, {position}={value}: expected retained failed migration"
                    );
                };
                assert_eq!(contents(&database)?, before);
                assert_eq!(contents(&Connection::open(backup)?)?, before);
                assert_eq!(
                    database
                        .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
                    version
                );
            }
        }
    }
    Ok(())
}

#[test]
fn earlier_occurrence_copy_history_gains_only_replayed_lineage() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("schema4-copy.deadpan");
    fs::create_dir(&path)?;
    fs::create_dir(path.join("Snapshots"))?;
    let legacy = Connection::open(path.join("project.sqlite"))?;
    legacy.pragma_update(None, "foreign_keys", false)?;
    legacy.execute_batch(include_str!("fixtures/v4-audio-lineage.sql"))?;
    drop(legacy);
    ProjectStore::migrate(&path)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let initial: String = database.query_row(
        "SELECT document FROM revisions WHERE parent_id IS NULL",
        [],
        |row| row.get(0),
    )?;
    assert!(
        ProjectDocument::from_json(&initial)?
            .audio_lineage()
            .is_empty()
    );
    let mut saw_copy = false;
    for (request, edit) in history_json(&database)? {
        let request: deadpan_core::CommandRequest = serde_json::from_str(&request)?;
        let edit: deadpan_core::EditTransaction = serde_json::from_str(&edit)?;
        if matches!(
            request.command,
            deadpan_core::Command::EditOccurrence { .. }
        ) && !edit.forward.audio_lineage.is_empty()
        {
            saw_copy = true;
        }
        let before = snapshot(&database, request.expected_revision.as_str())?;
        let after = snapshot(&database, request.new_revision.as_str())?;
        assert_eq!(edit.forward.apply(&before)?, after);
        assert_eq!(edit.inverse.apply(&after)?, before);
    }
    assert!(
        saw_copy,
        "the frozen database-4 fixture must exercise occurrence copying"
    );
    Ok(())
}

#[test]
fn schema_sixteen_retains_exact_core_eleven_history_and_stays_generic() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 16)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let before = contents(&database)?;
    let before_docs = docs(&database)?;
    let before_history = history_json(&database)?;
    let before_metadata = metadata(&database)?;
    let before_operational = operational_metadata(&database)?;
    let before_qualifications = qualification_metadata(&database)?;
    assert_eq!((before_docs.len(), before_history.len()), (14, 8));
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(16))
    ));
    let migrated = ProjectStore::migrate(&path)?;
    assert_eq!(
        (migrated.from_schema, migrated.to_schema),
        (16, DATABASE_SCHEMA_VERSION)
    );
    let backup = Connection::open(migrated.backup.unwrap())?;
    assert_eq!(contents(&backup)?, before);
    assert_core_eleven_replay(&before_docs, &before_history, &database)?;
    assert_eq!(metadata(&database)?, before_metadata);
    assert_eq!(operational_metadata(&database)?, before_operational);
    assert_eq!(qualification_metadata(&database)?, before_qualifications);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.single_source_state()?, None);
    // A legacy project gains no artificial floor, even if it happened to use
    // just one source. Every admitted historical edit remains undoable.
    let mut count = 0;
    while store.history_availability()?.0 {
        let before = store.snapshot()?;
        store.undo(
            before.revision_id(),
            RevisionId::new(format!("schema17-undo-{count}"))?,
        )?;
        count += 1;
    }
    assert!(count >= 3);
    store.validate()?;
    drop(store);
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)?.single_source_state()?,
        None
    );
    Ok(())
}

#[test]
fn schema_sixteen_rejects_premature_profile_vocabulary_without_touching_original() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 16)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    database.execute_batch("CREATE TABLE single_source(value TEXT);")?;
    let before = contents(&database)?;
    assert!(matches!(
        ProjectStore::migrate(&path),
        Err(StoreError::MigrationFailed { .. })
    ));
    assert_eq!(contents(&database)?, before);
    Ok(())
}

#[test]
fn schema_fourteen_preserves_explicit_basis_and_qualified_source_branches() -> Result {
    use deadpan_core::{AssetId, BasisState, CommandRequest, EditTransaction};
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 14)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_docs = docs(&database)?;
    let old_history = history_json(&database)?;
    let old_metadata = metadata(&database)?;
    let old_operational = operational_metadata(&database)?;
    let old_qualifications = qualification_metadata(&database)?;
    assert_eq!(old_docs.len(), 54);
    assert_eq!(old_history.len(), 28);
    assert_eq!(old_qualifications.len(), 2);
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(14))
    ));
    let migration = ProjectStore::migrate(&path)?;
    assert_eq!(
        (migration.from_schema, migration.to_schema),
        (14, DATABASE_SCHEMA_VERSION)
    );
    let backup = Connection::open(migration.backup.unwrap())?;
    assert_eq!(contents(&backup)?, before);
    assert_eq!(operational_metadata(&backup)?, old_operational);
    assert_eq!(qualification_metadata(&backup)?, old_qualifications);
    let new_docs = docs(&database)?;
    assert_eq!(new_docs.len(), old_docs.len());
    for ((old_id, old_json), (new_id, new_json)) in old_docs.iter().zip(&new_docs) {
        assert_eq!(old_id, new_id);
        let current = ProjectDocument::from_json(new_json)?;
        assert!(
            legacy_v9::Document::from_json(old_json)?.matches(&current),
            "{old_id}"
        );
        assert_eq!(current.basis_state(), &BasisState::explicit());
        let legacy: serde_json::Value = serde_json::from_str(old_json)?;
        assert_eq!(
            serde_json::to_value(current.presentation_basis())?,
            legacy["presentation_basis"]
        );
    }
    let new_history = history_json(&database)?;
    assert_eq!(new_history.len(), old_history.len());
    for ((old_request, old_edit), (new_request, new_edit)) in old_history.iter().zip(new_history) {
        let request: CommandRequest = serde_json::from_str(&new_request)?;
        assert_eq!(legacy_v9::upgrade_request(old_request)?, request);
        let edit: EditTransaction = serde_json::from_str(&new_edit)?;
        assert!(legacy_v9::matches_edit(old_edit, &edit)?);
        let prior = snapshot(&database, request.expected_revision.as_str())?;
        let after = snapshot(&database, request.new_revision.as_str())?;
        assert_eq!(edit.forward.apply(&prior)?, after);
        assert_eq!(edit.inverse.apply(&after)?, prior);
    }
    assert_eq!(metadata(&database)?, old_metadata);
    assert_eq!(operational_metadata(&database)?, old_operational);
    assert_eq!(qualification_metadata(&database)?, old_qualifications);
    // The SQL fixture deliberately has no source bytes. Every retained index
    // remains readable from immutable receipts, including the abandoned alias.
    let alias = AssetId::new("qualified-camera")?;
    let first_revision = RevisionId::new("schema14-first-import")?;
    let second_revision = RevisionId::new("schema14-second-import")?;
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let first = store.registered_source(&first_revision, &alias)?;
    let second = store.registered_source(&second_revision, &alias)?;
    assert_ne!(first.id(), second.id());
    let first_index = store.source_video_index(&first_revision, &alias)?;
    let second_index = store.source_video_index(&second_revision, &alias)?;
    assert_ne!(first_index.terminal_end(), second_index.terminal_end());
    let baseline = store.snapshot()?;
    assert_eq!(baseline.revision_id().as_str(), "schema14-pending-redo");
    assert_eq!(
        baseline.assets()[&alias].source_qualification.as_ref(),
        Some(second.id())
    );
    for (redo, revision) in [
        (true, "schema15-redo-inherited"),
        (false, "schema15-undo-rename"),
        (false, "schema15-undo-import"),
        (true, "schema15-redo-import"),
    ] {
        let next = RevisionId::new(revision)?;
        let head = store.snapshot()?.revision_id().clone();
        let relevance = retained_relevance(&store, &next)?;
        if redo {
            store.redo_reconciled(&head, next, &relevance)?;
        } else {
            store.undo_reconciled(&head, next, &relevance)?;
        }
        assert_eq!(store.snapshot()?.basis_state(), &BasisState::explicit());
        assert_eq!(
            store.snapshot()?.presentation_basis(),
            baseline.presentation_basis()
        );
        if revision == "schema15-redo-inherited" {
            assert_eq!(
                store.snapshot()?.nodes()[&NodeId::new("second-qualified-clip")?].label,
                "Schema 14 qualified branch"
            );
        }
        if revision == "schema15-undo-import" {
            assert!(!store.snapshot()?.assets().contains_key(&alias));
        }
    }
    assert_eq!(store.snapshot()?.nodes(), baseline.nodes());
    assert_eq!(store.snapshot()?.assets(), baseline.assets());
    store.validate()?;
    drop(store);
    let reopened = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(reopened.snapshot()?.nodes(), baseline.nodes());
    assert_eq!(reopened.registered_source(&first_revision, &alias)?, first);
    assert_eq!(
        reopened.registered_source(&second_revision, &alias)?,
        second
    );
    assert_eq!(
        reopened.source_video_index(&first_revision, &alias)?,
        first_index
    );
    assert_eq!(
        reopened.source_video_index(&second_revision, &alias)?,
        second_index
    );
    assert_eq!(operational_metadata(&database)?, old_operational);
    assert_eq!(qualification_metadata(&database)?, old_qualifications);
    Ok(())
}

fn retained_relevance(store: &ProjectStore, next: &RevisionId) -> Result<RelevancePlan> {
    Ok(RelevancePlan {
        from_revision: store.snapshot()?.revision_id().clone(),
        to_revision: next.clone(),
        observations: store
            .current_generation_requests()?
            .into_iter()
            .map(|request| RelevanceObservation {
                request_id: request.request_id,
                after_context: ContextObservation::Resolved(request.binding.context_sha256.clone()),
                binding: request.binding,
            })
            .collect(),
    })
}

fn qualification_metadata(connection: &Connection) -> Result<Vec<String>> {
    Ok(connection.prepare("SELECT json_array(id,original_content_id,original_ref,hex(snapshot)) FROM source_qualifications ORDER BY id")?
        .query_map([], |row| row.get(0))?
        .collect::<std::result::Result<_, _>>()?)
}

#[test]
fn every_legacy_snapshot_retains_its_exact_explicit_presentation_basis() -> Result {
    for version in 1..=14 {
        let scratch = tempfile::tempdir()?;
        let path = fixture_version(scratch.path(), version)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        let before = docs(&database)?;
        ProjectStore::migrate(&path)?;
        let after = docs(&database)?;
        assert_eq!(after.len(), before.len());
        for ((old_id, old_json), (new_id, new_json)) in before.iter().zip(after) {
            assert_eq!(old_id, &new_id);
            let old: serde_json::Value = serde_json::from_str(old_json)?;
            let current = ProjectDocument::from_json(&new_json)?;
            assert_eq!(
                current.basis_state(),
                &deadpan_core::BasisState::explicit(),
                "schema {version}, {old_id}"
            );
            assert_eq!(
                serde_json::to_value(current.presentation_basis())?,
                old["presentation_basis"],
                "schema {version}, {old_id}"
            );
        }
    }
    Ok(())
}

#[test]
fn every_legacy_schema_rejects_presentation_vocabulary_without_promotion() -> Result {
    for version in 1..=14 {
        for corruption in [
            "UPDATE revisions SET document=json_set(document,'$.basis_state',NULL) WHERE parent_id IS NULL",
            "UPDATE history SET request=json_set(request,'$.command',json('{\"command\":\"set_canvas\",\"width\":1920,\"height\":1080}')) WHERE id=(SELECT MIN(id) FROM history)",
            "UPDATE history SET request=json_set(request,'$.command',json('{\"command\":\"adopt_primary_geometry\",\"width\":1920,\"height\":1080}')) WHERE id=(SELECT MIN(id) FROM history)",
            "UPDATE history SET edit=json_set(edit,'$.forward.presentation',NULL) WHERE id=(SELECT MIN(id) FROM history)",
            "UPDATE history SET edit=json_set(edit,'$.inverse.presentation',NULL) WHERE id=(SELECT MIN(id) FROM history)",
        ] {
            let scratch = tempfile::tempdir()?;
            let path = fixture_version(scratch.path(), version)?;
            let database = Connection::open(path.join("project.sqlite"))?;
            database.execute_batch(corruption)?;
            assert!(database.changes() > 0, "schema {version}: {corruption}");
            let before = contents(&database)?;
            let operational = operational_metadata(&database)?;
            let qualifications = (version == 14)
                .then(|| qualification_metadata(&database))
                .transpose()?;
            let backup = match ProjectStore::migrate(&path) {
                Err(StoreError::MigrationFailed { backup, .. }) => backup,
                result => panic!("schema {version}: {corruption}: {result:?}"),
            };
            for connection in [&database, &Connection::open(backup)?] {
                assert_eq!(contents(connection)?, before);
                assert_eq!(operational_metadata(connection)?, operational);
                if let Some(qualifications) = &qualifications {
                    assert_eq!(&qualification_metadata(connection)?, qualifications);
                }
            }
        }
    }
    Ok(())
}

#[test]
fn schema_fourteen_rejects_new_primary_fields_and_damaged_historical_receipts() -> Result {
    for corruption in [
        "UPDATE history SET request=json_set(request,'$.command.primary',NULL) WHERE revision_id='schema14-first-import'",
        "UPDATE revisions SET document=json_set(document,'$.basis_state',json('{\"rate_origin\":\"provisional\",\"geometry_origin\":\"provisional\",\"primary\":null}')) WHERE id='schema14-first-import'",
        "UPDATE source_qualifications SET snapshot=CAST(CAST(snapshot AS TEXT)||' ' AS BLOB) WHERE id=(SELECT json_extract(document,'$.assets.\"qualified-camera\".source_qualification') FROM revisions WHERE id='schema14-first-import')",
        "DELETE FROM source_qualifications WHERE id=(SELECT json_extract(document,'$.assets.\"qualified-camera\".source_qualification') FROM revisions WHERE id='schema14-first-import')",
        "UPDATE source_qualifications SET original_ref=json_set(original_ref,'$.byte_length',1234)",
        "DELETE FROM original_media WHERE content_id=(SELECT original_content_id FROM source_qualifications LIMIT 1)",
    ] {
        let scratch = tempfile::tempdir()?;
        let path = fixture_version(scratch.path(), 14)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        database.pragma_update(None, "foreign_keys", false)?;
        database.execute_batch(corruption)?;
        assert!(database.changes() > 0, "{corruption}");
        let before = contents(&database)?;
        let operational = operational_metadata(&database)?;
        let qualifications = qualification_metadata(&database)?;
        let backup = match ProjectStore::migrate(&path) {
            Err(StoreError::MigrationFailed { backup, .. }) => backup,
            result => panic!("{corruption}: {result:?}"),
        };
        for connection in [&database, &Connection::open(backup)?] {
            assert_eq!(contents(connection)?, before);
            assert_eq!(operational_metadata(connection)?, operational);
            assert_eq!(qualification_metadata(connection)?, qualifications);
        }
    }
    Ok(())
}

#[test]
fn schema_thirteen_preserves_exact_placements_without_inventing_qualification() -> Result {
    use deadpan_core::{CommandRequest, ExactRatio, SourceAudioMapping, SourceVideoMapping};
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 13)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_docs = docs(&database)?;
    let old_history = history_json(&database)?;
    let old_metadata = metadata(&database)?;
    let old_operational = operational_metadata(&database)?;
    assert_eq!(old_docs.len(), 46);
    assert_eq!(old_history.len(), 25);
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(13))
    ));
    let migration = ProjectStore::migrate(&path)?;
    assert_eq!(
        (migration.from_schema, migration.to_schema),
        (13, DATABASE_SCHEMA_VERSION)
    );
    let backup = Connection::open(migration.backup.unwrap())?;
    assert_eq!(contents(&backup)?, before);
    assert_eq!(operational_metadata(&backup)?, old_operational);
    let new_docs = docs(&database)?;
    assert_eq!(new_docs.len(), old_docs.len());
    for ((old_id, old_json), (new_id, new_json)) in old_docs.iter().zip(&new_docs) {
        assert_eq!(old_id, new_id);
        let current = ProjectDocument::from_json(new_json)?;
        assert!(legacy_v8::Document::from_json(old_json)?.matches(&current));
        assert!(
            current
                .assets()
                .values()
                .all(|asset| asset.source_qualification.is_none())
        );
    }
    let new_history = history_json(&database)?;
    assert_eq!(new_history.len(), old_history.len());
    for ((old_request, old_edit), (new_request, new_edit)) in old_history.iter().zip(new_history) {
        assert_eq!(
            legacy_v8::upgrade_request(old_request)?,
            serde_json::from_str::<CommandRequest>(&new_request)?
        );
        assert!(legacy_v8::matches_edit(
            old_edit,
            &serde_json::from_str(&new_edit)?
        )?);
    }
    assert_eq!(metadata(&database)?, old_metadata);
    assert_eq!(operational_metadata(&database)?, old_operational);
    let qualification_count: u32 =
        database.query_row("SELECT COUNT(*) FROM source_qualifications", [], |row| {
            row.get(0)
        })?;
    assert_eq!(qualification_count, 0);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let baseline = store.snapshot()?;
    assert_eq!(baseline.revision_id().as_str(), "schema13-pending-redo");
    for (id, video_start, audio_start, video_frames, audio_frames, offset) in [
        (
            "source-negative",
            ExactRatio::new(2, 3)?,
            ExactRatio::new(-1, 147)?,
            ExactRatio::new(28750, 1001)?,
            ExactRatio::new(60000, 1001)?,
            -137,
        ),
        (
            "source-positive",
            ExactRatio::new(-3, 7)?,
            ExactRatio::new(3, 7)?,
            ExactRatio::new(120000, 1001)?,
            ExactRatio::new(120000, 1001)?,
            2401,
        ),
    ] {
        let NodeKind::Source { source } = &baseline.nodes()[&NodeId::new(id)?].kind else {
            panic!()
        };
        let SourceVideoMapping::Placement { start, frames, .. } = source.video_mapping else {
            panic!()
        };
        assert_eq!((start, frames), (video_start, video_frames));
        assert_eq!(
            source.audio_mapping,
            SourceAudioMapping::Placement {
                start: audio_start,
                frames: audio_frames
            }
        );
        assert_eq!(source.audio_offset.0, offset);
    }
    assert!(
        baseline
            .marks()
            .contains_key(&deadpan_core::MarkId::new("placement-local-mark")?)
    );
    let relevance = |store: &ProjectStore, next: &RevisionId| -> Result<RelevancePlan> {
        Ok(RelevancePlan {
            from_revision: store.snapshot()?.revision_id().clone(),
            to_revision: next.clone(),
            observations: store
                .current_generation_requests()?
                .into_iter()
                .map(|request| RelevanceObservation {
                    request_id: request.request_id,
                    after_context: ContextObservation::Resolved(
                        request.binding.context_sha256.clone(),
                    ),
                    binding: request.binding,
                })
                .collect(),
        })
    };
    let next = RevisionId::new("schema14-redo-inherited")?;
    store.redo_reconciled(
        baseline.revision_id(),
        next.clone(),
        &relevance(&store, &next)?,
    )?;
    let redone = store.snapshot()?;
    assert_eq!(
        redone.nodes()[&NodeId::new("source-negative")?].label,
        "Schema 13 signed stream placements"
    );
    assert_eq!(redone.assets(), baseline.assets());
    assert_eq!(redone.marks(), baseline.marks());
    let next = RevisionId::new("schema14-undo-inherited")?;
    store.undo_reconciled(
        redone.revision_id(),
        next.clone(),
        &relevance(&store, &next)?,
    )?;
    assert_eq!(store.snapshot()?.nodes(), baseline.nodes());
    store.validate()?;
    drop(store);
    let reopened = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(reopened.snapshot()?.nodes(), baseline.nodes());
    assert_eq!(reopened.snapshot()?.assets(), baseline.assets());
    assert_eq!(operational_metadata(&database)?, old_operational);
    Ok(())
}

#[test]
fn schema_thirteen_rejects_registration_vocabulary_and_corrupt_placement_history() -> Result {
    for corruption in [
        "UPDATE revisions SET document=json_set(document,'$.assets.\"original-av\".source_qualification',NULL) WHERE id='schema13-pending-redo'",
        "UPDATE revisions SET document=json_set(document,'$.assets.\"original-av\".source_qualification','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa') WHERE id='schema10-add-av-asset'",
        "UPDATE history SET request=json_set(request,'$.command.asset.source_qualification',NULL) WHERE revision_id='schema10-add-av-asset'",
        "UPDATE history SET request=json_set(request,'$.command.assets.\"accepted-native\".source_qualification',NULL) WHERE revision_id='accepted'",
        "UPDATE history SET request=json_set(request,'$.command.command','import_source','$.command.asset.source_qualification','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','$.command.insertion',NULL) WHERE revision_id='schema10-add-av-asset'",
        "UPDATE history SET edit=json_set(edit,'$.forward.assets.\"original-av\".after.source_qualification',NULL) WHERE revision_id='schema10-add-av-asset'",
        "UPDATE history SET edit=json_set(edit,'$.inverse.assets.\"original-av\".before.source_qualification',NULL) WHERE revision_id='schema10-add-av-asset'",
        "UPDATE history SET edit=json_set(edit,'$.forward.assets.\"accepted-native\".after.source_qualification',NULL) WHERE revision_id='accepted'",
        "UPDATE history SET edit=json_set(edit,'$.inverse.assets.\"accepted-native\".before.source_qualification',NULL) WHERE revision_id='accepted'",
        "UPDATE revisions SET document=json_set(document,'$.nodes.\"source-negative\".kind.source.video_mapping.start.numerator','3') WHERE id='schema13-place-video-negative'",
        "UPDATE history SET request=json_set(request,'$.command.edit.mapping.start.numerator','-2') WHERE revision_id='schema13-place-audio-negative'",
        "UPDATE history SET edit=json_set(edit,'$.forward.nodes.\"source-negative\".after.kind.source.video_mapping.start.numerator','3') WHERE revision_id='schema13-place-video-negative'",
        "UPDATE history SET edit=json_set(edit,'$.inverse.nodes.\"source-negative\".before.kind.source.audio_mapping.start.numerator','-2') WHERE revision_id='schema13-place-audio-negative'",
        "UPDATE revisions SET document=json_set(document,'$.schema_version',9) WHERE id='schema13-pending-redo'",
    ] {
        let scratch = tempfile::tempdir()?;
        let path = fixture_version(scratch.path(), 13)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        database.execute_batch(corruption)?;
        assert!(
            database.changes() > 0,
            "fixture mutation did not run: {corruption}"
        );
        let before = contents(&database)?;
        let operational = operational_metadata(&database)?;
        let backup = match ProjectStore::migrate(&path) {
            Err(StoreError::MigrationFailed { backup, .. }) => backup,
            result => panic!("{corruption}: {result:?}"),
        };
        assert_eq!(contents(&database)?, before, "{corruption}");
        assert_eq!(operational_metadata(&database)?, operational);
        let backup = Connection::open(backup)?;
        assert_eq!(contents(&backup)?, before);
        assert_eq!(operational_metadata(&backup)?, operational);
    }
    Ok(())
}

#[test]
fn legacy_database_rejects_preexisting_qualification_inventory_without_losing_it() -> Result {
    for version in [1, 13] {
        let scratch = tempfile::tempdir()?;
        let path = fixture_version(scratch.path(), version)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        database.execute_batch(
            "CREATE TABLE source_qualifications (unexpected TEXT NOT NULL);
             INSERT INTO source_qualifications VALUES ('retain this unexpected inventory');",
        )?;
        let before = contents(&database)?;
        let operational = operational_metadata(&database)?;
        let backup = match ProjectStore::migrate(&path) {
            Err(StoreError::MigrationFailed { backup, .. }) => backup,
            result => panic!("schema {version}: {result:?}"),
        };
        let backup = Connection::open(backup)?;
        for connection in [&database, &backup] {
            assert_eq!(contents(connection)?, before);
            assert_eq!(operational_metadata(connection)?, operational);
            let row: String = connection.query_row(
                "SELECT unexpected FROM source_qualifications",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(row, "retain this unexpected inventory");
        }
    }
    Ok(())
}

#[test]
fn schema_twelve_preserves_stream_mappings_and_accepts_reversible_exact_placement() -> Result {
    use deadpan_core::{CommandRequest, SourceAudioMapping, SourceVideoMapping};
    use serde_json::json;
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 12)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_docs = docs(&database)?;
    let old_history = history_json(&database)?;
    let old_metadata = metadata(&database)?;
    let old_operational = operational_metadata(&database)?;
    assert_eq!(old_docs.len(), 34);
    assert_eq!(old_history.len(), 18);
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(12))
    ));
    let migration = ProjectStore::migrate(&path)?;
    assert_eq!(
        (migration.from_schema, migration.to_schema),
        (12, DATABASE_SCHEMA_VERSION)
    );
    let backup = Connection::open(migration.backup.unwrap())?;
    assert_eq!(contents(&backup)?, before);
    assert_eq!(operational_metadata(&backup)?, old_operational);
    let new_docs = docs(&database)?;
    assert_eq!(new_docs.len(), old_docs.len());
    for ((old_id, old_json), (new_id, new_json)) in old_docs.iter().zip(&new_docs) {
        assert_eq!(old_id, new_id);
        assert!(
            legacy_v7::Document::from_json(old_json)?
                .matches(&ProjectDocument::from_json(new_json)?)
        );
    }
    let new_history = history_json(&database)?;
    assert_eq!(new_history.len(), old_history.len());
    for ((old_request, old_edit), (new_request, new_edit)) in old_history.iter().zip(new_history) {
        assert_eq!(
            legacy_v7::upgrade_request(old_request)?,
            serde_json::from_str::<CommandRequest>(&new_request)?
        );
        assert!(legacy_v7::matches_edit(
            old_edit,
            &serde_json::from_str(&new_edit)?
        )?);
    }
    assert_eq!(metadata(&database)?, old_metadata);
    assert_eq!(operational_metadata(&database)?, old_operational);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let relevance = |store: &ProjectStore, next: &RevisionId| -> Result<RelevancePlan> {
        Ok(RelevancePlan {
            from_revision: store.snapshot()?.revision_id().clone(),
            to_revision: next.clone(),
            observations: store
                .current_generation_requests()?
                .into_iter()
                .map(|request| RelevanceObservation {
                    request_id: request.request_id,
                    after_context: ContextObservation::Resolved(
                        request.binding.context_sha256.clone(),
                    ),
                    binding: request.binding,
                })
                .collect(),
        })
    };
    let next = RevisionId::new("schema13-redo-inherited")?;
    store.redo_reconciled(
        store.snapshot()?.revision_id(),
        next.clone(),
        &relevance(&store, &next)?,
    )?;
    let baseline = store.snapshot()?;
    let id = NodeId::new("source-negative")?;
    let NodeKind::Source { source } = &baseline.nodes()[&id].kind else {
        panic!()
    };
    assert!(matches!(
        source.video_mapping,
        SourceVideoMapping::Duration { .. }
    ));
    assert!(matches!(
        source.audio_mapping,
        SourceAudioMapping::Duration { .. }
    ));
    for (revision, command) in [
        (
            "schema13-place-video",
            json!({"command":"set_source_video_mapping","node":id,"mapping":{"type":"placement","start":{"numerator":"2","denominator":"3"},"frames":{"numerator":"28750","denominator":"1001"},"endpoints":"hold_adjacent"}}),
        ),
        (
            "schema13-place-audio",
            json!({"command":"edit_occurrence","instance":{"node":id,"repeats":[]},"edit":{"type":"set_source_audio_mapping","mapping":{"type":"placement","start":{"numerator":"-1","denominator":"147"},"frames":{"numerator":"60000","denominator":"1001"}},"offset":-137},"identities":{"nodes":[],"marks":[]}}),
        ),
    ] {
        let snapshot = store.snapshot()?;
        let request: CommandRequest = serde_json::from_value(json!({
            "project_id":snapshot.project_id(),"expected_revision":snapshot.revision_id(),"new_revision":revision,"command":command
        }))?;
        store.commit_reconciled(&request, &relevance(&store, &request.new_revision)?)?;
    }
    let changed = store.snapshot()?;
    assert_eq!(changed.duration()?, baseline.duration()?);
    assert_eq!(changed.marks(), baseline.marks());
    assert_eq!(changed.assets(), baseline.assets());
    for label in ["schema13-undo-audio", "schema13-undo-video"] {
        let next = RevisionId::new(label)?;
        store.undo_reconciled(
            store.snapshot()?.revision_id(),
            next.clone(),
            &relevance(&store, &next)?,
        )?;
    }
    assert_eq!(store.snapshot()?.nodes(), baseline.nodes());
    for label in ["schema13-redo-video", "schema13-redo-audio"] {
        let next = RevisionId::new(label)?;
        store.redo_reconciled(
            store.snapshot()?.revision_id(),
            next.clone(),
            &relevance(&store, &next)?,
        )?;
    }
    assert_eq!(store.snapshot()?.nodes(), changed.nodes());
    store.validate()?;
    drop(store);
    let reopened = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(reopened.snapshot()?.nodes(), changed.nodes());
    assert_eq!(reopened.snapshot()?.marks(), baseline.marks());
    assert_eq!(operational_metadata(&database)?, old_operational);
    Ok(())
}

#[test]
fn schema_twelve_rejects_placement_vocabulary_in_all_history_surfaces() -> Result {
    for corruption in [
        "UPDATE revisions SET document=json_set(document,'$.nodes.\"source-negative\".kind.source.video_mapping.start',NULL) WHERE id='schema12-video-negative'",
        "UPDATE revisions SET document=json_set(document,'$.nodes.\"source-negative\".kind.source.audio_mapping.type','placement','$.nodes.\"source-negative\".kind.source.audio_mapping.start',json('{\"numerator\":\"0\",\"denominator\":\"1\"}')) WHERE id='schema12-video-negative'",
        "UPDATE history SET request=json_set(request,'$.command.mapping.type','placement','$.command.mapping.start',json('{\"numerator\":\"0\",\"denominator\":\"1\"}')) WHERE revision_id='schema12-video-negative'",
        "UPDATE history SET request=json_set(request,'$.command.edit.mapping.start',NULL) WHERE revision_id='schema12-video-positive'",
        "UPDATE history SET edit=json_set(edit,'$.forward.nodes.\"source-negative\".after.kind.source.video_mapping.start',NULL) WHERE revision_id='schema12-video-negative'",
        "UPDATE history SET edit=json_set(edit,'$.inverse.nodes.\"source-negative\".before.kind.source.audio_mapping.start',NULL) WHERE revision_id='schema12-video-negative'",
        "UPDATE revisions SET document=json_set(document,'$.nodes.\"source-negative\".kind.source.video_mapping.frames.numerator','30000') WHERE id='schema12-video-negative'",
        "UPDATE revisions SET document=json_set(document,'$.schema_version',8) WHERE id='schema12-pending-redo'",
    ] {
        let scratch = tempfile::tempdir()?;
        let path = fixture_version(scratch.path(), 12)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        database.execute_batch(corruption)?;
        assert!(
            database.changes() > 0,
            "fixture mutation did not run: {corruption}"
        );
        let before = contents(&database)?;
        let operational = operational_metadata(&database)?;
        let backup = match ProjectStore::migrate(&path) {
            Err(StoreError::MigrationFailed { backup, .. }) => backup,
            result => panic!("{corruption}: {result:?}"),
        };
        assert_eq!(contents(&database)?, before, "{corruption}");
        assert_eq!(operational_metadata(&database)?, operational);
        let backup = Connection::open(backup)?;
        assert_eq!(contents(&backup)?, before);
        assert_eq!(operational_metadata(&backup)?, operational);
    }
    Ok(())
}

fn fixture(scratch: &Path) -> Result<PathBuf> {
    fixture_version(scratch, 1)
}

fn original_metadata(connection: &Connection) -> Result<Vec<String>> {
    Ok(connection.prepare("SELECT json_array(content_id, version, record) FROM original_media ORDER BY content_id")?
        .query_map([], |row| row.get(0))?.collect::<std::result::Result<_, _>>()?)
}

fn operational_metadata(connection: &Connection) -> Result<Vec<String>> {
    let mut rows = Vec::new();
    for table in [
        "hold_request_clocks",
        "generation_requests",
        "generation_attempts",
        "generation_candidate_receipts",
        "generation_attempt_heads",
        "generation_bundle_receipts",
        "original_media",
    ] {
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)",
            [table],
            |row| row.get(0),
        )?;
        rows.push(format!("{table}: {exists}"));
        if !exists {
            continue;
        }
        let mut statement = connection.prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))?;
        let count = statement.column_count();
        rows.extend(
            statement
                .query_map([], |row| {
                    (0..count)
                        .map(|index| row.get_ref(index).map(|value| format!("{value:?}")))
                        .collect::<std::result::Result<Vec<_>, _>>()
                        .map(|values| values.join("|"))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?,
        );
    }
    Ok(rows)
}

#[test]
fn schema_eleven_preserves_independent_audio_chronology_and_all_operational_rows() -> Result {
    use deadpan_core::{
        AudioSample, Command, CommandRequest, EndpointPolicy, ExactRatio, SourceAudioMapping,
        SourceVideoMapping,
    };
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 11)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let before = contents(&database)?;
    let before_docs = docs(&database)?;
    let before_history = history_json(&database)?;
    let before_metadata = metadata(&database)?;
    let before_operational = operational_metadata(&database)?;
    assert_eq!(before_docs.len(), 24);
    assert_eq!(before_history.len(), 13);
    assert_eq!(original_metadata(&database)?.len(), 1);
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(11))
    ));
    let outcome = ProjectStore::migrate(&path)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (11, DATABASE_SCHEMA_VERSION)
    );
    let backup = Connection::open(outcome.backup.unwrap())?;
    assert_eq!(contents(&backup)?, before);
    assert_eq!(operational_metadata(&backup)?, before_operational);
    assert_eq!(
        backup.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))?,
        11
    );
    let migrated_docs = docs(&database)?;
    assert_eq!(before_docs.len(), migrated_docs.len());
    for ((old_id, old_json), (new_id, new_json)) in before_docs.iter().zip(migrated_docs) {
        assert_eq!(old_id, &new_id);
        assert!(
            legacy_v6::Document::from_json(old_json)?
                .matches(&ProjectDocument::from_json(&new_json)?),
            "{old_id}"
        );
    }
    let migrated_history = history_json(&database)?;
    assert_eq!(before_history.len(), migrated_history.len());
    for ((old_request, old_edit), (new_request, new_edit)) in
        before_history.iter().zip(migrated_history)
    {
        assert_eq!(
            legacy_v6::upgrade_request(old_request)?,
            serde_json::from_str::<CommandRequest>(&new_request)?
        );
        assert!(legacy_v6::matches_edit(
            old_edit,
            &serde_json::from_str(&new_edit)?
        )?);
    }
    assert_eq!(metadata(&database)?, before_metadata);
    assert_eq!(operational_metadata(&database)?, before_operational);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let initial = store.snapshot()?;
    for (id, offset, numerator) in [
        ("source-negative", -137, 60000),
        ("source-positive", 2401, 120000),
    ] {
        let NodeKind::Source { source } = &initial.nodes()[&NodeId::new(id)?].kind else {
            panic!()
        };
        assert_eq!(
            source.audio_mapping,
            SourceAudioMapping::Duration {
                frames: ExactRatio::new(numerator, 1001)?
            }
        );
        assert_eq!(source.audio_offset, AudioSample(offset));
        assert_eq!(source.video_mapping, SourceVideoMapping::FitBeat);
    }
    assert_eq!(initial.marks().len(), 3);
    let relevance = |store: &ProjectStore, next: &RevisionId| -> Result<RelevancePlan> {
        Ok(RelevancePlan {
            from_revision: store.snapshot()?.revision_id().clone(),
            to_revision: next.clone(),
            observations: store
                .current_generation_requests()?
                .into_iter()
                .map(|request| RelevanceObservation {
                    request_id: request.request_id,
                    after_context: ContextObservation::Resolved(
                        request.binding.context_sha256.clone(),
                    ),
                    binding: request.binding,
                })
                .collect(),
        })
    };
    let next = RevisionId::new("schema12-redo")?;
    store.redo_reconciled(
        initial.revision_id(),
        next.clone(),
        &relevance(&store, &next)?,
    )?;
    let baseline = store.snapshot()?;
    assert_eq!(
        baseline.nodes()[&NodeId::new("source-negative")?].label,
        "Schema 11 independent audio"
    );
    assert_eq!(baseline.marks(), initial.marks());
    let next = RevisionId::new("schema12-video-mapping")?;
    let mapping = SourceVideoMapping::Duration {
        frames: ExactRatio::new(120000, 1001)?,
        endpoints: EndpointPolicy::Reject,
    };
    store.commit_reconciled(
        &CommandRequest {
            project_id: baseline.project_id().clone(),
            expected_revision: baseline.revision_id().clone(),
            new_revision: next.clone(),
            command: Command::SetSourceVideoMapping {
                node: NodeId::new("source-negative")?,
                mapping,
            },
        },
        &relevance(&store, &next)?,
    )?;
    let changed = store.snapshot()?;
    assert_eq!(changed.duration()?, baseline.duration()?);
    let NodeKind::Source { source } = &changed.nodes()[&NodeId::new("source-negative")?].kind
    else {
        panic!()
    };
    assert_eq!(source.video_mapping, mapping);
    assert_eq!(
        source.audio_mapping,
        SourceAudioMapping::Duration {
            frames: ExactRatio::new(60000, 1001)?
        }
    );
    assert_eq!(source.audio_offset, AudioSample(-137));
    let next = RevisionId::new("schema12-undo-mapping")?;
    store.undo_reconciled(
        changed.revision_id(),
        next.clone(),
        &relevance(&store, &next)?,
    )?;
    assert_eq!(store.snapshot()?.nodes(), baseline.nodes());
    assert_eq!(store.snapshot()?.marks(), baseline.marks());
    let next = RevisionId::new("schema12-redo-mapping")?;
    store.redo_reconciled(
        store.snapshot()?.revision_id(),
        next.clone(),
        &relevance(&store, &next)?,
    )?;
    assert_eq!(store.snapshot()?.nodes(), changed.nodes());
    assert_eq!(store.snapshot()?.marks(), changed.marks());
    store.validate()?;
    drop(store);
    let reopened = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(reopened.snapshot()?.nodes(), changed.nodes());
    assert_eq!(reopened.snapshot()?.marks(), changed.marks());
    reopened.validate()?;
    assert_eq!(operational_metadata(&database)?, before_operational);
    Ok(())
}

#[test]
fn schema_eleven_rejects_video_vocabulary_and_corruption_without_promotion() -> Result {
    for corruption in [
        "UPDATE revisions SET document=json_set(document,'$.nodes.\"source-negative\".kind.source.video_mapping',NULL) WHERE json_type(document,'$.nodes.\"source-negative\"') IS NOT NULL",
        "UPDATE revisions SET document=json_set(document,'$.nodes.\"source-negative\".kind.source.video_mapping',json('{\"type\":\"fit_beat\"}')) WHERE json_type(document,'$.nodes.\"source-negative\"') IS NOT NULL",
        "UPDATE history SET request=json_set(request,'$.command.subtree.nodes.\"source-negative\".kind.source.video_mapping',NULL) WHERE revision_id='schema10-insert-negative'",
        "UPDATE history SET edit=json_set(edit,'$.forward.nodes.\"source-negative\".after.kind.source.video_mapping',NULL) WHERE revision_id='schema11-audio-negative'",
        "UPDATE history SET request=json_set(request,'$.command',json('{\"command\":\"set_source_video_mapping\",\"node\":\"source-negative\",\"mapping\":{\"type\":\"fit_beat\"}}')) WHERE revision_id='schema11-audio-negative'",
        "UPDATE history SET request=json_set(request,'$.command.edit',json('{\"type\":\"set_source_video_mapping\",\"mapping\":{\"type\":\"fit_beat\"}}')) WHERE revision_id='schema11-audio-positive'",
        "UPDATE revisions SET document=json_remove(document,'$.nodes.\"source-negative\".kind.source.audio_mapping') WHERE id='schema11-audio-negative'",
        "UPDATE revisions SET document=json_set(document,'$.nodes.\"source-negative\".kind.source.audio_mapping',NULL) WHERE id='schema11-audio-negative'",
        "UPDATE revisions SET document=json_set(document,'$.nodes.\"source-negative\".kind.source.audio_mapping.frames.numerator','30000') WHERE id='schema11-audio-negative'",
        "UPDATE history SET request=json_set(request,'$.command.offset',0) WHERE revision_id='schema11-audio-negative'",
        "UPDATE history SET edit=json_set(edit,'$.inverse.nodes.\"source-negative\".before.kind.source.audio_offset',0) WHERE revision_id='schema11-audio-negative'",
        "UPDATE revisions SET document=json_set(document,'$.schema_version',7) WHERE id='schema11-pending-redo'",
        "UPDATE original_media SET version=2",
        "DROP TABLE original_media",
    ] {
        let scratch = tempfile::tempdir()?;
        let path = fixture_version(scratch.path(), 11)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        database.execute_batch(corruption)?;
        let before = contents(&database)?;
        let before_operational = operational_metadata(&database)?;
        let backup = match ProjectStore::migrate(&path) {
            Err(StoreError::MigrationFailed { backup, .. }) => backup,
            result => panic!("{corruption}: {result:?}"),
        };
        assert_eq!(contents(&database)?, before, "{corruption}");
        assert_eq!(
            operational_metadata(&database)?,
            before_operational,
            "{corruption}"
        );
        let backup = Connection::open(backup)?;
        assert_eq!(contents(&backup)?, before, "{corruption}");
        assert_eq!(
            operational_metadata(&backup)?,
            before_operational,
            "{corruption}"
        );
    }
    Ok(())
}

#[test]
fn schema_ten_preserves_source_audio_history_generated_receipts_and_originals() -> Result {
    use deadpan_core::{AudioSample, Command, CommandRequest, ExactRatio, SourceAudioMapping};
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 10)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let before = contents(&database)?;
    let before_docs = docs(&database)?;
    let before_history = history_json(&database)?;
    let before_metadata = metadata(&database)?;
    let before_requests = generation_metadata(&database)?;
    let before_attempts = attempt_metadata(&database)?;
    let before_originals = original_metadata(&database)?;
    assert_eq!(before_docs.len(), 14);
    assert_eq!(before_history.len(), 8);
    assert_eq!(before_originals.len(), 1);
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(10))
    ));
    let outcome = ProjectStore::migrate(&path)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (10, DATABASE_SCHEMA_VERSION)
    );
    let backup = Connection::open(outcome.backup.unwrap())?;
    assert_eq!(contents(&backup)?, before);
    assert_eq!(
        backup.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))?,
        10
    );
    assert_core_five_documents(&before_docs, &database)?;
    assert_core_five_history(&before_history, &database)?;
    assert_eq!(metadata(&database)?, before_metadata);
    assert_eq!(generation_metadata(&database)?, before_requests);
    assert_eq!(attempt_metadata(&database)?, before_attempts);
    assert_eq!(original_metadata(&database)?, before_originals);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let initial = store.snapshot()?;
    for (id, offset) in [("source-negative", -137), ("source-positive", 2401)] {
        let NodeKind::Source { source } = &initial.nodes()[&NodeId::new(id)?].kind else {
            panic!()
        };
        assert_eq!(source.audio_mapping, SourceAudioMapping::FitBeat);
        assert_eq!(source.audio_offset, AudioSample(offset));
    }
    assert_eq!(initial.marks().len(), 2);
    assert_eq!(store.original_records(None, 10)?.len(), 1);
    let relevance = |store: &ProjectStore, next: &RevisionId| -> Result<RelevancePlan> {
        Ok(RelevancePlan {
            from_revision: store.snapshot()?.revision_id().clone(),
            to_revision: next.clone(),
            observations: store
                .current_generation_requests()?
                .into_iter()
                .map(|request| RelevanceObservation {
                    request_id: request.request_id,
                    after_context: ContextObservation::Resolved(
                        request.binding.context_sha256.clone(),
                    ),
                    binding: request.binding,
                })
                .collect(),
        })
    };
    let next = RevisionId::new("schema11-redo")?;
    store.redo_reconciled(
        initial.revision_id(),
        next.clone(),
        &relevance(&store, &next)?,
    )?;
    let baseline = store.snapshot()?;
    assert_eq!(
        baseline.nodes()[&NodeId::new("source-negative")?].label,
        "Renamed source with negative offset"
    );
    let next = RevisionId::new("schema11-audio-mapping")?;
    store.commit_reconciled(
        &CommandRequest {
            project_id: baseline.project_id().clone(),
            expected_revision: baseline.revision_id().clone(),
            new_revision: next.clone(),
            command: Command::SetSourceAudioMapping {
                node: NodeId::new("source-negative")?,
                mapping: SourceAudioMapping::Duration {
                    frames: ExactRatio::new(60000, 1001)?,
                },
                offset: AudioSample(17),
            },
        },
        &relevance(&store, &next)?,
    )?;
    let changed = store.snapshot()?;
    assert_eq!(changed.duration()?, baseline.duration()?);
    let next = RevisionId::new("schema11-undo-mapping")?;
    store.undo_reconciled(
        changed.revision_id(),
        next.clone(),
        &relevance(&store, &next)?,
    )?;
    assert_eq!(store.snapshot()?.nodes(), baseline.nodes());
    assert_eq!(store.snapshot()?.marks(), baseline.marks());
    let next = RevisionId::new("schema11-redo-mapping")?;
    store.redo_reconciled(
        store.snapshot()?.revision_id(),
        next.clone(),
        &relevance(&store, &next)?,
    )?;
    assert_eq!(store.snapshot()?.nodes(), changed.nodes());
    store.validate()?;
    drop(store);
    let reopened = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(reopened.snapshot()?.nodes(), changed.nodes());
    assert_eq!(original_metadata(&database)?, before_originals);
    reopened.validate()?;
    Ok(())
}

#[test]
fn schema_ten_rejects_new_audio_vocabulary_and_corruption_without_promotion() -> Result {
    for corruption in [
        "UPDATE revisions SET document=json_set(document,'$.nodes.\"source-negative\".kind.source.audio_mapping',NULL) WHERE json_type(document,'$.nodes.\"source-negative\"') IS NOT NULL",
        "UPDATE revisions SET document=json_set(document,'$.nodes.\"source-negative\".kind.source.audio_mapping',json('{\"type\":\"fit_beat\"}')) WHERE json_type(document,'$.nodes.\"source-negative\"') IS NOT NULL",
        "UPDATE history SET request=json_set(request,'$.command.subtree.nodes.\"source-negative\".kind.source.audio_mapping',NULL) WHERE revision_id='schema10-insert-negative'",
        "UPDATE history SET edit=json_set(edit,'$.inverse.nodes.\"source-negative\".before.kind.source.audio_mapping',NULL) WHERE revision_id='schema10-insert-negative'",
        "UPDATE history SET request=json_set(request,'$.command',json('{\"command\":\"set_source_audio_mapping\",\"node\":\"source-negative\",\"mapping\":{\"type\":\"fit_beat\"},\"offset\":0}')) WHERE revision_id='schema10-rename-source'",
        "UPDATE revisions SET document=json_set(document,'$.nodes.\"source-negative\".kind.source.audio_offset',0) WHERE id='schema10-rename-source'",
        "UPDATE history SET request=json_set(request,'$.command.subtree.nodes.\"source-negative\".kind.source.audio_offset',0) WHERE revision_id='schema10-insert-negative'",
        "UPDATE history SET edit=json_set(edit,'$.forward.nodes.\"source-negative\".after.kind.source.audio_offset',0) WHERE revision_id='schema10-insert-negative'",
        "UPDATE original_media SET version=2",
        "UPDATE original_media SET record=json_set(record,'$.object.byte_length',0)",
        "DROP TABLE original_media",
    ] {
        let scratch = tempfile::tempdir()?;
        let path = fixture_version(scratch.path(), 10)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        database.execute_batch(corruption)?;
        let before = contents(&database)?;
        let backup = match ProjectStore::migrate(&path) {
            Err(StoreError::MigrationFailed { backup, .. }) => backup,
            result => panic!("{corruption}: {result:?}"),
        };
        assert_eq!(contents(&database)?, before, "{corruption}");
        assert_eq!(
            contents(&Connection::open(backup)?)?,
            before,
            "{corruption}"
        );
        assert_eq!(
            database.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))?,
            10
        );
    }
    Ok(())
}

#[test]
fn schema_nine_adds_empty_original_inventory_and_preserves_full_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 9)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let before = contents(&database)?;
    let before_docs = docs(&database)?;
    let before_history = history_json(&database)?;
    let before_requests = generation_metadata(&database)?;
    let before_attempts = attempt_metadata(&database)?;
    let admission: i64 = database.query_row("SELECT count(*) FROM generation_bundle_receipts WHERE json_type(bundle, '$.admission')='object'", [], |r| r.get(0))?;
    assert_eq!(
        admission, 1,
        "schema-9 fixture includes real six-object admission evidence"
    );
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(9))
    ));
    let outcome = ProjectStore::migrate(&path)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (9, DATABASE_SCHEMA_VERSION)
    );
    let backup = Connection::open(outcome.backup.unwrap())?;
    assert_eq!(contents(&backup)?, before);
    assert_eq!(
        backup.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))?,
        9
    );
    assert_core_five_documents(&before_docs, &database)?;
    assert_core_five_history(&before_history, &database)?;
    assert_eq!(docs(&backup)?, before_docs);
    assert_eq!(history_json(&backup)?, before_history);
    for connection in [&database, &backup] {
        assert_eq!(generation_metadata(connection)?, before_requests);
        assert_eq!(attempt_metadata(connection)?, before_attempts);
    }
    let store = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert!(store.original_records(None, 10)?.is_empty());
    store.validate()?;
    Ok(())
}

#[test]
fn legacy_schema_cannot_smuggle_modern_original_records_through_migration() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 9)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    database.execute_batch("CREATE TABLE original_media(unexpected TEXT);")?;
    let before = contents(&database)?;
    assert!(matches!(
        ProjectStore::migrate(&path),
        Err(StoreError::MigrationFailed { .. })
    ));
    assert_eq!(contents(&database)?, before);
    assert_eq!(
        database.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))?,
        9
    );
    Ok(())
}

#[test]
fn schema_eight_retains_unqualified_bundle_json_and_pending_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 8)?;
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(8))
    ));
    let database = Connection::open(path.join("project.sqlite"))?;
    let before = contents(&database)?;
    let before_docs = docs(&database)?;
    let before_history = history_json(&database)?;
    let before_metadata = metadata(&database)?;
    let before_requests = generation_metadata(&database)?;
    let before_attempts = attempt_metadata(&database)?;
    let bundle: String =
        database.query_row("SELECT bundle FROM generation_bundle_receipts", [], |row| {
            row.get(0)
        })?;
    let bridge_plan: String =
        database.query_row("SELECT bridge_plan FROM generation_requests", [], |row| {
            row.get(0)
        })?;
    assert!(!bundle.contains("admission"));
    let outcome = ProjectStore::migrate(&path)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (8, DATABASE_SCHEMA_VERSION)
    );
    let backup = Connection::open(outcome.backup.unwrap())?;
    assert_eq!(contents(&backup)?, before);
    assert_core_five_documents(&before_docs, &database)?;
    assert_core_five_history(&before_history, &database)?;
    assert_eq!(docs(&backup)?, before_docs);
    assert_eq!(history_json(&backup)?, before_history);
    for connection in [&database, &backup] {
        assert_eq!(metadata(connection)?, before_metadata);
        assert_eq!(generation_metadata(connection)?, before_requests);
        assert_eq!(attempt_metadata(connection)?, before_attempts);
        assert_eq!(
            connection.query_row("SELECT bundle FROM generation_bundle_receipts", [], |row| {
                row.get::<_, String>(0)
            })?,
            bundle
        );
        assert_eq!(
            connection.query_row("SELECT bridge_plan FROM generation_requests", [], |row| row
                .get::<_, String>(0))?,
            bridge_plan
        );
    }
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let selected = store
        .selected_generation_bundle(&RequestId::new("schema8-ready")?)?
        .unwrap();
    assert!(selected.receipt.admission().is_none());
    let next = RevisionId::new("after-migration-redo")?;
    let head = store.snapshot()?.revision_id().clone();
    let request = store.current_generation_requests()?.pop().unwrap();
    store.redo_reconciled(
        &head,
        next.clone(),
        &RelevancePlan {
            from_revision: head.clone(),
            to_revision: next,
            observations: vec![RelevanceObservation {
                request_id: request.request_id,
                after_context: ContextObservation::Resolved(request.binding.context_sha256.clone()),
                binding: request.binding,
            }],
        },
    )?;
    store.validate()?;
    Ok(())
}

#[test]
fn schema_eight_rejects_modern_evidence_and_corruption_without_promotion() -> Result {
    for corruption in [
        "UPDATE generation_bundle_receipts SET bundle=json_set(bundle,'$.admission',NULL)",
        "UPDATE generation_bundle_receipts SET bundle=json_set(bundle,'$.admission',json('{}'))",
        "UPDATE generation_bundle_receipts SET bundle=json_set(bundle,'$.validator.id','')",
        "UPDATE generation_bundle_receipts SET bundle=json_set(bundle,'$.validator.id',printf('%020000d',0))",
        "UPDATE history SET edit=json_set(edit,'$.forward.label','corrupt')",
    ] {
        let scratch = tempfile::tempdir()?;
        let path = fixture_version(scratch.path(), 8)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        database.execute_batch(corruption)?;
        let before = contents(&database)?;
        let bundle: String =
            database.query_row("SELECT bundle FROM generation_bundle_receipts", [], |row| {
                row.get(0)
            })?;
        let StoreError::MigrationFailed { backup, .. } = ProjectStore::migrate(&path).unwrap_err()
        else {
            panic!("{corruption}")
        };
        for connection in [&database, &Connection::open(backup)?] {
            assert_eq!(contents(connection)?, before);
            assert_eq!(
                connection.query_row(
                    "SELECT bundle FROM generation_bundle_receipts",
                    [],
                    |row| row.get::<_, String>(0)
                )?,
                bundle
            );
            assert_eq!(
                connection.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
                8
            );
        }
    }
    Ok(())
}
fn fixture_version(scratch: &Path, version: u32) -> Result<PathBuf> {
    let package = scratch.join("legacy.deadpan");
    fs::create_dir(&package)?;
    fs::create_dir(package.join("Snapshots"))?;
    let connection = Connection::open(package.join("project.sqlite"))?;
    connection.pragma_update(None, "foreign_keys", false)?;
    connection.execute_batch(match version {
        1 => include_str!("fixtures/v1-history.sql"),
        2 => include_str!("fixtures/v2-history.sql"),
        3 => include_str!("fixtures/v3-history.sql"),
        4 => include_str!("fixtures/v4-history.sql"),
        5 => include_str!("fixtures/v5-generation.sql"),
        6 => include_str!("fixtures/v6-attempts.sql"),
        7 => include_str!("fixtures/v7-selected.sql"),
        8 => include_str!("fixtures/v8-history.sql"),
        9 => include_str!("fixtures/v9-history.sql"),
        10 => include_str!("fixtures/v10-history.sql"),
        11 => include_str!("fixtures/v11-history.sql"),
        12 => include_str!("fixtures/v12-history.sql"),
        13 => include_str!("fixtures/v13-history.sql"),
        14 => include_str!("fixtures/v14-history.sql"),
        15 => include_str!("fixtures/v15-history.sql"),
        16 => include_str!("fixtures/v16-history.sql"),
        17 => include_str!("fixtures/v17-history.sql"),
        18 => include_str!("fixtures/v18-history.sql"),
        19 => include_str!("fixtures/v19-history.sql"),
        20 => include_str!("fixtures/v20-history.sql"),
        21 => include_str!("fixtures/v21-history.sql"),
        _ => panic!("unsupported fixture"),
    })?;
    Ok(package)
}
fn docs(connection: &Connection) -> Result<Vec<(String, String)>> {
    Ok(connection
        .prepare("SELECT id, document FROM revisions ORDER BY id")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?)
}
fn assert_core_five_documents(originals: &[(String, String)], connection: &Connection) -> Result {
    let migrated = docs(connection)?;
    assert_eq!(originals.len(), migrated.len());
    for ((old_id, old_json), (new_id, new_json)) in originals.iter().zip(migrated) {
        assert_eq!(old_id, &new_id);
        assert!(
            legacy_v5::Document::from_json(old_json)?
                .matches(&ProjectDocument::from_json(&new_json)?),
            "{old_id}"
        );
    }
    Ok(())
}

fn assert_core_five_history(originals: &[(String, String)], connection: &Connection) -> Result {
    let migrated = history_json(connection)?;
    assert_eq!(originals.len(), migrated.len());
    for ((old_request, old_edit), (new_request, new_edit)) in originals.iter().zip(migrated) {
        assert_eq!(
            legacy_v5::upgrade_request(old_request)?,
            serde_json::from_str::<deadpan_core::CommandRequest>(&new_request)?
        );
        assert!(legacy_v5::matches_edit(
            old_edit,
            &serde_json::from_str(&new_edit)?
        )?);
    }
    Ok(())
}

fn metadata(connection: &Connection) -> Result<String> {
    let mut parts = Vec::new();
    for sql in [
        "SELECT json_array(id,parent_id,kind) FROM revisions ORDER BY id",
        "SELECT json_array(id,parent_id,revision_id) FROM history ORDER BY id",
        "SELECT json_array(singleton,head_revision,cursor) FROM state",
        "SELECT json_array(position,history_id) FROM redo ORDER BY position",
    ] {
        parts.extend(
            connection
                .prepare(sql)?
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?,
        );
    }
    Ok(parts.join("\n"))
}
fn history_json(connection: &Connection) -> Result<Vec<(String, String)>> {
    Ok(connection
        .prepare("SELECT request,edit FROM history ORDER BY id")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?)
}
fn assert_core_four_documents(originals: &[(String, String)], connection: &Connection) -> Result {
    let current = docs(connection)?;
    assert_eq!(originals.len(), current.len());
    for ((old_id, old), (new_id, new)) in originals.iter().zip(current) {
        assert_eq!(*old_id, new_id);
        assert!(legacy_v4::Document::from_json(old)?.matches(&ProjectDocument::from_json(&new)?));
    }
    Ok(())
}
fn assert_core_four_history(originals: &[(String, String)], connection: &Connection) -> Result {
    let migrated = history_json(connection)?;
    assert_eq!(originals.len(), migrated.len());
    for ((old_request, old_edit), (new_request, new_edit)) in originals.iter().zip(migrated) {
        assert_eq!(
            legacy_v4::upgrade_request(old_request)?,
            serde_json::from_str::<deadpan_core::CommandRequest>(&new_request)?
        );
        assert!(legacy_v4::matches_edit(
            old_edit,
            &serde_json::from_str(&new_edit)?
        )?);
    }
    Ok(())
}
fn generation_metadata(connection: &Connection) -> Result<String> {
    let mut parts = Vec::new();
    for sql in [
        "SELECT json_array(hold_id,high_water) FROM hold_request_clocks ORDER BY hold_id",
        "SELECT json_array(request_id,project_id,hold_id,request_version,origin_revision,context_sha256,constraints,provider,relevance) FROM generation_requests ORDER BY request_id",
    ] {
        parts.extend(
            connection
                .prepare(sql)?
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?,
        );
    }
    Ok(parts.join("\n"))
}
fn attempt_metadata(connection: &Connection) -> Result<String> {
    let mut parts = Vec::new();
    for sql in [
        "SELECT json_array(request_id,attempt_id,ordinal,cancellation_token,state,worker_stage,transition_sequence,cancel_response,worker_candidate,failure_origin,failure_code,failure_detail) FROM generation_attempts ORDER BY request_id,attempt_id",
        "SELECT json_array(request_id,attempt_id,staged_ref,sha256,byte_length,video,provider,validator_id,validator_version,availability) FROM generation_candidate_receipts ORDER BY request_id,attempt_id",
        "SELECT json_array(request_id,high_water,latest_attempt_id,selected_ready_attempt_id) FROM generation_attempt_heads ORDER BY request_id",
    ] {
        parts.extend(
            connection
                .prepare(sql)?
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?,
        );
    }
    Ok(parts.join("\n"))
}
fn contents(connection: &Connection) -> Result<String> {
    let mut values = vec![
        metadata(connection)?,
        format!(
            "{}",
            connection.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?
        ),
    ];
    values.extend(docs(connection)?.into_iter().map(|(_, json)| json));
    values.extend(
        connection
            .prepare("SELECT json_array(request,edit) FROM history ORDER BY id")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?,
    );
    Ok(values.join("\n"))
}
fn snapshot(connection: &Connection, revision: &str) -> Result<ProjectDocument> {
    let json: String = connection.query_row(
        "SELECT document FROM revisions WHERE id=?1",
        [revision],
        |row| row.get(0),
    )?;
    Ok(ProjectDocument::from_json(&json)?)
}
fn order(document: &ProjectDocument) -> &IterationOrder {
    let NodeKind::Repeat { iterations, .. } =
        &document.nodes()[&NodeId::new("repeat").unwrap()].kind
    else {
        panic!("fixture Repeat")
    };
    iterations
}

#[test]
fn corrupt_old_tables_and_oversized_rows_retain_backup_before_validation() -> Result {
    let oversized_bytes = i64::try_from(deadpan_core::MAX_DOCUMENT_JSON_BYTES)? + 1;
    for version in [1, 2, 3, 4, 5, 6] {
        for missing_table in [true, false] {
            let scratch = tempfile::tempdir()?;
            let path = fixture_version(scratch.path(), version)?;
            let database_path = path.join("project.sqlite");
            let database = Connection::open(&database_path)?;
            database.pragma_update(None, "foreign_keys", false)?;
            if missing_table {
                database.execute_batch("DROP TABLE history")?;
            } else {
                database.pragma_update(None, "ignore_check_constraints", true)?;
                database.execute(
                    "UPDATE revisions SET document=CAST(zeroblob(?1) AS TEXT) WHERE kind='initial'",
                    [oversized_bytes],
                )?;
            }
            let original_revisions: i64 =
                database.query_row("SELECT COUNT(*) FROM revisions", [], |row| row.get(0))?;
            drop(database);
            let before = fs::read(&database_path)?;
            let failure = ProjectStore::migrate(&path).unwrap_err();
            let StoreError::MigrationFailed { backup, source } = failure else {
                panic!("expected migration failure with retained backup");
            };
            if !missing_table {
                assert!(matches!(*source, StoreError::Integrity(_)));
            }
            assert_eq!(fs::read(&database_path)?, before);
            assert!(backup.is_file());
            let backup = Connection::open(backup)?;
            assert_eq!(
                backup.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
                version
            );
            assert_eq!(
                backup.query_row("SELECT COUNT(*) FROM revisions", [], |row| row
                    .get::<_, i64>(0))?,
                original_revisions
            );
            if missing_table {
                assert!(!backup.query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='history')",
                    [],
                    |row| row.get::<_, bool>(0),
                )?);
            } else {
                assert!(backup.query_row(
                    "SELECT document=CAST(zeroblob(?1) AS TEXT) FROM revisions WHERE kind='initial'",
                    [oversized_bytes],
                    |row| row.get::<_, bool>(0),
                )?);
            }
        }
    }
    Ok(())
}

#[test]
fn old_binary_history_migrates_with_stable_ids_and_pending_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = fixture(scratch.path())?;
    for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
        assert!(matches!(
            ProjectStore::open(&path, mode),
            Err(StoreError::MigrationRequired(1))
        ));
    }
    let database = Connection::open(path.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_metadata = metadata(&database)?;
    let old_docs = docs(&database)?;
    drop(database);
    let migration = ProjectStore::migrate(&path)?;
    assert_eq!(
        (migration.from_schema, migration.to_schema),
        (1, DATABASE_SCHEMA_VERSION)
    );
    let backup = Connection::open(migration.backup.unwrap())?;
    assert_eq!(contents(&backup)?, before);
    let database = Connection::open(path.join("project.sqlite"))?;
    assert_eq!(metadata(&database)?, old_metadata);
    for (revision, json) in old_docs {
        assert!(
            legacy_v1::Document::from_json(&json)?.matches(&snapshot(&database, &revision)?),
            "{revision}"
        );
    }
    let original = snapshot(&database, "v1-wrap")?;
    let smaller = snapshot(&database, "v1-shrink")?;
    let grown = snapshot(&database, "v1-grow")?;
    let branch = snapshot(&database, "v1-branch")?;
    for position in 0..2 {
        assert_eq!(order(&original).at(position), order(&smaller).at(position));
        assert_eq!(order(&original).at(position), order(&grown).at(position));
        assert_eq!(order(&original).at(position), order(&branch).at(position));
    }
    assert_eq!(order(&grown).at(2).unwrap().allocation.as_str(), "v1-grow");
    assert_eq!(
        order(&branch).at(2).unwrap().allocation.as_str(),
        "v1-branch"
    );
    assert_ne!(order(&grown).at(2), order(&branch).at(2));
    assert!(
        order(&grown)
            .position(&order(&original).at(2).unwrap())
            .is_none()
    );
    drop(database);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(order(&store.snapshot()?).len(), 2);
    store.redo(
        store.snapshot()?.revision_id(),
        RevisionId::new("after-migration-redo-1")?,
    )?;
    assert_eq!(order(&store.snapshot()?), order(&branch));
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.redo(
        store.snapshot()?.revision_id(),
        RevisionId::new("after-migration-redo-2")?,
    )?;
    let current = store.snapshot()?;
    let NodeKind::Repeat { iterations, .. } = &current.nodes()[&NodeId::new("insert-repeat")?].kind
    else {
        panic!("inserted repeat")
    };
    assert_eq!(iterations.at(0).unwrap().allocation.as_str(), "v1-subtree");
    store.undo(
        current.revision_id(),
        RevisionId::new("after-migration-undo")?,
    )?;
    store.validate()?;
    drop(store);
    assert!(ProjectStore::migrate(&path)?.backup.is_none());
    Ok(())
}

#[test]
fn migration_promotes_with_a_live_wal_reader_without_replacing_its_snapshot() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = fixture(scratch.path())?;
    let reader = Connection::open(path.join("project.sqlite"))?;
    reader.pragma_update(None, "journal_mode", "WAL")?;
    reader.execute_batch("BEGIN")?;
    let before = contents(&reader)?;
    ProjectStore::migrate(&path)?;
    assert_eq!(
        contents(&reader)?,
        before,
        "existing read transaction retains schema 1"
    );
    ProjectStore::open(&path, AccessMode::ReadOnly)?.validate()?;
    reader.execute_batch("COMMIT")?;
    assert_eq!(
        reader.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
        DATABASE_SCHEMA_VERSION
    );
    Ok(())
}

#[test]
fn schema_four_migrates_core_vocabulary_and_preserves_authored_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 4)?;
    for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
        assert!(matches!(
            ProjectStore::open(&path, mode),
            Err(StoreError::MigrationRequired(4))
        ));
    }
    let database = Connection::open(path.join("project.sqlite"))?;
    let before = contents(&database)?;
    let original_docs = docs(&database)?;
    let original_history = history_json(&database)?;
    let original_metadata = metadata(&database)?;
    assert_eq!(original_docs.len(), 11);
    assert_eq!(original_history.len(), 6);
    let outcome = ProjectStore::migrate(&path)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (4, DATABASE_SCHEMA_VERSION)
    );
    assert_eq!(
        contents(&Connection::open(outcome.backup.unwrap())?)?,
        before
    );
    assert_core_four_documents(&original_docs, &database)?;
    assert_core_four_history(&original_history, &database)?;
    assert_eq!(metadata(&database)?, original_metadata);
    assert_eq!(
        database.query_row(
            "SELECT (SELECT COUNT(*) FROM hold_request_clocks) + (SELECT COUNT(*) FROM generation_requests)",
            [],
            |row| row.get::<_, i64>(0),
        )?,
        0
    );
    let shrunk = snapshot(&database, "v4-shrink")?;
    drop(database);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let restored = store.snapshot()?;
    assert_eq!(restored.overrides().len(), 1);
    assert_eq!(restored.marks().len(), 1);
    store.redo(restored.revision_id(), RevisionId::new("v5-redo")?)?;
    let redone = store.snapshot()?;
    assert_eq!(redone.nodes(), shrunk.nodes());
    assert_eq!(redone.overrides(), shrunk.overrides());
    assert_eq!(redone.marks(), shrunk.marks());
    store.undo(redone.revision_id(), RevisionId::new("v5-undo")?)?;
    let undone = store.snapshot()?;
    assert_eq!(undone.nodes(), restored.nodes());
    assert_eq!(undone.overrides(), restored.overrides());
    assert_eq!(undone.marks(), restored.marks());
    store.validate()?;
    drop(store);
    assert!(ProjectStore::migrate(&path)?.backup.is_none());
    Ok(())
}

#[test]
fn schema_four_rejects_corruption_and_operational_table_collisions_before_promotion() -> Result {
    for corruption in [
        "UPDATE history SET edit=json_set(edit,'$.inverse.overrides',json('{}')) WHERE revision_id='v4-clear'",
        "UPDATE revisions SET document=json_set(document,'$.schema_version',5) WHERE kind='initial'",
        "UPDATE revisions SET document=json_set(document,'$.generation_requests',json('{}')) WHERE kind='initial'",
        "CREATE TABLE generation_requests(preserved TEXT); INSERT INTO generation_requests VALUES ('existing')",
    ] {
        let scratch = tempfile::tempdir()?;
        let path = fixture_version(scratch.path(), 4)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        database.execute_batch(corruption)?;
        let before = contents(&database)?;
        let failure = ProjectStore::migrate(&path).unwrap_err();
        assert_eq!(failure.code(), "MigrationFailed", "{corruption}");
        let StoreError::MigrationFailed { backup, .. } = failure else {
            panic!("migration must retain its backup")
        };
        assert_eq!(contents(&Connection::open(backup)?)?, before);
        assert_eq!(contents(&database)?, before);
        if corruption.starts_with("CREATE") {
            assert_eq!(
                database.query_row("SELECT preserved FROM generation_requests", [], |row| {
                    row.get::<_, String>(0)
                })?,
                "existing"
            );
        }
    }
    Ok(())
}

#[test]
fn schema_five_preserves_requests_clocks_history_and_pending_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 5)?;
    for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
        assert!(matches!(
            ProjectStore::open(&path, mode),
            Err(StoreError::MigrationRequired(5))
        ));
    }
    let database = Connection::open(path.join("project.sqlite"))?;
    let original = contents(&database)?;
    let requests = generation_metadata(&database)?;
    let original_docs = docs(&database)?;
    let original_history = history_json(&database)?;
    let original_metadata = metadata(&database)?;
    let outcome = ProjectStore::migrate(&path)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (5, DATABASE_SCHEMA_VERSION)
    );
    let backup = Connection::open(outcome.backup.unwrap())?;
    assert_eq!(contents(&backup)?, original);
    assert_eq!(generation_metadata(&backup)?, requests);
    assert_eq!(generation_metadata(&database)?, requests);
    assert_core_four_documents(&original_docs, &database)?;
    assert_core_four_history(&original_history, &database)?;
    assert_eq!(metadata(&database)?, original_metadata);
    for table in [
        "generation_attempt_heads",
        "generation_attempts",
        "generation_candidate_receipts",
    ] {
        assert_eq!(
            database.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get::<_, i64>(0)
            })?,
            0
        );
    }
    drop(database);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let current = store
        .generation_request(&RequestId::new("current")?)?
        .unwrap();
    assert_eq!(current.binding.request_version.get(), 2);
    assert_eq!(current.relevance, Relevance::Current);
    for (id, expected) in [
        ("first", Relevance::Stale),
        ("changed-request", Relevance::Stale),
        ("deleted-request", Relevance::Detached),
    ] {
        assert_eq!(
            store
                .generation_request(&RequestId::new(id)?)?
                .unwrap()
                .relevance,
            expected
        );
    }
    let from = store.snapshot()?.revision_id().clone();
    let to = RevisionId::new("v6-redo")?;
    store.redo_reconciled(
        &from,
        to.clone(),
        &RelevancePlan {
            from_revision: from.clone(),
            to_revision: to.clone(),
            observations: vec![RelevanceObservation {
                request_id: current.request_id.clone(),
                binding: current.binding.clone(),
                after_context: ContextObservation::Resolved(current.binding.context_sha256.clone()),
            }],
        },
    )?;
    assert_eq!(
        store.generation_request(&current.request_id)?,
        Some(current.clone())
    );
    let next = store.allocate_generation_request(GenerationRequestInput {
        request_id: RequestId::new("post-migration")?,
        expected_revision: to,
        hold_id: current.binding.hold_id.clone(),
        context_sha256: current.binding.context_sha256.clone(),
        constraints: current.constraints.clone(),
        provider: current.provider.clone(),
    })?;
    assert_eq!(next.binding.request_version.get(), 3);
    store.validate()?;
    drop(store);
    assert!(ProjectStore::migrate(&path)?.backup.is_none());
    Ok(())
}

#[test]
fn schema_five_invalid_requests_and_table_collisions_never_promote() -> Result {
    for corruption in [
        "UPDATE generation_requests SET constraints=json_set(constraints,'$.unknown',1) WHERE request_id='current'",
        "UPDATE generation_requests SET constraints=printf('%020000d',0) WHERE request_id='current'",
        "UPDATE hold_request_clocks SET high_water=3 WHERE hold_id='hold'",
        "UPDATE generation_requests SET relevance='current' WHERE request_id='deleted-request'",
        "CREATE TABLE generation_attempts(preserved TEXT); INSERT INTO generation_attempts VALUES ('existing')",
    ] {
        let scratch = tempfile::tempdir()?;
        let path = fixture_version(scratch.path(), 5)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        database.pragma_update(None, "ignore_check_constraints", true)?;
        database.execute_batch(corruption)?;
        let before = contents(&database)?;
        let requests = generation_metadata(&database)?;
        let failure = ProjectStore::migrate(&path).unwrap_err();
        let StoreError::MigrationFailed { backup, .. } = failure else {
            panic!("migration must retain its backup: {corruption}")
        };
        let backup = Connection::open(backup)?;
        assert_eq!(contents(&database)?, before);
        assert_eq!(generation_metadata(&database)?, requests);
        assert_eq!(contents(&backup)?, before);
        assert_eq!(generation_metadata(&backup)?, requests);
        if corruption.starts_with("CREATE") {
            assert_eq!(
                database.query_row("SELECT preserved FROM generation_attempts", [], |row| {
                    row.get::<_, String>(0)
                })?,
                "existing"
            );
        }
    }
    Ok(())
}

#[test]
fn schema_six_preserves_attempts_and_defers_recovery_until_writer_open() -> Result {
    use deadpan_jobs::{AttemptId, CancellationToken, JobState, MessageIdentity};
    use deadpan_store::generation_attempts::BeginGenerationAttempt;

    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 6)?;
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(6))
    ));
    let database = Connection::open(path.join("project.sqlite"))?;
    let original = contents(&database)?;
    let original_docs = docs(&database)?;
    let original_history = history_json(&database)?;
    let original_metadata = metadata(&database)?;
    let requests = generation_metadata(&database)?;
    let attempts = attempt_metadata(&database)?;
    let outcome = ProjectStore::migrate(&path)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (6, DATABASE_SCHEMA_VERSION)
    );
    let backup = Connection::open(outcome.backup.unwrap())?;
    assert_eq!(contents(&backup)?, original);
    assert_eq!(generation_metadata(&backup)?, requests);
    assert_eq!(attempt_metadata(&backup)?, attempts);
    assert_core_four_documents(&original_docs, &database)?;
    assert_core_four_history(&original_history, &database)?;
    assert_eq!(metadata(&database)?, original_metadata);
    assert_eq!(generation_metadata(&database)?, requests);
    assert_eq!(attempt_metadata(&database)?, attempts);

    let running = MessageIdentity::new(RequestId::new("request-4")?, AttemptId::new("attempt-4")?);
    let readonly = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(
        readonly
            .generation_attempt(&running)?
            .unwrap()
            .checkpoint
            .state,
        JobState::Running
    );
    readonly.validate()?;
    drop(readonly);
    assert!(ProjectStore::migrate(&path)?.backup.is_none());
    assert_eq!(attempt_metadata(&database)?, attempts);

    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(
        store
            .generation_attempt(&running)?
            .unwrap()
            .checkpoint
            .state,
        JobState::Failed
    );
    assert_eq!(generation_metadata(&database)?, requests);
    let selected = store
        .selected_generation_candidate(&RequestId::new("request-0")?)?
        .unwrap();
    assert_eq!(selected.identity.attempt_id.as_str(), "selected");
    let next = MessageIdentity::new(
        RequestId::new("request-0")?,
        AttemptId::new("post-migration")?,
    );
    store.begin_generation_attempt(BeginGenerationAttempt {
        identity: next.clone(),
        cancellation_token: CancellationToken::new("post-migration-cancel")?,
    })?;
    assert_eq!(store.generation_attempt(&next)?.unwrap().ordinal, 3);
    let from = store.snapshot()?.revision_id().clone();
    let to = RevisionId::new("v7-redo")?;
    let mut observations = Vec::new();
    for index in 0..9 {
        let request = store
            .generation_request(&RequestId::new(format!("request-{index}"))?)?
            .unwrap();
        observations.push(RelevanceObservation {
            request_id: request.request_id,
            after_context: ContextObservation::Resolved(request.binding.context_sha256.clone()),
            binding: request.binding,
        });
    }
    store.redo_reconciled(
        &from,
        to.clone(),
        &RelevancePlan {
            from_revision: from.clone(),
            to_revision: to,
            observations,
        },
    )?;
    assert_eq!(
        store.snapshot()?.nodes()[&NodeId::new("hold-0")?].label,
        "Renamed"
    );
    store.validate()?;
    Ok(())
}

#[test]
fn schema_six_corruption_preserves_original_history_jobs_and_backup() -> Result {
    for corruption in [
        "UPDATE generation_attempts SET worker_candidate=json_set(worker_candidate,'$.unknown',1) WHERE attempt_id='selected'",
        "UPDATE generation_candidate_receipts SET video=json_set(video,'$.unknown',1) WHERE attempt_id='selected'",
        "UPDATE generation_attempt_heads SET selected_ready_attempt_id='attempt-0' WHERE request_id='request-0'",
        "UPDATE generation_attempt_heads SET high_water=1 WHERE request_id='request-0'",
        "UPDATE generation_attempts SET worker_candidate=printf('%020000d',0) WHERE attempt_id='selected'",
        "UPDATE generation_candidate_receipts SET provider=printf('%020000d',0) WHERE attempt_id='selected'",
        "UPDATE revisions SET document=json_set(document,'$.schema_version',5)",
        "UPDATE history SET request=json_set(request,'$.command.command','revert_generated_hold')",
    ] {
        let scratch = tempfile::tempdir()?;
        let path = fixture_version(scratch.path(), 6)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        database.pragma_update(None, "ignore_check_constraints", true)?;
        database.execute_batch(corruption)?;
        let original = contents(&database)?;
        let requests = generation_metadata(&database)?;
        let attempts = attempt_metadata(&database)?;
        let failure = ProjectStore::migrate(&path).unwrap_err();
        let StoreError::MigrationFailed { backup, .. } = failure else {
            panic!("migration must retain its backup: {corruption}")
        };
        let backup = Connection::open(backup)?;
        for connection in [&database, &backup] {
            assert_eq!(contents(connection)?, original, "{corruption}");
            assert_eq!(generation_metadata(connection)?, requests, "{corruption}");
            assert_eq!(attempt_metadata(connection)?, attempts, "{corruption}");
        }
    }
    Ok(())
}

#[test]
fn schema_seven_preserves_legacy_core_history_and_selected_legacy_attempt() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 7)?;
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(7))
    ));
    let database = Connection::open(path.join("project.sqlite"))?;
    let original = contents(&database)?;
    let original_docs = docs(&database)?;
    let original_history = history_json(&database)?;
    let original_metadata = metadata(&database)?;
    let requests = generation_metadata(&database)?;
    let attempts = attempt_metadata(&database)?;

    let outcome = ProjectStore::migrate(&path)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (7, DATABASE_SCHEMA_VERSION)
    );
    let backup = Connection::open(outcome.backup.unwrap())?;
    assert_eq!(contents(&backup)?, original);
    assert_eq!(generation_metadata(&backup)?, requests);
    assert_eq!(attempt_metadata(&backup)?, attempts);

    assert_core_five_documents(&original_docs, &database)?;
    assert_core_five_history(&original_history, &database)?;
    assert_eq!(metadata(&database)?, original_metadata);
    assert_eq!(generation_metadata(&database)?, requests);
    assert_eq!(attempt_metadata(&database)?, attempts);
    assert_eq!(
        database.query_row(
            "SELECT COUNT(*) FROM generation_requests WHERE bridge_plan IS NOT NULL",
            [],
            |row| row.get::<_, i64>(0),
        )?,
        0
    );
    assert_eq!(
        database.query_row(
            "SELECT COUNT(*) FROM generation_bundle_receipts",
            [],
            |row| { row.get::<_, i64>(0) }
        )?,
        0
    );

    let store = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    let selected = store
        .selected_generation_candidate(&RequestId::new("schema7-request")?)?
        .unwrap();
    assert_eq!(selected.identity.attempt_id.as_str(), "selected");
    assert_eq!(store.snapshot()?.revision_id().as_str(), "undo-rename");
    store.validate()?;
    Ok(())
}

#[test]
fn every_old_database_rejects_new_asset_hash_vocabulary_without_promotion() -> Result {
    let asset = serde_json::json!({
        "label": "Injected new identity", "content_hash": format!("blake3:{}", "a".repeat(64)),
        "video": null, "audio": null, "still_image": true, "frame_count": null,
    });
    for version in 1..=6 {
        let scratch = tempfile::tempdir()?;
        let path = fixture_version(scratch.path(), version)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        // Apply the same extra immutable asset to every snapshot. Chronology
        // still agrees; only the old schema's SHA-256 contract makes it invalid.
        database.execute(
            "UPDATE revisions SET document=json_set(document,'$.assets.injected',json(?1))",
            [asset.to_string()],
        )?;
        let original = contents(&database)?;
        let failure = ProjectStore::migrate(&path).unwrap_err();
        let StoreError::MigrationFailed { backup, .. } = failure else {
            panic!("legacy identity rejection must retain backup for schema {version}")
        };
        assert_eq!(contents(&database)?, original);
        assert_eq!(contents(&Connection::open(backup)?)?, original);
    }
    Ok(())
}

#[test]
fn corruption_is_rejected_without_promoting_any_migrated_rows() -> Result {
    for corruption in [
        "UPDATE revisions SET document=json_set(document,'$.nodes.repeat.kind.plays',7) WHERE id='v1-grow'",
        "UPDATE history SET edit=json_set(edit,'$.duration_delta',999) WHERE revision_id='v1-wrap'",
        "UPDATE history SET request=json_set(request,'$.command.command','move_plays') WHERE revision_id='v1-wrap'",
        "UPDATE history SET request=replace(request,'\"plays\":3','\"plays\":3,\"plays\":3') WHERE revision_id='v1-wrap'",
        "UPDATE revisions SET document=json_set(document,'$.schema_version',2) WHERE id='v1-insert'",
        "DELETE FROM redo WHERE position=2",
    ] {
        let scratch = tempfile::tempdir()?;
        let path = fixture(scratch.path())?;
        let database = Connection::open(path.join("project.sqlite"))?;
        database.execute_batch(corruption)?;
        let before = contents(&database)?;
        let error = ProjectStore::migrate(&path).unwrap_err();
        assert_eq!(error.code(), "MigrationFailed", "{corruption}");
        let StoreError::MigrationFailed { backup, .. } = error else {
            panic!("backup path is required")
        };
        assert!(backup.is_file());
        assert_eq!(contents(&database)?, before, "{corruption}");
        assert!(fs::read_dir(path.join("Snapshots"))?.any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(&format!("before-schema-{DATABASE_SCHEMA_VERSION}-"))
        }));
    }
    Ok(())
}

#[test]
fn writer_and_database_locks_prevent_migration_without_damage() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = fixture(scratch.path())?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let before = contents(&database)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path.join(".writer.lock"))?;
    lock.try_lock()?;
    assert!(matches!(
        ProjectStore::migrate(&path),
        Err(StoreError::AlreadyOpen)
    ));
    lock.unlock()?;
    database.execute_batch("BEGIN IMMEDIATE")?;
    let error = ProjectStore::migrate(&path).unwrap_err();
    assert_eq!(error.code(), "ProjectBusy");
    assert!(matches!(error, StoreError::MigrationFailed { .. }));
    assert_eq!(contents(&database)?, before);
    database.execute_batch("ROLLBACK")?;
    ProjectStore::migrate(&path)?;
    Ok(())
}

#[test]
fn schema_two_history_preserves_compact_identities_and_pending_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 2)?;
    for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
        assert!(matches!(
            ProjectStore::open(&path, mode),
            Err(StoreError::MigrationRequired(2))
        ));
    }
    let database = Connection::open(path.join("project.sqlite"))?;
    let original = contents(&database)?;
    let original_metadata = metadata(&database)?;
    let old_docs = docs(&database)?;
    let migration = ProjectStore::migrate(&path)?;
    assert_eq!(
        (migration.from_schema, migration.to_schema),
        (2, DATABASE_SCHEMA_VERSION)
    );
    assert_eq!(
        contents(&Connection::open(migration.backup.unwrap())?)?,
        original
    );
    assert_eq!(metadata(&database)?, original_metadata);
    for (revision, json) in old_docs {
        let current = snapshot(&database, &revision)?;
        assert!(
            legacy_v2::Document::from_json(&json)?.matches(&current),
            "{revision}"
        );
        assert!(current.marks().is_empty());
        assert!(current.overrides().is_empty());
    }
    let wrapped = snapshot(&database, "v2-wrap")?;
    let inserted = snapshot(&database, "v2-insert-plays")?;
    let moved = snapshot(&database, "v2-move-plays")?;
    let grown = snapshot(&database, "v2-grow-abandoned")?;
    let branch = snapshot(&database, "v2-branch")?;
    for old in 0..3 {
        let identity = order(&wrapped).at(old).unwrap();
        assert!(order(&inserted).position(&identity).is_some());
        assert!(order(&moved).position(&identity).is_some());
    }
    assert_ne!(order(&grown).at(3), order(&branch).at(3));
    let history = database
        .prepare("SELECT request,edit FROM history ORDER BY id")?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (request, edit) in history {
        let _: deadpan_core::CommandRequest = serde_json::from_str(&request)?;
        let edit: deadpan_core::EditTransaction = serde_json::from_str(&edit)?;
        assert!(edit.forward.marks.is_empty() && edit.inverse.marks.is_empty());
        assert!(edit.forward.overrides.is_empty() && edit.inverse.overrides.is_empty());
    }
    drop(database);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(order(&store.snapshot()?).len(), 3);
    store.redo(
        store.snapshot()?.revision_id(),
        RevisionId::new("v3-redo-branch")?,
    )?;
    assert_eq!(order(&store.snapshot()?), order(&branch));
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.redo(
        store.snapshot()?.revision_id(),
        RevisionId::new("v3-redo-subtree")?,
    )?;
    let current = store.snapshot()?;
    let NodeKind::Repeat { iterations, .. } =
        &current.nodes()[&NodeId::new("inserted-repeat")?].kind
    else {
        panic!()
    };
    assert_eq!(iterations.at(0).unwrap().allocation.as_str(), "v2-subtree");
    store.undo(current.revision_id(), RevisionId::new("v3-undo")?)?;
    store.validate()?;
    drop(store);
    assert!(ProjectStore::migrate(&path)?.backup.is_none());
    Ok(())
}

#[test]
fn old_schemas_reject_new_mark_vocabulary_without_changing_original() -> Result {
    for version in [1, 2] {
        let id = if version == 1 { "v1-wrap" } else { "v2-wrap" };
        let statements = [
            "UPDATE revisions SET document=json_set(document,'$.marks',json('{}'))",
            "UPDATE history SET edit=json_set(edit,'$.forward.marks',json('{}'))",
            "UPDATE history SET edit=json_set(edit,'$.inverse.marks',json('{}'))",
            "UPDATE history SET request=json_set(request,'$.command.command','set_mark')",
        ];
        let mut corruptions = statements
            .iter()
            .map(|sql| (*sql).to_owned())
            .collect::<Vec<_>>();
        corruptions.push(format!("UPDATE history SET request=json_set(request,'$.command.anchor_policy','first') WHERE revision_id='{id}'"));
        corruptions.push(format!(
            "UPDATE revisions SET document=json_set(document,'$.schema_version',3) WHERE id='{id}'"
        ));
        for corruption in corruptions {
            let scratch = tempfile::tempdir()?;
            let path = fixture_version(scratch.path(), version)?;
            let database = Connection::open(path.join("project.sqlite"))?;
            database.execute_batch(&corruption)?;
            let before = contents(&database)?;
            let failure = ProjectStore::migrate(&path).unwrap_err();
            assert_eq!(
                failure.code(),
                "MigrationFailed",
                "schema {version}, {corruption}"
            );
            let StoreError::MigrationFailed { backup, .. } = failure else {
                panic!()
            };
            assert_eq!(contents(&Connection::open(backup)?)?, before);
            assert_eq!(contents(&database)?, before);
        }
    }
    Ok(())
}

#[test]
fn schema_two_duplicate_identities_are_rejected_before_promotion() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 2)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    database.execute_batch("UPDATE revisions SET document=json_set(document,'$.nodes.repeat.kind.iterations.runs',json('[{\"allocation\":\"v2-wrap\",\"first\":0,\"count\":2},{\"allocation\":\"v2-wrap\",\"first\":1,\"count\":1}]')) WHERE id='v2-wrap'")?;
    let before = contents(&database)?;
    let failure = ProjectStore::migrate(&path).unwrap_err();
    assert_eq!(failure.code(), "MigrationFailed");
    assert_eq!(contents(&database)?, before);
    let StoreError::MigrationFailed { backup, .. } = failure else {
        panic!()
    };
    assert_eq!(contents(&Connection::open(backup)?)?, before);
    Ok(())
}

#[test]
fn legacy_projection_cannot_hide_marks_added_by_a_faulty_replay() -> Result {
    use deadpan_core::{
        Anchor, AnchorLossPolicy, BoundaryAnchor, Command, CommandRequest, InsertionBias, MarkId,
        ProjectFrame,
    };
    for version in [1, 2] {
        let scratch = tempfile::tempdir()?;
        let package = fixture_version(scratch.path(), version)?;
        let database = Connection::open(package.join("project.sqlite"))?;
        let initial_json: String = database.query_row(
            "SELECT document FROM revisions WHERE kind='initial'",
            [],
            |row| row.get(0),
        )?;
        let base = if version == 1 {
            legacy_v1::Document::from_json(&initial_json)?.upgrade()?
        } else {
            legacy_v2::Document::from_json(&initial_json)?.upgrade()?
        };
        let (request_json, edit_json): (String, String) = database.query_row(
            "SELECT request,edit FROM history WHERE parent_id IS NULL",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let request = if version == 1 {
            legacy_v1::upgrade_request(&request_json)?
        } else {
            legacy_v2::upgrade_request(&request_json)?
        };
        let expected = deadpan_core::apply(&base, &request)?;
        let matches = |edit: &deadpan_core::EditTransaction| {
            if version == 1 {
                legacy_v1::matches_edit(&edit_json, edit)
            } else {
                legacy_v2::matches_edit(&edit_json, edit)
            }
        };
        assert!(matches(&expected)?);
        let mark_edit = deadpan_core::apply(
            &base,
            &CommandRequest {
                project_id: base.project_id().clone(),
                expected_revision: base.revision_id().clone(),
                new_revision: RevisionId::new("injected-mark")?,
                command: Command::SetMark {
                    id: MarkId::new("unexpected")?,
                    owner: base.root().clone(),
                    label: "Should not be in old history".into(),
                    boundary: BoundaryAnchor {
                        coordinate: Anchor::Sequence {
                            frame: ProjectFrame(0),
                        },
                        bias: InsertionBias::Right,
                    },
                    loss_policy: AnchorLossPolicy::KeepUnresolved,
                },
            },
        )?;
        let mut forged = expected.clone();
        forged.forward.marks = mark_edit.forward.marks.clone();
        assert!(!matches(&forged)?);
        forged = expected;
        forged.inverse.marks = mark_edit.inverse.marks.clone();
        assert!(!matches(&forged)?);
        let mut marked = serde_json::to_value(mark_edit.forward.apply(&base)?)?;
        marked["revision_id"] = serde_json::json!(base.revision_id());
        let marked = ProjectDocument::from_json(&marked.to_string())?;
        let matches_document = if version == 1 {
            legacy_v1::Document::from_json(&initial_json)?.matches(&marked)
        } else {
            legacy_v2::Document::from_json(&initial_json)?.matches(&marked)
        };
        assert!(!matches_document);
    }
    Ok(())
}

#[test]
fn schema_three_history_preserves_every_mark_and_pending_redo() -> Result {
    use deadpan_core::{Anchor, InsertionBias, MarkId, MarkLossReason, MarkState};
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 3)?;
    for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
        assert!(matches!(
            ProjectStore::open(&path, mode),
            Err(StoreError::MigrationRequired(3))
        ));
    }
    let database = Connection::open(path.join("project.sqlite"))?;
    let original = contents(&database)?;
    let original_metadata = metadata(&database)?;
    let old_docs = docs(&database)?;
    let old_edits = database
        .prepare("SELECT id,edit FROM history ORDER BY id")?
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    assert_eq!(old_docs.len(), 23);
    assert_eq!(old_edits.len(), 17);
    let migration = ProjectStore::migrate(&path)?;
    assert_eq!(
        (migration.from_schema, migration.to_schema),
        (3, DATABASE_SCHEMA_VERSION)
    );
    let backup = migration.backup.unwrap();
    assert!(
        backup
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(&format!("before-schema-{DATABASE_SCHEMA_VERSION}-"))
    );
    assert_eq!(contents(&Connection::open(backup)?)?, original);
    assert_eq!(metadata(&database)?, original_metadata);
    for (revision, json) in old_docs {
        let current = snapshot(&database, &revision)?;
        assert!(
            legacy_v3::Document::from_json(&json)?.matches(&current),
            "{revision}"
        );
        let old: serde_json::Value = serde_json::from_str(&json)?;
        assert_eq!(
            serde_json::to_value(current.marks())?,
            old["marks"],
            "{revision}"
        );
        assert!(current.overrides().is_empty());
    }
    for (id, old_json) in old_edits {
        let json: String =
            database.query_row("SELECT edit FROM history WHERE id=?1", [id], |row| {
                row.get(0)
            })?;
        let current: deadpan_core::EditTransaction = serde_json::from_str(&json)?;
        assert!(legacy_v3::matches_edit(&old_json, &current)?);
        assert!(current.forward.overrides.is_empty() && current.inverse.overrides.is_empty());
    }
    let wrapped = snapshot(&database, "v3-wrap")?;
    let shrunk = snapshot(&database, "v3-shrink")?;
    let grown = snapshot(&database, "v3-grow")?;
    let retired = order(&wrapped).at(2).unwrap();
    assert!(order(&shrunk).position(&retired).is_none());
    assert!(order(&grown).position(&retired).is_none());
    assert_eq!(order(&grown).at(2).unwrap().allocation.as_str(), "v3-grow");
    let lost = MarkState::Unresolved {
        reason: MarkLossReason::OccurrenceMissing,
    };
    assert_eq!(shrunk.marks()[&MarkId::new("occurrence")?].state, lost);
    assert_eq!(grown.marks()[&MarkId::new("occurrence")?].state, lost);
    let deleted = snapshot(&database, "v3-delete")?;
    assert_eq!(
        deleted.marks()[&MarkId::new("owned-keep")?].state,
        MarkState::Unresolved {
            reason: MarkLossReason::OwnerMissing
        }
    );
    assert!(!deleted.marks().contains_key(&MarkId::new("owned-delete")?));
    assert_eq!(
        deleted.marks()[&MarkId::new("left")?].boundary.bias,
        InsertionBias::Left
    );
    assert_eq!(
        deleted.marks()[&MarkId::new("right")?].boundary.bias,
        InsertionBias::Right
    );
    assert!(matches!(
        deleted.marks()[&MarkId::new("pinned")?].boundary.coordinate,
        Anchor::Sequence {
            frame: deadpan_core::ProjectFrame(1)
        }
    ));
    let branch = snapshot(&database, "v3-branch")?;
    let mark_deleted = snapshot(&database, "v3-delete-mark")?;
    drop(database);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?.marks(), deleted.marks());
    store.redo(
        store.snapshot()?.revision_id(),
        RevisionId::new("v4-redo-branch")?,
    )?;
    assert_eq!(store.snapshot()?.marks(), branch.marks());
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.redo(
        store.snapshot()?.revision_id(),
        RevisionId::new("v4-redo-delete-mark")?,
    )?;
    assert_eq!(store.snapshot()?.marks(), mark_deleted.marks());
    store.undo(
        store.snapshot()?.revision_id(),
        RevisionId::new("v4-undo-delete-mark")?,
    )?;
    assert_eq!(store.snapshot()?.marks(), branch.marks());
    store.validate()?;
    drop(store);
    assert!(ProjectStore::migrate(&path)?.backup.is_none());
    Ok(())
}

#[test]
fn every_old_schema_rejects_override_vocabulary_before_promotion() -> Result {
    for version in [1, 2, 3] {
        for corruption in [
            "UPDATE revisions SET document=json_set(document,'$.overrides',json('{}'))",
            "UPDATE history SET edit=json_set(edit,'$.forward.overrides',json('{}'))",
            "UPDATE history SET edit=json_set(edit,'$.inverse.overrides',json('{}'))",
            "UPDATE history SET request=json_set(request,'$.command.subtree.overrides',json('{}')) WHERE json_extract(request,'$.command.command')='insert'",
            "UPDATE history SET request=json_set(request,'$.command.command','set_play_override')",
            "UPDATE history SET request=json_set(request,'$.command.command','clear_play_override')",
            "UPDATE revisions SET document=json_set(document,'$.schema_version',4)",
        ] {
            let scratch = tempfile::tempdir()?;
            let path = fixture_version(scratch.path(), version)?;
            let database = Connection::open(path.join("project.sqlite"))?;
            database.execute_batch(corruption)?;
            let before = contents(&database)?;
            let failure = ProjectStore::migrate(&path).unwrap_err();
            let StoreError::MigrationFailed { backup, .. } = failure else {
                panic!("schema {version}: {corruption}");
            };
            assert_eq!(contents(&database)?, before);
            assert_eq!(contents(&Connection::open(backup)?)?, before);
        }
    }
    Ok(())
}

#[test]
fn legacy_projection_cannot_hide_empty_override_entries_from_faulty_replay() -> Result {
    use deadpan_core::{EditTransaction, PlayOverrides, ValueChange};
    for version in [1, 2, 3] {
        let scratch = tempfile::tempdir()?;
        let path = fixture_version(scratch.path(), version)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        let initial_json: String = database.query_row(
            "SELECT document FROM revisions WHERE kind='initial'",
            [],
            |row| row.get(0),
        )?;
        let base = match version {
            1 => legacy_v1::Document::from_json(&initial_json)?.upgrade()?,
            2 => legacy_v2::Document::from_json(&initial_json)?.upgrade()?,
            _ => legacy_v3::Document::from_json(&initial_json)?.upgrade()?,
        };
        let (request_json, edit_json): (String, String) = database.query_row(
            "SELECT request,edit FROM history WHERE parent_id IS NULL",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let request = match version {
            1 => legacy_v1::upgrade_request(&request_json)?,
            2 => legacy_v2::upgrade_request(&request_json)?,
            _ => legacy_v3::upgrade_request(&request_json)?,
        };
        let expected = deadpan_core::apply(&base, &request)?;
        let matches = |edit: &EditTransaction| match version {
            1 => legacy_v1::matches_edit(&edit_json, edit),
            2 => legacy_v2::matches_edit(&edit_json, edit),
            _ => legacy_v3::matches_edit(&edit_json, edit),
        };
        assert!(matches(&expected)?);
        for forward in [true, false] {
            let mut forged = expected.clone();
            let patch = if forward {
                &mut forged.forward
            } else {
                &mut forged.inverse
            };
            patch.overrides.insert(
                base.root().clone(),
                ValueChange {
                    before: None,
                    after: Some(PlayOverrides::default()),
                },
            );
            assert!(!matches(&forged)?);
        }
    }
    Ok(())
}

#[test]
fn schema_three_mark_corruption_retains_original_and_backup() -> Result {
    for corruption in [
        "UPDATE revisions SET document=json_set(document,'$.marks.left.boundary.bias','right') WHERE id='v3-mark-left'",
        "UPDATE history SET request=json_set(request,'$.command.loss_policy','delete_owned') WHERE revision_id='v3-mark-occurrence'",
        "UPDATE history SET edit=json_set(edit,'$.inverse.marks.left.after.label','tampered') WHERE revision_id='v3-delete-mark'",
    ] {
        let scratch = tempfile::tempdir()?;
        let path = fixture_version(scratch.path(), 3)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        database.execute_batch(corruption)?;
        let before = contents(&database)?;
        let StoreError::MigrationFailed { backup, .. } = ProjectStore::migrate(&path).unwrap_err()
        else {
            panic!()
        };
        assert_eq!(contents(&database)?, before);
        assert_eq!(contents(&Connection::open(backup)?)?, before);
    }
    Ok(())
}

#[test]
fn schema_fifteen_preserves_presentation_chronology_and_all_operational_state() -> Result {
    use deadpan_core::{
        AudioBoundaryKind, AudioEdgePolicies, AudioEdgePolicy, Command, CommandRequest,
        EditTransaction,
    };
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 15)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_docs = docs(&database)?;
    let old_history = history_json(&database)?;
    let old_metadata = metadata(&database)?;
    let old_operational = operational_metadata(&database)?;
    let old_qualifications = qualification_metadata(&database)?;
    assert_eq!((old_docs.len(), old_history.len()), (61, 31));
    for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
        assert!(matches!(
            ProjectStore::open(&path, mode),
            Err(StoreError::MigrationRequired(15))
        ));
    }
    let migration = ProjectStore::migrate(&path)?;
    assert_eq!(
        (migration.from_schema, migration.to_schema),
        (15, DATABASE_SCHEMA_VERSION)
    );
    let backup = Connection::open(migration.backup.unwrap())?;
    assert_eq!(contents(&backup)?, before);
    assert_eq!(operational_metadata(&backup)?, old_operational);
    assert_eq!(qualification_metadata(&backup)?, old_qualifications);
    let new_docs = docs(&database)?;
    assert_eq!(new_docs.len(), old_docs.len());
    for ((old_id, old_json), (new_id, new_json)) in old_docs.iter().zip(new_docs) {
        assert_eq!(old_id, &new_id);
        let current = ProjectDocument::from_json(&new_json)?;
        assert!(
            legacy_v10::Document::from_json(old_json)?.matches(&current),
            "{old_id}"
        );
        assert!(
            current
                .nodes()
                .values()
                .all(|node| node.audio_edges == AudioEdgePolicies::default())
        );
        let old: serde_json::Value = serde_json::from_str(old_json)?;
        assert_eq!(
            serde_json::to_value(current.basis_state())?,
            old["basis_state"]
        );
        assert_eq!(
            serde_json::to_value(current.presentation_basis())?,
            old["presentation_basis"]
        );
    }
    let new_history = history_json(&database)?;
    assert_eq!(new_history.len(), old_history.len());
    for ((old_request, old_edit), (new_request, new_edit)) in old_history.iter().zip(new_history) {
        let request: CommandRequest = serde_json::from_str(&new_request)?;
        assert_eq!(legacy_v10::upgrade_request(old_request)?, request);
        let edit: EditTransaction = serde_json::from_str(&new_edit)?;
        assert!(legacy_v10::matches_edit(old_edit, &edit)?);
        let prior = snapshot(&database, request.expected_revision.as_str())?;
        let after = snapshot(&database, request.new_revision.as_str())?;
        assert_eq!(edit.forward.apply(&prior)?, after);
        assert_eq!(edit.inverse.apply(&after)?, prior);
    }
    assert_eq!(metadata(&database)?, old_metadata);
    assert_eq!(operational_metadata(&database)?, old_operational);
    assert_eq!(qualification_metadata(&database)?, old_qualifications);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let baseline = store.snapshot()?;
    assert_eq!(baseline.revision_id().as_str(), "schema15-pending-redo");
    assert!(baseline.basis_state().primary.is_some());
    let next = RevisionId::new("schema16-redo-canvas")?;
    store.redo_reconciled(
        baseline.revision_id(),
        next.clone(),
        &retained_relevance(&store, &next)?,
    )?;
    assert_eq!(
        (
            store.snapshot()?.presentation_basis().width,
            store.snapshot()?.presentation_basis().height
        ),
        (1280, 720)
    );
    let undo = RevisionId::new("schema16-undo-canvas")?;
    store.undo_reconciled(&next, undo.clone(), &retained_relevance(&store, &undo)?)?;
    assert_eq!(
        store.snapshot()?.presentation_basis(),
        baseline.presentation_basis()
    );
    assert_eq!(store.snapshot()?.basis_state(), baseline.basis_state());
    let node = NodeId::new("schema15-primary-clip")?;
    let command = CommandRequest {
        project_id: baseline.project_id().clone(),
        expected_revision: undo,
        new_revision: RevisionId::new("schema16-edge")?,
        command: Command::SetAudioEdge {
            node: node.clone(),
            edge: AudioBoundaryKind::NodeStart,
            policy: AudioEdgePolicy::Hard,
        },
    };
    let outcome = store.commit_reconciled(
        &command,
        &retained_relevance(&store, &command.new_revision)?,
    )?;
    assert_eq!(outcome.edit.duration_delta, 0);
    assert_eq!(
        store.snapshot()?.nodes()[&node].audio_edges.node_start,
        AudioEdgePolicy::Hard
    );
    let next = RevisionId::new("schema16-undo-edge")?;
    store.undo_reconciled(
        &command.new_revision,
        next.clone(),
        &retained_relevance(&store, &next)?,
    )?;
    assert_eq!(
        store.snapshot()?.nodes()[&node].audio_edges,
        AudioEdgePolicies::default()
    );
    let redo = RevisionId::new("schema16-redo-edge")?;
    store.redo_reconciled(&next, redo.clone(), &retained_relevance(&store, &redo)?)?;
    store.validate()?;
    drop(store);
    let reopened = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(
        reopened.snapshot()?.nodes()[&node].audio_edges.node_start,
        AudioEdgePolicy::Hard
    );
    assert_eq!(reopened.snapshot()?.basis_state(), baseline.basis_state());
    assert_eq!(operational_metadata(&database)?, old_operational);
    assert_eq!(qualification_metadata(&database)?, old_qualifications);
    Ok(())
}

#[test]
fn every_legacy_snapshot_defaults_audio_edges_without_retiming() -> Result {
    use deadpan_core::AudioEdgePolicies;
    for version in 1..=15 {
        let scratch = tempfile::tempdir()?;
        let path = fixture_version(scratch.path(), version)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        let old_metadata = metadata(&database)?;
        ProjectStore::migrate(&path)?;
        assert_eq!(metadata(&database)?, old_metadata);
        for (revision, wire) in docs(&database)? {
            let document = ProjectDocument::from_json(&wire)?;
            assert!(
                document
                    .nodes()
                    .values()
                    .all(|node| node.audio_edges == AudioEdgePolicies::default()),
                "database {version}, revision {revision}"
            );
        }
    }
    Ok(())
}

#[test]
fn every_legacy_schema_rejects_audio_edge_vocabulary_without_promotion() -> Result {
    use serde_json::{Value, json};
    for version in 1..=15 {
        for target in [
            "initial",
            "later",
            "forward",
            "inverse",
            "command",
            "occurrence",
        ] {
            let scratch = tempfile::tempdir()?;
            let path = fixture_version(scratch.path(), version)?;
            let database = Connection::open(path.join("project.sqlite"))?;
            if matches!(target, "initial" | "later") {
                let sql = if target == "initial" {
                    "SELECT id,document FROM revisions WHERE parent_id IS NULL"
                } else {
                    "SELECT id,document FROM revisions WHERE kind='edit' ORDER BY rowid DESC LIMIT 1"
                };
                let (id, wire): (String, String) =
                    database.query_row(sql, [], |row| Ok((row.get(0)?, row.get(1)?)))?;
                let mut wire: Value = serde_json::from_str(&wire)?;
                let root = wire["root"].as_str().unwrap().to_owned();
                wire["nodes"][root]["audio_edges"] = Value::Null;
                database.execute(
                    "UPDATE revisions SET document=?1 WHERE id=?2",
                    [&wire.to_string(), &id],
                )?;
            } else {
                let (id, request, edit): (i64, String, String) = database.query_row(
                    "SELECT id,request,edit FROM history ORDER BY id LIMIT 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )?;
                let mut request: Value = serde_json::from_str(&request)?;
                let mut edit: Value = serde_json::from_str(&edit)?;
                if matches!(target, "forward" | "inverse") {
                    let change = edit[target]["nodes"]
                        .as_object_mut()
                        .unwrap()
                        .values_mut()
                        .next()
                        .unwrap();
                    let node = if change["after"].is_object() {
                        &mut change["after"]
                    } else {
                        &mut change["before"]
                    };
                    node["audio_edges"] = Value::Null;
                } else if target == "command" {
                    request["command"] = json!({"command":"set_audio_edge", "node":"root", "edge":"node_start", "policy":"hard"});
                } else {
                    request["command"] = json!({"command":"edit_occurrence", "instance":{"node":"root","repeats":[]}, "edit":{"type":"set_audio_edge","edge":"node_start","policy":"hard"}, "identities":{"nodes":[],"marks":[]}});
                }
                database.execute(
                    "UPDATE history SET request=?1,edit=?2 WHERE id=?3",
                    rusqlite::params![request.to_string(), edit.to_string(), id],
                )?;
            }
            let before = contents(&database)?;
            let operational = operational_metadata(&database)?;
            let qualifications = (version >= 14)
                .then(|| qualification_metadata(&database))
                .transpose()?;
            let backup = match ProjectStore::migrate(&path) {
                Err(StoreError::MigrationFailed { backup, .. }) => backup,
                result => panic!("schema {version}, {target}: {result:?}"),
            };
            for connection in [&database, &Connection::open(backup)?] {
                assert_eq!(contents(connection)?, before);
                assert_eq!(operational_metadata(connection)?, operational);
                if let Some(qualifications) = &qualifications {
                    assert_eq!(&qualification_metadata(connection)?, qualifications);
                }
            }
        }
    }
    Ok(())
}

fn assert_core_eleven_replay(
    originals: &[(String, String)],
    original_history: &[(String, String)],
    database: &Connection,
) -> Result {
    use deadpan_core::{CommandRequest, EditTransaction, RetimePurpose};
    let migrated = docs(database)?;
    assert_eq!(migrated.len(), originals.len());
    for ((old_id, old_json), (new_id, new_json)) in originals.iter().zip(migrated) {
        assert_eq!(old_id, &new_id);
        let current = ProjectDocument::from_json(&new_json)?;
        assert!(legacy_v11::Document::from_json(old_json)?.matches(&current));
        for node in current.nodes().values() {
            if let NodeKind::Retime { purpose, .. } = node.kind {
                assert_eq!(purpose, RetimePurpose::Edit);
            }
        }
        // Default purpose must not grow every old node and exceed old caps.
        let mut old: serde_json::Value = serde_json::from_str(old_json)?;
        old["schema_version"] = serde_json::json!(deadpan_core::DOCUMENT_SCHEMA_VERSION);
        assert_eq!(serde_json::to_value(current)?, old);
    }
    let migrated_history = history_json(database)?;
    assert_eq!(original_history.len(), migrated_history.len());
    for ((old_request, old_edit), (new_request, new_edit)) in
        original_history.iter().zip(migrated_history)
    {
        let request: CommandRequest = serde_json::from_str(&new_request)?;
        let edit: EditTransaction = serde_json::from_str(&new_edit)?;
        assert_eq!(legacy_v11::upgrade_request(old_request)?, request);
        assert!(legacy_v11::matches_edit(old_edit, &edit)?);
        let before = snapshot(database, request.expected_revision.as_str())?;
        let after = snapshot(database, request.new_revision.as_str())?;
        assert_eq!(edit.forward.apply(&before)?, after);
        assert_eq!(edit.inverse.apply(&after)?, before);
    }
    Ok(())
}

#[test]
fn schema_seventeen_preserves_single_original_floor_and_authored_crop_history() -> Result {
    use deadpan_core::AudioEdgePolicy;
    use deadpan_store::single_source::SingleSourceState;
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 17)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_docs = docs(&database)?;
    let old_history = history_json(&database)?;
    let old_metadata = metadata(&database)?;
    let old_operational = operational_metadata(&database)?;
    let old_qualifications = qualification_metadata(&database)?;
    let old_profile: (String, i64) = database.query_row(
        "SELECT profile,baseline_history FROM single_source",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let profile: SingleSourceState = serde_json::from_str(&old_profile.0)?;
    assert_eq!((old_docs.len(), old_history.len()), (21, 12));
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(17))
    ));
    let migration = ProjectStore::migrate(&path)?;
    assert_eq!(
        (migration.from_schema, migration.to_schema),
        (17, DATABASE_SCHEMA_VERSION)
    );
    let backup = Connection::open(migration.backup.unwrap())?;
    assert_eq!(contents(&backup)?, before);
    assert_core_eleven_replay(&old_docs, &old_history, &database)?;
    assert_eq!(metadata(&database)?, old_metadata);
    assert_eq!(operational_metadata(&database)?, old_operational);
    assert_eq!(qualification_metadata(&database)?, old_qualifications);
    let new_profile: (String, i64) = database.query_row(
        "SELECT profile,baseline_history FROM single_source",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(new_profile, old_profile);
    let workflow: String =
        database.query_row("SELECT workflow FROM state", [], |row| row.get(0))?;
    assert_eq!(workflow, "single_source_v1");
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.single_source_state()?, Some(profile.clone()));
    let crop = NodeId::new("schema17-retime")?;
    assert_eq!(
        store.snapshot()?.nodes()[&crop].audio_edges.node_start,
        AudioEdgePolicy::Automatic
    );
    let initial_head = store.snapshot()?.revision_id().clone();
    let redo = RevisionId::new("schema18-redo-edge")?;
    store.redo(&initial_head, redo.clone())?;
    assert_eq!(
        store.snapshot()?.nodes()[&crop].audio_edges.node_start,
        AudioEdgePolicy::Hard
    );
    let mut count = 0;
    while store.history_availability()?.0 {
        let head = store.snapshot()?.revision_id().clone();
        store.undo(&head, RevisionId::new(format!("schema18-undo-{count}"))?)?;
        count += 1;
    }
    assert!(count >= 4);
    let SingleSourceState::Ready {
        node,
        baseline_revision,
        ..
    } = &profile
    else {
        panic!("fixture must be initialized");
    };
    let baseline = snapshot(&database, baseline_revision.as_str())?;
    let head = store.snapshot()?;
    assert_eq!(head.nodes(), baseline.nodes());
    assert_eq!(head.assets(), baseline.assets());
    assert!(matches!(head.nodes()[node].kind, NodeKind::Source { .. }));
    assert!(
        store
            .undo(head.revision_id(), RevisionId::new("below-floor")?)
            .is_err()
    );
    assert_eq!(store.snapshot()?, head);
    store.validate()?;
    drop(store);
    let reopened = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(reopened.single_source_state()?, Some(profile));
    assert!(!reopened.history_availability()?.0);
    assert_eq!(reopened.snapshot()?, head);
    Ok(())
}

#[test]
fn schema_seventeen_rejects_new_purpose_everywhere_and_missing_profile_without_promotion() -> Result
{
    for tamper in [
        "UPDATE revisions SET document=json_set(document,'$.nodes.\"schema17-retime\".kind.purpose',NULL) WHERE id='schema17-crop'",
        "UPDATE revisions SET document=json_set(document,'$.nodes.\"schema17-retime\".kind.purpose','partition') WHERE id='schema17-hard-edge'",
        "UPDATE history SET request=json_set(request,'$.command.subtree.nodes.\"schema17-retime\".kind.purpose','edit') WHERE revision_id='schema17-crop'",
        "UPDATE history SET edit=json_set(edit,'$.forward.nodes.\"schema17-retime\".after.kind.purpose','partition') WHERE revision_id='schema17-crop'",
        "UPDATE history SET edit=json_set(edit,'$.inverse.nodes.\"schema17-retime\".before.kind.purpose',NULL) WHERE revision_id='schema17-crop'",
        "DROP TABLE single_source",
        "DELETE FROM single_source",
        "ALTER TABLE state DROP COLUMN workflow",
    ] {
        let scratch = tempfile::tempdir()?;
        let path = fixture_version(scratch.path(), 17)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        database.execute_batch(tamper)?;
        let before = contents(&database)?;
        let error = ProjectStore::migrate(&path).unwrap_err();
        let StoreError::MigrationFailed { backup, .. } = error else {
            panic!("{error}");
        };
        assert_eq!(contents(&database)?, before, "{tamper}");
        assert_eq!(contents(&Connection::open(backup)?)?, before);
        assert_eq!(
            database.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
            17
        );
    }
    Ok(())
}

#[test]
fn schema_eighteen_replays_partitions_marks_branches_and_pending_redo() -> Result {
    use deadpan_core::{CommandRequest, EditTransaction, MarkId, MarkState, RetimePurpose};
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 18)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_docs = docs(&database)?;
    let old_history = history_json(&database)?;
    let old_metadata = metadata(&database)?;
    let old_operational = operational_metadata(&database)?;
    assert_eq!((old_docs.len(), old_history.len()), (10, 5));
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(18))
    ));
    let migration = ProjectStore::migrate(&path)?;
    assert_eq!(
        (migration.from_schema, migration.to_schema),
        (18, DATABASE_SCHEMA_VERSION)
    );
    assert_eq!(
        contents(&Connection::open(migration.backup.unwrap())?)?,
        before
    );
    assert_eq!(metadata(&database)?, old_metadata);
    assert_eq!(operational_metadata(&database)?, old_operational);
    for ((old_id, old_json), (new_id, new_json)) in old_docs.iter().zip(docs(&database)?) {
        assert_eq!(old_id, &new_id);
        let current = ProjectDocument::from_json(&new_json)?;
        assert!(legacy_v12::Document::from_json(old_json)?.matches(&current));
        let mut expected: serde_json::Value = serde_json::from_str(old_json)?;
        expected["schema_version"] = serde_json::json!(deadpan_core::DOCUMENT_SCHEMA_VERSION);
        assert_eq!(serde_json::to_value(current)?, expected);
    }
    for ((old_request, old_edit), (new_request, new_edit)) in
        old_history.iter().zip(history_json(&database)?)
    {
        let request: CommandRequest = serde_json::from_str(&new_request)?;
        let edit: EditTransaction = serde_json::from_str(&new_edit)?;
        assert_eq!(legacy_v12::upgrade_request(old_request)?, request);
        assert!(legacy_v12::matches_edit(old_edit, &edit)?);
        let before = snapshot(&database, request.expected_revision.as_str())?;
        let after = snapshot(&database, request.new_revision.as_str())?;
        assert_eq!(edit.forward.apply(&before)?, after);
        assert_eq!(edit.inverse.apply(&after)?, before);
    }
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.single_source_state()?, None);
    let baseline = store.snapshot()?;
    assert!(matches!(
        baseline.nodes()[&NodeId::new("partition")?].kind,
        NodeKind::Retime {
            purpose: RetimePurpose::Partition,
            ..
        }
    ));
    assert_eq!(
        baseline.marks()[&MarkId::new("edge")?].state,
        MarkState::Bound
    );
    let redo = RevisionId::new("fragment-schema-redo")?;
    store.redo(baseline.revision_id(), redo.clone())?;
    assert!(matches!(
        store.snapshot()?.marks()[&MarkId::new("cue")?].state,
        MarkState::Unresolved { .. }
    ));
    assert!(
        !store
            .snapshot()?
            .marks()
            .contains_key(&MarkId::new("edge")?)
    );
    store.undo(&redo, RevisionId::new("fragment-schema-undo")?)?;
    assert_eq!(store.snapshot()?.marks(), baseline.marks());
    assert_eq!(store.snapshot()?.nodes(), baseline.nodes());
    store.validate()?;
    drop(store);
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)?
            .snapshot()?
            .marks(),
        baseline.marks()
    );
    Ok(())
}

#[test]
fn schema_eighteen_rejects_fragment_vocabulary_without_promotion() -> Result {
    for direction in ["snapshot", "forward", "inverse", "command"] {
        for fragments in ["null", "[]"] {
            let scratch = tempfile::tempdir()?;
            let path = fixture_version(scratch.path(), 18)?;
            let database = Connection::open(path.join("project.sqlite"))?;
            if direction == "snapshot" {
                database.execute("UPDATE revisions SET document=json_set(document,'$.marks.cue.fragments',json(?1)) WHERE id='mark-local'", [fragments])?;
            } else if direction == "command" {
                database.execute("UPDATE history SET request=json_set(request,'$.command.fragments',json(?1)) WHERE revision_id='mark-local'", [fragments])?;
            } else {
                let pointer = if direction == "forward" {
                    "$.forward.marks.cue.after.fragments"
                } else {
                    "$.inverse.marks.cue.before.fragments"
                };
                database.execute("UPDATE history SET edit=json_set(edit,?1,json(?2)) WHERE revision_id='mark-local'", [pointer, fragments])?;
            }
            let before = contents(&database)?;
            let StoreError::MigrationFailed { backup, .. } =
                ProjectStore::migrate(&path).unwrap_err()
            else {
                panic!("expected retained failed migration");
            };
            assert_eq!(contents(&database)?, before);
            assert_eq!(contents(&Connection::open(backup)?)?, before);
            assert_eq!(
                database.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
                18
            );
        }
    }
    Ok(())
}

#[test]
fn every_legacy_database_retains_single_binding_marks_without_default_json_growth() -> Result {
    for version in 1..=18 {
        let scratch = tempfile::tempdir()?;
        let path = fixture_version(scratch.path(), version)?;
        ProjectStore::migrate(&path)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        for (revision, wire) in docs(&database)? {
            let document = ProjectDocument::from_json(&wire)?;
            for mark in document.marks().values() {
                assert_eq!(
                    mark.binding_count(),
                    1,
                    "schema {version}, revision {revision}"
                );
                assert!(
                    !serde_json::to_value(mark)?
                        .as_object()
                        .unwrap()
                        .contains_key("fragments")
                );
            }
        }
    }
    Ok(())
}

#[test]
fn schema_nineteen_replays_fragment_loss_branches_and_pending_redo() -> Result {
    use deadpan_core::{CommandRequest, EditTransaction, MarkId, MarkState, RetimePurpose};
    let scratch = tempfile::tempdir()?;
    let path = fixture_version(scratch.path(), 19)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_docs = docs(&database)?;
    let old_history = history_json(&database)?;
    let old_metadata = metadata(&database)?;
    let old_operational = operational_metadata(&database)?;
    assert_eq!((old_docs.len(), old_history.len()), (6, 3));
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(19))
    ));
    let migration = ProjectStore::migrate(&path)?;
    assert_eq!(
        (migration.from_schema, migration.to_schema),
        (19, DATABASE_SCHEMA_VERSION)
    );
    assert_eq!(
        contents(&Connection::open(migration.backup.unwrap())?)?,
        before
    );
    assert_eq!(metadata(&database)?, old_metadata);
    assert_eq!(operational_metadata(&database)?, old_operational);
    for ((old_id, old_json), (new_id, new_json)) in old_docs.iter().zip(docs(&database)?) {
        assert_eq!(old_id, &new_id);
        let current = ProjectDocument::from_json(&new_json)?;
        assert!(legacy_v13::Document::from_json(old_json)?.matches(&current));
        let mut expected: serde_json::Value = serde_json::from_str(old_json)?;
        expected["schema_version"] = serde_json::json!(deadpan_core::DOCUMENT_SCHEMA_VERSION);
        assert_eq!(serde_json::to_value(current)?, expected);
    }
    for ((old_request, old_edit), (new_request, new_edit)) in
        old_history.iter().zip(history_json(&database)?)
    {
        let request: CommandRequest = serde_json::from_str(&new_request)?;
        let edit: EditTransaction = serde_json::from_str(&new_edit)?;
        assert_eq!(legacy_v13::upgrade_request(old_request)?, request);
        assert!(legacy_v13::matches_edit(old_edit, &edit)?);
        let before = snapshot(&database, request.expected_revision.as_str())?;
        let after = snapshot(&database, request.new_revision.as_str())?;
        assert_eq!(edit.forward.apply(&before)?, after);
        assert_eq!(edit.inverse.apply(&after)?, before);
    }
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.single_source_state()?, None);
    let baseline = store.snapshot()?;
    assert_eq!(baseline.revision_id().as_str(), "undo-delete");
    assert_eq!(baseline.nodes()[&NodeId::new("second")?].label, "Kept name");
    assert!(matches!(
        baseline.nodes()[&NodeId::new("partition")?].kind,
        NodeKind::Retime {
            purpose: RetimePurpose::Partition,
            ..
        }
    ));
    let owned = MarkId::new("owned")?;
    let kept = MarkId::new("kept")?;
    assert_eq!(baseline.marks()[&owned].binding_count(), 3);
    assert_eq!(baseline.marks()[&kept].binding_count(), 3);
    let redo = RevisionId::new("split-schema-redo")?;
    store.redo(baseline.revision_id(), redo.clone())?;
    let deleted = store.snapshot()?;
    assert_eq!(deleted.marks()[&owned].binding_count(), 2);
    assert_eq!(deleted.marks()[&owned].owner, NodeId::new("second")?);
    assert_eq!(deleted.marks()[&kept].binding_count(), 3);
    assert!(matches!(
        deleted.marks()[&kept].state,
        MarkState::Unresolved { .. }
    ));
    assert!(
        deleted.marks()[&kept]
            .fragments
            .iter()
            .all(|fragment| fragment.state == MarkState::Bound)
    );
    store.undo(&redo, RevisionId::new("split-schema-undo")?)?;
    assert_eq!(store.snapshot()?.marks(), baseline.marks());
    assert_eq!(store.snapshot()?.nodes(), baseline.nodes());
    store.validate()?;
    drop(store);
    let reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(reopened.snapshot()?.marks(), baseline.marks());
    assert_eq!(reopened.snapshot()?.nodes(), baseline.nodes());
    // Re-migration is an idempotent validation, not a second backup or rewrite.
    drop(reopened);
    assert!(ProjectStore::migrate(&path)?.backup.is_none());
    Ok(())
}

#[test]
fn every_legacy_database_rejects_split_ingress_without_promotion() -> Result {
    use serde_json::json;
    for version in 1..=19 {
        for kind in ["direct", "occurrence", "extra_null", "split_null"] {
            let scratch = tempfile::tempdir()?;
            let path = fixture_version(scratch.path(), version)?;
            let database = Connection::open(path.join("project.sqlite"))?;
            let (id, wire): (i64, String) = database.query_row(
                "SELECT id,request FROM history ORDER BY id LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            let mut request: serde_json::Value = serde_json::from_str(&wire)?;
            match kind {
                "direct" => {
                    request["command"] = json!({"command":"split","node":"first","at":4,"identities":{"nodes":["left","right"]}})
                }
                "occurrence" => {
                    request["command"] = json!({"command":"edit_occurrence","instance":{"node":"first","repeats":[]},"edit":{"type":"split","at":4,"identities":{"nodes":["left","right"]}},"identities":{"nodes":[],"marks":[]}})
                }
                "split_null" => {
                    request["command"] =
                        json!({"command":"split","node":"first","at":null,"identities":null})
                }
                _ => request["command"]["at"] = serde_json::Value::Null,
            }
            database.execute(
                "UPDATE history SET request=?1 WHERE id=?2",
                rusqlite::params![request.to_string(), id],
            )?;
            let before = contents(&database)?;
            let StoreError::MigrationFailed { backup, .. } =
                ProjectStore::migrate(&path).unwrap_err()
            else {
                panic!("schema {version} {kind}: expected retained failed migration");
            };
            assert_eq!(contents(&database)?, before, "schema {version} {kind}");
            assert_eq!(contents(&Connection::open(backup)?)?, before);
            assert_eq!(
                database.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
                version
            );
        }
    }
    Ok(())
}

#[test]
fn schema_nineteen_rejects_unknown_fragment_fields_in_all_history_positions() -> Result {
    for position in [
        "initial",
        "later",
        "forward_before",
        "forward_after",
        "inverse_before",
        "inverse_after",
    ] {
        let scratch = tempfile::tempdir()?;
        let path = fixture_version(scratch.path(), 19)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        if matches!(position, "initial" | "later") {
            database.execute("UPDATE revisions SET document=json_set(document,'$.marks.owned.fragments[0].future',null) WHERE id=?1", [if position == "initial" { "initial" } else { "delete" }])?;
        } else {
            let (direction, side) = position.split_once('_').unwrap();
            let pointer = format!("$.{direction}.marks.owned.{side}.fragments[0].future");
            database.execute(
                "UPDATE history SET edit=json_set(edit,?1,null) WHERE revision_id='delete'",
                [pointer],
            )?;
        }
        let before = contents(&database)?;
        let StoreError::MigrationFailed { backup, .. } = ProjectStore::migrate(&path).unwrap_err()
        else {
            panic!("expected retained failed migration at {position}");
        };
        assert_eq!(contents(&database)?, before);
        assert_eq!(contents(&Connection::open(backup)?)?, before);
        assert_eq!(
            database.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
            19
        );
    }
    Ok(())
}
