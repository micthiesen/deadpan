use super::*;
use deadpan_core::*;
use serde_json::json;

fn fixture(root: &Path) -> Result<PathBuf> {
    let package = current_fixture(
        root,
        include_str!("../fixtures/current-sound_routes.json"),
        Some(include_str!("../fixtures/current-sound_routes-media.sql")),
    )?;
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
fn sound_ripple_history_is_atomic_durable_and_rechecks_unchanged_recipe_receipts() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
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
