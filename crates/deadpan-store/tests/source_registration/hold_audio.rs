use super::*;
use deadpan_core::{
    AssetRecord, AudioTimingId, FrameDuration, HoldAudio, HoldRecipe, HoldVideo, InstancePath,
    OccurrenceEdit, OccurrenceIdentities, ProjectFrame, SourceAudio, SourceSpan, SourceTimeBase,
    SourceTimestamp, SplitIdentities,
};

fn edit(document: &ProjectDocument, next: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(next),
        command,
    }
}

fn insert_hold(store: &mut ProjectStore, audio: HoldAudio) -> Result {
    store.commit(&edit(
        &store.snapshot()?,
        "hold",
        Command::InsertTime {
            at: ProjectFrame(0),
            hold: HoldRecipe {
                duration: FrameDuration::new(3)?,
                video: HoldVideo::Background,
                audio,
                picture_context: None,
            },
            id: NodeId::new("hold")?,
            identities: SplitIdentities::default(),
            timing: AudioTimingId {
                allocation: revision("hold"),
                ordinal: 0,
            },
        },
    ))?;
    Ok(())
}

fn source(document: &ProjectDocument) -> Result<SourceAudio> {
    let full = document.assets()[&id("camera")].audio.unwrap();
    Ok(SourceAudio {
        asset: id("camera"),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: full.start().ticks + 17,
                time_base: full.start().time_base,
            },
            SourceTimestamp {
                ticks: full.start().ticks + 113,
                time_base: full.start().time_base,
            },
        )?,
    })
}

fn setter(audio: HoldAudio, occurrence: bool) -> Result<Command> {
    let node = NodeId::new("hold")?;
    Ok(if occurrence {
        Command::EditOccurrence {
            instance: InstancePath {
                node,
                repeats: Vec::new(),
            },
            edit: OccurrenceEdit::SetHoldAudio { audio },
            identities: OccurrenceIdentities::default(),
        }
    } else {
        Command::SetHoldAudio { node, audio }
    })
}

#[test]
fn hold_audio_policy_history_is_atomic_durable_and_timing_neutral() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "offset-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    let input = request(&store, &original, "import", "camera", Some("clip"))?;
    store.register_source(&input, &decoded, None, limits(), &active())?;
    insert_hold(&mut store, HoldAudio::Silence)?;
    let baseline = store.snapshot()?;
    let selected = source(&baseline)?;
    let request = edit(
        &baseline,
        "room",
        setter(
            HoldAudio::RoomTone {
                source: selected.clone(),
            },
            false,
        )?,
    );
    let preview = store.preview(&request)?;
    assert_eq!(store.snapshot()?, baseline);
    let baseline_counts = counts(&path)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    database.execute_batch("CREATE TRIGGER fail_hold_history BEFORE INSERT ON history BEGIN SELECT RAISE(FAIL,'forced Hold history failure'); END;")?;
    assert!(store.commit(&request).is_err());
    assert_eq!(store.snapshot()?, baseline);
    assert_eq!(counts(&path)?, baseline_counts);
    database.execute_batch("DROP TRIGGER fail_hold_history")?;
    assert_eq!(store.commit(&request)?.edit, preview);
    let room = store.snapshot()?;
    assert_eq!(room.duration()?, baseline.duration()?);
    assert_eq!(
        room.nodes()[&NodeId::new("clip")?],
        baseline.nodes()[&NodeId::new("clip")?]
    );
    assert_eq!(preview.inverse.apply(&room)?, baseline);
    assert!(store.commit(&request).is_err());
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, room);
    store.commit(&edit(
        &room,
        "tail",
        setter(
            HoldAudio::Tail {
                source: selected,
                maximum: FrameDuration::new(1)?,
            },
            true,
        )?,
    ))?;
    let tail = store.snapshot()?;
    store.commit(&edit(&tail, "silence", setter(HoldAudio::Silence, false)?))?;
    let silent = store.snapshot()?;
    assert_eq!(silent.nodes(), baseline.nodes());
    assert_eq!(silent.audio_bindings(), baseline.audio_bindings());
    store.undo(silent.revision_id(), revision("undo-silence"))?;
    assert_eq!(store.snapshot()?.nodes(), tail.nodes());
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.redo(store.snapshot()?.revision_id(), revision("redo-silence"))?;
    assert_eq!(store.snapshot()?.nodes(), silent.nodes());
    store.validate()?;
    drop(store);
    ProjectStore::open(&path, AccessMode::ReadOnly)?.validate()?;
    Ok(())
}

