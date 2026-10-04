//! Speech activity is a durable annotation outside document history.
use std::error::Error;

use deadpan_analysis::{ActivityAudio, SpeechActivity};
use deadpan_core::{NodeId, ProjectDocument, ProjectId, RevisionId};
use deadpan_store::{
    AccessMode, DATABASE_SCHEMA_VERSION, MAX_SPEECH_ACTIVITY, ProjectStore, SpeechActivityKey,
    StoreError,
};
use rusqlite::Connection;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

/// One second of analysis audio: speech, then a quiet half second.
fn activity(origin: i64) -> SpeechActivity {
    let audio = ActivityAudio {
        origin,
        sample_rate: 48_000,
        samples: 16_000,
    };
    let speech = (0..32).map(|hop| if hop < 16 { 230 } else { 5 }).collect();
    let energy = (0..100)
        .map(|frame| if frame < 50 { 180 } else { 20 })
        .collect();
    SpeechActivity::new(audio, speech, energy).unwrap()
}

fn key(engine: &str) -> SpeechActivityKey {
    SpeechActivityKey {
        content: "blake3:original".into(),
        audio_stream: 1,
        model_sha256: "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987".into(),
        engine: engine.into(),
    }
}

fn project(scratch: &tempfile::TempDir) -> Result<std::path::PathBuf> {
    let path = scratch.path().join("activity.deadpan");
    let document = ProjectDocument::new_automatic(
        ProjectId::new("activity")?,
        RevisionId::new("base")?,
        NodeId::new("root")?,
    )?;
    drop(ProjectStore::create(&path, &document)?);
    Ok(path)
}

#[test]
fn activity_saves_replaces_and_reopens_without_touching_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = project(&scratch)?;
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let head = store.head_revision()?;
    store.save_speech_activity(&key("whisper.cpp 1.8.3"), &activity(0))?;
    assert_eq!(
        store.speech_activity(&key("whisper.cpp 1.8.3"))?,
        Some(activity(0))
    );
    store.save_speech_activity(&key("whisper.cpp 1.8.3"), &activity(1_024))?;
    store.save_speech_activity(&key("other"), &activity(0))?;
    assert_eq!(store.head_revision()?, head);
    assert!(store.preview_undo(&head, RevisionId::new("undo")?).is_err());
    drop(store);

    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    reader.validate()?;
    let stored = reader.speech_activity(&key("whisper.cpp 1.8.3"))?.unwrap();
    assert_eq!(stored, activity(1_024));
    assert_eq!(stored.pauses().len(), 1);
    assert_eq!(
        reader.speech_activity_keys_for_content("blake3:original")?,
        [key("other"), key("whisper.cpp 1.8.3")]
    );
    assert!(
        reader
            .speech_activity_keys_for_content("blake3:other")?
            .is_empty()
    );
    assert!(reader.speech_activity(&key("missing"))?.is_none());
    assert!(
        reader
            .save_speech_activity(&key("missing"), &activity(0))
            .is_err()
    );
    Ok(())
}

#[test]
fn activity_keys_are_validated_and_the_count_is_bounded() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = project(&scratch)?;
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    for invalid in [
        SpeechActivityKey {
            model_sha256: "ABC".into(),
            ..key("e")
        },
        SpeechActivityKey {
            content: String::new(),
            ..key("e")
        },
        SpeechActivityKey {
            engine: "x\u{1}".into(),
            ..key("e")
        },
    ] {
        assert!(store.save_speech_activity(&invalid, &activity(0)).is_err());
    }
    for index in 0..MAX_SPEECH_ACTIVITY {
        store.save_speech_activity(&key(&format!("e{index}")), &activity(0))?;
    }
    assert!(
        store
            .save_speech_activity(&key("one-too-many"), &activity(0))
            .is_err()
    );
    store.save_speech_activity(&key("e0"), &activity(5))?;
    Ok(())
}

#[test]
fn tampered_stored_activity_fails_on_read() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = project(&scratch)?;
    {
        let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        store.save_speech_activity(&key("e"), &activity(0))?;
    }
    let connection = Connection::open(path.join("project.sqlite"))?;
    // One probability too few for the recorded length.
    connection.execute("UPDATE speech_activity SET speech=substr(speech,2)", [])?;
    drop(connection);
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert!(reader.speech_activity(&key("e")).is_err());
    // Keys list without reading, so a caller can skip the unreadable row.
    assert_eq!(
        reader.speech_activity_keys_for_content("blake3:original")?,
        [key("e")]
    );
    drop(reader);
    let connection = Connection::open(path.join("project.sqlite"))?;
    connection.execute("UPDATE speech_activity SET samples=?1", [i64::MAX])?;
    drop(connection);
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::Integrity(_))
    ));
    Ok(())
}

#[test]
fn a_schema59_package_is_read_without_activity_and_upgraded_by_its_writer() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = project(&scratch)?;
    {
        let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        store.save_transcript(
            &deadpan_store::TranscriptKey {
                content: "blake3:original".into(),
                audio_stream: 1,
                model_sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002"
                    .into(),
                language: "en".into(),
                engine: "whisper.cpp 1.8.3".into(),
            },
            &deadpan_analysis::Transcript::new(
                deadpan_analysis::AnalysedAudio {
                    origin: 0,
                    sample_rate: 48_000,
                    duration_cs: 100,
                },
                Vec::new(),
            )?,
        )?;
    }
    // Recreate the schema-59 layout: no speech activity table.
    let connection = Connection::open(path.join("project.sqlite"))?;
    connection.execute_batch(
        "DROP TABLE original_provenance; DROP TABLE speech_activity; DROP TABLE shot_analysis; PRAGMA user_version=59;",
    )?;
    drop(connection);
    let version = |path: &std::path::Path| -> Result<u32> {
        Ok(
            Connection::open(path.join("project.sqlite"))?.pragma_query_value(
                None,
                "user_version",
                |row| row.get(0),
            )?,
        )
    };

    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert!(reader.speech_activity(&key("e"))?.is_none());
    assert!(
        reader
            .speech_activity_keys_for_content("blake3:original")?
            .is_empty()
    );
    drop(reader);
    assert_eq!(version(&path)?, 59);

    let writer = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(version(&path)?, DATABASE_SCHEMA_VERSION);
    assert_eq!(
        writer.transcript_keys_for_content("blake3:original")?.len(),
        1
    );
    writer.save_speech_activity(&key("e"), &activity(0))?;
    drop(writer);
    // Opening again is a no-op upgrade.
    let writer = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(writer.speech_activity(&key("e"))?, Some(activity(0)));
    drop(writer);

    // The migration entrypoint upgrades the same way and reports it.
    let connection = Connection::open(path.join("project.sqlite"))?;
    connection.execute_batch(
        "DROP TABLE original_provenance; DROP TABLE speech_activity; DROP TABLE shot_analysis; PRAGMA user_version=59;",
    )?;
    drop(connection);
    let outcome = ProjectStore::migrate(&path)?;
    assert_eq!(
        (outcome.from_schema, outcome.to_schema),
        (59, DATABASE_SCHEMA_VERSION)
    );
    assert!(outcome.backup.is_none());
    assert_eq!(version(&path)?, DATABASE_SCHEMA_VERSION);
    Ok(())
}
