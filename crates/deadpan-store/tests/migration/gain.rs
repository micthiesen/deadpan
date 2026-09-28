use super::*;
use deadpan_core::*;
use serde_json::{Value, json};

fn fixture(root: &Path) -> Result<PathBuf> {
    let package = root.join("gain-history.deadpan");
    fs::create_dir(&package)?;
    fs::create_dir(package.join("Snapshots"))?;
    let originals = package.join("Media/Originals");
    fs::create_dir_all(&originals)?;
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../native/deadpan-source/tests/fixtures/offset-bframes.mp4"),
        originals.join("blake3-2a71661b9ab6925c88996ee355b96cd96c3041ee775c3ccbe2681513397a613f"),
    )?;
    let database = Connection::open(package.join("project.sqlite"))?;
    database.pragma_update(None, "foreign_keys", false)?;
    database.execute_batch(include_str!("../fixtures/v38-gain-history.sql"))?;
    Ok(package)
}

fn gain() -> AudioTreatments {
    AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::new(-6000).unwrap(), false, vec![], vec![]).unwrap(),
    )
}

#[test]
fn actual_schema38_history_preserves_hold_setters_isolation_receipts_and_pending_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_documents = docs(&database)?;
    let old_history = history_json(&database)?;
    let old_metadata = metadata(&database)?;
    let old_operational = operational_metadata(&database)?;
    let old_qualifications = qualification_metadata(&database)?;
    assert_eq!((old_documents.len(), old_history.len()), (30, 21));
    assert!(matches!(
        ProjectStore::open(&package, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(38))
    ));
    let outcome = ProjectStore::migrate(&package)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (38, DATABASE_SCHEMA_VERSION)
    );
    assert_eq!(
        contents(&Connection::open(outcome.backup.unwrap())?)?,
        before
    );
    assert_eq!(metadata(&database)?, old_metadata);
    assert_eq!(operational_metadata(&database)?, old_operational);
    assert_eq!(qualification_metadata(&database)?, old_qualifications);
    for ((old_id, old), (new_id, new)) in old_documents.iter().zip(docs(&database)?) {
        assert_eq!(old_id, &new_id);
        let modern = ProjectDocument::from_json(&new)?;
        let legacy = legacy_v32::Document::from_json(old)?;
        assert!(legacy.matches(&modern));
        assert_eq!(legacy.upgrade()?, modern);
        assert!(
            modern
                .nodes()
                .values()
                .all(|node| node.audio_treatments.is_empty())
        );
        let mut expected: Value = serde_json::from_str(old)?;
        expected["schema_version"] = json!(DOCUMENT_SCHEMA_VERSION);
        assert_eq!(serde_json::to_value(modern)?, expected);
    }
    let mut direct = 0;
    let mut occurrence = 0;
    for ((old_request, old_edit), (new_request, new_edit)) in
        old_history.iter().zip(history_json(&database)?)
    {
        let request: CommandRequest = serde_json::from_str(&new_request)?;
        let edit: EditTransaction = serde_json::from_str(&new_edit)?;
        direct += usize::from(matches!(request.command, Command::SetHoldAudio { .. }));
        occurrence += usize::from(matches!(
            request.command,
            Command::EditOccurrence {
                edit: OccurrenceEdit::SetHoldAudio { .. },
                ..
            }
        ));
        assert_eq!(legacy_v32::upgrade_request(old_request)?, request);
        assert!(legacy_v32::matches_edit(old_edit, &edit)?);
        assert_eq!(old_request, &new_request);
        assert_eq!(old_edit, &new_edit);
        let prior = snapshot(&database, request.expected_revision.as_str())?;
        let after = snapshot(&database, request.new_revision.as_str())?;
        assert_eq!(edit.forward.apply(&prior)?, after);
        assert_eq!(edit.inverse.apply(&after)?, prior);
    }
    assert_eq!((direct, occurrence), (3, 1));
    let isolated = snapshot(&database, "core32-isolated-silence")?;
    assert!(
        matches!(isolated.nodes()[&NodeId::new("gain-history-isolated-hold")?].kind, NodeKind::Hold { ref recipe } if recipe.audio == HoldAudio::Silence)
    );
    assert!(!isolated.overrides().is_empty());
    let tail = snapshot(&database, "core32-abandoned-tail")?;
    let hold = NodeId::new("core31-first-pause-hold")?;
    assert!(
        matches!(tail.nodes()[&hold].kind, NodeKind::Hold { ref recipe } if matches!(recipe.audio, HoldAudio::Tail { .. }))
    );
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    assert!(store.single_source_state()?.is_none());
    let baseline = store.snapshot()?;
    store.redo(
        baseline.revision_id(),
        RevisionId::new("gain-migrated-redo")?,
    )?;
    let redone = store.snapshot()?;
    assert!(
        matches!(redone.nodes()[&hold].kind, NodeKind::Hold { ref recipe } if recipe.audio == HoldAudio::Silence)
    );
    store.undo(redone.revision_id(), RevisionId::new("gain-migrated-undo")?)?;
    assert_eq!(store.snapshot()?.nodes(), baseline.nodes());
    store.validate()?;
    drop(store);
    ProjectStore::open(&package, AccessMode::ReadOnly)?.validate()?;
    assert!(ProjectStore::migrate(&package)?.backup.is_none());
    Ok(())
}

