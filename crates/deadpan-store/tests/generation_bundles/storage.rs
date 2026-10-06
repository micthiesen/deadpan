//! Reference tracking, explicit cleanup and portable copies over synthetic
//! admitted bundles. The store verifies bytes, not decoded video.
use super::*;
use std::os::fd::AsFd;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_store::original_media::{OriginalMediaLimits, OriginalOwnership};
use deadpan_store::portable::copy_portable;
use deadpan_store::storage::{
    CleanupPolicy, EntryState, ReferenceClass, RemovedEntry, StorageReport,
};

/// A Ready variant whose three masters have their own bytes.
pub(super) fn distinct_variant(
    store: &mut ProjectStore,
    request: &StoredGenerationRequest,
    attempt: &str,
    ordinal: u64,
    tag: &str,
) -> Result<(MessageIdentity, BundleValidationReceipt, [Vec<u8>; 3])> {
    let bytes = [
        format!("native master {tag}").into_bytes(),
        format!("sampled master {tag}").into_bytes(),
        format!("provenance {tag}").into_bytes(),
    ];
    for value in &bytes {
        store.promote_generated_object(&mut Cursor::new(value), &object(value), media_limits())?;
    }
    let identity = begin(store, request, attempt)?;
    let candidate = native_candidate_for(request, ordinal);
    complete_bridge(store, &identity, &candidate)?;
    let receipt = BundleValidationReceipt::new(
        &candidate,
        object(&bytes[0]),
        object(&bytes[1]),
        object(&bytes[2]),
        constraints().video,
        plan(),
        ValidatorIdentity::new("deadpan-media", "bridge-1")?,
    )?
    .with_admission(admission(request.binding.context_sha256.clone()))?;
    store.record_generation_bundle_ready(&identity, &candidate, receipt.clone(), media_limits())?;
    Ok((identity, receipt, bytes))
}

pub(super) fn state(report: &StorageReport, bytes: &[u8]) -> EntryState {
    let digest = object(bytes).content().digest().to_owned();
    report
        .namespace("generated")
        .unwrap()
        .entries
        .iter()
        .find(|entry| entry.digest.as_deref() == Some(digest.as_str()))
        .map_or(EntryState::Unexpected, |entry| entry.state.clone())
}

fn referenced_by(report: &StorageReport, bytes: &[u8]) -> Vec<ReferenceClass> {
    match state(report, bytes) {
        EntryState::Referenced { by } => by,
        other => panic!("expected a referenced object, found {other:?}"),
    }
}

struct Accepted {
    package: PathBuf,
    store: ProjectStore,
    accepted: [Vec<u8>; 3],
    discarded: [Vec<u8>; 3],
    orphan: Vec<u8>,
}

