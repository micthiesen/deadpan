use super::*;
use deadpan_core::*;
use serde_json::{Value, json};

fn fixture(root: &Path) -> Result<PathBuf> {
    let package = root.join("retime-history.deadpan");
    fs::create_dir(&package)?;
    fs::create_dir(package.join("Snapshots"))?;
    let database = Connection::open(package.join("project.sqlite"))?;
    database.pragma_update(None, "foreign_keys", false)?;
    database.execute_batch(include_str!("../fixtures/v33-retime-history.sql"))?;
    Ok(package)
}

#[test]
fn actual_schema33_history_retains_source_splices_abandoned_edits_and_pending_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_documents = docs(&database)?;
    let old_history = history_json(&database)?;
    let old_metadata = metadata(&database)?;
    let old_operational = operational_metadata(&database)?;
    assert_eq!((old_documents.len(), old_history.len()), (39, 18));
    assert!(matches!(
        ProjectStore::open(&package, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(33))
    ));
    let outcome = ProjectStore::migrate(&package)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (33, DATABASE_SCHEMA_VERSION)
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
        let legacy = legacy_v27::Document::from_json(old)?;
        assert!(legacy.matches(&modern));
        assert_eq!(legacy.upgrade()?, modern);
        let mut expected: Value = serde_json::from_str(old)?;
        expected["schema_version"] = json!(DOCUMENT_SCHEMA_VERSION);
        assert_eq!(serde_json::to_value(modern)?, expected);
    }
    let mut splices = 0;
    for ((old_request, old_edit), (new_request, new_edit)) in
        old_history.iter().zip(history_json(&database)?)
    {
        let request: CommandRequest = serde_json::from_str(&new_request)?;
        let edit: EditTransaction = serde_json::from_str(&new_edit)?;
        assert_eq!(legacy_v27::upgrade_request(old_request)?, request);
        assert!(legacy_v27::matches_edit(old_edit, &edit)?);
        // Retime authoring adds commands, not new node or binding fields.
        // An admitted old patch must retain its exact persisted meaning.
        assert_eq!(old_request, &new_request);
        assert_eq!(old_edit, &new_edit);
        let prior = snapshot(&database, request.expected_revision.as_str())?;
        let after = snapshot(&database, request.new_revision.as_str())?;
        assert_eq!(edit.forward.apply(&prior)?, after);
        assert_eq!(edit.inverse.apply(&after)?, prior);
        splices += usize::from(matches!(request.command, Command::SpliceSource { .. }));
    }
    assert_eq!(splices, 3);
    let abandoned = snapshot(&database, "core27-abandoned-splice")?;
    assert!(
        abandoned
            .nodes()
            .contains_key(&NodeId::new("core27-abandoned-paste")?)
    );
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let baseline = store.snapshot()?;
    assert!(
        baseline
            .nodes()
            .contains_key(&NodeId::new("core27-root-paste")?)
    );
    assert!(
        !baseline
            .nodes()
            .contains_key(&NodeId::new("core27-abandoned-paste")?)
    );
    let nested = NodeId::new("core27-nested-paste")?;
    assert!(!baseline.nodes().contains_key(&nested));
    assert_eq!(
        baseline.audio_bindings().bindings()[&NodeId::new("core23-detached-gap")?]
            .lattice
            .reference
            .recipe,
        AudioRecipeKind::RepeatGap
    );
    store.redo(baseline.revision_id(), RevisionId::new("v33-pending-redo")?)?;
    let redone = store.snapshot()?;
    assert!(redone.nodes().contains_key(&nested));
    assert_eq!(
        redone.duration()?.frames(),
        baseline.duration()?.frames() + 30
    );
    store.undo(redone.revision_id(), RevisionId::new("v33-undo-redo")?)?;
    assert_authored_equal(&store.snapshot()?, &baseline)?;
    store.validate()?;
    drop(store);
    ProjectStore::open(&package, AccessMode::ReadOnly)?.validate()?;
    assert!(ProjectStore::migrate(&package)?.backup.is_none());
    Ok(())
}

fn request(document: &ProjectDocument, name: &str, command: Command) -> Result<CommandRequest> {
    Ok(CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(name)?,
        command,
    })
}

fn wrap() -> Result<Command> {
    Ok(Command::WrapRetime {
        node: NodeId::new("first")?,
        id: NodeId::new("retime")?,
        duration: FrameDuration::new(8)?,
        pitch: PitchPolicy::Preserve,
    })
}

fn set() -> Result<Command> {
    Ok(Command::SetRetime {
        node: NodeId::new("retime")?,
        duration: FrameDuration::new(3)?,
        pitch: PitchPolicy::FollowSpeed,
    })
}

