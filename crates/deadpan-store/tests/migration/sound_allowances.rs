use super::*;
use deadpan_core::*;
use serde_json::{Value, json};

fn fixture(root: &Path) -> Result<PathBuf> {
    let package = root.join("sound-allowance-history.deadpan");
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
    database.execute_batch(include_str!("../fixtures/v36-sound-allowance-history.sql"))?;
    Ok(package)
}

fn assert_authored_equal(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = json!(actual.revision_id());
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

#[test]
fn actual_schema36_sound_history_preserves_routes_branches_receipts_and_pending_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_documents = docs(&database)?;
    let old_history = history_json(&database)?;
    let old_metadata = metadata(&database)?;
    let old_operational = operational_metadata(&database)?;
    let old_qualifications = qualification_metadata(&database)?;
    assert_eq!((old_documents.len(), old_history.len()), (15, 10));
    assert!(matches!(
        ProjectStore::open(&package, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(36))
    ));
    let outcome = ProjectStore::migrate(&package)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (36, DATABASE_SCHEMA_VERSION)
    );
    assert_eq!(
        contents(&Connection::open(outcome.backup.unwrap())?)?,
        before
    );
    assert_eq!(metadata(&database)?, old_metadata);
    assert_eq!(operational_metadata(&database)?, old_operational);
    assert_eq!(qualification_metadata(&database)?, old_qualifications);
    let mut saw_routes = false;
    for ((old_id, old), (new_id, new)) in old_documents.iter().zip(docs(&database)?) {
        assert_eq!(old_id, &new_id);
        let modern = ProjectDocument::from_json(&new)?;
        let legacy = legacy_v30::Document::from_json(old)?;
        assert!(legacy.matches(&modern));
        assert_eq!(legacy.upgrade()?, modern);
        assert!(modern.sound_allowances().is_empty());
        saw_routes |= !modern.sound_routes().is_empty();
        let mut expected: Value = serde_json::from_str(old)?;
        expected["schema_version"] = json!(DOCUMENT_SCHEMA_VERSION);
        assert_eq!(serde_json::to_value(modern)?, expected);
    }
    assert!(saw_routes);
    for ((old_request, old_edit), (new_request, new_edit)) in
        old_history.iter().zip(history_json(&database)?)
    {
        let request: CommandRequest = serde_json::from_str(&new_request)?;
        let edit: EditTransaction = serde_json::from_str(&new_edit)?;
        assert_eq!(legacy_v30::upgrade_request(old_request)?, request);
        assert!(legacy_v30::matches_edit(old_edit, &edit)?);
        assert_eq!(old_request, &new_request);
        assert_eq!(old_edit, &new_edit);
        let prior = snapshot(&database, request.expected_revision.as_str())?;
        let after = snapshot(&database, request.new_revision.as_str())?;
        assert_eq!(edit.forward.apply(&prior)?, after);
        assert_eq!(edit.inverse.apply(&after)?, prior);
    }
    let impact = SoundId::new("impact")?;
    let bed = SoundId::new("bed")?;
    let abandoned = snapshot(&database, "core30-abandoned-replacement")?;
    assert_eq!(abandoned.sounds()[&impact].offset, AudioSample(941));
    assert!(!abandoned.sound_routes().contains_key(&impact));
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let baseline = store.snapshot()?;
    assert_eq!(baseline.sounds()[&impact].offset, AudioSample(137));
    assert_eq!(baseline.sound_routes().len(), 2);
    assert!(
        baseline
            .sound_routes()
            .values()
            .all(|route| route.edits.len() == 3)
    );
    store.redo(baseline.revision_id(), RevisionId::new("v36-pending-redo")?)?;
    let redone = store.snapshot()?;
    assert!(!redone.sound_routes().contains_key(&bed));
    assert_eq!(redone.sounds()[&bed].offset, AudioSample(71));
    store.undo(redone.revision_id(), RevisionId::new("v36-undo-redo")?)?;
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
        panic!("forged schema36 history was accepted");
    };
    for connection in [database, &Connection::open(backup)?] {
        assert_eq!(contents(connection)?, before);
        assert_eq!(operational_metadata(connection)?, operational);
        assert_eq!(qualification_metadata(connection)?, qualifications);
    }
    Ok(())
}

