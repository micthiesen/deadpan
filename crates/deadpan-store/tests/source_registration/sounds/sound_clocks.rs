use super::*;
use deadpan_core::{SliceCaptureSelection, SoundClockJournal};
use deadpan_store::registers::{RegisterName, RegisterValue};
use std::{collections::BTreeMap, sync::Arc};

#[path = "sound_clocks/repeats.rs"]
mod repeats;

fn capture_child(
    document: &ProjectDocument,
    parent: &NodeId,
    child: &NodeId,
    name: &str,
) -> Result<CapturedEditSlice> {
    Ok(CapturedEditSlice::capture_selection(
        document,
        parent,
        &SliceCaptureSelection::Child {
            node: child.clone(),
        },
        AudioTimingId {
            allocation: revision(name),
            ordinal: 0,
        },
    )?)
}

fn captured_journal(
    slice: &CapturedEditSlice,
    owner: &NodeId,
    sound: &SoundId,
) -> Result<SoundClockJournal> {
    let wire: serde_json::Value = serde_json::from_str(&slice.to_json()?)?;
    let journals: BTreeMap<NodeId, BTreeMap<SoundId, SoundClockJournal>> =
        serde_json::from_value(wire["audio_bindings"]["sound_clocks"].clone())?;
    Ok(journals[owner][sound].clone())
}

fn scope_origin_history(
    document: &ProjectDocument,
    journal: &SoundClockJournal,
) -> Result<Vec<deadpan_core::ExactRatio>> {
    journal
        .clocks()
        .iter()
        .map(|reference| {
            let layout = &document.audio_bindings().timings()[reference.timing()];
            Ok(layout
                .project(
                    &InstancePath {
                        node: reference.scope().clone(),
                        repeats: vec![],
                    },
                    deadpan_core::ExactRatio::ZERO,
                    None,
                    100_000,
                )?
                .origin)
        })
        .collect()
}

fn slice_scope_origin_history(
    slice: &CapturedEditSlice,
    journal: &SoundClockJournal,
) -> Result<Vec<deadpan_core::ExactRatio>> {
    let wire: serde_json::Value = serde_json::from_str(&slice.to_json()?)?;
    let bindings = deadpan_core::AudioBindingState::from_json(&wire["audio_bindings"].to_string())?;
    journal
        .clocks()
        .iter()
        .map(|reference| {
            let layout = &bindings.timings()[reference.timing()];
            Ok(layout
                .project(
                    &InstancePath {
                        node: reference.scope().clone(),
                        repeats: vec![],
                    },
                    deadpan_core::ExactRatio::ZERO,
                    None,
                    100_000,
                )?
                .origin)
        })
        .collect()
}

fn register_slice(
    bank: &deadpan_store::registers::RegisterBank,
    slot: char,
) -> Arc<CapturedEditSlice> {
    let name = RegisterName::new(slot).unwrap();
    let Some(value) = bank.entries.get(&name) else {
        panic!("missing edited slice register {slot}");
    };
    let RegisterValue::Edited { slice } = value.as_ref() else {
        panic!("register {slot} is not an edited slice");
    };
    Arc::clone(slice)
}

fn register_rows(path: &std::path::Path) -> Result<Vec<String>> {
    let database = Connection::open(path.join("project.sqlite"))?;
    Ok(database
        .prepare(
            "SELECT json_array(id,capture_revision,capture_step,value) FROM register_contents ORDER BY id",
        )?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<_, _>>()?)
}

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
        moved.audio_bindings().sound_clocks()[&owner][&local]
            .clocks()
            .iter()
            .map(|reference| reference.timing().clone())
            .collect::<Vec<_>>(),
        vec![move_clock.clone()]
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
        returned.audio_bindings().sound_clocks()[&owner][&local]
            .clocks()
            .iter()
            .map(|reference| reference.timing().clone())
            .collect::<Vec<_>>(),
        vec![move_clock, return_clock]
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

