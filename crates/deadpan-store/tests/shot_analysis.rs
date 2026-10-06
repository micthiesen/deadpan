//! Shot analysis is a durable annotation outside document history.
use std::error::Error;

use deadpan_analysis::{
    MEASURE_BYTES, SIGNATURE_VERSION, ShotAnalysis, ShotMeasurer, ShotProgress, ShotProgressTail,
};
use deadpan_core::{NodeId, ProjectDocument, ProjectId, RevisionId};
use deadpan_store::{AccessMode, MAX_SHOT_ANALYSES, ProjectStore, ShotAnalysisKey, StoreError};
use rusqlite::Connection;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

/// Twenty quiet pictures with one cut at picture 10.
fn analysis(cut: u8) -> ShotAnalysis {
    let changes = (0..20)
        .map(|picture| match picture {
            0 => [0, 0, 0],
            10 => [cut, 200, cut],
            11 => [2, 1, cut],
            _ => [2, 1, 2],
        })
        .collect();
    ShotAnalysis::from_changes(changes).unwrap()
}

fn key(version: &str) -> ShotAnalysisKey {
    ShotAnalysisKey {
        content: "blake3:original".into(),
        video_stream: 0,
        signature_version: version.into(),
    }
}

fn project(scratch: &tempfile::TempDir) -> Result<std::path::PathBuf> {
    let path = scratch.path().join("shots.deadpan");
    let document = ProjectDocument::new_automatic(
        ProjectId::new("shots")?,
        RevisionId::new("base")?,
        NodeId::new("root")?,
    )?;
    drop(ProjectStore::create(&path, &document)?);
    Ok(path)
}

#[test]
fn shots_save_replace_and_reopen_without_touching_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = project(&scratch)?;
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let head = store.head_revision()?;
    store.save_shot_analysis(&key("older"), &analysis(90))?;
    store.save_shot_analysis(&key(SIGNATURE_VERSION), &analysis(90))?;
    let stored = store.shot_analysis(&key(SIGNATURE_VERSION), 20)?.unwrap();
    assert_eq!(stored, analysis(90));
    assert_eq!(stored.boundaries(), [10]);
    store.save_shot_analysis(&key(SIGNATURE_VERSION), &analysis(10))?;
    assert_eq!(store.head_revision()?, head);
    assert!(store.preview_undo(&head, RevisionId::new("undo")?).is_err());
    drop(store);

    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    reader.validate()?;
    let stored = reader.shot_analysis(&key(SIGNATURE_VERSION), 20)?.unwrap();
    assert_eq!(stored, analysis(10));
    assert!(stored.boundaries().is_empty());
    assert_eq!(
        reader.shot_analysis_keys_for_content("blake3:original")?,
        [key(SIGNATURE_VERSION)],
        "the current measurement replaced the older one"
    );
    assert!(
        reader
            .shot_analysis_keys_for_content("blake3:other")?
            .is_empty()
    );
    assert!(reader.shot_analysis(&key("missing"), 20)?.is_none());
    // A different expected picture count is an integrity failure.
    assert!(matches!(
        reader.shot_analysis(&key(SIGNATURE_VERSION), 21),
        Err(StoreError::Integrity(_))
    ));
    assert!(
        reader
            .save_shot_analysis(&key("missing"), &analysis(90))
            .is_err()
    );
    Ok(())
}

#[test]
fn shot_keys_are_validated_and_the_count_is_bounded() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = project(&scratch)?;
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    for invalid in [
        ShotAnalysisKey {
            content: String::new(),
            ..key("v")
        },
        ShotAnalysisKey {
            signature_version: "x\u{1}".into(),
            ..key("v")
        },
        ShotAnalysisKey {
            signature_version: "v".repeat(129),
            ..key("v")
        },
    ] {
        assert!(store.save_shot_analysis(&invalid, &analysis(90)).is_err());
    }
    let source = |index: usize| ShotAnalysisKey {
        content: format!("blake3:original-{index}"),
        ..key("v")
    };
    for index in 0..MAX_SHOT_ANALYSES as usize {
        store.save_shot_analysis(&source(index), &analysis(90))?;
    }
    assert!(
        store
            .save_shot_analysis(&source(MAX_SHOT_ANALYSES as usize), &analysis(90))
            .is_err()
    );
    store.save_shot_analysis(&source(0), &analysis(10))?;
    // An empty analysis (no pictures) round-trips.
    store.save_shot_analysis(&source(1), &ShotAnalysis::new(Vec::new())?)?;
    assert_eq!(store.shot_analysis(&source(1), 0)?.unwrap().pictures(), 0);
    Ok(())
}

