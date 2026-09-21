#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::error::Error;
use std::fs;
use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use deadpan_core::{
    ColorPolicy, FrameRate, NodeId, PresentationBasis, ProjectDocument, ProjectId, RevisionId,
};
use deadpan_jobs::artifact::{ArtifactLimits, ArtifactWorkspace};
use deadpan_jobs::protocol::{Sha256, WorkspaceArtifact, WorkspaceRef};
use deadpan_store::generated_media::{
    GeneratedContentId, GeneratedMediaLimits, GeneratedObjectRef,
};
use deadpan_store::{AccessMode, ProjectStore, StoreError};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn document() -> Result<ProjectDocument> {
    Ok(ProjectDocument::new(
        ProjectId::new("generated-media-project")?,
        RevisionId::new("r0")?,
        PresentationBasis {
            width: 768,
            height: 320,
            frame_rate: FrameRate::new(30_000, 1_001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?)
}

fn reference(bytes: &[u8]) -> Result<GeneratedObjectRef> {
    Ok(GeneratedObjectRef::new(
        GeneratedContentId::new(blake3::hash(bytes).to_hex().to_string())?,
        u64::try_from(bytes.len())?,
    )?)
}

fn limits() -> Result<GeneratedMediaLimits> {
    Ok(GeneratedMediaLimits::new(1_048_576)?)
}

fn stored_path(package: &Path, expected: &GeneratedObjectRef) -> PathBuf {
    package
        .join("Media/Generated")
        .join(format!("blake3-{}", expected.content().digest()))
}

#[test]
fn references_keep_the_hash_algorithm_and_reject_forged_wire_values() -> Result {
    let expected = reference(b"abc")?;
    assert_eq!(
        expected.content().digest(),
        "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"
    );
    let wire = serde_json::to_value(&expected)?;
    assert_eq!(wire["content"]["algorithm"], "blake3");
    assert_eq!(wire["byte_length"], 3);
    assert_eq!(
        serde_json::from_value::<GeneratedObjectRef>(wire.clone())?,
        expected
    );
    for invalid in [
        serde_json::json!({"content":{"algorithm":"sha256","digest":expected.content().digest()},"byte_length":3}),
        serde_json::json!({"content":{"algorithm":"blake3","digest":"../elsewhere"},"byte_length":3}),
        serde_json::json!({"content":{"algorithm":"blake3","digest":expected.content().digest()},"byte_length":0}),
        serde_json::json!({"content":{"algorithm":"blake3","digest":expected.content().digest()},"byte_length":true}),
        serde_json::json!({"content":{"algorithm":"blake3","digest":expected.content().digest()},"byte_length":3,"path":"/tmp/injected"}),
    ] {
        assert!(serde_json::from_value::<GeneratedObjectRef>(invalid).is_err());
    }
    Ok(())
}

#[test]
fn publication_is_durable_idempotent_and_separate_from_authored_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("original.deadpan");
    let initial = document()?;
    let mut store = ProjectStore::create(&package, &initial)?;
    let mut reader = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let bytes = b"bounded generated-object storage fixture";
    let expected = reference(bytes)?;
    assert_eq!(
        store.promote_generated_object(&mut Cursor::new(bytes), &expected, limits()?)?,
        expected
    );
    let path = stored_path(&package, &expected);
    let inode = fs::metadata(&path)?.ino();
    store.promote_generated_object(&mut Cursor::new(bytes), &expected, limits()?)?;
    assert_eq!(fs::metadata(&path)?.ino(), inode);
    assert_eq!(fs::read_dir(package.join("Media/Generated"))?.count(), 1);
    assert_eq!(store.snapshot()?, initial);
    assert_eq!(reader.snapshot()?, initial);

    // Publication is not an edit and creates no undo entry or candidate receipt.
    assert!(matches!(
        store.undo(initial.revision_id(), RevisionId::new("no-edit")?),
        Err(StoreError::NothingToUndo)
    ));
    let mut snapshot = reader.snapshot_generated_object(&expected, limits()?)?;
    assert_eq!(snapshot.reference(), &expected);
    let mut observed = Vec::new();
    snapshot.read_to_end(&mut observed)?;
    assert_eq!(observed, bytes);

    struct MustNotRead;
    impl Read for MustNotRead {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            panic!("read-only publication must fail before consuming input")
        }
    }
    assert!(matches!(
        reader.promote_generated_object(&mut MustNotRead, &expected, limits()?),
        Err(StoreError::ReadOnly)
    ));
    drop(reader);
    drop(store);
    let relocated = scratch.path().join("relocated.deadpan");
    fs::rename(&package, &relocated)?;
    let reopened = ProjectStore::open(&relocated, AccessMode::ReadOnly)?;
    let mut snapshot = reopened.snapshot_generated_object(&expected, limits()?)?;
    observed.clear();
    snapshot.read_to_end(&mut observed)?;
    assert_eq!(observed, bytes);
    assert_eq!(reopened.snapshot()?, initial);
    Ok(())
}

#[test]
fn worker_snapshot_can_be_published_after_the_worker_workspace_is_removed() -> Result {
    let scratch = tempfile::tempdir()?;
    let worker = scratch.path().join("worker");
    fs::create_dir_all(worker.join("outputs"))?;
    fs::write(worker.join("outputs/object"), b"abc")?;
    let workspace = ArtifactWorkspace::open(&worker)?;
    let declaration = WorkspaceArtifact::new(
        WorkspaceRef::new("outputs/object")?,
        Sha256::new("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")?,
        3,
    )?;
    let mut frozen = workspace.snapshot(
        &WorkspaceRef::new("outputs")?,
        &declaration,
        ArtifactLimits::new(3)?,
    )?;
    drop(workspace);
    fs::remove_dir_all(worker)?;
    let package = scratch.path().join("offline.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let expected = reference(b"abc")?;
    store.promote_generated_object(&mut frozen, &expected, limits()?)?;
    drop(frozen);
    drop(store);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let mut readback = store.snapshot_generated_object(&expected, limits()?)?;
    let mut bytes = Vec::new();
    readback.read_to_end(&mut bytes)?;
    assert_eq!(bytes, b"abc");
    Ok(())
}

#[test]
fn returned_snapshot_survives_later_mutation_and_corrupt_object_is_not_replaced() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("snapshot.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let original = b"original immutable content";
    let expected = reference(original)?;
    store.promote_generated_object(&mut Cursor::new(original), &expected, limits()?)?;
    let mut snapshot = store.snapshot_generated_object(&expected, limits()?)?;
    let path = stored_path(&package, &expected);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    let corrupted = vec![b'x'; original.len()];
    fs::write(&path, &corrupted)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444))?;
    let mut observed = Vec::new();
    snapshot.read_to_end(&mut observed)?;
    assert_eq!(observed, original);
    snapshot.seek(SeekFrom::Start(0))?;
    observed.clear();
    snapshot.read_to_end(&mut observed)?;
    assert_eq!(observed, original);
    assert_eq!(
        store
            .snapshot_generated_object(&expected, limits()?)
            .unwrap_err()
            .code(),
        "GeneratedMediaHashMismatch"
    );
    assert_eq!(
        store
            .promote_generated_object(&mut Cursor::new(original), &expected, limits()?)
            .unwrap_err()
            .code(),
        "GeneratedMediaHashMismatch"
    );
    assert_eq!(fs::read(&path)?, corrupted);
    Ok(())
}

