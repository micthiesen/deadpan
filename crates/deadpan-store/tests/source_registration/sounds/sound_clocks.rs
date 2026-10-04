use super::*;

#[test]
fn sound_clock_transports_are_atomic_durable_and_reversible_with_fresh_revisions() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "offset-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    store.register_source(
        &request(&store, &original, "import", "camera", Some("clip"))?,
        &decoded,
        None,
        limits(),
        &active(),
    )?;
    let before = store.snapshot()?;
    let owner = NodeId::new("clip")?;
    let local = SoundId::new("overlay")?;
    store.commit(&edit(
        &before,
        "sound",
        Command::SetBeatSound {
            owner: owner.clone(),
            id: local.clone(),
            event: event(&before)?,
        },
    ))?;
    let saved = store.snapshot()?;
    let move_clock = AudioTimingId {
        allocation: revision("moved"),
        ordinal: 0,
    };
    let insert = edit(
        &saved,
        "moved",
        Command::InsertTime {
            at: ProjectFrame(0),
            hold: HoldRecipe {
                duration: FrameDuration::new(1)?,
                video: HoldVideo::Background,
                picture_context: None,
                audio: HoldAudio::Silence,
            },
            id: NodeId::new("prefix")?,
            identities: SplitIdentities::default(),
            timing: move_clock.clone(),
        },
    );
    let preview = store.preview(&insert)?;
    assert_eq!(store.snapshot()?, saved);
    let before_counts = counts(&path)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    database.execute_batch("CREATE TRIGGER fail_sound_clock BEFORE INSERT ON history BEGIN SELECT RAISE(FAIL,'forced sound clock history failure'); END;")?;
    assert!(store.commit(&insert).is_err());
    assert_eq!(store.snapshot()?, saved);
    assert_eq!(counts(&path)?, before_counts);
    database.execute_batch("DROP TRIGGER fail_sound_clock")?;
    assert_eq!(store.commit(&insert)?.edit, preview);
    let moved = store.snapshot()?;
    assert_eq!(
        moved.audio_bindings().sound_clocks()[&owner][&local].clocks(),
        std::slice::from_ref(&move_clock)
    );
    let return_clock = AudioTimingId {
        allocation: revision("returned"),
        ordinal: 0,
    };
    store.commit(&edit(
        &moved,
        "returned",
        Command::DeleteRipple {
            node: NodeId::new("prefix")?,
            timing: return_clock.clone(),
        },
    ))?;
    let returned = store.snapshot()?;
    assert_eq!(
        returned.audio_bindings().sound_clocks()[&owner][&local].clocks(),
        &[move_clock, return_clock]
    );
    assert_eq!(returned.duration()?, saved.duration()?);
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, returned);
    store.validate()?;
    store.undo(returned.revision_id(), revision("undo-return"))?;
    assert_authored(&store.snapshot()?, &moved)?;
    store.undo(&revision("undo-return"), revision("undo-move"))?;
    assert_authored(&store.snapshot()?, &saved)?;
    store.redo(&revision("undo-move"), revision("redo-move"))?;
    assert_authored(&store.snapshot()?, &moved)?;
    store.redo(&revision("redo-move"), revision("redo-return"))?;
    assert_authored(&store.snapshot()?, &returned)?;
    assert_eq!(
        store.commit(&insert).unwrap_err().code(),
        "RevisionConflict"
    );
    drop(store);
    ProjectStore::open(&path, AccessMode::ReadOnly)?.validate()?;
    Ok(())
}
