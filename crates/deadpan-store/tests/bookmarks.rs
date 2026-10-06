//! Linked originals and sounds carry system bookmarks, so a file that was
//! moved or renamed is found again and relinked only after its bytes match.
#![cfg(target_os = "macos")]

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_core::*;
use deadpan_store::ProjectStore;
use deadpan_store::original_media::{
    OriginalAvailability, OriginalMediaLimits, OriginalOwnership, moved_location,
};

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

fn limits() -> OriginalMediaLimits {
    OriginalMediaLimits::new(1 << 20, Duration::from_secs(30)).expect("limits")
}

fn file(folder: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf> {
    let path = folder.join(name);
    std::fs::write(&path, bytes)?;
    Ok(path.canonicalize()?)
}

#[test]
fn a_moved_and_renamed_linked_file_is_found_and_relinked_after_verification() -> Result {
    let scratch = tempfile::tempdir()?;
    let root = scratch.path().canonicalize()?;
    let source = file(&root, "boing.wav", b"external sound bytes")?;
    let mut store = ProjectStore::create(&root.join("bookmarks.deadpan"), &document()?)?;
    let cancelled = AtomicBool::new(false);
    let record = store
        .retain_original(
            &source,
            OriginalOwnership::linked_at(&source),
            limits(),
            &cancelled,
        )?
        .record;
    let linked = record.linked().ok_or("not linked")?;
    assert!(
        linked
            .bookmark()
            .is_some_and(|bookmark| !bookmark.is_empty())
    );
    // In place: nothing to find.
    assert_eq!(moved_location(&record), None);

    std::fs::create_dir(root.join("Sounds"))?;
    let moved = root.join("Sounds/boing renamed.wav");
    std::fs::rename(&source, &moved)?;
    assert_eq!(
        store.original_availability(&record)?,
        OriginalAvailability::Missing
    );
    let candidate = moved_location(&record).ok_or("bookmark did not find the file")?;
    assert_eq!(candidate.path(), moved);
    assert!(candidate.bookmark().is_some());
    let prepared = store.original_import_handle()?.prepare_relink(
        &record,
        record.version(),
        candidate,
        limits(),
        &cancelled,
    )?;
    let relinked = store.relink_prepared_original(&prepared, &cancelled)?;
    assert_eq!(relinked.version(), record.version() + 1);
    assert_eq!(
        relinked.linked().map(|linked| linked.path()),
        Some(moved.as_path())
    );
    assert_eq!(
        store.original_availability(&relinked)?,
        OriginalAvailability::Present
    );
    store.snapshot_original(relinked.object().content(), limits(), &cancelled)?;
    Ok(())
}

#[test]
fn a_moved_file_whose_bytes_changed_is_never_relinked() -> Result {
    let scratch = tempfile::tempdir()?;
    let root = scratch.path().canonicalize()?;
    let source = file(&root, "clip.mp4", b"the registered original bytes")?;
    let mut store = ProjectStore::create(&root.join("changed.deadpan"), &document()?)?;
    let cancelled = AtomicBool::new(false);
    let record = store
        .retain_original(
            &source,
            OriginalOwnership::linked_at(&source),
            limits(),
            &cancelled,
        )?
        .record;
    let moved = root.join("elsewhere.mp4");
    std::fs::rename(&source, &moved)?;
    // Same length, different content: the bookmark finds it, the hash refuses.
    std::fs::write(&moved, b"THE REGISTERED ORIGINAL BYTES")?;
    let candidate = moved_location(&record).ok_or("bookmark did not find the file")?;
    let refused = store
        .original_import_handle()?
        .prepare_relink(&record, record.version(), candidate, limits(), &cancelled)
        .err()
        .ok_or("changed bytes were accepted")?;
    assert_eq!(refused.code(), "OriginalContentMismatch");
    let current = store
        .original_record(record.object().content())?
        .ok_or("record vanished")?;
    assert_eq!(current, record, "the record is unchanged");
    // A deleted file has nowhere to resolve to.
    std::fs::remove_file(&moved)?;
    assert_eq!(moved_location(&record), None);
    Ok(())
}

#[test]
fn records_without_a_bookmark_and_managed_copies_have_no_candidate() -> Result {
    let scratch = tempfile::tempdir()?;
    let root = scratch.path().canonicalize()?;
    let source = file(&root, "plain.mov", b"bytes")?;
    let other = file(&root, "managed.mov", b"other bytes")?;
    let mut store = ProjectStore::create(&root.join("plain.deadpan"), &document()?)?;
    let cancelled = AtomicBool::new(false);
    let linked = store
        .retain_original(
            &source,
            OriginalOwnership::Linked { bookmark: None },
            limits(),
            &cancelled,
        )?
        .record;
    let managed = store
        .retain_original(&other, OriginalOwnership::Managed, limits(), &cancelled)?
        .record;
    std::fs::rename(&source, root.join("gone.mov"))?;
    assert_eq!(moved_location(&linked), None);
    assert_eq!(moved_location(&managed), None);
    Ok(())
}
