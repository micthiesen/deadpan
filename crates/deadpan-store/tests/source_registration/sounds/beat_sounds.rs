use super::*;
use deadpan_core::{
    BeatSound, CapturedEditSlice, LeafEdit, OccurrenceIdentities, RegisterName, RegisterValue,
    ResolvedStep, ResolvedTransaction, SliceAttachments, SliceCaptureSelection,
    SlicePasteIdentities,
};
use std::collections::BTreeMap;
use std::sync::Arc;

#[path = "sound_clocks.rs"]
mod sound_clocks;

fn event(document: &ProjectDocument) -> Result<BeatSound> {
    let root = sound(document)?;
    Ok(BeatSound {
        label: root.label,
        source: root.source,
        mapping: root.mapping,
        offset: root.offset,
        gain_millidecibels: root.gain_millidecibels,
        start_edge: root.start_edge,
        end_edge: root.end_edge,
        overflow: root.overflow,
    })
}

fn capture(document: &ProjectDocument) -> Result<CapturedEditSlice> {
    Ok(CapturedEditSlice::capture_selection(
        document,
        document.root(),
        &SliceCaptureSelection::Child {
            node: NodeId::new("clip")?,
        },
        AudioTimingId {
            allocation: revision("copy-capture"),
            ordinal: 0,
        },
    )?)
}

