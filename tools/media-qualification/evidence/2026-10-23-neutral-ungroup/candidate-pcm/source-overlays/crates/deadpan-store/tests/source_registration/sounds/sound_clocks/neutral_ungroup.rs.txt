//! Durable neutral removal of an explicitly unpaired historical wrapper.
use super::introduced_groups::{group, setup, tables};
use super::*;
use deadpan_core::{AudioBindingState, GainDb};
use std::collections::BTreeSet;

fn node(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn ungroup(document: &ProjectDocument) -> CommandRequest {
    edit(
        document,
        "neutral-ungroup",
        Command::Ungroup {
            node: node("new-g"),
        },
    )
}

fn media_bytes(store: &ProjectStore) -> Result<BTreeMap<String, Vec<u8>>> {
    let records = store.original_records(None, 10)?;
    assert_eq!(records.len(), 2);
    records
        .into_iter()
        .map(|record| {
            let mut snapshot =
                store.snapshot_original(record.object().content(), limits(), &active())?;
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut snapshot, &mut bytes)?;
            Ok((record.object().content().to_string(), bytes))
        })
        .collect()
}

fn bindings_without_group(document: &ProjectDocument) -> Result<AudioBindingState> {
    let mut wire = serde_json::to_value(document.audio_bindings())?;
    let clocks = wire["sound_clocks"].as_object_mut().unwrap();
    for events in clocks.values_mut() {
        for journal in events.as_object_mut().unwrap().values_mut() {
            for reference in journal["clocks"].as_array_mut().unwrap() {
                let mut groups: BTreeSet<NodeId> =
                    serde_json::from_value(reference["introduced_groups"].clone())?;
                assert!(groups.remove(&node("new-g")));
                if groups.is_empty() {
                    reference
                        .as_object_mut()
                        .unwrap()
                        .remove("introduced_groups");
                } else {
                    reference["introduced_groups"] = serde_json::to_value(groups)?;
                }
            }
        }
    }
    Ok(AudioBindingState::from_json(&wire.to_string())?)
}

fn prepare_grouped(store: &mut ProjectStore) -> Result<ProjectDocument> {
    store.commit(&group(&store.snapshot()?)?)?;
    let grouped = store.snapshot()?;
    store.commit(&edit(
        &grouped,
        "later-group-label",
        Command::Rename {
            node: node("new-g"),
            label: "Later group label".into(),
        },
    ))?;
    let renamed = store.snapshot()?;
    let mut event = renamed.beat_sounds()[&node("owner")][&SoundId::new("overlay")?].clone();
    event.label = "Later sound parameters".into();
    event.treatments = event.treatments.with_trim(GainDb::new(-4500)?)?;
    store.commit(&edit(
        &renamed,
        "later-sound-parameters",
        Command::SetBeatSound {
            owner: node("owner"),
            id: SoundId::new("overlay")?,
            event,
        },
    ))?;
    let parameters = store.snapshot()?;
    store.commit(&edit(
        &parameters,
        "later-sibling-label",
        Command::Rename {
            node: node("tail"),
            label: "Later sibling label".into(),
        },
    ))?;
    let current = store.snapshot()?;
    assert_eq!(current.audio_bindings(), grouped.audio_bindings());
    let journal =
        &current.audio_bindings().sound_clocks()[&node("owner")][&SoundId::new("overlay")?];
    assert_eq!(journal.scope(), &node("top"));
    assert_eq!(journal.clocks().len(), 1);
    assert_eq!(
        journal.clocks()[0].introduced_groups(),
        &BTreeSet::from([node("new-g")])
    );
    Ok(current)
}