fn rejected_without_promotion(package: &Path, database: &Connection) -> Result {
    let before = contents(database)?;
    let operational = operational_metadata(database)?;
    let qualifications = qualification_metadata(database)?;
    let StoreError::MigrationFailed { backup, .. } = ProjectStore::migrate(package).unwrap_err()
    else {
        panic!("forged schema38 history was accepted");
    };
    for connection in [database, &Connection::open(backup)?] {
        assert_eq!(contents(connection)?, before);
        assert_eq!(operational_metadata(connection)?, operational);
        assert_eq!(qualification_metadata(connection)?, qualifications);
    }
    Ok(())
}

#[test]
fn legacy_history_rejects_gain_presence_in_snapshots_and_both_patch_sides() -> Result {
    for position in [
        "snapshot",
        "forward-before",
        "forward-after",
        "inverse-before",
        "inverse-after",
    ] {
        for value in [
            Value::Null,
            json!(AudioTreatments::default()),
            json!(gain()),
        ] {
            let scratch = tempfile::tempdir()?;
            let package = fixture(scratch.path())?;
            let database = Connection::open(package.join("project.sqlite"))?;
            if position == "snapshot" {
                database.execute("UPDATE revisions SET document=json_set(document,?1,json(?2)) WHERE id='core32-hold-room-tone'",
                    ["$.nodes.\"core31-first-pause-hold\".audio_treatments".to_owned(), value.to_string()])?;
            } else {
                let (direction, side) = position.split_once('-').unwrap();
                let path = format!(
                    "$.{direction}.nodes.\"core31-first-pause-hold\".{side}.audio_treatments"
                );
                database.execute("UPDATE history SET edit=json_set(edit,?1,json(?2)) WHERE revision_id='core32-hold-room-tone'", [path, value.to_string()])?;
            }
            rejected_without_promotion(&package, &database)?;
        }
    }
    Ok(())
}

#[test]
fn legacy_history_rejects_direct_and_occurrence_gain_commands() -> Result {
    for occurrence in [false, true] {
        let scratch = tempfile::tempdir()?;
        let package = fixture(scratch.path())?;
        let database = Connection::open(package.join("project.sqlite"))?;
        let encoded: String = database.query_row(
            "SELECT request FROM history WHERE revision_id='core32-hold-room-tone'",
            [],
            |row| row.get(0),
        )?;
        let mut request: Value = serde_json::from_str(&encoded)?;
        request["command"] = if occurrence {
            json!({"command":"edit_occurrence", "instance":{"node":"core31-first-pause-hold","repeats":[]},
                "edit":{"type":"set_audio_treatments","treatments":gain()},"identities":{"nodes":[],"marks":[]}})
        } else {
            json!({"command":"set_audio_treatments","node":"core31-first-pause-hold","treatments":gain()})
        };
        database.execute(
            "UPDATE history SET request=?1 WHERE revision_id='core32-hold-room-tone'",
            [request.to_string()],
        )?;
        rejected_without_promotion(&package, &database)?;
    }
    Ok(())
}

#[test]
fn authored_gain_is_durable_reversible_and_revision_guarded_after_migration() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    ProjectStore::migrate(&package)?;
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let before = store.snapshot()?;
    let request = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: RevisionId::new("authored-gain")?,
        command: Command::SetAudioTreatments {
            node: before.root().clone(),
            treatments: gain(),
        },
    };
    let preview = store.preview(&request)?;
    assert_eq!(store.snapshot()?, before);
    store.commit(&request)?;
    let after = store.snapshot()?;
    assert_eq!(after, preview.forward.apply(&before)?);
    assert_eq!(after.duration()?, before.duration()?);
    assert_eq!(after.audio_bindings(), before.audio_bindings());
    assert_eq!(after.audio_lineage(), before.audio_lineage());
    assert!(store.commit(&request).is_err());
    store.undo(after.revision_id(), RevisionId::new("authored-gain-undo")?)?;
    let undone = store.snapshot()?;
    assert_eq!(undone.nodes(), before.nodes());
    assert_ne!(undone.revision_id(), before.revision_id());
    store.redo(undone.revision_id(), RevisionId::new("authored-gain-redo")?)?;
    assert_eq!(store.snapshot()?.nodes(), after.nodes());
    store.validate()?;
    drop(store);
    let reopened = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_eq!(reopened.snapshot()?.nodes(), after.nodes());
    reopened.validate()?;
    Ok(())
}
