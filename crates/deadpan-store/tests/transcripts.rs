//! Transcripts are durable annotations outside document history.
use std::error::Error;

use deadpan_analysis::{AnalysedAudio, Transcript, Word};
use deadpan_core::{NodeId, ProjectDocument, ProjectId, RevisionId};
use deadpan_store::{AccessMode, MAX_TRANSCRIPTS, ProjectStore, StoreError, TranscriptKey};
use rusqlite::Connection;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn transcript(words: &[(&str, u32, u32)]) -> Transcript {
    Transcript::new(
        AnalysedAudio {
            origin: 0,
            sample_rate: 48_000,
            duration_cs: 1_000,
        },
        words
            .iter()
            .map(|(text, start, end)| Word {
                text: (*text).into(),
                start_cs: *start,
                end_cs: *end,
                probability: 0.9,
                segment: 0,
            })
            .collect(),
    )
    .unwrap()
}

fn key(language: &str) -> TranscriptKey {
    TranscriptKey {
        content: "blake3:original".into(),
        audio_stream: 1,
        model_sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002".into(),
        language: language.into(),
        engine: "whisper.cpp 1.8.3".into(),
    }
}

fn project(scratch: &tempfile::TempDir) -> Result<std::path::PathBuf> {
    let path = scratch.path().join("transcripts.deadpan");
    let document = ProjectDocument::new_automatic(
        ProjectId::new("transcripts")?,
        RevisionId::new("base")?,
        NodeId::new("root")?,
    )?;
    drop(ProjectStore::create(&path, &document)?);
    Ok(path)
}

#[test]
fn transcripts_save_replace_and_reopen_without_touching_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = project(&scratch)?;
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let head = store.head_revision()?;
    let first = transcript(&[("hello", 10, 40)]);
    store.save_transcript(&key("en"), &first)?;
    assert_eq!(store.transcript(&key("en"))?, Some(first));
    let replaced = transcript(&[("hello", 10, 40), ("again", 50, 90)]);
    store.save_transcript(&key("en"), &replaced)?;
    store.save_transcript(&key("auto"), &transcript(&[("hallo", 10, 40)]))?;
    assert_eq!(store.head_revision()?, head);
    // No history entry exists to undo after saving annotations.
    assert!(store.preview_undo(&head, RevisionId::new("undo")?).is_err());
    drop(store);

    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    reader.validate()?;
    assert_eq!(reader.transcript(&key("en"))?, Some(replaced));
    let all = reader.transcripts_for_content("blake3:original")?;
    assert_eq!(
        all.iter()
            .map(|(key, _)| key.language.as_str())
            .collect::<Vec<_>>(),
        ["auto", "en"]
    );
    assert!(reader.transcripts_for_content("blake3:other")?.is_empty());
    assert!(reader.transcript(&key("de"))?.is_none());
    assert!(
        reader
            .save_transcript(&key("de"), &transcript(&[]))
            .is_err()
    );
    Ok(())
}

#[test]
fn guarded_transcripts_reject_new_and_replacement_writes_without_history_changes() -> Result {
    for existing in [false, true] {
        for reject_at in [1, 2] {
            let scratch = tempfile::tempdir()?;
            let path = project(&scratch)?;
            let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
            let key = key("en");
            let previous = transcript(&[("before", 10, 40)]);
            let replacement = transcript(&[("after", 50, 90)]);
            if existing {
                store.save_transcript(&key, &previous)?;
            }
            let head = store.head_revision()?;
            let mut calls = 0;
            let result = store.save_transcript_guarded(&key, &replacement, || {
                calls += 1;
                if calls == 2 {
                    // The final guard sees the pending write before rejecting it.
                    assert_eq!(store.transcript(&key)?, Some(replacement.clone()));
                }
                if calls == reject_at {
                    Err(StoreError::Integrity("publication interrupted".into()))
                } else {
                    Ok(())
                }
            });
            assert!(
                matches!(result, Err(StoreError::Integrity(detail)) if detail == "publication interrupted")
            );
            assert_eq!(calls, reject_at);
            assert_eq!(store.head_revision()?, head);
            assert!(store.preview_undo(&head, RevisionId::new("undo")?).is_err());
            let expected = existing.then_some(previous);
            assert_eq!(store.transcript(&key)?, expected);
            drop(store);
            let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
            assert_eq!(reader.transcript(&key)?, expected);
            assert_eq!(reader.head_revision()?, head);
        }
    }
    Ok(())
}

#[test]
fn guarded_transcripts_commit_when_both_checks_permit_publication() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = project(&scratch)?;
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let head = store.head_revision()?;
    for value in [
        transcript(&[("before", 10, 40)]),
        transcript(&[("after", 50, 90)]),
    ] {
        let mut calls = 0;
        store.save_transcript_guarded(&key("en"), &value, || {
            calls += 1;
            Ok(())
        })?;
        assert_eq!(calls, 2);
        assert_eq!(store.transcript(&key("en"))?, Some(value));
    }
    assert_eq!(store.head_revision()?, head);
    assert!(store.preview_undo(&head, RevisionId::new("undo")?).is_err());
    Ok(())
}

#[test]
fn keys_are_validated_and_the_count_is_bounded() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = project(&scratch)?;
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let empty = transcript(&[]);
    for invalid in [
        TranscriptKey {
            model_sha256: "ABC".into(),
            ..key("en")
        },
        TranscriptKey {
            content: String::new(),
            ..key("en")
        },
        TranscriptKey {
            engine: "x\u{1}".into(),
            ..key("en")
        },
    ] {
        assert!(store.save_transcript(&invalid, &empty).is_err());
    }
    for index in 0..MAX_TRANSCRIPTS {
        store.save_transcript(&key(&format!("l{index}")), &empty)?;
    }
    assert!(store.save_transcript(&key("one-too-many"), &empty).is_err());
    // Replacing an existing key is still allowed at the cap.
    store.save_transcript(&key("l0"), &transcript(&[("still", 0, 10)]))?;
    Ok(())
}

#[test]
fn tampered_stored_transcripts_fail_on_read_and_validation() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = project(&scratch)?;
    {
        let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        store.save_transcript(&key("en"), &transcript(&[("hello", 10, 40)]))?;
    }
    let connection = Connection::open(path.join("project.sqlite"))?;
    connection.execute(
        "UPDATE transcripts SET value=json_set(value,'$.words[0].end_cs',5000)",
        [],
    )?;
    drop(connection);
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert!(reader.transcript(&key("en")).is_err());
    assert!(reader.transcripts_for_content("blake3:original").is_err());
    // Keys list without parsing, so a caller can skip the unreadable row.
    assert_eq!(
        reader.transcript_keys_for_content("blake3:original")?,
        [key("en")]
    );
    Ok(())
}