#[test]
fn neutral_ungroup_is_atomic_durable_and_portable_after_intervening_edits() -> Result {
    for source_owner in [true, false] {
        let scratch = tempfile::tempdir()?;
        let (path, mut store) = setup(scratch.path(), source_owner)?;
        let unwrapped = store.snapshot()?;
        let bytes = media_bytes(&store)?;
        // Keep actual captured media and register bytes alongside the history.
        let seed = capture_child(&unwrapped, &node("top"), &node("owner"), "neutral-seed")?;
        let bank = store.save_register(
            unwrapped.project_id(),
            unwrapped.revision_id(),
            RegisterName::new('z')?,
            RegisterValue::Edited {
                slice: Arc::new(seed),
            },
        )?;
        let saved_register_rows = register_rows(&path)?;
        let before = prepare_grouped(&mut store)?;
        let expected_bindings = bindings_without_group(&before)?;
        let rows = tables(&path)?;
        let command = ungroup(&before);
        let preview = store.preview(&command)?;
        assert_eq!(store.snapshot()?, before);
        assert_eq!(tables(&path)?, rows);
        assert_eq!(store.registers()?, bank);
        let database = Connection::open(path.join("project.sqlite"))?;
        database.execute_batch(
            "CREATE TRIGGER fail_neutral_ungroup BEFORE INSERT ON history BEGIN
             SELECT RAISE(FAIL,'forced neutral ungroup history failure'); END;",
        )?;
        assert!(store.commit(&command).is_err());
        assert_eq!(store.snapshot()?, before);
        assert_eq!(tables(&path)?, rows);
        assert_eq!(store.registers()?, bank);
        database.execute_batch("DROP TRIGGER fail_neutral_ungroup")?;
        drop(database);
        assert_eq!(store.commit(&command)?.edit, preview);
        let after = store.snapshot()?;
        assert!(!after.nodes().contains_key(&node("new-g")));
        assert_eq!(after.duration()?, before.duration()?);
        assert_eq!(after.audio_bindings(), &expected_bindings);
        assert_eq!(after.beat_sounds(), before.beat_sounds());
        assert_eq!(after.nodes()[&node("top")], unwrapped.nodes()[&node("top")]);
        for name in ["owner", "tail", "lead", "prefix"] {
            assert_eq!(after.nodes()[&node(name)], before.nodes()[&node(name)]);
        }
        assert_eq!(preview.inverse.apply(&after)?, before);
        assert_eq!(ProjectDocument::from_json(&after.to_json()?)?, after);
        let after_rows = tables(&path)?;
        for table in [
            "source_qualifications",
            "original_media",
            "register_contents",
        ] {
            assert_eq!(after_rows[table], rows[table], "{table}");
        }
        assert_eq!(register_rows(&path)?, saved_register_rows);
        assert_eq!(media_bytes(&store)?, bytes);
        store.validate()?;
        drop(store);

        let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        assert_eq!(store.snapshot()?, after);
        assert_eq!(store.registers()?, bank);
        store.undo(after.revision_id(), revision("undo-neutral-ungroup"))?;
        assert_authored(&store.snapshot()?, &before)?;
        store.redo(
            &revision("undo-neutral-ungroup"),
            revision("redo-neutral-ungroup"),
        )?;
        let redone = store.snapshot()?;
        assert_authored(&redone, &after)?;
        assert_eq!(redone.audio_bindings(), &expected_bindings);
        assert_eq!(store.registers()?, bank);
        assert_eq!(register_rows(&path)?, saved_register_rows);
        assert_eq!(media_bytes(&store)?, bytes);
        store.validate()?;
        drop(store);

        let destination = scratch.path().join("NeutralPortable.deadpan");
        let report = deadpan_store::portable::copy_portable(&path, &destination, &active())?;
        assert_eq!(report.revision_id, redone.revision_id().as_str());
        let portable = ProjectStore::open(&destination, AccessMode::ReadOnly)?;
        assert_eq!(portable.snapshot()?, redone);
        assert_eq!(portable.registers()?, bank);
        assert_eq!(register_rows(&destination)?, saved_register_rows);
        assert_eq!(media_bytes(&portable)?, bytes);
        let portable_rows = tables(&destination)?;
        assert_eq!(
            portable_rows["source_qualifications"],
            rows["source_qualifications"]
        );
        portable.validate()?;
    }
    Ok(())
}

#[test]
fn neutral_ungroup_rechecks_unchanged_sound_media_before_commit() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = setup(scratch.path(), false)?;
    let before = prepare_grouped(&mut store)?;
    let command = ungroup(&before);
    store.preview(&command)?;
    let qualification = before.assets()[&id("event")]
        .source_qualification
        .as_ref()
        .unwrap();
    let database = Connection::open(path.join("project.sqlite"))?;
    let saved: Vec<u8> = database.query_row(
        "SELECT snapshot FROM source_qualifications WHERE id=?1",
        [qualification.as_str()],
        |row| row.get(0),
    )?;
    database.execute(
        "UPDATE source_qualifications SET snapshot=X'00' WHERE id=?1",
        [qualification.as_str()],
    )?;
    let damaged_rows = tables(&path)?;
    assert!(matches!(
        store.preview(&command),
        Err(StoreError::SourceRegistration(_))
    ));
    assert!(matches!(
        store.commit(&command),
        Err(StoreError::SourceRegistration(_))
    ));
    assert_eq!(store.snapshot()?, before);
    assert_eq!(tables(&path)?, damaged_rows);
    database.execute(
        "UPDATE source_qualifications SET snapshot=?1 WHERE id=?2",
        rusqlite::params![saved, qualification.as_str()],
    )?;
    store.preview(&command)?;
    let (content, original): (String, String) = database.query_row(
        "SELECT original_media.content_id,original_media.record FROM original_media
         JOIN source_qualifications ON original_media.content_id=source_qualifications.original_content_id
         WHERE source_qualifications.id=?1",
        [qualification.as_str()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let mut changed: serde_json::Value = serde_json::from_str(&original)?;
    changed["sha256"] = serde_json::to_value([9_u8; 32])?;
    database.execute(
        "UPDATE original_media SET record=?1 WHERE content_id=?2",
        rusqlite::params![changed.to_string(), content],
    )?;
    let damaged_rows = tables(&path)?;
    assert!(matches!(
        store.preview(&command),
        Err(StoreError::SourceRegistration(_))
    ));
    assert!(matches!(
        store.commit(&command),
        Err(StoreError::SourceRegistration(_))
    ));
    assert_eq!(store.snapshot()?, before);
    assert_eq!(tables(&path)?, damaged_rows);
    database.execute(
        "UPDATE original_media SET record=?1 WHERE content_id=?2",
        rusqlite::params![original, content],
    )?;
    drop(database);
    store.preview(&command)?;
    store.commit(&command)?;
    store.validate()?;
    Ok(())
}
