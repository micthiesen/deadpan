//! Manual analysis corrections are durable, versioned and undoable outside
//! document history.
use std::error::Error;

use deadpan_analysis::{AnalysedAudio, CorrectedTranscript, Corrections, Transcript, Word};
use deadpan_core::{NodeId, ProjectDocument, ProjectId, RevisionId};
use deadpan_store::{
    AccessMode, CorrectionChange, CorrectionsKey, MAX_CORRECTION_UNDO, ProjectStore, StoreError,
};
use rusqlite::Connection;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn transcript() -> Transcript {
    let word = |text: &str, start_cs, end_cs| Word {
        text: text.into(),
        start_cs,
        end_cs,
        probability: 0.8,
        segment: 0,
    };
    Transcript::new(
        AnalysedAudio {
            origin: 0,
            sample_rate: 48_000,
            duration_cs: 500,
        },
        vec![word("to", 10, 20), word("day", 21, 40), word("now", 50, 90)],
    )
    .unwrap()
}

fn key() -> CorrectionsKey {
    CorrectionsKey {
        content: "blake3:original".into(),
        audio_stream: 1,
    }
}

fn project(scratch: &tempfile::TempDir) -> Result<std::path::PathBuf> {
    let path = scratch.path().join("corrections.deadpan");
    let document = ProjectDocument::new_automatic(
        ProjectId::new("corrections")?,
        RevisionId::new("base")?,
        NodeId::new("root")?,
    )?;
    drop(ProjectStore::create(&path, &document)?);
    Ok(path)
}

fn merged() -> Corrections {
    let recognized = transcript();
    let none = Corrections::empty(Corrections::clock_of(&recognized));
    none.merge_words(&CorrectedTranscript::recognized(recognized), 0)
        .unwrap()
}

#[test]
fn corrections_apply_undo_redo_and_reopen_without_touching_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = project(&scratch)?;
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let head = store.head_revision()?;
    assert_eq!(store.analysis_corrections(&key())?, None);

    let first = merged();
    let saved = store.change_analysis_corrections(
        &key(),
        0,
        CorrectionChange::Apply {
            corrections: first.clone(),
            label: "Join “to” and “day”".into(),
        },
    )?;
    assert_eq!(saved.version, 1);
    assert_eq!(saved.corrections, first);
    assert_eq!(saved.undo.as_deref(), Some("Join “to” and “day”"));
    assert_eq!(saved.redo, None);

    // A stale version is refused without writing.
    let stale = store.change_analysis_corrections(&key(), 0, CorrectionChange::Undo);
    assert!(matches!(stale, Err(StoreError::AnalysisCorrections(_))));

    let undone = store.change_analysis_corrections(&key(), 1, CorrectionChange::Undo)?;
    assert_eq!(undone.version, 2);
    assert!(undone.corrections.is_empty());
    assert_eq!(undone.undo, None);
    assert_eq!(undone.redo.as_deref(), Some("Join “to” and “day”"));
    assert!(
        store
            .change_analysis_corrections(&key(), 2, CorrectionChange::Undo)
            .is_err()
    );
    let redone = store.change_analysis_corrections(&key(), 2, CorrectionChange::Redo)?;
    assert_eq!(redone.corrections, first);
    assert_eq!(redone.version, 3);

    // Corrections are not edits.
    assert_eq!(store.head_revision()?, head);
    drop(store);
    let reopened = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    reopened.validate_full()?;
    let stored = reopened.analysis_corrections(&key())?.unwrap();
    assert_eq!((stored.corrections, stored.version), (first, 3));
    assert!(matches!(
        reopened.change_analysis_corrections(&key(), 3, CorrectionChange::Undo),
        Err(StoreError::ReadOnly)
    ));
    Ok(())
}

#[test]
fn a_new_change_clears_redo_and_undo_is_bounded() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = project(&scratch)?;
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let clock = Corrections::clock_of(&transcript());
    let mut version = 0;
    for step in 0..(MAX_CORRECTION_UNDO + 5) {
        let corrections = if step % 2 == 0 {
            merged()
        } else {
            Corrections::empty(clock)
        };
        version = store
            .change_analysis_corrections(
                &key(),
                version,
                CorrectionChange::Apply {
                    corrections,
                    label: format!("step {step}"),
                },
            )?
            .version;
    }
    let mut undone = 0;
    while let Ok(stored) =
        store.change_analysis_corrections(&key(), version, CorrectionChange::Undo)
    {
        version = stored.version;
        undone += 1;
    }
    assert_eq!(undone, MAX_CORRECTION_UNDO);
    let redo = store.change_analysis_corrections(&key(), version, CorrectionChange::Redo)?;
    let applied = store.change_analysis_corrections(
        &key(),
        redo.version,
        CorrectionChange::Apply {
            corrections: merged(),
            label: "again".into(),
        },
    )?;
    assert_eq!(applied.redo, None);
    Ok(())
}

#[test]
fn damaged_corrections_fail_validation_and_can_be_discarded() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = project(&scratch)?;
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    for (version, label) in [(0, "first"), (1, "second")] {
        store.change_analysis_corrections(
            &key(),
            version,
            CorrectionChange::Apply {
                corrections: merged(),
                label: label.into(),
            },
        )?;
    }
    drop(store);
    let database = Connection::open(path.join("project.sqlite"))?;
    database.execute(
        "UPDATE analysis_corrections SET value=json_set(value,'$.rule','other')",
        [],
    )?;
    // Corrupt the newest Undo step too; the oldest stays readable.
    database.execute(
        "UPDATE analysis_correction_steps SET value=json_set(value,'$.rule','other')
            WHERE stack='undo' AND position=(SELECT max(position) FROM analysis_correction_steps)",
        [],
    )?;
    drop(database);

    // Opening tolerates them; explicit validation and reads report them.
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert!(store.analysis_corrections(&key()).is_err());
    assert!(store.validate().is_err());
    let unreadable = store.unreadable_analysis_corrections()?;
    let mut places: Vec<_> = unreadable.iter().map(|bad| bad.place).collect();
    places.sort_unstable();
    assert_eq!(places, ["current", "undo"]);

    let clock = Corrections::clock_of(&transcript());
    assert!(
        store
            .change_analysis_corrections(&key(), 1, CorrectionChange::DiscardUnreadable { clock })
            .is_err(),
        "a stale version is refused"
    );
    let discarded = store.change_analysis_corrections(
        &key(),
        2,
        CorrectionChange::DiscardUnreadable { clock },
    )?;
    assert!(discarded.corrections.is_empty());
    assert_eq!(discarded.version, 3);
    assert_eq!(discarded.undo.as_deref(), Some("first"));
    store.validate()?;
    // Undo reaches the last readable state.
    let undone = store.change_analysis_corrections(&key(), 3, CorrectionChange::Undo)?;
    assert!(undone.corrections.is_empty());
    assert!(matches!(
        store.change_analysis_corrections(
            &key(),
            undone.version,
            CorrectionChange::DiscardUnreadable { clock }
        ),
        Err(StoreError::AnalysisCorrections(_))
    ));
    Ok(())
}
