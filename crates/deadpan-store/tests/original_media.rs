#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::error::Error;
use std::fs;
use std::io::Read;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_core::{
    ColorPolicy, FrameRate, NodeId, PresentationBasis, ProjectDocument, ProjectId, RevisionId,
};
use deadpan_store::original_media::{
    LinkedOriginal, OriginalMediaError, OriginalMediaLimits, OriginalOwnership,
    OriginalRetentionMethod,
};
use deadpan_store::{AccessMode, ProjectStore, StoreError};
use rusqlite::Connection;
use sha2::{Digest, Sha256};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn project(parent: &Path) -> Result<(PathBuf, ProjectStore)> {
    let path = parent.join("originals.deadpan");
    let document = ProjectDocument::new(
        ProjectId::new("originals")?,
        RevisionId::new("r0")?,
        PresentationBasis {
            width: 320,
            height: 180,
            frame_rate: FrameRate::new(30_000, 1001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    let store = ProjectStore::create(&path, &document)?;
    Ok((path, store))
}

fn limits() -> OriginalMediaLimits {
    OriginalMediaLimits::new(1_048_576, Duration::from_secs(10)).unwrap()
}
fn active() -> AtomicBool {
    AtomicBool::new(false)
}
fn fixture() -> &'static [u8] {
    include_bytes!("../../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4")
}

#[test]
fn managed_original_preserves_all_container_bytes_deduplicates_and_survives_relocation() -> Result {
    let scratch = tempfile::tempdir()?;
    let (package, mut store) = project(scratch.path())?;
    let original = scratch.path().join("camera.mp4");
    fs::write(&original, fixture())?;
    let before = store.snapshot()?.to_json()?;
    let retained =
        store.retain_original(&original, OriginalOwnership::Managed, limits(), &active())?;
    assert!(matches!(
        retained.method,
        OriginalRetentionMethod::Cloned | OriginalRetentionMethod::Copied
    ));
    let record = retained.record;
    assert!(record.managed());
    assert!(record.linked().is_none());
    assert_eq!(
        record.object().byte_length(),
        u64::try_from(fixture().len())?
    );
    assert_eq!(record.sha256(), <[u8; 32]>::from(Sha256::digest(fixture())));
    assert_eq!(
        record.object().content().digest(),
        blake3::hash(fixture()).to_hex().as_str()
    );
    let renamed = scratch.path().join("another-name.mp4");
    fs::write(&renamed, fixture())?;
    let same = store.retain_original(&renamed, OriginalOwnership::Managed, limits(), &active())?;
    assert_eq!(same.method, OriginalRetentionMethod::Existing);
    assert_eq!(same.record, record);
    assert_eq!(
        store.snapshot()?.to_json()?,
        before,
        "byte retention cannot fabricate an authored import"
    );
    let object = package
        .join("Media/Originals")
        .join(format!("blake3-{}", record.object().content().digest()));
    assert_eq!(fs::metadata(&object)?.permissions().mode() & 0o222, 0);
    assert_eq!(fs::read_dir(package.join("Media/Originals"))?.count(), 1);
    fs::write(&original, b"changed source")?;
    fs::remove_file(&renamed)?;
    drop(store);
    let moved = scratch.path().join("moved.deadpan");
    fs::rename(&package, &moved)?;
    let reader = ProjectStore::open(&moved, AccessMode::ReadOnly)?;
    let mut snapshot = reader.snapshot_original(record.object().content(), limits(), &active())?;
    let mut bytes = Vec::new();
    snapshot.read_to_end(&mut bytes)?;
    assert_eq!(
        bytes,
        fixture(),
        "including the original AAC packets and container metadata"
    );
    assert_eq!(reader.original_records(None, 10)?, vec![record]);
    Ok(())
}

#[test]
fn linked_original_relink_checks_content_and_monotonic_location_version() -> Result {
    let scratch = tempfile::tempdir()?;
    let (_, mut store) = project(scratch.path())?;
    let path = scratch.path().join("linked.mp4");
    fs::write(&path, fixture())?;
    let retained = store.retain_original(
        &path,
        OriginalOwnership::Linked {
            bookmark: Some(vec![1, 2, 3]),
        },
        limits(),
        &active(),
    )?;
    assert_eq!(retained.method, OriginalRetentionMethod::Linked);
    assert!(!retained.record.managed());
    let key = retained.record.object().content();
    let mut frozen = store.snapshot_original(key, limits(), &active())?;
    let moved = scratch.path().join("moved.mp4");
    fs::rename(&path, &moved)?;
    assert_eq!(
        store
            .snapshot_original(key, limits(), &active())
            .err()
            .unwrap()
            .code(),
        "OriginalOffline"
    );
    let wrong = scratch.path().join("wrong.mp4");
    fs::write(&wrong, b"different full content")?;
    let before = store.original_record(key)?.unwrap();
    assert!(matches!(
        store.relink_original(
            key,
            before.version(),
            LinkedOriginal::new(wrong, None)?,
            limits(),
            &active()
        ),
        Err(StoreError::OriginalMedia(
            OriginalMediaError::IdentityMismatch
        ))
    ));
    assert_eq!(store.original_record(key)?.unwrap(), before);
    let after = store.relink_original(
        key,
        before.version(),
        LinkedOriginal::new(moved.clone(), Some(vec![4]))?,
        limits(),
        &active(),
    )?;
    assert_eq!(after.version(), before.version() + 1);
    assert_eq!(after.linked().unwrap().path(), moved);
    assert_eq!(after.linked().unwrap().bookmark(), Some([4].as_slice()));
    assert!(matches!(
        store.relink_original(
            key,
            before.version(),
            LinkedOriginal::new(path, None)?,
            limits(),
            &active()
        ),
        Err(StoreError::OriginalMedia(
            OriginalMediaError::VersionConflict { .. }
        ))
    ));
    fs::write(&moved, b"modified after relinking")?;
    assert_eq!(
        store
            .snapshot_original(key, limits(), &active())
            .err()
            .unwrap()
            .code(),
        "OriginalContentMismatch"
    );
    let mut bytes = Vec::new();
    frozen.read_to_end(&mut bytes)?;
    assert_eq!(
        bytes,
        fixture(),
        "previous read snapshots retain their own bytes"
    );
    Ok(())
}

#[test]
fn readonly_cancelled_oversized_and_special_inputs_do_not_create_records() -> Result {
    let scratch = tempfile::tempdir()?;
    let (package, mut store) = project(scratch.path())?;
    let source = scratch.path().join("source.mp4");
    fs::write(&source, fixture())?;
    let cancelled = AtomicBool::new(true);
    assert_eq!(
        store
            .retain_original(&source, OriginalOwnership::Managed, limits(), &cancelled)
            .unwrap_err()
            .code(),
        "OriginalCancelled"
    );
    let small = OriginalMediaLimits::new(1, Duration::from_secs(1))?;
    assert!(
        store
            .retain_original(&source, OriginalOwnership::Managed, small, &active())
            .is_err()
    );
    assert!(
        store
            .retain_original(
                scratch.path(),
                OriginalOwnership::Managed,
                limits(),
                &active()
            )
            .is_err()
    );
    let alias = scratch.path().join("alias.mp4");
    symlink(&source, &alias)?;
    assert!(
        store
            .retain_original(&alias, OriginalOwnership::Managed, limits(), &active())
            .is_err()
    );
    let pipe = scratch.path().join("pipe");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&pipe)
            .status()?
            .success()
    );
    assert!(matches!(
        store.retain_original(&pipe, OriginalOwnership::Managed, limits(), &active()),
        Err(StoreError::OriginalMedia(OriginalMediaError::NotRegular))
    ));
    assert!(store.original_records(None, 10)?.is_empty());
    assert_eq!(fs::read_dir(package.join("Media/Originals"))?.count(), 0);
    drop(store);
    let mut reader = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert!(matches!(
        reader.retain_original(
            &scratch.path().join("not-found"),
            OriginalOwnership::Managed,
            limits(),
            &active()
        ),
        Err(StoreError::ReadOnly)
    ));
    Ok(())
}

