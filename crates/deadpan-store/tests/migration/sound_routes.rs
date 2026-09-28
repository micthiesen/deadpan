use super::*;
use deadpan_core::*;
use serde_json::{Value, json};

fn fixture(root: &Path) -> Result<PathBuf> {
    let package = root.join("sound-route-history.deadpan");
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
    database.execute_batch(include_str!("../fixtures/v35-sound-route-history.sql"))?;
    Ok(package)
}

fn request(document: &ProjectDocument, name: &str, command: Command) -> Result<CommandRequest> {
    Ok(CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(name)?,
        command,
    })
}

fn pause(document: &ProjectDocument, name: &str, at: i64) -> Result<CommandRequest> {
    request(
        document,
        name,
        Command::InsertTime {
            at: ProjectFrame(at),
            hold: HoldRecipe {
                picture_context: None,
                duration: FrameDuration::new(1)?,
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            },
            id: NodeId::new(format!("{name}-hold"))?,
            identities: SplitIdentities::default(),
            timing: AudioTimingId {
                allocation: RevisionId::new(name)?,
                ordinal: 0,
            },
        },
    )
}

fn assert_authored_equal(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = json!(actual.revision_id());
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

#[test]
fn actual_schema35_sound_history_preserves_recipes_receipts_branches_and_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let database = Connection::open(package.join("project.sqlite"))?;
    let before = contents(&database)?;
    let old_documents = docs(&database)?;
    let old_history = history_json(&database)?;
    let old_metadata = metadata(&database)?;
    let old_operational = operational_metadata(&database)?;
    let old_qualifications = qualification_metadata(&database)?;
    assert_eq!((old_documents.len(), old_history.len()), (13, 8));
    assert!(matches!(
        ProjectStore::open(&package, AccessMode::ReadOnly),
        Err(StoreError::MigrationRequired(35))
    ));
    let outcome = ProjectStore::migrate(&package)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (35, DATABASE_SCHEMA_VERSION)
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
        let legacy = legacy_v29::Document::from_json(old)?;
        assert!(legacy.matches(&modern));
        assert_eq!(legacy.upgrade()?, modern);
        assert!(modern.sound_routes().is_empty());
        let mut expected: Value = serde_json::from_str(old)?;
        expected["schema_version"] = json!(DOCUMENT_SCHEMA_VERSION);
        assert_eq!(serde_json::to_value(modern)?, expected);
    }
    for ((old_request, old_edit), (new_request, new_edit)) in
        old_history.iter().zip(history_json(&database)?)
    {
        let request: CommandRequest = serde_json::from_str(&new_request)?;
        let edit: EditTransaction = serde_json::from_str(&new_edit)?;
        assert_eq!(legacy_v29::upgrade_request(old_request)?, request);
        assert!(legacy_v29::matches_edit(old_edit, &edit)?);
        assert_eq!(old_request, &new_request);
        assert_eq!(old_edit, &new_edit);
        let prior = snapshot(&database, request.expected_revision.as_str())?;
        let after = snapshot(&database, request.new_revision.as_str())?;
        assert_eq!(edit.forward.apply(&prior)?, after);
        assert_eq!(edit.inverse.apply(&after)?, prior);
    }
    let sound = SoundId::new("impact")?;
    assert_eq!(
        snapshot(&database, "core29-abandoned-impact")?.sounds()[&sound].offset,
        AudioSample(941)
    );
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let baseline = store.snapshot()?;
    assert_eq!(baseline.sounds()[&sound].offset, AudioSample(277));
    store.redo(baseline.revision_id(), RevisionId::new("v35-pending-redo")?)?;
    assert!(!store.snapshot()?.sounds().contains_key(&sound));
    store.undo(
        store.snapshot()?.revision_id(),
        RevisionId::new("v35-undo-redo")?,
    )?;
    assert_authored_equal(&store.snapshot()?, &baseline)?;
    store.validate()?;
    drop(store);
    ProjectStore::open(&package, AccessMode::ReadOnly)?.validate()?;
    assert!(ProjectStore::migrate(&package)?.backup.is_none());
    Ok(())
}

