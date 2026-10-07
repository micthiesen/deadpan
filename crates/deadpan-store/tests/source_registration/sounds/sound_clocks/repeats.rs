use super::*;

fn timing(name: &str) -> AudioTimingId {
    AudioTimingId {
        allocation: revision(name),
        ordinal: 0,
    }
}

#[test]
fn repeated_sound_clocks_and_isolation_survive_register_reopen_undo_and_redo() -> Result {
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
    let repeat = NodeId::new("repeat")?;
    let sound = SoundId::new("overlay")?;
    let before = store.snapshot()?;
    store.commit(&edit(
        &before,
        "sound",
        Command::SetBeatSound {
            owner: owner.clone(),
            id: sound.clone(),
            event: event(&before)?,
        },
    ))?;
    let saved = store.snapshot()?;
    let wrapping = edit(
        &saved,
        "wrapped",
        Command::RepeatSelection {
            parent: saved.root().clone(),
            selection: SliceCaptureSelection::Child {
                node: owner.clone(),
            },
            plays: 2,
            identities: deadpan_core::RepeatSelectionIdentities {
                repeat: repeat.clone(),
                group: None,
                split: SplitIdentities::default(),
            },
            timing: timing("wrapped"),
        },
    );
    let preview = store.preview(&wrapping)?;
    assert_eq!(store.snapshot()?, saved);
    assert_eq!(store.commit(&wrapping)?.edit, preview);
    let wrapped = store.snapshot()?;
    store.commit(&edit(
        &wrapped,
        "grown",
        Command::SetRepeatPlays {
            node: repeat.clone(),
            plays: 3,
            timing: timing("grown"),
        },
    ))?;
    let grown = store.snapshot()?;
    store.commit(&edit(
        &grown,
        "gapped",
        Command::SetRepeatGaps {
            node: repeat.clone(),
            gap: Some(HoldRecipe {
                duration: FrameDuration::new(1)?,
                video: HoldVideo::Background,
                picture_context: None,
                audio: HoldAudio::Silence,
            }),
            branches: vec![],
            timing: timing("gapped"),
        },
    ))?;
    let gapped = store.snapshot()?;
    let count = gapped.first_play_attachment_nodes(&repeat)?;
    assert_eq!(count, 1);
    let isolated_owner = NodeId::new("isolated-clip")?;
    store.commit(&edit(
        &gapped,
        "isolated",
        Command::KeepFirstPlayAttachments {
            node: repeat.clone(),
            identities: OccurrenceIdentities {
                nodes: vec![isolated_owner.clone()],
                marks: vec![],
            },
        },
    ))?;
    let isolated = store.snapshot()?;
    assert!(!isolated.beat_sounds().contains_key(&owner));
    assert_eq!(
        isolated.beat_sounds()[&isolated_owner],
        saved.beat_sounds()[&owner]
    );
    let clocks = isolated.audio_bindings().sound_clocks()[&isolated_owner][&sound].clocks();
    assert_eq!(clocks.len(), 3);
    assert!(
        matches!(clocks[0].repeats().steps(), [deadpan_core::SoundClockRepeatStep::Introduced { plays, .. }] if plays.len() == 2)
    );
    let slice = capture_child(&isolated, isolated.root(), &repeat, "capture-repeat")?;
    let bank = store.save_register(
        isolated.project_id(),
        isolated.revision_id(),
        RegisterName::new('r')?,
        RegisterValue::Edited {
            slice: Arc::new(slice),
        },
    )?;
    drop(store);

    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, isolated);
    assert_eq!(store.registers()?, bank);
    store.undo(isolated.revision_id(), revision("undo-isolation"))?;
    assert_authored(&store.snapshot()?, &gapped)?;
    store.redo(&revision("undo-isolation"), revision("redo-isolation"))?;
    assert_authored(&store.snapshot()?, &isolated)?;
    let copy = register_slice(&store.registers()?, 'r');
    let destination = store.snapshot()?;
    store.commit(&paste(&destination, &copy, "pasted-repeat")?)?;
    let pasted = store.snapshot()?;
    assert_eq!(pasted.beat_sounds().len(), 2);
    assert!(
        pasted
            .beat_sounds()
            .values()
            .all(|events| events[&sound] == saved.beat_sounds()[&owner][&sound])
    );
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, pasted);
    store.undo(pasted.revision_id(), revision("undo-paste"))?;
    assert_authored(&store.snapshot()?, &isolated)?;
    store.redo(&revision("undo-paste"), revision("redo-paste"))?;
    assert_authored(&store.snapshot()?, &pasted)?;
    store.validate()?;
    Ok(())
}