fn assert_authored_equal(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = json!(actual.revision_id());
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

#[test]
fn retime_wrap_update_and_navigation_are_durable_atomic_edits() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("modern-retime.deadpan");
    let initial = super::sequence_insert::nested_initial(true)?;
    let mut store = ProjectStore::create(&package, &initial)?;
    let wrapped_request = request(&initial, "wrap", wrap()?)?;
    let wrapped_preview = store.preview(&wrapped_request)?;
    assert_eq!(store.snapshot()?, initial);
    assert_eq!(store.commit(&wrapped_request)?.edit, wrapped_preview);
    let wrapped = store.snapshot()?;
    assert_eq!(wrapped.duration()?.frames(), 11);
    assert_eq!(wrapped_preview.inverse.apply(&wrapped)?, initial);
    drop(store);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, wrapped);
    let changed_request = request(&wrapped, "set", set()?)?;
    let changed = store.commit(&changed_request)?.edit;
    let adjusted = store.snapshot()?;
    assert_eq!(adjusted.duration()?.frames(), 6);
    let NodeKind::Retime {
        mapping,
        pitch,
        duration,
        ..
    } = adjusted.nodes()[&NodeId::new("retime")?].kind
    else {
        panic!("retime wrapper missing");
    };
    assert_eq!(mapping, FrameRange::new(ProjectFrame(0), ProjectFrame(4))?);
    assert_eq!(pitch, PitchPolicy::FollowSpeed);
    assert_eq!(duration.frames(), 3);
    assert_eq!(changed.inverse.apply(&adjusted)?, wrapped);
    assert!(store.commit(&changed_request).is_err());
    assert_eq!(store.snapshot()?, adjusted);
    store.undo(adjusted.revision_id(), RevisionId::new("undo-set")?)?;
    assert_authored_equal(&store.snapshot()?, &wrapped)?;
    drop(store);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let undone = store.snapshot()?;
    store.redo(undone.revision_id(), RevisionId::new("redo-set")?)?;
    assert_authored_equal(&store.snapshot()?, &adjusted)?;
    store.undo(
        &RevisionId::new("redo-set")?,
        RevisionId::new("undo-set-again")?,
    )?;
    store.undo(
        &RevisionId::new("undo-set-again")?,
        RevisionId::new("undo-wrap")?,
    )?;
    assert_authored_equal(&store.snapshot()?, &initial)?;
    drop(store);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    store.redo(
        &RevisionId::new("undo-wrap")?,
        RevisionId::new("redo-wrap")?,
    )?;
    store.redo(
        &RevisionId::new("redo-wrap")?,
        RevisionId::new("redo-both")?,
    )?;
    assert_authored_equal(&store.snapshot()?, &adjusted)?;
    store.validate()?;
    drop(store);
    ProjectStore::open(&package, AccessMode::ReadOnly)?.validate()?;
    let database = Connection::open(package.join("project.sqlite"))?;
    assert_eq!(history_json(&database)?.len(), 2);
    assert_eq!(docs(&database)?.len(), 9);
    Ok(())
}

#[test]
fn schema27_rejects_new_retime_commands_even_with_matching_modern_history() -> Result {
    for update in [false, true] {
        for occurrence in [false, true] {
            let mut initial = super::sequence_insert::nested_initial(true)?;
            if update {
                let wrapped = request(&initial, "initial-wrapper", wrap()?)?;
                initial = deadpan_core::apply(&initial, &wrapped)?
                    .forward
                    .apply(&initial)?;
            }
            let command = if occurrence {
                Command::EditOccurrence {
                    instance: InstancePath {
                        node: NodeId::new(if update { "retime" } else { "first" })?,
                        repeats: Vec::new(),
                    },
                    edit: if update {
                        OccurrenceEdit::SetRetime {
                            duration: FrameDuration::new(3)?,
                            pitch: PitchPolicy::FollowSpeed,
                        }
                    } else {
                        OccurrenceEdit::WrapRetime {
                            id: NodeId::new("retime")?,
                            duration: FrameDuration::new(8)?,
                            pitch: PitchPolicy::Preserve,
                        }
                    },
                    identities: OccurrenceIdentities::default(),
                }
            } else if update {
                set()?
            } else {
                wrap()?
            };
            let scratch = tempfile::tempdir()?;
            let package = scratch.path().join("forged.deadpan");
            let mut store = ProjectStore::create(&package, &initial)?;
            store.commit(&request(&initial, "later-command", command)?)?;
            store.validate()?;
            drop(store);
            let database = Connection::open(package.join("project.sqlite"))?;
            // Only this negative fixture relabels modern content. The positive
            // fixture was authored and reopened by the preserved schema33 CLI.
            database.execute(
                "UPDATE revisions SET document=json_set(document,'$.schema_version',27)",
                [],
            )?;
            database.pragma_update(None, "user_version", 33)?;
            for (_, wire) in docs(&database)? {
                legacy_v27::Document::from_json(&wire)?;
            }
            assert!(legacy_v27::upgrade_request(&history_json(&database)?[0].0).is_err());
            let before = contents(&database)?;
            let StoreError::MigrationFailed { backup, .. } =
                ProjectStore::migrate(&package).unwrap_err()
            else {
                panic!("schema33 accepted new command: update={update}, occurrence={occurrence}");
            };
            assert_eq!(contents(&database)?, before);
            assert_eq!(contents(&Connection::open(backup)?)?, before);
            assert_eq!(
                database.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
                33
            );
        }
    }
    Ok(())
}

#[test]
fn schema27_snapshot_request_and_patch_fields_remain_closed_without_promotion() -> Result {
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
            panic!("schema33 accepted unknown {position} field");
        };
        assert_eq!(contents(&database)?, before);
        assert_eq!(contents(&Connection::open(backup)?)?, before);
        assert_eq!(
            database.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
            33
        );
    }
    Ok(())
}
