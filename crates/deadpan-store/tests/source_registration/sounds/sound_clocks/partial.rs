use super::*;

#[test]
fn split_sound_clocks_commit_atomically_reopen_and_undo_with_fresh_revisions() -> Result {
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
    let sound = SoundId::new("overlay")?;
    let recipe = event(&before)?;
    store.commit(&edit(
        &before,
        "sound",
        Command::SetBeatSound {
            owner: owner.clone(),
            id: sound.clone(),
            event: recipe.clone(),
        },
    ))?;
    let before = store.snapshot()?;
    let at = ProjectFrame(2);
    let count = before
        .insert_time_target(at)?
        .split
        .ok_or("expected interior")?
        .required_ids;
    let command = edit(
        &before,
        "split-sound",
        Command::InsertTime {
            at,
            hold: HoldRecipe {
                duration: FrameDuration::new(1)?,
                video: HoldVideo::Background,
                picture_context: None,
                audio: HoldAudio::Silence,
            },
            id: NodeId::new("pause")?,
            identities: SplitIdentities {
                nodes: (0..count)
                    .map(|i| NodeId::new(format!("split-{i}")))
                    .collect::<std::result::Result<_, _>>()?,
            },
            timing: AudioTimingId {
                allocation: revision("split-sound"),
                ordinal: 0,
            },
        },
    );
    let preview = store.preview(&command)?;
    let before_counts = counts(&path)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    database.execute_batch("CREATE TRIGGER fail_split_sound BEFORE INSERT ON history BEGIN SELECT RAISE(FAIL,'forced split sound failure'); END;")?;
    assert!(store.commit(&command).is_err());
    assert_eq!(store.snapshot()?, before);
    assert_eq!(counts(&path)?, before_counts);
    database.execute_batch("DROP TRIGGER fail_split_sound")?;
    assert_eq!(store.commit(&command)?.edit, preview);
    let after = store.snapshot()?;
    assert_eq!(after.duration()?.frames(), before.duration()?.frames() + 1);
    assert_eq!(after.beat_sounds().len(), 2);
    for (alias, events) in after.beat_sounds() {
        assert_eq!(events[&sound], recipe);
        let journal = &after.audio_bindings().sound_clocks()[alias][&sound];
        assert_eq!(journal.scope(), alias);
        assert_eq!(journal.clocks().len(), 1);
        assert_eq!(journal.clocks()[0].owner(), &owner);
    }
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, after);
    store.undo(after.revision_id(), revision("undo-split"))?;
    assert_authored(&store.snapshot()?, &before)?;
    store.redo(&revision("undo-split"), revision("redo-split"))?;
    assert_authored(&store.snapshot()?, &after)?;
    store.validate()?;
    let final_snapshot = store.snapshot()?;
    drop(store);
    let store = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(store.snapshot()?, final_snapshot);
    store.validate()?;
    Ok(())
}

#[test]
fn partial_sound_register_reopens_after_owner_deletion_and_paste_is_atomic() -> Result {
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
    let owner = NodeId::new("clip")?;
    let sound = SoundId::new("overlay")?;
    let before = store.snapshot()?;
    let recipe = event(&before)?;
    store.commit(&edit(
        &before,
        "sound",
        Command::SetBeatSound {
            owner: owner.clone(),
            id: sound.clone(),
            event: recipe.clone(),
        },
    ))?;
    let sounded = store.snapshot()?;
    let full_duration = sounded.duration()?;
    assert!(full_duration.frames() > 4);
    let slice = CapturedEditSlice::capture(
        &sounded,
        sounded.root(),
        deadpan_core::FrameRange::new(ProjectFrame(1), ProjectFrame(4))?,
        AudioTimingId {
            allocation: revision("partial-capture"),
            ordinal: 0,
        },
    )?;
    assert_eq!(captured_journal(&slice, &owner, &sound)?.clocks().len(), 1);
    let bank = store.save_register(
        sounded.project_id(),
        sounded.revision_id(),
        RegisterName::new('a')?,
        RegisterValue::Edited {
            slice: Arc::new(slice),
        },
    )?;
    let rows = register_rows(&path)?;
    store.commit(&edit(
        &sounded,
        "remove-original-owner",
        Command::DeleteRipple {
            node: owner,
            timing: AudioTimingId {
                allocation: revision("remove-original-owner"),
                ordinal: 0,
            },
        },
    ))?;
    let deleted = store.snapshot()?;
    assert!(deleted.beat_sounds().is_empty());
    drop(store);

    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.registers()?, bank);
    let slice = register_slice(&bank, 'a');
    let command = paste(&deleted, &slice, "partial-paste")?;
    let preview = store.preview(&command)?;
    assert_eq!(store.snapshot()?, deleted);
    let before_counts = counts(&path)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    database.execute_batch("CREATE TRIGGER fail_partial_sound BEFORE INSERT ON history BEGIN SELECT RAISE(FAIL,'forced partial sound failure'); END;")?;
    assert!(store.commit(&command).is_err());
    assert_eq!(store.snapshot()?, deleted);
    assert_eq!(counts(&path)?, before_counts);
    assert_eq!(register_rows(&path)?, rows);
    database.execute_batch("DROP TRIGGER fail_partial_sound")?;
    assert_eq!(store.commit(&command)?.edit, preview);
    let pasted = store.snapshot()?;
    assert_eq!(pasted.duration()?.frames(), 3);
    let copied_owner = pasted.beat_sounds().keys().next().unwrap();
    assert_eq!(pasted.beat_sounds()[copied_owner][&sound], recipe);
    assert_eq!(pasted.node_duration(copied_owner)?, full_duration);
    assert_eq!(
        pasted.audio_bindings().sound_clocks()[copied_owner][&sound]
            .clocks()
            .len(),
        1
    );

    let recopy = CapturedEditSlice::capture(
        &pasted,
        &NodeId::new("partial-paste-node-0")?,
        deadpan_core::FrameRange::new(ProjectFrame(1), ProjectFrame(3))?,
        AudioTimingId {
            allocation: revision("partial-recapture"),
            ordinal: 0,
        },
    )?;
    assert_eq!(
        captured_journal(&recopy, copied_owner, &sound)?
            .clocks()
            .len(),
        2
    );
    let bank = store.save_register(
        pasted.project_id(),
        pasted.revision_id(),
        RegisterName::new('b')?,
        RegisterValue::Edited {
            slice: Arc::new(recopy),
        },
    )?;
    let recopy = register_slice(&bank, 'b');
    store.commit(&paste(&pasted, &recopy, "partial-repaste")?)?;
    let twice = store.snapshot()?;
    assert_eq!(twice.duration()?.frames(), 5);
    assert_eq!(twice.beat_sounds().len(), 2);
    drop(store);

    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, twice);
    assert_eq!(store.registers()?, bank);
    store.undo(twice.revision_id(), revision("undo-repaste"))?;
    assert_authored(&store.snapshot()?, &pasted)?;
    store.undo(&revision("undo-repaste"), revision("undo-paste"))?;
    assert_authored(&store.snapshot()?, &deleted)?;
    store.redo(&revision("undo-paste"), revision("redo-paste"))?;
    assert_authored(&store.snapshot()?, &pasted)?;
    store.redo(&revision("redo-paste"), revision("redo-repaste"))?;
    assert_authored(&store.snapshot()?, &twice)?;
    assert_eq!(store.registers()?, bank);
    store.validate()?;
    let final_snapshot = store.snapshot()?;
    drop(store);
    let store = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(store.snapshot()?, final_snapshot);
    assert_eq!(store.registers()?, bank);
    store.validate()?;
    Ok(())
}