#[test]
fn failed_database_commit_keeps_verified_object_and_retry_reuses_it() -> Result {
    let scratch = tempfile::tempdir()?;
    let (package, mut store) = project(scratch.path())?;
    let source = scratch.path().join("source.mp4");
    fs::write(&source, fixture())?;
    let database = Connection::open(package.join("project.sqlite"))?;
    database.execute_batch("CREATE TRIGGER fail_original BEFORE INSERT ON original_media BEGIN SELECT RAISE(ABORT,'injected commit failure'); END;")?;
    assert!(
        store
            .retain_original(&source, OriginalOwnership::Managed, limits(), &active())
            .is_err()
    );
    assert!(store.original_records(None, 10)?.is_empty());
    assert_eq!(
        fs::read_dir(package.join("Media/Originals"))?.count(),
        1,
        "durable objects survive database failure"
    );
    database.execute_batch("DROP TRIGGER fail_original")?;
    let retained =
        store.retain_original(&source, OriginalOwnership::Managed, limits(), &active())?;
    assert_eq!(retained.method, OriginalRetentionMethod::Existing);
    store.validate()?;
    Ok(())
}

#[test]
fn same_filename_is_not_identity_and_inventory_pages_without_duplicates() -> Result {
    let scratch = tempfile::tempdir()?;
    let (_, mut store) = project(scratch.path())?;
    for (directory, bytes) in [("one", b"one original"), ("two", b"two original")] {
        fs::create_dir(scratch.path().join(directory))?;
        let path = scratch.path().join(directory).join("same.mp4");
        fs::write(&path, bytes)?;
        store.retain_original(&path, OriginalOwnership::Managed, limits(), &active())?;
    }
    let first = store.original_records(None, 1)?;
    let second = store.original_records(Some(first[0].object().content()), 1)?;
    assert_eq!(first[0].label(), second[0].label());
    assert_ne!(first[0].object().content(), second[0].object().content());
    assert!(
        store
            .original_records(Some(second[0].object().content()), 1)?
            .is_empty()
    );
    assert!(store.original_records(None, 0).is_err());
    assert!(store.original_records(None, 1001).is_err());
    Ok(())
}

