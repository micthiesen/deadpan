use super::*;
use deadpan_core::*;
use std::collections::BTreeMap;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn bound_initial() -> Result<ProjectDocument> {
    let empty = ProjectDocument::new(
        ProjectId::new("pause-project")?,
        RevisionId::new("initial")?,
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )?;
    let mut wire = serde_json::to_value(empty)?;
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (node("root"), BeatNode::sequence("Root", vec![node("hold")])),
        (
            node("hold"),
            BeatNode::hold(
                "Original hold",
                HoldRecipe {
                    duration: FrameDuration::new(6)?,
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                },
            ),
        ),
    ]))?;
    let unbound = ProjectDocument::from_json(&wire.to_string())?;
    let captured = capture_unbound_audio_bindings(
        &unbound,
        AudioTimingId {
            allocation: RevisionId::new("old-timing")?,
            ordinal: 0,
        },
    )?;
    wire["audio_bindings"] = serde_json::to_value(&captured)?;
    wire["audio_bindings"]["bindings"]["hold"]["resume"] = serde_json::to_value(AudioResume {
        local_boundary: ExactRatio::ONE,
        phase: AudioLocalPhase {
            constant: ExactRatio::new(1, 7)?,
            terms: vec![AudioPhaseTerm {
                placement: captured.bindings()[&node("hold")].lattice.clone(),
                from_local: ExactRatio::ZERO,
                to_local: ExactRatio::ONE,
            }],
        },
    })?;
    Ok(ProjectDocument::from_json(&wire.to_string())?)
}

fn command(document: &ProjectDocument, revision: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(revision).unwrap(),
        command,
    }
}

fn pause(document: &ProjectDocument, revision: &str, at: i64, frames: i64) -> CommandRequest {
    command(
        document,
        revision,
        Command::InsertTime {
            at: ProjectFrame(at),
            hold: HoldRecipe {
                duration: FrameDuration::new(frames).unwrap(),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            },
            id: node(&format!("{revision}-hold")),
            identities: SplitIdentities {
                nodes: (0..document.nodes().len() + 4)
                    .map(|i| node(&format!("{revision}-split-{i}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: RevisionId::new(revision).unwrap(),
                ordinal: 0,
            },
        },
    )
}

// Only commands and JSON vocabulary shipped in core 16 are used. Downgrading
// the version tags creates a closed DB22 fixture with real binding patches,
// undo/redo chronology and redo state, rather than inventing those rows.
fn schema22_fixture(path: &Path) -> Result<ProjectDocument> {
    let initial = bound_initial()?;
    let mut store = ProjectStore::create(path, &initial)?;
    store.commit(&command(
        &initial,
        "split",
        Command::Split {
            node: node("hold"),
            at: FrameDuration::new(2)?,
            identities: SplitIdentities {
                nodes: vec![node("left"), node("right"), node("copy")],
            },
        },
    ))?;
    store.commit(&command(
        &store.snapshot()?,
        "rename",
        Command::Rename {
            node: node("copy"),
            label: "Renamed right hold".into(),
        },
    ))?;
    store.undo(&RevisionId::new("rename")?, RevisionId::new("undo-name")?)?;
    store.redo(
        &RevisionId::new("undo-name")?,
        RevisionId::new("redo-name")?,
    )?;
    store.undo(
        &RevisionId::new("redo-name")?,
        RevisionId::new("pending-redo")?,
    )?;
    let current = store.snapshot()?;
    store.validate()?;
    drop(store);
    let database = Connection::open(path.join("project.sqlite"))?;
    database.execute(
        "UPDATE revisions SET document=json_set(document,'$.schema_version',16)",
        [],
    )?;
    database.pragma_update(None, "user_version", 22)?;
    for (_, json) in docs(&database)? {
        legacy_v16::Document::from_json(&json)?;
    }
    Ok(current)
}

#[test]
fn schema22_preserves_bindings_patches_and_redo_in_the_backed_up_migration() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("bound-v22.deadpan");
    let expected = schema22_fixture(&path)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_documents = docs(&database)?;
    let old_history = history_json(&database)?;
    let old_metadata = metadata(&database)?;
    let old_operational = operational_metadata(&database)?;
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(22))
    ));
    let migration = ProjectStore::migrate(&path)?;
    assert_eq!(
        (migration.from_schema, migration.to_schema),
        (22, DATABASE_SCHEMA_VERSION)
    );
    assert_eq!(
        contents(&Connection::open(migration.backup.unwrap())?)?,
        before
    );
    assert_eq!(metadata(&database)?, old_metadata);
    assert_eq!(operational_metadata(&database)?, old_operational);
    for ((old_id, old), (new_id, new)) in old_documents.iter().zip(docs(&database)?) {
        assert_eq!(old_id, &new_id);
        let current = ProjectDocument::from_json(&new)?;
        let retained = legacy_v16::Document::from_json(old)?;
        assert!(retained.matches(&current));
        assert_eq!(retained.upgrade()?, current);
        let mut old: serde_json::Value = serde_json::from_str(old)?;
        old["schema_version"] = serde_json::json!(DOCUMENT_SCHEMA_VERSION);
        assert_eq!(serde_json::to_value(&current)?, old);
        assert!(!current.audio_bindings().is_empty());
    }
    for ((old_request, old_edit), (new_request, new_edit)) in
        old_history.iter().zip(history_json(&database)?)
    {
        let request: CommandRequest = serde_json::from_str(&new_request)?;
        let edit: EditTransaction = serde_json::from_str(&new_edit)?;
        assert_eq!(legacy_v16::upgrade_request(old_request)?, request);
        assert!(legacy_v16::matches_edit(old_edit, &edit)?);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(old_edit)?,
            serde_json::to_value(edit)?
        );
    }
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, expected);
    assert!(store.single_source_state()?.is_none());
    store.redo(expected.revision_id(), RevisionId::new("migrated-redo")?)?;
    assert_eq!(
        store.snapshot()?.nodes()[&node("copy")].label,
        "Renamed right hold"
    );
    store.validate()?;
    drop(store);
    ProjectStore::open(&path, AccessMode::ReadOnly)?.validate()?;
    assert!(ProjectStore::migrate(&path)?.backup.is_none());
    Ok(())
}

