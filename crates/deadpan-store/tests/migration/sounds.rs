use super::*;
use deadpan_core::*;
use serde_json::{Value, json};

fn fixture(root: &Path) -> Result<PathBuf> {
    let package = root.join("sound-history.deadpan");
    fs::create_dir(&package)?;
    fs::create_dir(package.join("Snapshots"))?;
    let database = Connection::open(package.join("project.sqlite"))?;
    database.pragma_update(None, "foreign_keys", false)?;
    database.execute_batch(include_str!("../fixtures/v34-sound-history.sql"))?;
    Ok(package)
}

#[test]
fn actual_schema34_history_retains_retimes_abandoned_edits_and_pending_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_documents = docs(&database)?;
    let old_history = history_json(&database)?;
    let old_metadata = metadata(&database)?;
    let old_operational = operational_metadata(&database)?;
    assert_eq!((old_documents.len(), old_history.len()), (47, 22));
    assert!(matches!(
        ProjectStore::open(&package, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(34))
    ));
    let outcome = ProjectStore::migrate(&package)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (34, DATABASE_SCHEMA_VERSION)
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
        let legacy = legacy_v28::Document::from_json(old)?;
        assert!(legacy.matches(&modern));
        assert_eq!(legacy.upgrade()?, modern);
        assert!(modern.sounds().is_empty());
        let mut expected: Value = serde_json::from_str(old)?;
        expected["schema_version"] = json!(DOCUMENT_SCHEMA_VERSION);
        assert_eq!(serde_json::to_value(modern)?, expected);
    }
    let mut retimes = 0;
    for ((old_request, old_edit), (new_request, new_edit)) in
        old_history.iter().zip(history_json(&database)?)
    {
        let request: CommandRequest = serde_json::from_str(&new_request)?;
        let edit: EditTransaction = serde_json::from_str(&new_edit)?;
        assert_eq!(legacy_v28::upgrade_request(old_request)?, request);
        assert!(legacy_v28::matches_edit(old_edit, &edit)?);
        assert_eq!(old_request, &new_request);
        assert_eq!(old_edit, &new_edit);
        let prior = snapshot(&database, request.expected_revision.as_str())?;
        let after = snapshot(&database, request.new_revision.as_str())?;
        assert_eq!(edit.forward.apply(&prior)?, after);
        assert_eq!(edit.inverse.apply(&after)?, prior);
        retimes += usize::from(matches!(
            request.command,
            Command::WrapRetime { .. }
                | Command::SetRetime { .. }
                | Command::EditOccurrence {
                    edit: OccurrenceEdit::SetRetime { .. },
                    ..
                }
        ));
    }
    assert_eq!(retimes, 4);
    let retime = NodeId::new("core28-retime")?;
    let duration = |document: &ProjectDocument| match document.nodes()[&retime].kind {
        NodeKind::Retime { duration, .. } => duration.frames(),
        _ => panic!("missing Retime"),
    };
    assert_eq!(
        duration(&snapshot(&database, "core28-abandoned-retime")?),
        12
    );
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let baseline = store.snapshot()?;
    assert_eq!(duration(&baseline), 6);
    store.redo(baseline.revision_id(), RevisionId::new("v34-pending-redo")?)?;
    let redone = store.snapshot()?;
    assert_eq!(duration(&redone), 7);
    store.undo(redone.revision_id(), RevisionId::new("v34-undo-redo")?)?;
    assert_eq!(store.snapshot()?.nodes(), baseline.nodes());
    assert!(store.snapshot()?.sounds().is_empty());
    store.validate()?;
    drop(store);
    ProjectStore::open(&package, AccessMode::ReadOnly)?.validate()?;
    assert!(ProjectStore::migrate(&package)?.backup.is_none());
    Ok(())
}

#[test]
fn schema28_rejects_sound_state_even_empty_or_null_without_promotion() -> Result {
    for position in ["initial", "later", "forward", "inverse"] {
        for value in ["{}", "null"] {
            let scratch = tempfile::tempdir()?;
            let package = fixture(scratch.path())?;
            let database = Connection::open(package.join("project.sqlite"))?;
            if matches!(position, "initial" | "later") {
                let selector = if position == "initial" {
                    "parent_id IS NULL"
                } else {
                    "parent_id IS NOT NULL"
                };
                database.execute(&format!("UPDATE revisions SET document=json_set(document,'$.sounds',json(?1)) WHERE id=(SELECT id FROM revisions WHERE {selector} LIMIT 1)"), [value])?;
            } else {
                database.execute("UPDATE history SET edit=json_set(edit,?1,json(?2)) WHERE id=(SELECT MIN(id) FROM history)", [format!("$.{position}.sounds"), value.into()])?;
            }
            let before = contents(&database)?;
            let StoreError::MigrationFailed { backup, .. } =
                ProjectStore::migrate(&package).unwrap_err()
            else {
                panic!("schema34 accepted {position} sounds={value}");
            };
            assert_eq!(contents(&database)?, before);
            assert_eq!(contents(&Connection::open(backup)?)?, before);
            assert_eq!(
                database.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
                34
            );
        }
    }
    Ok(())
}

#[test]
fn schema28_rejects_sound_commands_and_later_command_fields() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let (request, _) = &history_json(&database)?[0];
    let mut request: Value = serde_json::from_str(request)?;
    request["command"] = json!({"command":"delete_sound", "id":"sound"});
    let json = serde_json::to_string(&request)?;
    let modern: CommandRequest = serde_json::from_str(&json)?;
    assert!(matches!(modern.command, Command::DeleteSound { .. }));
    assert!(legacy_v28::upgrade_request(&json).is_err());
    database.execute(
        "UPDATE history SET request=?1 WHERE id=(SELECT MIN(id) FROM history)",
        [json],
    )?;
    let before = contents(&database)?;
    let StoreError::MigrationFailed { backup, .. } = ProjectStore::migrate(&package).unwrap_err()
    else {
        panic!("schema34 accepted sound command");
    };
    assert_eq!(contents(&database)?, before);
    assert_eq!(contents(&Connection::open(backup)?)?, before);
    Ok(())
}