#[test]
fn sound_ripple_history_is_atomic_durable_and_rechecks_unchanged_recipe_receipts() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    ProjectStore::migrate(&package)?;
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let baseline = store.snapshot()?;
    let insertion = pause(&baseline, "first-pause", 0)?;
    let preview = store.preview(&insertion)?;
    assert_eq!(store.snapshot()?, baseline);
    let database = Connection::open(package.join("project.sqlite"))?;
    let before = contents(&database)?;
    let receipt: Vec<u8> =
        database.query_row("SELECT snapshot FROM source_qualifications", [], |row| {
            row.get(0)
        })?;
    database.execute("UPDATE source_qualifications SET snapshot=X'00'", [])?;
    assert!(matches!(
        store.preview(&insertion),
        Err(StoreError::SourceRegistration(_))
    ));
    assert!(matches!(
        store.commit(&insertion),
        Err(StoreError::SourceRegistration(_))
    ));
    assert_eq!(contents(&database)?, before);
    database.execute("UPDATE source_qualifications SET snapshot=?1", [receipt])?;
    database.execute_batch("CREATE TRIGGER fail_route_history BEFORE INSERT ON history BEGIN SELECT RAISE(FAIL,'forced sound route history failure'); END;")?;
    assert!(store.commit(&insertion).is_err());
    assert_eq!(contents(&database)?, before);
    database.execute_batch("DROP TRIGGER fail_route_history")?;
    assert_eq!(store.commit(&insertion)?.edit, preview);
    let first = store.snapshot()?;
    assert_eq!(first.sounds(), baseline.sounds());
    assert_eq!(first.sound_routes().len(), 2);
    for route in first.sound_routes().values() {
        assert_eq!(route.recipe_extent, baseline.duration()?);
        assert_eq!(route.edits.len(), 1);
    }
    assert_eq!(preview.inverse.apply(&first)?, baseline);
    drop(store);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, first);
    store.commit(&pause(&first, "second-pause", 1)?)?;
    let second = store.snapshot()?;
    assert!(
        second
            .sound_routes()
            .values()
            .all(|route| route.edits.len() == 2)
    );
    store.undo(second.revision_id(), RevisionId::new("undo-second")?)?;
    assert_authored_equal(&store.snapshot()?, &first)?;
    drop(store);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    store.redo(
        store.snapshot()?.revision_id(),
        RevisionId::new("redo-second")?,
    )?;
    assert_authored_equal(&store.snapshot()?, &second)?;
    store.validate()?;
    drop(store);
    ProjectStore::open(&package, AccessMode::ReadOnly)?.validate()?;
    Ok(())
}

fn assert_rejected_without_promotion(package: &Path, database: &Connection) -> Result {
    let before = contents(database)?;
    let operational = operational_metadata(database)?;
    let qualifications = qualification_metadata(database)?;
    let StoreError::MigrationFailed { backup, .. } = ProjectStore::migrate(package).unwrap_err()
    else {
        panic!("forged schema35 history was accepted");
    };
    for connection in [database, &Connection::open(backup)?] {
        assert_eq!(contents(connection)?, before);
        assert_eq!(operational_metadata(connection)?, operational);
        assert_eq!(qualification_metadata(connection)?, qualifications);
    }
    Ok(())
}

#[test]
fn schema29_rejects_route_fields_even_empty_or_null() -> Result {
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
                database.execute(&format!("UPDATE revisions SET document=json_set(document,'$.sound_routes',json(?1)) WHERE id=(SELECT id FROM revisions WHERE {selector} LIMIT 1)"), [value])?;
            } else {
                database.execute("UPDATE history SET edit=json_set(edit,?1,json(?2)) WHERE id=(SELECT MIN(id) FROM history)", [format!("$.{position}.sound_routes"), value.into()])?;
            }
            assert_rejected_without_promotion(&package, &database)?;
        }
    }
    Ok(())
}

#[test]
fn schema29_rejects_replace_sound_even_with_matching_patches() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let database = Connection::open(package.join("project.sqlite"))?;
    database.execute("UPDATE history SET request=json_set(request,'$.command.command','replace_sound') WHERE revision_id='core29-update-impact'", [])?;
    let wire: String = database.query_row(
        "SELECT request FROM history WHERE revision_id='core29-update-impact'",
        [],
        |row| row.get(0),
    )?;
    assert!(matches!(
        serde_json::from_str::<CommandRequest>(&wire)?.command,
        Command::ReplaceSound { .. }
    ));
    assert!(legacy_v29::upgrade_request(&wire).is_err());
    assert_rejected_without_promotion(&package, &database)
}

#[test]
fn schema29_rejects_new_sound_bearing_split_even_without_new_route_fields() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    ProjectStore::migrate(&package)?;
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let before = store.snapshot()?;
    let split = request(
        &before,
        "modern-split",
        Command::Split {
            node: NodeId::new("repeat")?,
            at: FrameDuration::new(1)?,
            identities: SplitIdentities {
                nodes: (0..16)
                    .map(|n| NodeId::new(format!("split-{n}")))
                    .collect::<std::result::Result<_, _>>()?,
            },
        },
    )?;
    store.commit(&split)?;
    assert!(store.snapshot()?.sound_routes().is_empty());
    store.validate()?;
    drop(store);
    let database = Connection::open(package.join("project.sqlite"))?;
    // Negative fixture only: every modern patch matches and no new field is
    // present, but this command was forbidden when schema29 had any sounds.
    database.execute(
        "UPDATE revisions SET document=json_set(document,'$.schema_version',29)",
        [],
    )?;
    database.pragma_update(None, "user_version", 35)?;
    for (_, wire) in docs(&database)? {
        legacy_v29::Document::from_json(&wire)?;
    }
    legacy_v29::upgrade_request(&serde_json::to_string(&split)?)?;
    assert!(legacy_v29::validate_request_context(&before, &split).is_err());
    assert_rejected_without_promotion(&package, &database)
}