/// Two variants of one request: the first accepted then undone, the second
/// discarded, plus an object no row ever named.
fn accepted_then_undone(scratch: &Path) -> Result<Accepted> {
    let package = scratch.join("storage.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let request = allocate_bridge(&mut store, "request", 7)?;
    publish_inputs(&mut store)?;
    let (first, first_receipt, accepted) =
        distinct_variant(&mut store, &request, "attempt-1", 1, "one")?;
    let (second, _, discarded) = distinct_variant(&mut store, &request, "attempt-2", 2, "two")?;
    store.select_generation_bundle_variant(&first)?;
    let input = GenerationAcceptance {
        expected_revision: store.snapshot()?.revision_id().clone(),
        new_revision: RevisionId::new("accepted")?,
        identity: first,
        expected_receipt: first_receipt,
        native_asset: AssetId::new("native")?,
        sampled_asset: AssetId::new("sampled")?,
    };
    store.accept_generation_bundle(
        &input,
        &unchanged_relevance(&store, &input.new_revision)?,
        media_limits(),
    )?;
    store.discard_generation_bundle_variant(&second)?;
    let orphan = b"published but never recorded".to_vec();
    store.promote_generated_object(&mut Cursor::new(&orphan), &object(&orphan), media_limits())?;
    let undo = RevisionId::new("undo-acceptance")?;
    store.undo_reconciled(
        &RevisionId::new("accepted")?,
        undo.clone(),
        &unchanged_relevance(&store, &undo)?,
    )?;
    assert!(
        store.snapshot()?.assets().is_empty(),
        "the head no longer names the masters"
    );
    Ok(Accepted {
        package,
        store,
        accepted,
        discarded,
        orphan,
    })
}

#[test]
fn history_receipts_and_registers_pin_objects_and_only_discards_are_removable() -> Result {
    let scratch = tempfile::tempdir()?;
    let fixture = accepted_then_undone(scratch.path())?;
    let report = fixture.store.storage_report(Duration::ZERO)?;
    for bytes in &fixture.accepted {
        // An undone acceptance stays in history; its receipt still pins it.
        let by = referenced_by(&report, bytes);
        assert!(by.contains(&ReferenceClass::GenerationReceipt), "{by:?}");
    }
    assert!(referenced_by(&report, &fixture.accepted[0]).contains(&ReferenceClass::History));
    for bytes in INPUT_BYTES {
        assert!(matches!(
            state(&report, bytes),
            EntryState::Referenced { .. }
        ));
    }
    for bytes in fixture.discarded.iter().chain([&fixture.orphan]) {
        assert_eq!(state(&report, bytes), EntryState::Unreferenced);
    }
    let generated = report.namespace("generated").unwrap();
    assert_eq!(generated.removable_entries, 4);
    // A grace period keeps fresh unreferenced objects.
    let fresh = fixture.store.storage_report(Duration::from_secs(3600))?;
    assert_eq!(fresh.removable_bytes, 0);
    Ok(())
}

#[test]
fn cleanup_is_explicit_skips_readers_and_keeps_history_usable() -> Result {
    let scratch = tempfile::tempdir()?;
    let Accepted {
        package,
        mut store,
        accepted,
        discarded,
        orphan,
    } = accepted_then_undone(scratch.path())?;
    let everything = |dry_run| CleanupPolicy::everything(Duration::ZERO, dry_run);

    // A dry run names the candidates and changes nothing.
    let preview = store.clean_storage(everything(true))?;
    assert_eq!(preview.removed.len(), 4);
    for bytes in discarded.iter().chain([&orphan]) {
        assert!(stored_path(&package, &object(bytes)).exists());
    }
    // The default grace period protects freshly published objects.
    let graced = store.clean_storage(CleanupPolicy::everything(
        deadpan_store::storage::DEFAULT_GRACE,
        false,
    ))?;
    assert!(graced.removed.is_empty());

    // A reader (in any process) holds a shared lock while it reads.
    let held = fs::File::open(stored_path(&package, &object(&discarded[0])))?;
    rustix::fs::flock(held.as_fd(), rustix::fs::FlockOperation::LockShared)?;
    let outcome = store.clean_storage(everything(false))?;
    assert_eq!(outcome.removed.len(), 3, "{outcome:?}");
    assert_eq!(outcome.in_use.len(), 1);
    assert!(stored_path(&package, &object(&discarded[0])).exists());
    for bytes in discarded[1..].iter().chain([&orphan]) {
        assert!(!stored_path(&package, &object(bytes)).exists());
    }
    drop(held);
    let outcome = store.clean_storage(everything(false))?;
    assert_eq!(outcome.removed.len(), 1);
    assert!(!stored_path(&package, &object(&discarded[0])).exists());
    assert!(store.clean_storage(everything(false))?.removed.is_empty());

    // Everything a retained revision or live receipt names is intact, and
    // the undone acceptance redoes with all of it.
    for bytes in accepted.iter().map(Vec::as_slice).chain(INPUT_BYTES) {
        store.snapshot_generated_object(&object(bytes), media_limits())?;
    }
    let redo = RevisionId::new("redo-acceptance")?;
    store.redo_reconciled(
        &RevisionId::new("undo-acceptance")?,
        redo.clone(),
        &unchanged_relevance(&store, &redo)?,
    )?;
    assert_eq!(store.snapshot()?.assets().len(), 2);
    store.validate_full()?;
    drop(store);

    // Cleanup needs the writable store.
    let read_only = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert!(matches!(
        read_only.clean_storage_read_only_probe(),
        Err(StoreError::ReadOnly)
    ));
    Ok(())
}

trait ReadOnlyProbe {
    fn clean_storage_read_only_probe(self) -> std::result::Result<(), StoreError>;
}

impl ReadOnlyProbe for ProjectStore {
    fn clean_storage_read_only_probe(mut self) -> std::result::Result<(), StoreError> {
        self.clean_storage(CleanupPolicy::everything(Duration::ZERO, true))
            .map(|_| ())
    }
}

#[test]
fn stale_unaccepted_variants_become_removable_but_accepted_ones_stay() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("stale.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let request = allocate_bridge(&mut store, "request", 7)?;
    publish_inputs(&mut store)?;
    let (_, _, offered) = distinct_variant(&mut store, &request, "attempt-1", 1, "offered")?;
    let report = store.storage_report(Duration::ZERO)?;
    // A present Ready variant of a current request can still be accepted.
    for bytes in &offered {
        assert_eq!(
            referenced_by(&report, bytes),
            vec![ReferenceClass::GenerationReceipt]
        );
    }
    // A new request for the Hold makes it stale: it can never be accepted.
    allocate_bridge(&mut store, "replacement", 20)?;
    let report = store.storage_report(Duration::ZERO)?;
    for bytes in &offered {
        assert_eq!(state(&report, bytes), EntryState::Unreferenced);
    }
    Ok(())
}

#[test]
fn portable_copy_owns_linked_originals_and_only_referenced_media() -> Result {
    let scratch = tempfile::tempdir()?;
    let Accepted {
        package,
        mut store,
        accepted,
        discarded,
        orphan,
    } = accepted_then_undone(scratch.path())?;
    // A linked original outside the package.
    let outside = scratch.path().join("outside");
    fs::create_dir(&outside)?;
    let linked = outside.join("source.bin");
    fs::write(&linked, b"original bytes kept outside the package")?;
    let linked = linked.canonicalize()?;
    let limits = OriginalMediaLimits::default();
    let cancelled = AtomicBool::new(false);
    let retained = store.retain_original(
        &linked,
        OriginalOwnership::Linked { bookmark: None },
        limits,
        &cancelled,
    )?;
    assert!(!retained.record.managed());
    let head = store.snapshot()?.revision_id().clone();
    drop(store);

    let elsewhere = scratch.path().join("elsewhere");
    fs::create_dir(&elsewhere)?;
    let destination = elsewhere.join("Portable.deadpan");
    let report = copy_portable(&package, &destination, &cancelled)?;
    assert_eq!(report.destination, destination.canonicalize()?);
    assert_eq!(report.revision_id, head.as_str());
    assert_eq!(report.originals.len(), 1);
    assert_eq!(report.originals[0].from, "linked");
    assert_eq!(report.omitted_generated, 4);
    let copied: Vec<_> = report
        .generated
        .iter()
        .map(|object| object.digest.clone())
        .collect();
    for bytes in accepted.iter().map(Vec::as_slice).chain(INPUT_BYTES) {
        assert!(copied.contains(&object(bytes).content().digest().to_owned()));
    }
    for bytes in discarded.iter().chain([&orphan]) {
        assert!(!stored_path(&destination, &object(bytes)).exists());
    }
    // A second copy to the same name is refused, never merged.
    assert!(matches!(
        copy_portable(&package, &destination, &cancelled),
        Err(StoreError::PackageAlreadyExists(_))
    ));
    // No staging directory remains beside the copy.
    assert_eq!(fs::read_dir(&elsewhere)?.count(), 1);

    // The copy needs neither the source package nor the linked file.
    fs::remove_dir_all(&package)?;
    fs::remove_dir_all(&outside)?;
    let mut copy = ProjectStore::open(&destination, AccessMode::ReadWrite)?;
    copy.validate_full()?;
    let record = copy
        .original_record(retained.record.object().content())?
        .unwrap();
    assert!(record.managed());
    assert!(record.linked().is_none());
    copy.snapshot_original(retained.record.object().content(), limits, &cancelled)?;
    let redo = RevisionId::new("redo-in-copy")?;
    copy.redo_reconciled(&head, redo.clone(), &unchanged_relevance(&copy, &redo)?)?;
    assert_eq!(copy.snapshot()?.assets().len(), 2);
    for bytes in accepted.iter().map(Vec::as_slice).chain(INPUT_BYTES) {
        copy.snapshot_generated_object(&object(bytes), media_limits())?;
    }
    // The copy has nothing left to clean.
    assert!(
        copy.clean_storage(CleanupPolicy::everything(Duration::ZERO, true))?
            .removed
            .is_empty()
    );
    Ok(())
}

#[test]
fn a_missing_linked_original_refuses_the_copy_and_leaves_nothing() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("missing.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let linked = scratch.path().join("gone.bin");
    fs::write(&linked, b"soon missing")?;
    let linked = linked.canonicalize()?;
    store.retain_original(
        &linked,
        OriginalOwnership::Linked { bookmark: None },
        OriginalMediaLimits::default(),
        &AtomicBool::new(false),
    )?;
    drop(store);
    fs::remove_file(&linked)?;
    let out = scratch.path().join("out");
    fs::create_dir(&out)?;
    let error =
        copy_portable(&package, &out.join("Copy.deadpan"), &AtomicBool::new(false)).unwrap_err();
    assert!(
        error.to_string().contains("could not copy the original"),
        "{error}"
    );
    assert_eq!(fs::read_dir(&out)?.count(), 0);
    Ok(())
}

fn write_object(directory: &Path, bytes: &[u8]) -> Result<PathBuf> {
    let path = directory.join(format!("blake3-{}", object(bytes).content().digest()));
    fs::write(&path, bytes)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444))?;
    Ok(path)
}