#[test]
fn package_path_replacement_cannot_redirect_publication() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("active.deadpan");
    let moved = scratch.path().join("moved.deadpan");
    let mut original = ProjectStore::create(&path, &document()?)?;
    fs::rename(&path, &moved)?;
    let replacement = ProjectStore::create(&path, &document()?)?;
    let expected = reference(b"belongs to original package")?;
    original.promote_generated_object(
        &mut Cursor::new(b"belongs to original package"),
        &expected,
        limits()?,
    )?;
    assert!(stored_path(&moved, &expected).is_file());
    assert!(!stored_path(&path, &expected).exists());
    assert!(
        replacement
            .snapshot_generated_object(&expected, limits()?)
            .is_err()
    );
    Ok(())
}

#[test]
fn unsafe_generated_directory_and_hard_links_are_rejected() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("contained.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let bytes = b"contained";
    let expected = reference(bytes)?;
    let generated = package.join("Media/Generated");
    let outside = scratch.path().join("outside");
    fs::create_dir(&outside)?;
    fs::remove_dir(&generated)?;
    symlink(&outside, &generated)?;
    assert!(
        store
            .promote_generated_object(&mut Cursor::new(bytes), &expected, limits()?)
            .is_err()
    );
    assert_eq!(fs::read_dir(&outside)?.count(), 0);
    fs::remove_file(&generated)?;
    fs::create_dir(&generated)?;
    store.promote_generated_object(&mut Cursor::new(bytes), &expected, limits()?)?;
    fs::hard_link(stored_path(&package, &expected), outside.join("alias"))?;
    assert!(
        store
            .snapshot_generated_object(&expected, limits()?)
            .is_err()
    );
    assert!(
        store
            .promote_generated_object(&mut Cursor::new(bytes), &expected, limits()?)
            .is_err()
    );
    assert_eq!(fs::read(outside.join("alias"))?, bytes);
    Ok(())
}

#[test]
fn metadata_only_packages_still_open_but_missing_media_never_looks_available() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("missing.deadpan");
    let store = ProjectStore::create(&package, &document()?)?;
    drop(store);
    fs::remove_dir_all(package.join("Media"))?;
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, document()?);
    let expected = reference(b"unavailable")?;
    assert!(
        store
            .snapshot_generated_object(&expected, limits()?)
            .is_err()
    );
    assert!(
        store
            .promote_generated_object(&mut Cursor::new(b"unavailable"), &expected, limits()?)
            .is_err()
    );
    assert!(!package.join("Media").exists());
    Ok(())
}

#[test]
fn shared_directory_permissions_restrict_media_operations_without_blocking_inspection() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("shared.deadpan");
    drop(ProjectStore::create(&package, &document()?)?);
    fs::set_permissions(&package, fs::Permissions::from_mode(0o770))?;
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_eq!(store.snapshot()?, document()?);
    assert!(
        store
            .snapshot_generated_object(&reference(b"data")?, limits()?)
            .is_err()
    );
    Ok(())
}
