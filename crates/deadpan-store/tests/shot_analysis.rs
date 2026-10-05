//! Shot analysis is a durable annotation outside document history.
use std::error::Error;

use deadpan_analysis::{SIGNATURE_VERSION, ShotAnalysis};
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
    ShotAnalysis::new(changes).unwrap()
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
        "UPDATE shot_analysis SET changes=X'0100{}'",
        "00".repeat(38)
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