#[test]
fn tampered_stored_shots_fail_on_read() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = project(&scratch)?;
    {
        let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        store.save_shot_analysis(&key("v"), &analysis(90))?;
    }
    let tamper = |sql: &str| -> Result {
        let connection = Connection::open(path.join("project.sqlite"))?;
        connection.execute(sql, [])?;
        Ok(())
    };
    // One byte short of two per picture.
    tamper("UPDATE shot_analysis SET changes=substr(changes,2)")?;
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert!(reader.shot_analysis(&key("v"), 20).is_err());
    // Keys list without reading, so a caller can skip the unreadable row.
    assert_eq!(
        reader.shot_analysis_keys_for_content("blake3:original")?,
        [key("v")]
    );
    drop(reader);
    // A first picture with a predecessor change fails ShotAnalysis::new.
    tamper(&format!(
        "UPDATE shot_analysis SET changes=X'01{}'",
        "00".repeat(20 * MEASURE_BYTES - 1)
    ))?;
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert!(matches!(
        reader.shot_analysis(&key("v"), 20),
        Err(StoreError::Integrity(_))
    ));
    drop(reader);
    tamper("UPDATE shot_analysis SET pictures=9223372036854775807")?;
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::Integrity(_))
    ));
    Ok(())
}

#[test]
fn a_schema60_package_is_refused_unchanged() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = project(&scratch)?;
    let downgrade = |path: &std::path::Path| -> Result {
        let connection = Connection::open(path.join("project.sqlite"))?;
        connection.execute_batch(
            "DROP TABLE original_provenance; DROP TABLE shot_analysis; PRAGMA user_version=60;",
        )?;
        Ok(())
    };
    let version = |path: &std::path::Path| -> Result<u32> {
        Ok(
            Connection::open(path.join("project.sqlite"))?.pragma_query_value(
                None,
                "user_version",
                |row| row.get(0),
            )?,
        )
    };
    downgrade(&path)?;
    // Database 64 refuses earlier development packages without writing.
    for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
        assert!(matches!(
            ProjectStore::open(&path, mode),
            Err(deadpan_store::StoreError::UnsupportedSchema(60))
        ));
    }
    assert!(matches!(
        ProjectStore::migrate(&path),
        Err(deadpan_store::StoreError::UnsupportedSchema(60))
    ));
    assert_eq!(version(&path)?, 60);
    Ok(())
}

/// Progress of a 120-picture scan stopped after `next` pictures, from
/// synthetic changes only.
fn progress(next: usize) -> ShotProgress {
    let analysis = analysis(90);
    ShotProgress::new(120, analysis.measures()[..next.min(20)].to_vec()).unwrap()
}

