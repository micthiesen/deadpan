use super::*;
use deadpan_core::{
    AudioEdgePolicy, AudioSample, AudioTimingId, FrameDuration, HoldAudio, HoldRecipe, HoldVideo,
    InstancePath, ProjectFrame, SoundEvent, SoundHoldIssuer, SoundId, SoundOverflowPolicy,
    SourceAudio, SourceAudioMapping, SourceSpan, SourceTimestamp, SplitIdentities,
};

fn sound(document: &ProjectDocument) -> Result<SoundEvent> {
    let full = document.assets()[&id("camera")].audio.unwrap();
    let span = SourceSpan::new(
        full.start(),
        SourceTimestamp {
            ticks: full.start().ticks + (full.end().ticks - full.start().ticks) / 2,
            time_base: full.start().time_base,
        },
    )?;
    Ok(SoundEvent {
        owner: document.root().clone(),
        label: "Measured overlay".into(),
        source: SourceAudio {
            asset: id("camera"),
            span,
        },
        mapping: SourceAudioMapping::natural_rate(span, document.presentation_basis().frame_rate)?,
        offset: AudioSample(7),
        gain_millidecibels: -3000,
        start_edge: AudioEdgePolicy::Automatic,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    })
}

fn edit(document: &ProjectDocument, next: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(next),
        command,
    }
}

#[test]
fn qualified_sound_edits_are_atomic_durable_and_keep_picture_time() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "offset-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    let input = request(&store, &original, "import", "camera", Some("clip"))?;
    store.register_source(&input, &decoded, None, limits(), &active())?;
    let baseline = store.snapshot()?;
    let sound_id = SoundId::new("overlay")?;
    let event = sound(&baseline)?;
    let add = edit(
        &baseline,
        "sound-add",
        Command::SetSound {
            id: sound_id.clone(),
            event: event.clone(),
        },
    );
    let preview = store.preview(&add)?;
    assert_eq!(store.snapshot()?, baseline);
    assert_eq!(counts(&path)?, (2, 1, 1));
    let database = Connection::open(path.join("project.sqlite"))?;
    database.execute_batch("CREATE TRIGGER fail_sound_history BEFORE INSERT ON history BEGIN SELECT RAISE(FAIL,'forced sound history failure'); END;")?;
    assert!(store.commit(&add).is_err());
    assert_eq!(store.snapshot()?, baseline);
    assert_eq!(counts(&path)?, (2, 1, 1));
    database.execute_batch("DROP TRIGGER fail_sound_history")?;
    assert_eq!(store.commit(&add)?.edit, preview);
    let added = store.snapshot()?;
    assert_eq!(added.nodes(), baseline.nodes());
    assert_eq!(added.duration()?, baseline.duration()?);
    assert_eq!(added.sounds().get(&sound_id), Some(&event));
    assert_eq!(preview.inverse.apply(&added)?, baseline);
    assert!(store.commit(&add).is_err());
    assert_eq!(store.snapshot()?, added);
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, added);
    let mut changed = event;
    changed.offset = AudioSample(19);
    changed.gain_millidecibels = -6000;
    store.commit(&edit(
        &added,
        "sound-update",
        Command::SetSound {
            id: sound_id.clone(),
            event: changed.clone(),
        },
    ))?;
    let updated = store.snapshot()?;
    store.commit(&edit(
        &updated,
        "sound-delete",
        Command::DeleteSound {
            id: sound_id.clone(),
        },
    ))?;
    let deleted = store.snapshot()?;
    assert!(deleted.sounds().is_empty());
    assert_eq!(deleted.nodes(), baseline.nodes());
    store.undo(deleted.revision_id(), revision("undo-sound-delete"))?;
    assert_eq!(store.snapshot()?.sounds().get(&sound_id), Some(&changed));
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.redo(
        store.snapshot()?.revision_id(),
        revision("redo-sound-delete"),
    )?;
    assert!(store.snapshot()?.sounds().is_empty());
    assert_eq!(
        store.snapshot_at(added.revision_id())?.sounds(),
        added.sounds()
    );
    store.validate()?;
    drop(store);
    ProjectStore::open(&path, AccessMode::ReadOnly)?.validate()?;
    Ok(())
}

#[test]
fn sound_command_rechecks_receipt_inside_its_revision_transaction() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "offset-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    let input = request(&store, &original, "import", "camera", Some("clip"))?;
    let imported = store.register_source(&input, &decoded, None, limits(), &active())?;
    let baseline = store.snapshot()?;
    let add = edit(
        &baseline,
        "sound-add",
        Command::SetSound {
            id: SoundId::new("overlay")?,
            event: sound(&baseline)?,
        },
    );
    store.preview(&add)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let receipt: Vec<u8> = database.query_row(
        "SELECT snapshot FROM source_qualifications WHERE id=?1",
        [imported.qualification.as_str()],
        |row| row.get(0),
    )?;
    database.execute(
        "UPDATE source_qualifications SET snapshot=X'00' WHERE id=?1",
        [imported.qualification.as_str()],
    )?;
    assert!(matches!(
        store.preview(&add),
        Err(StoreError::SourceRegistration(_))
    ));
    assert!(matches!(
        store.commit(&add),
        Err(StoreError::SourceRegistration(_))
    ));
    assert_eq!(store.snapshot()?, baseline);
    assert_eq!(counts(&path)?, (2, 1, 1));
    database.execute(
        "UPDATE source_qualifications SET snapshot=?1 WHERE id=?2",
        rusqlite::params![receipt, imported.qualification.as_str()],
    )?;
    store.commit(&add)?;
    store.validate()?;
    Ok(())
}