fn paste(
    document: &ProjectDocument,
    slice: &CapturedEditSlice,
    name: &str,
) -> Result<CommandRequest> {
    let count = slice.identity_requirements()?;
    assert_eq!(count.marks, 0);
    Ok(edit(
        document,
        name,
        Command::SpliceSlice {
            parent: document.root().clone(),
            index: document.children(document.root()).count(),
            slice: slice.clone(),
            identities: SlicePasteIdentities {
                authored: OccurrenceIdentities {
                    nodes: (0..count.nodes)
                        .map(|i| NodeId::new(format!("{name}-node-{i}")))
                        .collect::<std::result::Result<_, _>>()?,
                    marks: Vec::new(),
                },
                aliases: (0..count.aliases)
                    .map(|i| NodeId::new(format!("{name}-alias-{i}")))
                    .collect::<std::result::Result<_, _>>()?,
            },
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    ))
}

fn assert_authored(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = serde_json::to_value(actual.revision_id())?;
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

#[test]
fn beat_sound_save_copy_rename_and_history_preserve_owner_local_ids() -> Result {
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
    let sound = event(&before)?;
    let add = edit(
        &before,
        "sound",
        Command::SetBeatSound {
            owner: owner.clone(),
            id: local.clone(),
            event: sound.clone(),
        },
    );
    let preview = store.preview(&add)?;
    assert_eq!(store.snapshot()?, before);
    let saved_counts = counts(&path)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    database.execute_batch("CREATE TRIGGER fail_beat_sound BEFORE INSERT ON history BEGIN SELECT RAISE(FAIL,'forced beat sound history failure'); END;")?;
    assert!(store.commit(&add).is_err());
    assert_eq!(store.snapshot()?, before);
    assert_eq!(counts(&path)?, saved_counts);
    database.execute_batch("DROP TRIGGER fail_beat_sound")?;
    assert_eq!(store.commit(&add)?.edit, preview);
    let added = store.snapshot()?;
    assert_eq!(added.beat_sounds()[&owner][&local], sound);
    assert_eq!(added.nodes(), before.nodes());
    assert_eq!(added.duration()?, before.duration()?);
    assert_eq!(preview.inverse.apply(&added)?, before);
    assert_eq!(store.commit(&add).unwrap_err().code(), "RevisionConflict");
    store.commit(&edit(
        &added,
        "rename",
        Command::Rename {
            node: owner.clone(),
            label: "Same owner, new label".into(),
        },
    ))?;
    let renamed = store.snapshot()?;
    assert_eq!(renamed.beat_sounds(), added.beat_sounds());
    let slice = capture(&renamed)?;
    // Keep the immutable owner capture, then remove the live event so this
    // copy test follows the historical recipe independently of live transport.
    store.commit(&edit(
        &renamed,
        "remove-before-copy",
        Command::DeleteBeatSound {
            owner: owner.clone(),
            id: local.clone(),
        },
    ))?;
    let destination = store.snapshot()?;
    assert!(destination.beat_sounds().is_empty());
    let copy = paste(&destination, &slice, "copied")?;
    let copy_preview = store.preview(&copy)?;
    store.commit(&copy)?;
    let pasted = store.snapshot()?;
    assert_eq!(pasted.beat_sounds().len(), 1);
    let copied_owner = pasted.beat_sounds().keys().next().unwrap().clone();
    assert_ne!(copied_owner, owner);
    assert_eq!(pasted.beat_sounds()[&copied_owner][&local], sound);
    assert_eq!(copy_preview.inverse.apply(&pasted)?, destination);
    // A metadata write can restore the independent original owner address.
    store.commit(&edit(
        &pasted,
        "restore-original",
        Command::SetBeatSound {
            owner: owner.clone(),
            id: local.clone(),
            event: sound.clone(),
        },
    ))?;
    let copied = store.snapshot()?;
    assert_eq!(copied.beat_sounds().len(), 2);
    assert_eq!(copied.beat_sounds()[&owner][&local], sound);
    assert_eq!(copied.beat_sounds()[&copied_owner][&local], sound);
    store.commit(&edit(
        &copied,
        "delete-local",
        Command::DeleteBeatSound {
            owner: owner.clone(),
            id: local.clone(),
        },
    ))?;
    let deleted = store.snapshot()?;
    assert!(!deleted.beat_sounds().contains_key(&owner));
    assert_eq!(deleted.beat_sounds()[&copied_owner][&local], sound);
    store.undo(deleted.revision_id(), revision("undo-local"))?;
    let undone = store.snapshot()?;
    assert_eq!(undone.revision_id(), &revision("undo-local"));
    assert_authored(&undone, &copied)?;
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, undone);
    store.redo(undone.revision_id(), revision("redo-local"))?;
    assert_authored(&store.snapshot()?, &deleted)?;
    store.undo(&revision("redo-local"), revision("undo-delete-again"))?;
    store.undo(&revision("undo-delete-again"), revision("undo-restore"))?;
    assert_authored(&store.snapshot()?, &pasted)?;
    store.undo(&revision("undo-restore"), revision("undo-copy"))?;
    assert_authored(&store.snapshot()?, &destination)?;
    store.redo(&revision("undo-copy"), revision("redo-copy"))?;
    assert_authored(&store.snapshot()?, &pasted)?;
    store.redo(&revision("redo-copy"), revision("redo-restore"))?;
    assert_authored(&store.snapshot()?, &copied)?;
    store.validate()?;
    drop(store);
    let reopened = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_authored(&reopened.snapshot()?, &copied)?;
    Ok(())
}

#[test]
fn copied_beat_sound_checks_missing_and_changed_receipts_in_each_compound_leaf() -> Result {
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
    let captured = store.snapshot()?;
    let slice = capture(&captured)?;
    store.commit(&edit(
        &captured,
        "remove-before-copy",
        Command::DeleteBeatSound {
            owner: owner.clone(),
            id: local.clone(),
        },
    ))?;
    let saved = store.snapshot()?;
    assert!(saved.beat_sounds().is_empty());
    let copy = paste(&saved, &slice, "copied")?;
    let mut changed = event(&saved)?;
    changed.gain_millidecibels -= 1000;
    let update = edit(
        &saved,
        "changed",
        Command::SetBeatSound {
            owner: owner.clone(),
            id: local,
            event: changed,
        },
    );
    let compound = edit(
        &saved,
        "outer",
        Command::Compound {
            transaction: ResolvedTransaction::new(
                store.registers()?.version,
                BTreeMap::new(),
                vec![
                    ResolvedStep::Edit {
                        edit: LeafEdit::new(
                            revision("stage-rename"),
                            Command::Rename {
                                node: owner,
                                label: "Would change before failed copy".into(),
                            },
                        )?,
                    },
                    ResolvedStep::Edit {
                        edit: LeafEdit::new(copy.new_revision.clone(), copy.command.clone())?,
                    },
                ],
            )?,
        },
    );
    store.preview(&copy)?;
    store.preview(&update)?;
    store.preview_compound(&compound)?;
    let saved_counts = counts(&path)?;
    // A serialized source import is not qualification, even when it reuses an
    // exact receipt name and is followed by an otherwise valid owned sound.
    let alias = id("forged-alias");
    let import = Command::ImportSource {
        id: alias.clone(),
        asset: saved.assets()[&id("camera")].clone(),
        insertion: None,
        primary: None,
    };
    let mut alias_sound = event(&saved)?;
    alias_sound.source.asset = alias;
    let forged_import = edit(
        &saved,
        "forged-import",
        Command::Compound {
            transaction: ResolvedTransaction::new(
                0,
                BTreeMap::new(),
                vec![
                    ResolvedStep::Edit {
                        edit: LeafEdit::new(revision("forged-source"), import)?,
                    },
                    ResolvedStep::Edit {
                        edit: LeafEdit::new(
                            revision("forged-sound"),
                            Command::SetBeatSound {
                                owner: NodeId::new("clip")?,
                                id: SoundId::new("imported-overlay")?,
                                event: alias_sound,
                            },
                        )?,
                    },
                ],
            )?,
        },
    );
    deadpan_core::apply(&saved, &forged_import)?;
    assert_eq!(
        store.preview_compound(&forged_import).unwrap_err().code(),
        "SourceAdmissionUnavailable"
    );
    assert_eq!(
        store
            .commit_compound(&forged_import, None)
            .unwrap_err()
            .code(),
        "SourceAdmissionUnavailable"
    );
    // A valid-looking copied recipe still must agree with the historical owner.
    let mut forged = serde_json::to_value(&slice)?;
    forged["beat_sounds"]["clip"]["overlay"]["gain_millidecibels"] = (-5000).into();
    let forged: CapturedEditSlice = serde_json::from_value(forged)?;
    let forged_copy = paste(&saved, &forged, "forged-copy")?;
    deadpan_core::apply(&saved, &forged_copy)?;
    assert_eq!(
        store.preview(&forged_copy).unwrap_err().code(),
        "InvalidCommand"
    );
    assert_eq!(
        store.commit(&forged_copy).unwrap_err().code(),
        "InvalidCommand"
    );
    assert_eq!(store.snapshot()?, saved);
    assert_eq!(counts(&path)?, saved_counts);
    let database = Connection::open(path.join("project.sqlite"))?;
    let receipt: (String, String, Vec<u8>) = database.query_row(
        "SELECT original_content_id,original_ref,snapshot FROM source_qualifications WHERE id=?1",
        [imported.qualification.as_str()],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    for missing in [false, true] {
        if missing {
            database.execute(
                "DELETE FROM source_qualifications WHERE id=?1",
                [imported.qualification.as_str()],
            )?;
        } else {
            database.execute(
                "UPDATE source_qualifications SET snapshot=X'00' WHERE id=?1",
                [imported.qualification.as_str()],
            )?;
        }
        for command in [&copy, &update] {
            assert!(matches!(
                store.preview(command),
                Err(StoreError::SourceRegistration(_))
            ));
            assert!(matches!(
                store.commit(command),
                Err(StoreError::SourceRegistration(_))
            ));
        }
        assert!(matches!(
            store.preview_compound(&compound),
            Err(StoreError::SourceRegistration(_))
        ));
        assert!(matches!(
            store.commit_compound(&compound, None),
            Err(StoreError::SourceRegistration(_))
        ));
        assert_eq!(store.snapshot()?, saved);
        assert_eq!(counts(&path)?.0, saved_counts.0);
        assert_eq!(counts(&path)?.1, saved_counts.1);
        assert_eq!(store.registers()?.version, 0);
        assert!(ProjectStore::open(&path, AccessMode::ReadOnly).is_err());
        database.execute("INSERT OR REPLACE INTO source_qualifications(id,original_content_id,original_ref,snapshot) VALUES(?1,?2,?3,?4)",
            rusqlite::params![imported.qualification.as_str(), receipt.0, receipt.1, receipt.2])?;
    }
    store.commit_compound(&compound, None)?;
    let copied = store.snapshot()?;
    assert_eq!(copied.beat_sounds().len(), 1);
    assert_eq!(counts(&path)?.1, saved_counts.1 + 1);
    store.undo(copied.revision_id(), revision("undo-compound"))?;
    assert_authored(&store.snapshot()?, &saved)?;
    drop(store);
    ProjectStore::open(&path, AccessMode::ReadOnly)?.validate()?;
    Ok(())
}

#[test]
fn historical_copy_restores_admitted_beat_sound_media_after_registration_undo() -> Result {
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
    let imported = store.snapshot()?;
    let local = SoundId::new("overlay")?;
    let event = event(&imported)?;
    store.commit(&edit(
        &imported,
        "sound",
        Command::SetBeatSound {
            owner: NodeId::new("clip")?,
            id: local.clone(),
            event: event.clone(),
        },
    ))?;
    let saved = store.snapshot()?;
    let slice = capture(&saved)?;
    store.undo(saved.revision_id(), revision("undo-sound"))?;
    store.undo(&revision("undo-sound"), revision("undo-import"))?;
    let empty = store.snapshot()?;
    assert!(empty.assets().is_empty());
    let copy = paste(&empty, &slice, "restored")?;
    let preview = store.preview_edit_slice(&copy)?;
    assert_eq!(preview.document().beat_sounds().len(), 1);
    store.commit(&copy)?;
    let restored = store.snapshot()?;
    let owner = restored.beat_sounds().values().next().unwrap();
    assert_eq!(owner[&local], event);
    assert_eq!(restored.assets(), saved.assets());
    store.undo(restored.revision_id(), revision("undo-restored"))?;
    assert_authored(&store.snapshot()?, &empty)?;
    store.redo(&revision("undo-restored"), revision("redo-restored"))?;
    assert_authored(&store.snapshot()?, &restored)?;
    drop(store);
    ProjectStore::open(&path, AccessMode::ReadOnly)?.validate()?;
    Ok(())
}

/// `dib` and `dab` on a beat that owns a sound: the store admits both cuts by
/// recapturing the exact attachment choice at the immutable revision, removes
/// the same content, and keeps the choice in the durable register across
/// reopen. Pasting the `ib` copy adds no sound; the `ab` copy restores it.
#[test]
fn beat_object_cuts_keep_their_attachment_choice_in_durable_registers() -> Result {
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
    store.commit(&edit(
        &before,
        "sound",
        Command::SetBeatSound {
            owner: owner.clone(),
            id: SoundId::new("overlay")?,
            event: event(&before)?,
        },
    ))?;
    let sounded = store.snapshot()?;
    let mut removed = Vec::new();
    for (register, attachments) in [
        ('i', SliceAttachments::Excluded),
        ('a', SliceAttachments::Owned),
    ] {
        let current = store.snapshot()?;
        // Re-add the sound for the second cut so both cuts see an owner.
        let current = if current.beat_sounds().is_empty() {
            store.undo(current.revision_id(), revision("restore-for-ab"))?;
            store.snapshot()?
        } else {
            current
        };
        assert_eq!(current.beat_sounds().len(), 1);
        let slice = CapturedEditSlice::capture_selection_with(
            &current,
            current.root(),
            &SliceCaptureSelection::Child {
                node: owner.clone(),
            },
            attachments,
            AudioTimingId {
                allocation: revision(&format!("capture-{register}")),
                ordinal: 0,
            },
        )?;
        let request = edit(
            &current,
            &format!("cut-{register}"),
            Command::DeleteRipple {
                node: owner.clone(),
                timing: AudioTimingId {
                    allocation: revision(&format!("cut-{register}")),
                    ordinal: 0,
                },
            },
        );
        store.cut_to_register(
            &request,
            RegisterName::new(register)?,
            Arc::new(slice),
            None,
        )?;
        let after = store.snapshot()?;
        assert!(after.beat_sounds().is_empty());
        assert!(!after.nodes().contains_key(&owner));
        removed.push(after);
    }
    // Both cuts removed the same authored content.
    assert_eq!(removed[0].nodes(), removed[1].nodes());
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let bank = store.registers()?;
    let slice = |register: char| -> Result<CapturedEditSlice> {
        match bank.entries[&RegisterName::new(register)?].as_ref() {
            RegisterValue::Edited { slice } => Ok(slice.as_ref().clone()),
            _ => panic!("edited register expected"),
        }
    };
    let (inner, around) = (slice('i')?, slice('a')?);
    assert_eq!(inner.attachments(), SliceAttachments::Excluded);
    assert_eq!(around.attachments(), SliceAttachments::Owned);
    assert_eq!(inner.range(), around.range());
    for (copy, sounds) in [(&inner, 0), (&around, 1)] {
        let destination = store.snapshot()?;
        let request = paste(&destination, copy, &format!("paste-{sounds}"))?;
        store.commit(&request)?;
        let pasted = store.snapshot()?;
        assert_eq!(pasted.beat_sounds().len(), sounds);
        store.undo(pasted.revision_id(), revision(&format!("unpaste-{sounds}")))?;
    }
    assert_eq!(sounded.beat_sounds().len(), 1);
    store.validate()?;
    Ok(())
}