#[test]
fn retained_beat_sound_clocks_survive_register_reopen_recapture_and_multiple_pastes() -> Result {
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
    store.commit(&edit(
        &before,
        "sound",
        Command::SetBeatSound {
            owner: owner.clone(),
            id: sound.clone(),
            event: event(&before)?,
        },
    ))?;
    let added = store.snapshot()?;
    let move_clock = AudioTimingId {
        allocation: revision("clock-move"),
        ordinal: 0,
    };
    store.commit(&edit(
        &added,
        "clock-move",
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
    ))?;
    let moved = store.snapshot()?;
    let return_clock = AudioTimingId {
        allocation: revision("clock-return"),
        ordinal: 0,
    };
    store.commit(&edit(
        &moved,
        "clock-return",
        Command::DeleteRipple {
            node: NodeId::new("prefix")?,
            timing: return_clock.clone(),
        },
    ))?;
    let returned = store.snapshot()?;
    let live_journal = returned.audio_bindings().sound_clocks()[&owner][&sound].clone();
    assert_eq!(live_journal.scope(), &owner);
    assert_eq!(
        live_journal
            .clocks()
            .iter()
            .map(|reference| reference.timing().clone())
            .collect::<Vec<_>>(),
        vec![move_clock.clone(), return_clock.clone()]
    );
    assert_eq!(
        scope_origin_history(&returned, &live_journal)?,
        vec![
            deadpan_core::ExactRatio::ZERO,
            deadpan_core::ExactRatio::integer(1)
        ]
    );

    let capture_clock = AudioTimingId {
        allocation: revision("register-capture"),
        ordinal: 0,
    };
    let slice = capture_child(&returned, returned.root(), &owner, "register-capture")?;
    let source_slice_journal = captured_journal(&slice, &owner, &sound)?;
    assert_eq!(source_slice_journal.scope(), &owner);
    assert_eq!(
        source_slice_journal
            .clocks()
            .iter()
            .map(|reference| reference.timing().clone())
            .collect::<Vec<_>>(),
        vec![move_clock, return_clock, capture_clock]
    );
    assert!(
        source_slice_journal
            .clocks()
            .iter()
            .all(|reference| reference.owner() == &owner)
    );
    assert_eq!(
        slice_scope_origin_history(&slice, &source_slice_journal)?,
        vec![
            deadpan_core::ExactRatio::ZERO,
            deadpan_core::ExactRatio::integer(1),
            deadpan_core::ExactRatio::ZERO,
        ]
    );

    let slot_a = RegisterName::new('a')?;
    let bank = store.save_register(
        returned.project_id(),
        returned.revision_id(),
        slot_a,
        RegisterValue::Edited {
            slice: Arc::new(slice.clone()),
        },
    )?;
    let stored_json = register_slice(&bank, 'a').to_json()?;
    assert_eq!(stored_json, slice.to_json()?);
    drop(store);

    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.registers()?, bank);
    let slice = register_slice(&store.registers()?, 'a');
    assert_eq!(slice.to_json()?, stored_json);
    let current = store.snapshot()?;
    store.commit(&edit(
        &current,
        "delete-source",
        Command::DeleteRipple {
            node: owner.clone(),
            timing: AudioTimingId {
                allocation: revision("delete-source"),
                ordinal: 0,
            },
        },
    ))?;
    let deleted = store.snapshot()?;
    assert!(deleted.beat_sounds().is_empty());
    assert!(deleted.audio_bindings().sound_clocks().is_empty());
    assert_eq!(store.registers()?, bank);

    let first_paste = paste(&deleted, &slice, "paste-first")?;
    let preview = store.preview(&first_paste)?;
    assert_eq!(store.snapshot()?, deleted);
    let first_edit = store.commit(&first_paste)?.edit;
    assert_eq!(first_edit, preview);
    let pasted_once = store.snapshot()?;
    let first_owner = pasted_once.beat_sounds().keys().next().unwrap().clone();
    assert_ne!(first_owner, owner);
    let first_journal = pasted_once.audio_bindings().sound_clocks()[&first_owner][&sound].clone();
    assert_eq!(first_journal.scope(), &first_owner);
    assert_eq!(
        first_journal.clocks().len(),
        source_slice_journal.clocks().len()
    );
    assert_eq!(
        scope_origin_history(&pasted_once, &first_journal)?,
        vec![
            deadpan_core::ExactRatio::ZERO,
            deadpan_core::ExactRatio::integer(1),
            deadpan_core::ExactRatio::ZERO,
        ]
    );
    for (source, copied) in source_slice_journal
        .clocks()
        .iter()
        .zip(first_journal.clocks())
    {
        assert_eq!(copied.timing().allocation, revision("paste-first"));
        assert_ne!(copied.scope(), source.scope());
        assert_ne!(copied.owner(), source.owner());
    }

    let second_paste = paste(&pasted_once, &slice, "paste-second")?;
    store.commit(&second_paste)?;
    let pasted_twice = store.snapshot()?;
    let mut owners = pasted_twice
        .beat_sounds()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    owners.sort();
    assert_eq!(owners.len(), 2);
    assert!(owners.iter().all(|candidate| candidate != &owner));
    let second_owner = owners
        .iter()
        .find(|candidate| *candidate != &first_owner)
        .unwrap()
        .clone();
    let second_journal =
        pasted_twice.audio_bindings().sound_clocks()[&second_owner][&sound].clone();
    assert_eq!(second_journal.scope(), &second_owner);
    assert_eq!(
        second_journal.clocks().len(),
        source_slice_journal.clocks().len()
    );
    assert_eq!(
        scope_origin_history(&pasted_twice, &second_journal)?,
        vec![
            deadpan_core::ExactRatio::ZERO,
            deadpan_core::ExactRatio::integer(1),
            deadpan_core::ExactRatio::ZERO,
        ]
    );
    for (first, second) in first_journal.clocks().iter().zip(second_journal.clocks()) {
        assert_ne!(first.timing(), second.timing());
        assert_ne!(first.scope(), second.scope());
        assert_ne!(first.owner(), second.owner());
        assert_eq!(second.timing().allocation, revision("paste-second"));
    }

    // Recapturing the first pasted sound creates a new immutable history value;
    // it must not rewrite slot a's historical references or layouts.
    let first_root = NodeId::new("paste-first-node-0")?;
    let recaptured = capture_child(
        &pasted_twice,
        pasted_twice.root(),
        &first_root,
        "recapture-first",
    )?;
    let recaptured_journal = captured_journal(&recaptured, &first_owner, &sound)?;
    assert_eq!(
        recaptured_journal.clocks().len(),
        first_journal.clocks().len() + 1
    );
    assert_eq!(
        slice_scope_origin_history(&recaptured, &recaptured_journal)?,
        vec![
            deadpan_core::ExactRatio::ZERO,
            deadpan_core::ExactRatio::integer(1),
            deadpan_core::ExactRatio::ZERO,
            deadpan_core::ExactRatio::ZERO,
        ]
    );
    let after_recapture = store.save_register(
        pasted_twice.project_id(),
        pasted_twice.revision_id(),
        RegisterName::new('b')?,
        RegisterValue::Edited {
            slice: Arc::new(recaptured),
        },
    )?;
    assert_eq!(
        register_slice(&after_recapture, 'a').to_json()?,
        stored_json
    );

    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let reopened_bank = store.registers()?;
    assert_eq!(reopened_bank, after_recapture);
    assert_eq!(register_slice(&reopened_bank, 'a').to_json()?, stored_json);
    store.undo(pasted_twice.revision_id(), revision("undo-paste-second"))?;
    assert_authored(&store.snapshot()?, &pasted_once)?;
    store.undo(&revision("undo-paste-second"), revision("undo-paste-first"))?;
    assert_authored(&store.snapshot()?, &deleted)?;
    store.undo(
        &revision("undo-paste-first"),
        revision("undo-delete-source"),
    )?;
    assert_authored(&store.snapshot()?, &returned)?;
    store.redo(
        &revision("undo-delete-source"),
        revision("redo-delete-source"),
    )?;
    assert_authored(&store.snapshot()?, &deleted)?;
    store.redo(
        &revision("redo-delete-source"),
        revision("redo-paste-first"),
    )?;
    assert_authored(&store.snapshot()?, &pasted_once)?;
    store.redo(&revision("redo-paste-first"), revision("redo-paste-second"))?;
    assert_authored(&store.snapshot()?, &pasted_twice)?;
    assert_eq!(store.registers()?, after_recapture);
    store.validate()?;
    Ok(())
}