#[test]
fn tampered_record_and_managed_bytes_fail_verification() -> Result {
    let scratch = tempfile::tempdir()?;
    let (package, mut store) = project(scratch.path())?;
    let path = scratch.path().join("source.mp4");
    fs::write(&path, fixture())?;
    let record = store
        .retain_original(&path, OriginalOwnership::Managed, limits(), &active())?
        .record;
    let database = Connection::open(package.join("project.sqlite"))?;
    database.execute(
        "UPDATE original_media SET record=json_set(record,'$.sha256[0]',?1)",
        [u32::from(record.sha256()[0] ^ 1)],
    )?;
    assert_eq!(
        store
            .snapshot_original(record.object().content(), limits(), &active())
            .err()
            .unwrap()
            .code(),
        "OriginalContentMismatch"
    );
    database.execute(
        "UPDATE original_media SET record=?1",
        [serde_json::to_string(&record)?],
    )?;
    let object = package
        .join("Media/Originals")
        .join(format!("blake3-{}", record.object().content().digest()));
    fs::set_permissions(&object, fs::Permissions::from_mode(0o600))?;
    let mut corrupt = fixture().to_vec();
    corrupt[10] ^= 1;
    fs::write(&object, corrupt)?;
    fs::set_permissions(&object, fs::Permissions::from_mode(0o444))?;
    assert_eq!(
        store
            .snapshot_original(record.object().content(), limits(), &active())
            .err()
            .unwrap()
            .code(),
        "OriginalContentMismatch"
    );
    fs::remove_file(&object)?;
    assert_eq!(
        store
            .snapshot_original(record.object().content(), limits(), &active())
            .err()
            .unwrap()
            .code(),
        "OriginalOffline"
    );
    database.execute("UPDATE original_media SET version=version+1", [])?;
    assert!(store.validate().is_err());
    drop(store);
    assert!(ProjectStore::open(&package, AccessMode::ReadOnly).is_err());
    Ok(())
}