#[test]
fn orphaned_originals_and_unfinished_writes_are_removed_but_recorded_originals_stay() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("orphans.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let source = scratch.path().join("kept.bin");
    fs::write(&source, b"a retained original")?;
    let retained = store.retain_original(
        &source.canonicalize()?,
        OriginalOwnership::Managed,
        OriginalMediaLimits::default(),
        &AtomicBool::new(false),
    )?;
    // A copy whose inventory commit never happened, and an interrupted write.
    let orphan = write_object(&package.join("Media/Originals"), b"never recorded original")?;
    let pending = package.join("Media/Generated/.pending-interrupted");
    fs::write(&pending, b"partial")?;
    let report = store.storage_report(Duration::ZERO)?;
    let originals = report.namespace("originals").unwrap();
    assert_eq!(originals.removable_entries, 1);
    assert_eq!(report.namespace("generated").unwrap().pending_bytes, 7);
    let outcome = store.clean_storage(CleanupPolicy::everything(Duration::ZERO, false))?;
    assert_eq!(outcome.removed.len(), 2, "{outcome:?}");
    assert!(!orphan.exists());
    assert!(!pending.exists());
    store.snapshot_original(
        retained.record.object().content(),
        OriginalMediaLimits::default(),
        &AtomicBool::new(false),
    )?;
    Ok(())
}

