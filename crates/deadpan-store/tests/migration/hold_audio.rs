use super::*;
use deadpan_core::*;
use serde_json::{Value, json};

fn fixture(root: &Path) -> Result<PathBuf> {
    let package = root.join("hold-audio-history.deadpan");
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
    database.execute_batch(include_str!("../fixtures/v37-hold-audio-history.sql"))?;
    Ok(package)
}

fn assert_authored_equal(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = json!(actual.revision_id());
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

#[test]
fn actual_schema37_history_preserves_allowances_routes_branches_receipts_and_pending_redo() -> Result
{
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_documents = docs(&database)?;
    let old_history = history_json(&database)?;
    let old_metadata = metadata(&database)?;
    let old_operational = operational_metadata(&database)?;
    let old_qualifications = qualification_metadata(&database)?;
    assert_eq!((old_documents.len(), old_history.len()), (20, 14));
    assert!(matches!(
        ProjectStore::open(&package, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(37))
    ));
    let outcome = ProjectStore::migrate(&package)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (37, DATABASE_SCHEMA_VERSION)
    );
    assert_eq!(
        contents(&Connection::open(outcome.backup.unwrap())?)?,
        before
    );
    assert_eq!(metadata(&database)?, old_metadata);
    assert_eq!(operational_metadata(&database)?, old_operational);
    assert_eq!(qualification_metadata(&database)?, old_qualifications);
    let mut saw_routes = false;
    let mut saw_allowances = false;
    for ((old_id, old), (new_id, new)) in old_documents.iter().zip(docs(&database)?) {
        assert_eq!(old_id, &new_id);
        let modern = ProjectDocument::from_json(&new)?;
        let legacy = legacy_v31::Document::from_json(old)?;
        assert!(legacy.matches(&modern));
        assert_eq!(legacy.upgrade()?, modern);
        saw_allowances |= !modern.sound_allowances().is_empty();
        saw_routes |= !modern.sound_routes().is_empty();
        let mut expected: Value = serde_json::from_str(old)?;
        expected["schema_version"] = json!(DOCUMENT_SCHEMA_VERSION);
        assert_eq!(serde_json::to_value(modern)?, expected);
    }
    assert!(saw_routes && saw_allowances);
    for ((old_request, old_edit), (new_request, new_edit)) in
        old_history.iter().zip(history_json(&database)?)
    {
        let request: CommandRequest = serde_json::from_str(&new_request)?;
        let edit: EditTransaction = serde_json::from_str(&new_edit)?;
        assert_eq!(legacy_v31::upgrade_request(old_request)?, request);
        assert!(legacy_v31::matches_edit(old_edit, &edit)?);
        assert_eq!(old_request, &new_request);
        assert_eq!(old_edit, &new_edit);
        let prior = snapshot(&database, request.expected_revision.as_str())?;
        let after = snapshot(&database, request.new_revision.as_str())?;
        assert_eq!(edit.forward.apply(&prior)?, after);
        assert_eq!(edit.inverse.apply(&after)?, prior);
    }
    let impact = SoundId::new("impact")?;
    let bed = SoundId::new("bed")?;
    let abandoned = snapshot(&database, "core31-abandoned-replacement")?;
    assert_eq!(abandoned.sounds()[&impact].offset, AudioSample(941));
    assert!(!abandoned.sound_routes().contains_key(&impact));
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let baseline = store.snapshot()?;
    assert_eq!(baseline.sound_allowances().len(), 2);
    let abandoned_revoke = snapshot(&database, "core31-abandoned-revoke")?;
    assert!(!abandoned_revoke.sound_allowances().contains_key(&impact));
    assert!(abandoned_revoke.sound_allowances().contains_key(&bed));
    assert_eq!(baseline.sounds()[&impact].offset, AudioSample(137));
    assert_eq!(baseline.sound_routes().len(), 2);
    assert!(
        baseline
            .sound_routes()
            .values()
            .all(|route| route.edits.len() == 3)
    );
    store.redo(baseline.revision_id(), RevisionId::new("v37-pending-redo")?)?;
    let redone = store.snapshot()?;
    assert!(!redone.sound_routes().contains_key(&bed));
    assert_eq!(redone.sounds()[&bed].offset, AudioSample(71));
    store.undo(redone.revision_id(), RevisionId::new("v37-undo-redo")?)?;
    assert_authored_equal(&store.snapshot()?, &baseline)?;
    store.validate()?;
    drop(store);
    ProjectStore::open(&package, AccessMode::ReadOnly)?.validate()?;
    assert!(ProjectStore::migrate(&package)?.backup.is_none());
    Ok(())
}