#[test]
fn sound_allowance_edits_recheck_sources_and_survive_atomic_history_reopen() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "offset-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    let input = request(&store, &original, "import", "camera", Some("clip"))?;
    store.register_source(&input, &decoded, None, limits(), &active())?;
    let sound_id = SoundId::new("overlay")?;
    let imported = store.snapshot()?;
    store.commit(&edit(
        &imported,
        "sound-add",
        Command::SetSound {
            id: sound_id.clone(),
            event: sound(&imported)?,
        },
    ))?;
    let hold = NodeId::new("silent-hold")?;
    store.commit(&edit(
        &store.snapshot()?,
        "pause",
        Command::InsertTime {
            at: ProjectFrame(0),
            hold: HoldRecipe {
                picture_context: None,
                duration: FrameDuration::new(2)?,
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            },
            id: hold.clone(),
            identities: SplitIdentities::default(),
            timing: AudioTimingId {
                allocation: revision("pause"),
                ordinal: 0,
            },
        },
    ))?;
    let baseline = store.snapshot()?;
    assert!(baseline.sound_routes().contains_key(&sound_id));
    let issuer = SoundHoldIssuer::Node {
        instance: InstancePath {
            node: hold,
            repeats: Vec::new(),
        },
    };
    let allow = edit(
        &baseline,
        "allow-sound",
        Command::SetSoundAllowance {
            sound: sound_id.clone(),
            issuer: issuer.clone(),
            allowed: true,
        },
    );
    let preview = store.preview(&allow)?;
    assert_eq!(store.snapshot()?, baseline);
    let baseline_counts = counts(&path)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    database.execute_batch("CREATE TRIGGER fail_allowance_history BEFORE INSERT ON history BEGIN SELECT RAISE(FAIL,'forced allowance history failure'); END;")?;
    assert!(store.commit(&allow).is_err());
    assert_eq!(store.snapshot()?, baseline);
    assert_eq!(counts(&path)?, baseline_counts);
    database.execute_batch("DROP TRIGGER fail_allowance_history")?;

    // The event and its sample route are unchanged by this command. Receipt
    // admission must still run for both adding and removing the allowance.
    for allowed in [true, false] {
        let before = store.snapshot()?;
        let change = edit(
            &before,
            if allowed { "allow-sound" } else { "deny-sound" },
            Command::SetSoundAllowance {
                sound: sound_id.clone(),
                issuer: issuer.clone(),
                allowed,
            },
        );
        let before_counts = counts(&path)?;
        let (receipt, original_ref): (Vec<u8>, String) = database.query_row(
            "SELECT snapshot,original_ref FROM source_qualifications",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        for tamper in [
            "UPDATE source_qualifications SET snapshot=X'00'",
            "UPDATE source_qualifications SET original_ref=json_set(original_ref,'$.byte_length',1234)",
        ] {
            database.execute(tamper, [])?;
            assert!(matches!(
                store.preview(&change),
                Err(StoreError::SourceRegistration(_))
            ));
            assert!(matches!(
                store.commit(&change),
                Err(StoreError::SourceRegistration(_))
            ));
            assert_eq!(store.snapshot()?, before);
            assert_eq!(counts(&path)?, before_counts);
            database.execute(
                "UPDATE source_qualifications SET snapshot=?1,original_ref=?2",
                rusqlite::params![receipt, original_ref],
            )?;
        }
        let committed = store.commit(&change)?;
        if allowed {
            assert_eq!(committed.edit, preview);
        }
        let after = store.snapshot()?;
        assert_eq!(after.sounds(), baseline.sounds());
        assert_eq!(after.sound_routes(), baseline.sound_routes());
        assert_eq!(after.nodes(), baseline.nodes());
        assert_eq!(after.sound_allowances().contains_key(&sound_id), allowed);
        assert_eq!(committed.edit.inverse.apply(&after)?, before);
        drop(store);
        store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        assert_eq!(store.snapshot()?, after);
    }
    let denied = store.snapshot()?;
    store.undo(denied.revision_id(), revision("undo-deny"))?;
    let allowed = store.snapshot()?;
    assert!(allowed.sound_allowances().contains_key(&sound_id));
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.redo(allowed.revision_id(), revision("redo-deny"))?;
    assert!(store.snapshot()?.sound_allowances().is_empty());
    assert_eq!(store.snapshot()?.sound_routes(), baseline.sound_routes());
    store.validate()?;
    drop(store);
    ProjectStore::open(&path, AccessMode::ReadOnly)?.validate()?;
    Ok(())
}