#[test]
fn schema22_rejects_new_commands_and_nested_binding_vocabulary_without_promotion() -> Result {
    for tamper in ["command", "initial", "forward", "inverse", "mismatch"] {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join("tampered-v22.deadpan");
        let current = schema22_fixture(&path)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        if tamper == "command" {
            let request = serde_json::to_string(&pause(&current, "future-command", 0, 1))?;
            assert!(legacy_v16::upgrade_request(&request).is_err());
            database.execute(
                "UPDATE history SET request=?1 WHERE revision_id='split'",
                [request],
            )?;
        } else if tamper == "initial" {
            database.execute("UPDATE revisions SET document=json_set(document,'$.audio_bindings.future',json('null')) WHERE parent_id IS NULL", [])?;
        } else if tamper == "mismatch" {
            database.execute("UPDATE history SET edit=json_set(edit,'$.changed_ids',json('[]')) WHERE revision_id='split'", [])?;
        } else {
            let pointer = format!("$.{tamper}.audio_bindings.before.future");
            database.execute(
                "UPDATE history SET edit=json_set(edit,?1,json('{}')) WHERE revision_id='split'",
                [pointer],
            )?;
        }
        let before = contents(&database)?;
        let StoreError::MigrationFailed { backup, .. } = ProjectStore::migrate(&path).unwrap_err()
        else {
            panic!("{tamper} must retain a failed migration backup");
        };
        assert_eq!(contents(&database)?, before);
        assert_eq!(contents(&Connection::open(backup)?)?, before);
        assert_eq!(
            database.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
            22
        );
    }
    Ok(())
}

#[test]
fn all_older_database_grammars_reject_insert_time_before_replay() -> Result {
    let request = serde_json::to_string(&pause(&bound_initial()?, "future-command", 0, 1))?;
    for version in 1..=21 {
        let scratch = tempfile::tempdir()?;
        let path = fixture_version(scratch.path(), version)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        database.execute(
            "UPDATE history SET request=?1 WHERE id=(SELECT MIN(id) FROM history)",
            [&request],
        )?;
        let before = contents(&database)?;
        let StoreError::MigrationFailed { backup, .. } = ProjectStore::migrate(&path).unwrap_err()
        else {
            panic!("schema {version} must not replay InsertTime");
        };
        assert_eq!(contents(&database)?, before);
        assert_eq!(contents(&Connection::open(backup)?)?, before);
    }
    Ok(())
}

#[test]
fn atomic_insert_time_history_survives_reopen_undo_redo_and_rejected_requests() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("insert-time.deadpan");
    let initial = bound_initial()?;
    let mut store = ProjectStore::create(&path, &initial)?;
    let request = pause(&initial, "first-pause", 2, 1);
    let preview = store.preview(&request)?;
    assert_eq!(preview.duration_delta, 1);
    assert_eq!(store.snapshot()?, initial);
    store.commit(&request)?;
    let first = store.snapshot()?;
    assert_eq!(first.duration()?.frames(), 7);
    assert_eq!(first, preview.forward.apply(&initial)?);
    assert!(first.audio_bindings().timings().len() > initial.audio_bindings().timings().len());
    store.commit(&pause(&first, "second-pause", 4, 2))?;
    let second = store.snapshot()?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let stable = contents(&database)?;
    for rejected in [request, pause(&second, "zero-pause", 0, 0)] {
        assert!(store.preview(&rejected).is_err());
        assert!(store.commit(&rejected).is_err());
        assert_eq!(store.snapshot()?, second);
        assert_eq!(contents(&database)?, stable);
    }
    assert_eq!(history_json(&database)?.len(), 2);
    assert_eq!(docs(&database)?.len(), 3);
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, second);
    store.undo(second.revision_id(), RevisionId::new("undo-second")?)?;
    let undone = store.snapshot()?;
    assert_eq!(undone.nodes(), first.nodes());
    assert_eq!(undone.audio_bindings(), first.audio_bindings());
    assert_ne!(undone.revision_id(), first.revision_id());
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.redo(undone.revision_id(), RevisionId::new("redo-second")?)?;
    let redone = store.snapshot()?;
    assert_eq!(redone.nodes(), second.nodes());
    assert_eq!(redone.audio_bindings(), second.audio_bindings());
    assert_ne!(redone.revision_id(), second.revision_id());
    store.validate()?;
    drop(store);
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)?.snapshot()?,
        redone
    );
    Ok(())
}
