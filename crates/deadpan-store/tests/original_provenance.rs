//! Remote-original provenance is private operational metadata keyed by retained bytes.
#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::error::Error;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_core::{NodeId, ProjectDocument, ProjectId, RevisionId};
use deadpan_store::original_media::{OriginalContentId, OriginalMediaLimits, OriginalOwnership};
use deadpan_store::original_provenance::{
    FormatRole, HelperVersion, PROVENANCE_SCHEMA, RemoteOriginalProvenance, SelectedFormat,
};
use deadpan_store::{AccessMode, ProjectStore, StoreError};
use rusqlite::Connection;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn provenance() -> RemoteOriginalProvenance {
    RemoteOriginalProvenance {
        schema: PROVENANCE_SCHEMA,
        service: "youtube".into(),
        source_id: "Z4C82eyhwgU".into(),
        source_url: "https://www.youtube.com/watch?v=Z4C82eyhwgU".into(),
        title: "A title \u{1F999}".into(),
        author: Some("Blender".into()),
        author_id: None,
        author_url: None,
        license: None,
        upload_date: Some("20140805".into()),
        duration_millis: Some(146_000),
        thumbnail_url: None,
        retrieved_at_unix_seconds: 1_791_158_400,
        helpers: vec![HelperVersion {
            name: "yt-dlp".into(),
            version: "2026.08.19".into(),
        }],
        formats: vec![SelectedFormat {
            role: FormatRole::Video,
            format_id: "137".into(),
            container: Some("mp4".into()),
            codec: Some("avc1.640028".into()),
            width: Some(1920),
            height: Some(1080),
            fps_millis: Some(24_000),
            bitrate_kbps: Some(2889),
            byte_length: 10,
        }],
        assembly: "deadpan-media-worker-remux-v1".into(),
    }
}

fn retained(scratch: &tempfile::TempDir) -> Result<(std::path::PathBuf, OriginalContentId)> {
    let path = scratch.path().join("provenance.deadpan");
    let document = ProjectDocument::new_automatic(
        ProjectId::new("provenance")?,
        RevisionId::new("base")?,
        NodeId::new("root")?,
    )?;
    let mut store = ProjectStore::create(&path, &document)?;
    let source = scratch.path().join("original.mp4");
    std::fs::write(&source, b"retained original bytes")?;
    let outcome = store.retain_original(
        &source,
        OriginalOwnership::Managed,
        OriginalMediaLimits::new(1024, Duration::from_secs(10))?,
        &AtomicBool::new(false),
    )?;
    Ok((path, outcome.record.object().content().clone()))
}

#[test]
fn provenance_saves_reopens_and_never_touches_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, content) = retained(&scratch)?;
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let head = store.snapshot()?.revision_id().clone();
    assert!(store.original_provenance(&content)?.is_none());
    store.save_original_provenance(&content, &provenance())?;
    assert_eq!(store.snapshot()?.revision_id(), &head);
    drop(store);

    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(reader.original_provenance(&content)?, Some(provenance()));
    reader.validate()?;
    assert!(matches!(
        reader.save_original_provenance(&content, &provenance()),
        Err(StoreError::ReadOnly)
    ));
    Ok(())
}

#[test]
fn provenance_rejects_unretained_originals_and_unsafe_text() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, content) = retained(&scratch)?;
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let missing = OriginalContentId::new("0".repeat(64))?;
    assert!(
        store
            .save_original_provenance(&missing, &provenance())
            .is_err()
    );

    let mut control = provenance();
    control.title = "line\nbreak".into();
    let mut insecure = provenance();
    insecure.source_url = "http://www.youtube.com/watch?v=Z4C82eyhwgU".into();
    let mut empty = provenance();
    empty.formats.clear();
    let mut oversized = provenance();
    oversized.title = "x".repeat(5000);
    for invalid in [control, insecure, empty, oversized] {
        assert!(store.save_original_provenance(&content, &invalid).is_err());
    }
    assert!(store.original_provenance(&content)?.is_none());
    Ok(())
}

#[test]
fn a_schema61_package_gains_the_provenance_table_from_its_writer() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, content) = retained(&scratch)?;
    Connection::open(path.join("project.sqlite"))?
        .execute_batch("DROP TABLE original_provenance; PRAGMA user_version=61;")?;

    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert!(reader.original_provenance(&content)?.is_none());
    reader.validate()?;
    drop(reader);

    let writer = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    writer.save_original_provenance(&content, &provenance())?;
    assert_eq!(writer.original_provenance(&content)?, Some(provenance()));
    Ok(())
}