#[test]
fn direct_and_occurrence_policy_commands_recheck_receipt_and_original_binding() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "offset-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    let input = request(&store, &original, "import", "camera", Some("clip"))?;
    store.register_source(&input, &decoded, None, limits(), &active())?;
    insert_hold(&mut store, HoldAudio::Silence)?;
    let baseline = store.snapshot()?;
    let baseline_counts = counts(&path)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let (receipt, original_ref): (Vec<u8>, String) = database.query_row(
        "SELECT snapshot,original_ref FROM source_qualifications",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    for occurrence in [false, true] {
        for audio in [
            HoldAudio::RoomTone {
                source: source(&baseline)?,
            },
            HoldAudio::Tail {
                source: source(&baseline)?,
                maximum: FrameDuration::new(1)?,
            },
        ] {
            let request = edit(&baseline, "policy", setter(audio, occurrence)?);
            store.preview(&request)?;
            for tamper in [
                "UPDATE source_qualifications SET snapshot=X'00'",
                "UPDATE source_qualifications SET original_ref=json_set(original_ref,'$.byte_length',1234)",
            ] {
                database.execute(tamper, [])?;
                assert!(matches!(
                    store.preview(&request),
                    Err(StoreError::SourceRegistration(_))
                ));
                assert!(matches!(
                    store.commit(&request),
                    Err(StoreError::SourceRegistration(_))
                ));
                assert_eq!(store.snapshot()?, baseline);
                assert_eq!(counts(&path)?, baseline_counts);
                database.execute(
                    "UPDATE source_qualifications SET snapshot=?1,original_ref=?2",
                    rusqlite::params![receipt, original_ref],
                )?;
            }
        }
    }
    store.validate()?;
    Ok(())
}

#[test]
fn hold_audio_rejects_outside_and_fractional_source_sample_spans_without_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "offset-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    let input = request(&store, &original, "import", "camera", Some("clip"))?;
    store.register_source(&input, &decoded, None, limits(), &active())?;
    insert_hold(&mut store, HoldAudio::Silence)?;
    let before = store.snapshot()?;
    let baseline_counts = counts(&path)?;
    let full = before.assets()[&id("camera")].audio.unwrap();
    let half_sample = SourceTimeBase::new(1, full.start().time_base.denominator() * 2)?;
    for span in [
        SourceSpan::new(
            SourceTimestamp {
                ticks: full.start().ticks - 1,
                time_base: full.start().time_base,
            },
            full.end(),
        )?,
        SourceSpan::new(
            full.start(),
            SourceTimestamp {
                ticks: full.end().ticks + 1,
                time_base: full.end().time_base,
            },
        )?,
        SourceSpan::new(
            SourceTimestamp {
                ticks: full.start().ticks * 2 + 1,
                time_base: half_sample,
            },
            SourceTimestamp {
                ticks: full.start().ticks * 2 + 101,
                time_base: half_sample,
            },
        )?,
    ] {
        for occurrence in [false, true] {
            let command = setter(
                HoldAudio::RoomTone {
                    source: SourceAudio {
                        asset: id("camera"),
                        span,
                    },
                },
                occurrence,
            )?;
            let request = edit(&before, "invalid-span", command);
            assert!(store.preview(&request).is_err());
            assert!(store.commit(&request).is_err());
            assert_eq!(store.snapshot()?, before);
            assert_eq!(counts(&path)?, baseline_counts);
        }
    }
    Ok(())
}

#[test]
fn legacy_unqualified_hold_survives_unrelated_edits_but_cannot_author_a_new_source_policy() -> Result
{
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let time_base = SourceTimeBase::new(1, 96_000)?;
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 1,
            time_base,
        },
        SourceTimestamp {
            ticks: 101,
            time_base,
        },
    )?;
    store.commit(&edit(
        &store.snapshot()?,
        "legacy-asset",
        Command::AddAsset {
            id: id("camera"),
            asset: AssetRecord {
                label: "Legacy evidence-free audio".into(),
                content_hash: "a".repeat(64),
                video: None,
                audio: Some(span),
                still_image: false,
                frame_count: None,
                source_qualification: None,
            },
        },
    ))?;
    let audio = HoldAudio::RoomTone {
        source: SourceAudio {
            asset: id("camera"),
            span,
        },
    };
    insert_hold(&mut store, audio.clone())?;
    let legacy = store.snapshot()?;
    store.commit(&edit(
        &legacy,
        "rename",
        Command::Rename {
            node: NodeId::new("hold")?,
            label: "Still legacy".into(),
        },
    ))?;
    let before = store.snapshot()?;
    for occurrence in [false, true] {
        let request = edit(
            &before,
            "refuse-new-policy",
            setter(audio.clone(), occurrence)?,
        );
        assert!(matches!(
            store.preview(&request),
            Err(StoreError::SourceRegistration(_))
        ));
        assert!(matches!(
            store.commit(&request),
            Err(StoreError::SourceRegistration(_))
        ));
        assert_eq!(store.snapshot()?, before);
    }
    store.validate()?;
    // Matching modern core patches cannot bypass source admission on reopen.
    let forged = edit(&legacy, "rename", setter(audio, false)?);
    let transaction = deadpan_core::apply(&legacy, &forged)?;
    let next = transaction.forward.apply(&legacy)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    database.execute(
        "UPDATE revisions SET document=?1 WHERE id='rename'",
        [next.to_json()?],
    )?;
    database.execute(
        "UPDATE history SET request=?1,edit=?2 WHERE revision_id='rename'",
        [
            serde_json::to_string(&forged)?,
            serde_json::to_string(&transaction)?,
        ],
    )?;
    assert!(matches!(
        store.validate(),
        Err(StoreError::SourceRegistration(_))
    ));
    drop(store);
    assert!(ProjectStore::open(&path, AccessMode::ReadOnly).is_err());
    Ok(())
}