#[test]
fn confirmed_removal_touches_only_the_previewed_files() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("previewed.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let generated = package.join("Media/Generated");
    let first = write_object(&generated, b"first orphan")?;
    let replaced = write_object(&generated, b"replaced orphan")?;
    // The preview can come from a read-only open, off the writer.
    let preview = ProjectStore::open(&package, AccessMode::ReadOnly)?
        .preview_storage_cleanup(Duration::ZERO)?;
    assert_eq!(preview.removed.len(), 2);
    // After the preview: one entry is replaced by another file under the
    // same name (the Changed path), and a new orphan appears.
    fs::remove_file(&replaced)?;
    write_object(&generated, b"replaced orphan")?;
    let later = write_object(&generated, b"new since the preview")?;
    let outcome = store.clean_previewed_storage(Duration::ZERO, &preview.removed)?;
    assert_eq!(outcome.removed.len(), 1, "{outcome:?}");
    assert!(!first.exists());
    assert!(
        replaced.exists(),
        "a different file under a previewed name stays"
    );
    assert!(later.exists(), "nothing new since the preview is removed");
    // A hand-made entry without a listed identity never matches.
    let forged = RemovedEntry::for_test(
        "generated",
        &format!(
            "blake3-{}",
            object(b"new since the preview").content().digest()
        ),
        21,
    );
    assert!(
        store
            .clean_previewed_storage(Duration::ZERO, &[forged])?
            .removed
            .is_empty()
    );
    Ok(())
}