#[test]
fn retained_sound_clock_register_rechecks_receipt_and_original_without_losing_copy() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "offset-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    let imported = store.register_source(
        &request(&store, &original, "import", "camera", Some("clip"))?,
        &decoded,
        None,
        limits(),
        &active(),
    )?;
    let owner = NodeId::new("clip")?;
    let sound = SoundId::new("overlay")?;
    let before_sound = store.snapshot()?;
    store.commit(&edit(
        &before_sound,
        "sound",
        Command::SetBeatSound {
            owner: owner.clone(),
            id: sound.clone(),
            event: event(&before_sound)?,
        },
    ))?;
    let sounded = store.snapshot()?;
    let slice = capture_child(&sounded, sounded.root(), &owner, "admission-capture")?;
    let bank = store.save_register(
        sounded.project_id(),
        sounded.revision_id(),
        RegisterName::new('a')?,
        RegisterValue::Edited {
            slice: Arc::new(slice),
        },
    )?;
    let slice = register_slice(&bank, 'a');
    let delete = edit(
        &sounded,
        "delete-source",
        Command::DeleteRipple {
            node: owner,
            timing: AudioTimingId {
                allocation: revision("delete-source"),
                ordinal: 0,
            },
        },
    );
    store.commit(&delete)?;
    let deleted = store.snapshot()?;
    assert!(deleted.beat_sounds().is_empty());
    let paste_request = paste(&deleted, &slice, "admission-paste")?;
    store.preview(&paste_request)?;
    store.preview_edit_slice(&paste_request)?;
    let saved_rows = register_rows(&path)?;
    let before_counts = counts(&path)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    // This separate connection deliberately injects damaged registry state.
    // Keep the production store connection's foreign-key enforcement intact.
    database.pragma_update(None, "foreign_keys", false)?;
    let receipt: (String, String, Vec<u8>) = database.query_row(
        "SELECT original_content_id,original_ref,snapshot FROM source_qualifications WHERE id=?1",
        [imported.qualification.as_str()],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let content_id = original.object().content().to_string();
    let original_row: (i64, String) = database.query_row(
        "SELECT version,record FROM original_media WHERE content_id=?1",
        [&content_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    for corruption in [
        "missing-receipt",
        "changed-receipt",
        "missing-original",
        "changed-original",
    ] {
        match corruption {
            "missing-receipt" => {
                database.execute(
                    "DELETE FROM source_qualifications WHERE id=?1",
                    [imported.qualification.as_str()],
                )?;
            }
            "changed-receipt" => {
                database.execute(
                    "UPDATE source_qualifications SET snapshot=X'00' WHERE id=?1",
                    [imported.qualification.as_str()],
                )?;
            }
            "missing-original" => {
                database.execute(
                    "DELETE FROM original_media WHERE content_id=?1",
                    [&content_id],
                )?;
            }
            "changed-original" => {
                let mut wire: serde_json::Value = serde_json::from_str(&original_row.1)?;
                wire["sha256"] = serde_json::to_value([9_u8; 32])?;
                database.execute(
                    "UPDATE original_media SET record=?1 WHERE content_id=?2",
                    rusqlite::params![wire.to_string(), content_id],
                )?;
            }
            _ => unreachable!(),
        }

        assert!(
            matches!(
                store.preview(&paste_request),
                Err(StoreError::SourceRegistration(_))
            ),
            "{corruption}: regular preview admitted the historical clock"
        );
        assert!(
            matches!(
                store.preview_edit_slice(&paste_request),
                Err(StoreError::SourceRegistration(_))
            ),
            "{corruption}: edited-slice preview admitted the historical clock"
        );
        assert!(
            matches!(
                store.commit(&paste_request),
                Err(StoreError::SourceRegistration(_))
            ),
            "{corruption}: commit admitted the historical clock"
        );
        assert_eq!(store.snapshot()?, deleted, "{corruption}");
        let after_counts = counts(&path)?;
        assert_eq!(
            (after_counts.0, after_counts.1),
            (before_counts.0, before_counts.1)
        );
        assert_eq!(register_rows(&path)?, saved_rows, "{corruption}");

        match corruption {
            "missing-receipt" | "changed-receipt" => {
                database.execute(
                    "INSERT OR REPLACE INTO source_qualifications(id,original_content_id,original_ref,snapshot) VALUES(?1,?2,?3,?4)",
                    rusqlite::params![
                        imported.qualification.as_str(),
                        receipt.0,
                        receipt.1,
                        receipt.2,
                    ],
                )?;
            }
            "missing-original" | "changed-original" => {
                database.execute(
                    "INSERT OR REPLACE INTO original_media(content_id,version,record) VALUES(?1,?2,?3)",
                    rusqlite::params![content_id, original_row.0, original_row.1],
                )?;
            }
            _ => unreachable!(),
        }
        assert_eq!(
            store.registers()?,
            bank,
            "{corruption}: retained register changed"
        );
    }

    database.pragma_update(None, "foreign_keys", true)?;
    assert_eq!(
        database.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
            row.get::<_, i64>(0)
        })?,
        0
    );
    let preview = store.preview(&paste_request)?;
    assert_eq!(store.commit(&paste_request)?.edit, preview);
    let copied = store.snapshot()?;
    assert_eq!(copied.beat_sounds().len(), 1);
    assert_eq!(store.registers()?, bank);
    store.validate()?;
    Ok(())
}