#[test]
fn scan_progress_is_saved_outside_history_and_removed_by_the_analysis() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = project(&scratch)?;
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let head = store.head_revision()?;
    let current = key(SIGNATURE_VERSION);
    assert!(store.shot_scan_progress(&current, 120)?.is_none());
    store.save_shot_scan_progress(&key("older"), &progress(5))?;
    store.save_shot_scan_progress(&current, &progress(10))?;
    store.save_shot_scan_progress(&current, &progress(20))?;
    assert_eq!(store.head_revision()?, head);
    // A newer measurement supersedes older progress of the same pictures.
    assert!(store.shot_scan_progress(&key("older"), 120)?.is_none());
    let stored = store.shot_scan_progress(&current, 120)?.unwrap();
    assert_eq!(stored, progress(20));
    assert_eq!(stored.next(), 20);
    // A resumed measurer accepts it.
    assert_eq!(ShotMeasurer::resume(stored)?.next_ordinal(), 0);
    assert!(matches!(
        store.shot_scan_progress(&current, 121),
        Err(StoreError::Integrity(_))
    ));
    drop(store);
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    reader.validate()?;
    assert_eq!(
        reader.shot_scan_progress(&current, 120)?,
        Some(progress(20))
    );
    assert!(
        reader
            .save_shot_scan_progress(&current, &progress(1))
            .is_err()
    );
    drop(reader);

    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let other = ShotAnalysisKey {
        content: "blake3:other".into(),
        ..current.clone()
    };
    store.save_shot_scan_progress(&other, &progress(3))?;
    store.save_shot_analysis(&current, &analysis(90))?;
    assert!(store.shot_scan_progress(&current, 120)?.is_none());
    assert!(store.shot_scan_progress(&other, 120)?.is_some());
    store.delete_shot_scan_progress(&other)?;
    assert!(store.shot_scan_progress(&other, 120)?.is_none());
    drop(store);

    // Tampering is refused on read.
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.save_shot_scan_progress(&current, &progress(20))?;
    drop(store);
    let connection = Connection::open(path.join("project.sqlite"))?;
    connection.execute("UPDATE shot_scan_progress SET next_picture=19", [])?;
    drop(connection);
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert!(reader.shot_scan_progress(&current, 120).is_err());
    Ok(())
}

#[test]
fn checkpoints_append_tails_and_keep_one_scan() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = project(&scratch)?;
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let current = key(SIGNATURE_VERSION);
    let whole = analysis(90).measures().to_vec();
    // Tails of the synthetic measures, which have no spans.
    let tail = |start: usize, end: usize| {
        ShotProgressTail::new(120, start, whole[start..end].to_vec()).unwrap()
    };
    // A tail that starts later than any saved progress is refused.
    assert!(matches!(
        store.append_shot_scan_progress(&current, &tail(5, 8)),
        Err(StoreError::Integrity(_))
    ));
    assert_eq!(store.append_shot_scan_progress(&current, &tail(0, 6))?, 6);
    assert_eq!(store.append_shot_scan_progress(&current, &tail(4, 12))?, 12);
    assert!(matches!(
        store.append_shot_scan_progress(&current, &tail(13, 14)),
        Err(StoreError::Integrity(_))
    ));
    assert_eq!(
        store.append_shot_scan_progress(&current, &tail(12, 20))?,
        20
    );
    assert_eq!(
        store.shot_scan_progress(&current, 120)?,
        Some(ShotProgress::new(120, whole.clone())?)
    );
    // A tail of other pictures does not join.
    let other_count = ShotProgressTail::new(121, 3, whole[3..5].to_vec())?;
    assert!(
        store
            .append_shot_scan_progress(&current, &other_count)
            .is_err()
    );
    // A tail from the first picture replaces the progress.
    assert_eq!(store.append_shot_scan_progress(&current, &tail(0, 2))?, 2);
    assert_eq!(store.shot_scan_progress(&current, 120)?.unwrap().next(), 2);

    // Progress is kept for one scan: saving another key's removes it.
    let other = ShotAnalysisKey {
        content: "blake3:other".into(),
        ..current.clone()
    };
    let stream = ShotAnalysisKey {
        video_stream: 1,
        ..current.clone()
    };
    store.save_shot_scan_progress(&other, &progress(3))?;
    assert!(store.shot_scan_progress(&current, 120)?.is_none());
    store.save_shot_scan_progress(&stream, &progress(4))?;
    assert!(store.shot_scan_progress(&other, 120)?.is_none());
    assert_eq!(store.shot_scan_progress(&stream, 120)?.unwrap().next(), 4);
    let rows: i64 = Connection::open(path.join("project.sqlite"))?.query_row(
        "SELECT count(*) FROM shot_scan_progress",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(rows, 1);
    // A read-only store saves nothing.
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    reader.validate()?;
    assert!(
        reader
            .append_shot_scan_progress(&stream, &tail(4, 6))
            .is_err()
    );
    Ok(())
}