#[test]
fn checkpoints_pin_what_they_name_and_unreadable_ones_block_cleanup() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("checkpointed.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let request = allocate_bridge(&mut store, "request", 7)?;
    publish_inputs(&mut store)?;
    let (variant, _, bytes) = distinct_variant(&mut store, &request, "attempt-1", 1, "kept")?;
    let checkpoint = store.checkpoint()?;
    store.discard_generation_bundle_variant(&variant)?;
    // The live database no longer needs the discarded variant, but the
    // checkpoint (a restorable database) still names it.
    let report = store.storage_report(Duration::ZERO)?;
    assert_eq!(report.checkpoints.len(), 1, "{:?}", report.checkpoints);
    assert_eq!(
        referenced_by(&report, &bytes[0]),
        vec![ReferenceClass::Checkpoint]
    );
    assert!(
        store
            .clean_storage(CleanupPolicy::everything(Duration::ZERO, false))?
            .removed
            .is_empty()
    );
    let garbage = package.join("Snapshots/garbage.sqlite");
    fs::write(&garbage, b"not a database")?;
    let error = store
        .clean_storage(CleanupPolicy::everything(Duration::ZERO, false))
        .unwrap_err();
    assert!(error.to_string().contains("garbage.sqlite"), "{error}");
    fs::remove_file(&garbage)?;
    fs::remove_file(&checkpoint)?;
    // The discarded variant's three masters and, with no live receipt left,
    // its three conditioning inputs.
    let outcome = store.clean_storage(CleanupPolicy::everything(Duration::ZERO, false))?;
    assert_eq!(outcome.removed.len(), 6, "{outcome:?}");
    Ok(())
}

#[test]
fn portable_copy_detects_media_removed_after_its_database_snapshot() -> Result {
    let scratch = tempfile::tempdir()?;
    let Accepted {
        package, accepted, ..
    } = accepted_then_undone(scratch.path())?;
    let out = scratch.path().join("out");
    fs::create_dir(&out)?;
    // An object the snapshot references disappears before media is listed.
    let victim = stored_path(&package, &object(&accepted[1]));
    let error = deadpan_store::portable::copy_portable_observed(
        &package,
        &out.join("Copy.deadpan"),
        &AtomicBool::new(false),
        &mut || fs::remove_file(&victim).unwrap(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("missing"), "{error}");
    assert_eq!(fs::read_dir(&out)?.count(), 0, "no staging remains");
    // An object published after the snapshot is not part of the copy.
    fs::write(&victim, &accepted[1])?;
    fs::set_permissions(&victim, fs::Permissions::from_mode(0o444))?;
    let late = b"published after the snapshot".to_vec();
    let generated = package.join("Media/Generated");
    let report = deadpan_store::portable::copy_portable_observed(
        &package,
        &out.join("Copy.deadpan"),
        &AtomicBool::new(false),
        &mut || {
            write_object(&generated, &late).unwrap();
        },
    )?;
    assert!(!stored_path(&out.join("Copy.deadpan"), &object(&late)).exists());
    assert!(report.generated.len() >= 6);
    // Cancellation leaves no staging directory.
    let cancelled = AtomicBool::new(false);
    let error = deadpan_store::portable::copy_portable_observed(
        &package,
        &out.join("Cancelled.deadpan"),
        &cancelled,
        &mut || cancelled.store(true, std::sync::atomic::Ordering::Release),
    )
    .unwrap_err();
    assert!(error.to_string().contains("cancelled"), "{error}");
    assert_eq!(fs::read_dir(&out)?.count(), 1);
    Ok(())
}