#[test]
fn schema30_rejects_allowance_fields_in_all_snapshot_and_patch_positions() -> Result {
    for position in ["initial", "later", "abandoned", "forward", "inverse"] {
        for value in ["{}", "null"] {
            let scratch = tempfile::tempdir()?;
            let package = fixture(scratch.path())?;
            let database = Connection::open(package.join("project.sqlite"))?;
            if matches!(position, "initial" | "later" | "abandoned") {
                let selector = match position {
                    "initial" => "parent_id IS NULL",
                    "abandoned" => "id='core30-abandoned-replacement'",
                    _ => "id='core30-delete-pause'",
                };
                database.execute(&format!("UPDATE revisions SET document=json_set(document,'$.sound_allowances',json(?1)) WHERE id=(SELECT id FROM revisions WHERE {selector} LIMIT 1)"), [value])?;
            } else {
                database.execute("UPDATE history SET edit=json_set(edit,?1,json(?2)) WHERE revision_id='core30-update-impact'", [format!("$.{position}.sound_allowances"), value.into()])?;
            }
            assert_rejected_without_promotion(&package, &database)?;
        }
    }
    Ok(())
}

fn allowance(document: &ProjectDocument) -> Result<CommandRequest> {
    Ok(CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new("modern-allowance")?,
        command: Command::SetSoundAllowance {
            sound: SoundId::new("impact")?,
            issuer: SoundHoldIssuer::Node {
                instance: InstancePath {
                    node: NodeId::new("core30-first-pause-hold")?,
                    repeats: Vec::new(),
                },
            },
            allowed: true,
        },
    })
}

#[test]
fn schema30_rejects_allowance_commands_in_abandoned_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let before: String = database.query_row(
        "SELECT document FROM revisions WHERE id='core30-update-impact'",
        [],
        |row| row.get(0),
    )?;
    let document = legacy_v30::Document::from_json(&before)?.upgrade()?;
    let command = allowance(&document)?.command;
    database.execute("UPDATE history SET request=json_set(request,'$.command',json(?1)) WHERE revision_id='core30-abandoned-replacement'", [serde_json::to_string(&command)?])?;
    let wire: String = database.query_row(
        "SELECT request FROM history WHERE revision_id='core30-abandoned-replacement'",
        [],
        |row| row.get(0),
    )?;
    let request: CommandRequest = serde_json::from_str(&wire)?;
    assert!(matches!(request.command, Command::SetSoundAllowance { .. }));
    assert!(legacy_v30::upgrade_request(&wire).is_err());
    assert!(legacy_v30::validate_request_context(&document, &request).is_err());
    assert_rejected_without_promotion(&package, &database)
}

#[test]
fn schema30_projections_never_discard_modern_allowances() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let old: String = database.query_row(
        "SELECT document FROM revisions WHERE id='core30-delete-pause'",
        [],
        |row| row.get(0),
    )?;
    let legacy = legacy_v30::Document::from_json(&old)?;
    let before = legacy.clone().upgrade()?;
    let request = allowance(&before)?;
    let transaction = deadpan_core::apply(&before, &request)?;
    let after = transaction.forward.apply(&before)?;
    assert!(!legacy.matches(&after));
    assert!(!after.sound_allowances().is_empty());
    // Relabeling the revision alone cannot conceal new allowance state from
    // the frozen document projection.
    let mut old_after: Value = serde_json::from_str(&old)?;
    old_after["revision_id"] = json!(after.revision_id());
    assert!(!legacy_v30::Document::from_json(&old_after.to_string())?.matches(&after));
    let mut forged = serde_json::to_value(&transaction)?;
    for direction in ["forward", "inverse"] {
        forged[direction]
            .as_object_mut()
            .unwrap()
            .remove("sound_allowances");
    }
    assert!(!legacy_v30::matches_edit(
        &forged.to_string(),
        &transaction
    )?);
    Ok(())
}

#[test]
fn schema30_frozen_guard_retains_root_split_and_temporal_occurrence_rejections() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let old: String = database.query_row(
        "SELECT document FROM revisions WHERE id='core30-delete-pause'",
        [],
        |row| row.get(0),
    )?;
    let document = legacy_v30::Document::from_json(&old)?.upgrade()?;
    let mut request = allowance(&document)?;
    request.command = Command::Split {
        node: document.root().clone(),
        at: FrameDuration::new(1)?,
        identities: SplitIdentities::default(),
    };
    assert!(legacy_v30::validate_request_context(&document, &request).is_err());
    request.command = Command::Split {
        node: NodeId::new("clip")?,
        at: FrameDuration::new(1)?,
        identities: SplitIdentities::default(),
    };
    legacy_v30::validate_request_context(&document, &request)?;
    for edit in [
        OccurrenceEdit::Split {
            at: FrameDuration::new(1)?,
            identities: SplitIdentities::default(),
        },
        OccurrenceEdit::Delete,
        OccurrenceEdit::SetHoldDuration {
            duration: FrameDuration::new(4)?,
        },
    ] {
        request.command = Command::EditOccurrence {
            instance: InstancePath {
                node: NodeId::new("core30-first-pause-hold")?,
                repeats: Vec::new(),
            },
            edit,
            identities: OccurrenceIdentities::default(),
        };
        assert!(legacy_v30::validate_request_context(&document, &request).is_err());
    }
    Ok(())
}
