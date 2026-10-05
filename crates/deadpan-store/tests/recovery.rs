//! Writer crash detection, recovered-work reporting and Original restore.
#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::process::Command as ProcessCommand;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_core::*;
use deadpan_jobs::{AttemptId, CancellationToken, RequestId, render::*};
use deadpan_store::original_media::{
    LinkedOriginal, OriginalAvailability, OriginalMediaLimits, OriginalOwnership,
};
use deadpan_store::render_jobs::{BeginRenderAttempt, RenderAttemptTransition};
use deadpan_store::{AccessMode, ProjectStore};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn document() -> Result<ProjectDocument> {
    Ok(ProjectDocument::new(
        ProjectId::new("project")?,
        RevisionId::new("r0")?,
        PresentationBasis {
            width: 640,
            height: 360,
            frame_rate: FrameRate::new(30, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?)
}

fn insert(store: &mut ProjectStore, revision: &str, node: &str) -> Result<()> {
    let current = store.snapshot()?;
    let id = NodeId::new(node)?;
    store.commit(&CommandRequest {
        project_id: current.project_id().clone(),
        expected_revision: current.revision_id().clone(),
        new_revision: RevisionId::new(revision)?,
        command: Command::Insert {
            parent: current.root().clone(),
            index: 0,
            subtree: Subtree {
                overrides: Default::default(),
                gap_overrides: Default::default(),
                root: id.clone(),
                nodes: BTreeMap::from([(
                    id,
                    BeatNode::hold(
                        "Pause",
                        HoldRecipe {
                            picture_context: None,
                            duration: FrameDuration::new(12)?,
                            video: HoldVideo::Background,
                            audio: HoldAudio::Silence,
                        },
                    ),
                )]),
            },
        },
    })?;
    Ok(())
}

fn start_render(store: &mut ProjectStore) -> Result<()> {
    let document = store.snapshot()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let intent = store.create_render_job(
        RenderIntent {
            schema_version: 1,
            job_id: RequestId::new("job")?,
            project_id: document.project_id().clone(),
            revision_id: document.revision_id().clone(),
            document_sha256: document_sha256(&document, &AtomicBool::new(false), deadline)?,
            range: FrameRange::new(ProjectFrame(0), ProjectFrame(12))?,
            policy: RenderPolicy::Engineering(RenderEngineeringPolicy {
                schema_version: 1,
                selection: RenderSelection::ExplicitEngineering,
                encoder: RenderEncoder::Software,
                b_frames: RenderBFrames::None,
            }),
        },
        &AtomicBool::new(false),
        deadline,
    )?;
    let queued = store.begin_render_attempt(BeginRenderAttempt {
        job_id: intent.job_id.clone(),
        attempt_id: AttemptId::new("attempt")?,
        cancellation_token: CancellationToken::new("cancel-attempt")?,
        checkpoint_attempt_id: None,
    })?;
    store.transition_render_attempt(&queued.identity(), RenderAttemptTransition::Encoding)?;
    Ok(())
}

#[test]
fn a_clean_close_is_not_reported_as_a_crash() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("clean.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    assert!(store.open_recovery().is_clean());
    assert!(path.join(".writer.session").is_file());
    insert(&mut store, "r1", "hold")?;
    drop(store);
    assert!(!path.join(".writer.session").exists());
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert!(reader.open_recovery().is_clean());
    drop(reader);
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert!(store.open_recovery().is_clean());
    assert_eq!(store.snapshot()?.revision_id().as_str(), "r1");
    Ok(())
}

/// A real process exit with the writer open: no destructor, no close.
#[test]
fn a_killed_writer_is_detected_and_its_committed_edits_and_jobs_are_recovered() -> Result {
    const CHILD: &str = "DEADPAN_TEST_KILLED_WRITER";
    if let Some(path) = std::env::var_os(CHILD) {
        let mut store = ProjectStore::open(Path::new(&path), AccessMode::ReadWrite)?;
        insert(&mut store, "r2", "second")?;
        start_render(&mut store)?;
        // The kernel releases the lock; the session marker stays behind.
        std::process::exit(77);
    }
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("killed.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    insert(&mut store, "r1", "first")?;
    drop(store);
    let child = ProcessCommand::new(std::env::current_exe()?)
        .args([
            "--exact",
            "a_killed_writer_is_detected_and_its_committed_edits_and_jobs_are_recovered",
            "--nocapture",
        ])
        .env(CHILD, &path)
        .output()?;
    assert_eq!(
        child.status.code(),
        Some(77),
        "{}",
        String::from_utf8_lossy(&child.stderr)
    );
    assert!(path.join(".writer.session").is_file());

    // A reader neither recovers nor consumes the evidence.
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert!(reader.open_recovery().is_clean());
    drop(reader);

    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let recovery = store.open_recovery().clone();
    let previous = recovery
        .unclean_previous_writer
        .as_ref()
        .expect("unclean writer reported");
    assert!(previous.marker.starts_with("deadpan writer pid="));
    assert_eq!(recovery.interrupted_render_count, 1);
    assert_eq!(recovery.interrupted_renders[0].job_id, "job");
    assert_eq!(recovery.interrupted_renders[0].attempt_id, "attempt");
    assert_eq!(recovery.interrupted_renders[0].checkpoint_attempt_id, None);
    assert_eq!(recovery.interrupted_generation_count, 0);
    // Every committed edit survives at its last committed revision.
    let snapshot = store.snapshot()?;
    assert_eq!(snapshot.revision_id().as_str(), "r2");
    assert!(snapshot.nodes().contains_key(&NodeId::new("first")?));
    assert!(snapshot.nodes().contains_key(&NodeId::new("second")?));
    let attempts = store.render_attempts(&RequestId::new("job")?, 0, 8)?;
    assert_eq!(attempts.len(), 1);
    assert_eq!(
        attempts[0].diagnostic.as_ref().map(|d| d.code.as_str()),
        Some("InterruptedOnOpen")
    );
    store.validate_full()?;
    drop(store);

    // An intervening writer (for example a headless command) cannot swallow
    // the findings: they stay until a host acknowledges them.
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.open_recovery(), &recovery);
    store.acknowledge_recovery()?;
    assert!(store.open_recovery().is_clean());
    drop(store);
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert!(store.open_recovery().is_clean());
    Ok(())
}

#[test]
fn copied_and_damaged_markers_are_handled_without_false_crashes() -> Result {
    let scratch = tempfile::tempdir()?;
    let source = scratch.path().join("source.deadpan");
    drop(ProjectStore::create(&source, &document()?)?);
    // A package copied while its writer was open carries a marker for the
    // original package's identity: the copy did not crash.
    let marker = {
        let store = ProjectStore::open(&source, AccessMode::ReadWrite)?;
        let marker = std::fs::read_to_string(source.join(".writer.session"))?;
        drop(store);
        marker
    };
    assert!(marker.contains(" package="), "{marker}");
    let copy = scratch.path().join("copy.deadpan");
    std::fs::create_dir(&copy)?;
    for directory in [
        "Media/Originals",
        "Media/Generated",
        "Media/RenderCandidates",
        "Snapshots",
        "Reports",
    ] {
        std::fs::create_dir_all(copy.join(directory))?;
    }
    for name in ["project.sqlite", "manifest.json"] {
        std::fs::copy(source.join(name), copy.join(name))?;
    }
    std::fs::write(copy.join(".writer.session"), &marker)?;
    std::fs::write(copy.join(".writer-session-orphan.tmp"), b"left by a crash")?;
    let store = ProjectStore::open(&copy, AccessMode::ReadWrite)?;
    assert!(
        store.open_recovery().is_clean(),
        "{:?}",
        store.open_recovery()
    );
    assert!(!copy.join(".writer-session-orphan.tmp").exists());
    drop(store);

    // A directory where the marker belongs is reported, and opening works.
    std::fs::create_dir(source.join(".writer.session"))?;
    let store = ProjectStore::open(&source, AccessMode::ReadWrite)?;
    let recovery = store.open_recovery();
    assert!(recovery.unclean_previous_writer.is_some());
    assert!(recovery.record_error.is_some(), "{recovery:?}");
    Ok(())
}

fn original(directory: &Path, name: &str, bytes: &[u8]) -> Result<std::path::PathBuf> {
    let path = directory.join(name);
    std::fs::write(&path, bytes)?;
    Ok(path.canonicalize()?)
}

fn limits() -> OriginalMediaLimits {
    OriginalMediaLimits::new(1 << 20, Duration::from_secs(30)).expect("limits")
}

#[test]
fn a_missing_managed_copy_is_reported_and_restored_only_from_identical_bytes() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("restore.deadpan");
    let source = original(scratch.path(), "interview.mp4", b"the original bytes")?;
    let different = original(scratch.path(), "other.mp4", b"some other bytes!!")?;
    let mut store = ProjectStore::create(&path, &document()?)?;
    let cancelled = AtomicBool::new(false);
    let record = store
        .retain_original(&source, OriginalOwnership::Managed, limits(), &cancelled)?
        .record;
    assert_eq!(
        store.original_availability(&record)?,
        OriginalAvailability::Present
    );
    let object = path
        .join("Media/Originals")
        .join(format!("blake3-{}", record.object().content().digest()));
    std::fs::remove_file(&object)?;
    assert_eq!(
        store.original_availability(&record)?,
        OriginalAvailability::Missing
    );
    let error = store
        .snapshot_original(record.object().content(), limits(), &cancelled)
        .err()
        .expect("missing copy refuses snapshots");
    assert_eq!(error.code(), "OriginalOffline");

    let handle = store.original_import_handle()?;
    let refused = handle
        .prepare_restore(&record, &different, limits(), &cancelled)
        .err()
        .expect("different content is refused");
    assert_eq!(refused.code(), "OriginalContentMismatch");
    assert!(!object.exists(), "refused bytes were never published");

    let prepared = handle.prepare_restore(&record, &source, limits(), &cancelled)?;
    assert_eq!(prepared.quarantined, None);
    let restored = store.retain_prepared_original(&prepared.retention, &cancelled)?;
    assert_eq!(restored.record, record, "record and location version kept");
    assert_eq!(
        store.original_availability(&record)?,
        OriginalAvailability::Present
    );
    store.snapshot_original(record.object().content(), limits(), &cancelled)?;
    Ok(())
}

/// A retained copy damaged in place (same length or not) is moved aside and
/// replaced from the identical file; a different file is refused first and
/// nothing changes.
#[test]
fn a_damaged_managed_copy_is_quarantined_and_restored_from_identical_bytes() -> Result {
    use std::os::unix::fs::PermissionsExt;
    let bytes = b"the original bytes, intact";
    for damage in [&b"THE ORIGINAL BYTES, INTACT"[..], &b"short"[..]] {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join("damaged.deadpan");
        let source = original(scratch.path(), "interview.mp4", bytes)?;
        let different = original(scratch.path(), "other.mp4", b"some other bytes")?;
        let mut store = ProjectStore::create(&path, &document()?)?;
        let cancelled = AtomicBool::new(false);
        let record = store
            .retain_original(&source, OriginalOwnership::Managed, limits(), &cancelled)?
            .record;
        let directory = path.join("Media/Originals");
        let object = directory.join(format!("blake3-{}", record.object().content().digest()));
        std::fs::set_permissions(&object, std::fs::Permissions::from_mode(0o644))?;
        std::fs::write(&object, damage)?;
        std::fs::set_permissions(&object, std::fs::Permissions::from_mode(0o444))?;
        assert!(
            store
                .snapshot_original(record.object().content(), limits(), &cancelled)
                .is_err()
        );
        let handle = store.original_import_handle()?;
        let refused = handle
            .prepare_restore(&record, &different, limits(), &cancelled)
            .err()
            .expect("a different file is refused");
        assert_eq!(refused.code(), "OriginalContentMismatch");
        assert_eq!(std::fs::read(&object)?, damage, "nothing changed");

        let prepared = handle.prepare_restore(&record, &source, limits(), &cancelled)?;
        let quarantined = prepared
            .quarantined
            .clone()
            .expect("damaged copy moved aside");
        assert_eq!(std::fs::read(directory.join(&quarantined))?, damage);
        let restored = store.retain_prepared_original(&prepared.retention, &cancelled)?;
        assert_eq!(restored.record, record);
        assert_eq!(std::fs::read(&object)?, bytes);
        store.snapshot_original(record.object().content(), limits(), &cancelled)?;
        // An intact copy is left alone.
        let again = handle.prepare_restore(&record, &source, limits(), &cancelled)?;
        assert_eq!(again.quarantined, None);
    }
    Ok(())
}

#[test]
fn a_moved_linked_original_is_missing_until_relinked_to_the_same_content() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("linked.deadpan");
    let source = original(scratch.path(), "clip.mp4", b"linked original bytes")?;
    let mut store = ProjectStore::create(&path, &document()?)?;
    let cancelled = AtomicBool::new(false);
    let record = store
        .retain_original(
            &source,
            OriginalOwnership::Linked { bookmark: None },
            limits(),
            &cancelled,
        )?
        .record;
    let moved = scratch.path().join("moved").join("clip.mp4");
    std::fs::create_dir(moved.parent().unwrap())?;
    std::fs::rename(&source, &moved)?;
    assert_eq!(
        store.original_availability(&record)?,
        OriginalAvailability::Missing
    );
    let moved = moved.canonicalize()?;
    let relinked = store.relink_original(
        record.object().content(),
        record.version(),
        LinkedOriginal::new(moved.clone(), None)?,
        limits(),
        &cancelled,
    )?;
    assert_eq!(relinked.version(), record.version() + 1);
    assert_eq!(
        store.original_availability(&relinked)?,
        OriginalAvailability::Present
    );
    std::fs::write(&moved, b"changed bytes, same name")?;
    assert!(matches!(
        store.original_availability(&relinked)?,
        OriginalAvailability::Unreadable { .. }
    ));
    Ok(())
}