fn assert_rejected_without_promotion(package: &Path, database: &Connection) -> Result {
    let before = contents(database)?;
    let operational = operational_metadata(database)?;
    let qualifications = qualification_metadata(database)?;
    let StoreError::MigrationFailed { backup, .. } = ProjectStore::migrate(package).unwrap_err()
    else {
        panic!("forged schema37 history was accepted");
    };
    for connection in [database, &Connection::open(backup)?] {
        assert_eq!(contents(connection)?, before);
        assert_eq!(operational_metadata(connection)?, operational);
        assert_eq!(qualification_metadata(connection)?, qualifications);
    }
    Ok(())
}

#[test]
fn schema31_rejects_direct_and_occurrence_setters_even_with_matching_modern_history() -> Result {
    for occurrence in [false, true] {
        let scratch = tempfile::tempdir()?;
        let package = fixture(scratch.path())?;
        let database = Connection::open(package.join("project.sqlite"))?;
        let old: String = database.query_row(
            "SELECT document FROM revisions WHERE id='core31-update-impact'",
            [],
            |row| row.get(0),
        )?;
        let before = legacy_v31::Document::from_json(&old)?.upgrade()?;
        let node = NodeId::new("core31-first-pause-hold")?;
        let asset = AssetId::new("camera")?;
        let audio = HoldAudio::RoomTone {
            source: SourceAudio {
                span: before.assets()[&asset].audio.unwrap(),
                asset,
            },
        };
        let command = if occurrence {
            Command::EditOccurrence {
                instance: InstancePath {
                    node,
                    repeats: Vec::new(),
                },
                edit: OccurrenceEdit::SetHoldAudio { audio },
                identities: OccurrenceIdentities::default(),
            }
        } else {
            Command::SetHoldAudio { node, audio }
        };
        let request = CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: RevisionId::new("core31-abandoned-replacement")?,
            command,
        };
        let edit = deadpan_core::apply(&before, &request)?;
        let after = edit.forward.apply(&before)?;
        assert!(after.sound_allowances().is_empty());
        let mut old_after = serde_json::to_value(&after)?;
        old_after["schema_version"] = json!(31);
        assert!(legacy_v31::Document::from_json(&old_after.to_string())?.matches(&after));
        assert!(legacy_v31::matches_edit(
            &serde_json::to_string(&edit)?,
            &edit
        )?);
        let wire = serde_json::to_string(&request)?;
        assert!(legacy_v31::upgrade_request(&wire).is_err());
        assert!(legacy_v31::validate_request_context(&before, &request).is_err());
        database.execute(
            "UPDATE revisions SET document=?1 WHERE id='core31-abandoned-replacement'",
            [old_after.to_string()],
        )?;
        database.execute("UPDATE history SET request=?1,edit=?2 WHERE revision_id='core31-abandoned-replacement'", [wire, serde_json::to_string(&edit)?])?;
        assert_rejected_without_promotion(&package, &database)?;
    }
    Ok(())
}

#[test]
fn schema31_compares_allowances_in_snapshots_and_both_patch_directions() -> Result {
    for position in ["snapshot", "forward", "inverse"] {
        let scratch = tempfile::tempdir()?;
        let package = fixture(scratch.path())?;
        let database = Connection::open(package.join("project.sqlite"))?;
        if position == "snapshot" {
            database.execute("UPDATE revisions SET document=json_remove(document,'$.sound_allowances') WHERE id='core31-grant-impact'", [])?;
        } else {
            database.execute("UPDATE history SET edit=json_remove(edit,?1) WHERE revision_id='core31-grant-impact'", [format!("$.{position}.sound_allowances")])?;
        }
        assert_rejected_without_promotion(&package, &database)?;
    }
    Ok(())
}

#[test]
fn schema31_rejects_hold_policy_patches_forged_under_an_old_command() -> Result {
    for direction in ["forward", "inverse"] {
        let scratch = tempfile::tempdir()?;
        let package = fixture(scratch.path())?;
        let database = Connection::open(package.join("project.sqlite"))?;
        let path =
            format!("$.{direction}.nodes.\"core31-first-pause-hold\".after.kind.recipe.audio");
        database.execute("UPDATE history SET edit=json_set(edit,?1,json(?2)) WHERE revision_id='core31-rename-pause'", [path, json!({"type":"tail", "source":{"asset":"camera","span":{"start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":1,"time_base":{"numerator":1,"denominator":48000}}}},"maximum":1}).to_string()])?;
        assert_rejected_without_promotion(&package, &database)?;
    }
    Ok(())
}
