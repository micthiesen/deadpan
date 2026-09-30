use super::*;
use std::{fs, os::unix::fs::symlink, time::Duration};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(20)
}

fn sealed<'a>(destination: &'a Destination, bytes: &[u8]) -> PartialFile<'a> {
    let cancelled = AtomicBool::new(false);
    let mut partial = destination.create_partial(bytes.len() as u64).unwrap();
    partial
        .writer(&cancelled, deadline())
        .unwrap()
        .write_all(bytes)
        .unwrap();
    partial
        .seal(bytes.len() as u64, &cancelled, deadline())
        .unwrap();
    partial
}

fn io_code(error: &io::Error) -> &str {
    error
        .get_ref()
        .unwrap()
        .downcast_ref::<FsError>()
        .unwrap()
        .code()
}

#[test]
fn bounded_copy_readback_and_no_replace_commit_preserve_owner_writable_mode() {
    let folder = tempfile::tempdir().unwrap();
    let destination = Destination::pin(folder.path(), OsStr::new("movie.mp4")).unwrap();
    let cancelled = AtomicBool::new(false);
    let bytes = vec![0x52; IO_BYTES + 19];
    let mut partial = destination.create_partial(bytes.len() as u64).unwrap();
    let original_path = partial.path();
    assert!(
        original_path
            .file_name()
            .unwrap()
            .as_bytes()
            .ends_with(b".partial")
    );
    assert_eq!(
        partial.reader(&cancelled, deadline()).err().unwrap().code(),
        "partial_state"
    );
    {
        let mut writer = partial.writer(&cancelled, deadline()).unwrap();
        assert_eq!(writer.write(&bytes).unwrap(), IO_BYTES);
        writer.write_all(&bytes[IO_BYTES..]).unwrap();
        writer.flush().unwrap();
    }
    assert_eq!(fs::metadata(&original_path).unwrap().mode() & 0o777, 0o600);
    partial
        .seal(bytes.len() as u64, &cancelled, deadline())
        .unwrap();
    assert_eq!(
        partial.writer(&cancelled, deadline()).err().unwrap().code(),
        "partial_state"
    );
    let mut actual = Vec::new();
    partial
        .reader(&cancelled, deadline())
        .unwrap()
        .read_to_end(&mut actual)
        .unwrap();
    assert_eq!(actual, bytes);
    partial.commit(&cancelled, deadline()).unwrap();
    assert!(partial.is_published());
    assert_eq!(partial.path(), original_path);
    assert!(!original_path.exists());
    assert_eq!(fs::read(destination.path()).unwrap(), bytes);
    partial.confirm_published().unwrap();
    assert_eq!(
        fs::metadata(destination.path()).unwrap().mode() & 0o777,
        0o600
    );
    let repeated = partial.commit(&cancelled, deadline()).unwrap_err();
    assert!(repeated.published());
    assert_eq!(repeated.code(), "partial_state");
}

#[test]
fn invalid_names_limits_and_existing_entries_never_change_destinations() {
    let folder = tempfile::tempdir().unwrap();
    for name in ["", ".", "..", "../escape", "two/names", "nul\0name"] {
        assert_eq!(
            Destination::pin(folder.path(), OsStr::new(name))
                .err()
                .unwrap()
                .code(),
            "invalid_destination"
        );
    }
    assert!(Destination::pin(folder.path(), OsStr::new(&"a".repeat(256))).is_err());
    fs::write(folder.path().join("existing"), b"keep").unwrap();
    fs::create_dir(folder.path().join("directory")).unwrap();
    symlink("missing", folder.path().join("symlink")).unwrap();
    for name in ["existing", "directory", "symlink"] {
        let error = Destination::pin(folder.path(), OsStr::new(name))
            .err()
            .unwrap();
        assert_eq!(error.code(), "destination_exists");
        assert!(!error.published());
    }
    assert_eq!(fs::read(folder.path().join("existing")).unwrap(), b"keep");
    assert_eq!(
        fs::read_link(folder.path().join("symlink")).unwrap(),
        Path::new("missing")
    );
    let destination = Destination::pin(folder.path(), OsStr::new("movie.mp4")).unwrap();
    for limit in [0, MAX_BYTES + 1] {
        assert_eq!(
            destination.create_partial(limit).err().unwrap().code(),
            "partial_limit"
        );
    }
}

#[test]
fn bounded_write_and_failed_exact_length_keep_partial_bytes() {
    let folder = tempfile::tempdir().unwrap();
    let destination = Destination::pin(folder.path(), OsStr::new("movie.mp4")).unwrap();
    let cancelled = AtomicBool::new(false);
    let mut partial = destination.create_partial(4).unwrap();
    let path = partial.path();
    let error = partial
        .writer(&cancelled, deadline())
        .unwrap()
        .write_all(b"12345")
        .unwrap_err();
    assert_eq!(io_code(&error), "partial_limit");
    assert_eq!(fs::read(&path).unwrap(), b"1234");
    assert_eq!(
        partial.seal(3, &cancelled, deadline()).unwrap_err().code(),
        "partial_state"
    );
    drop(partial);
    assert_eq!(fs::read(path).unwrap(), b"1234");
    assert!(!destination.path().exists());
}

#[test]
fn cancellation_and_deadline_before_commit_leave_complete_partial() {
    for expired in [false, true] {
        let folder = tempfile::tempdir().unwrap();
        let destination = Destination::pin(folder.path(), OsStr::new("movie.mp4")).unwrap();
        let mut partial = sealed(&destination, b"complete movie");
        let cancelled = AtomicBool::new(!expired);
        let end = if expired { Instant::now() } else { deadline() };
        let error = partial.commit(&cancelled, end).unwrap_err();
        assert_eq!(
            error.code(),
            if expired {
                "deadline_exceeded"
            } else {
                "cancelled"
            }
        );
        assert!(!error.published());
        assert_eq!(error.partial_path(), Some(partial.path().as_path()));
        assert_eq!(fs::read(partial.path()).unwrap(), b"complete movie");
        assert!(!destination.path().exists());
    }
}

#[test]
fn control_errors_retain_typed_sources_through_read_and_write_traits() {
    let folder = tempfile::tempdir().unwrap();
    let destination = Destination::pin(folder.path(), OsStr::new("movie.mp4")).unwrap();
    let cancelled = AtomicBool::new(false);
    let mut partial = destination.create_partial(4).unwrap();
    {
        let mut writer = partial.writer(&cancelled, deadline()).unwrap();
        cancelled.store(true, Ordering::Release);
        assert_eq!(io_code(&writer.write(b"test").unwrap_err()), "cancelled");
    }
    cancelled.store(false, Ordering::Release);
    partial
        .writer(&cancelled, deadline())
        .unwrap()
        .write_all(b"test")
        .unwrap();
    partial.seal(4, &cancelled, deadline()).unwrap();
    let mut reader = partial.reader(&cancelled, deadline()).unwrap();
    cancelled.store(true, Ordering::Release);
    assert_eq!(io_code(&reader.read(&mut [0; 4]).unwrap_err()), "cancelled");
}

#[test]
fn final_name_race_is_no_replace_and_keeps_complete_partial() {
    let folder = tempfile::tempdir().unwrap();
    let destination = Destination::pin(folder.path(), OsStr::new("movie.mp4")).unwrap();
    let mut partial = sealed(&destination, b"our movie");
    let error = partial
        .commit_with_sync(&AtomicBool::new(false), deadline(), |phase, _| {
            if phase == SyncPhase::BeforeRename {
                fs::write(destination.path(), b"other movie")?;
            }
            Ok(())
        })
        .unwrap_err();
    assert_eq!(error.code(), "destination_exists");
    assert!(!error.published());
    assert_eq!(fs::read(destination.path()).unwrap(), b"other movie");
    assert_eq!(fs::read(partial.path()).unwrap(), b"our movie");
}

#[test]
fn sibling_destinations_share_the_same_directory_pin() {
    let folder = tempfile::tempdir().unwrap();
    let movie = Destination::pin(folder.path(), OsStr::new("movie.mp4")).unwrap();
    let report = movie.for_name(OsStr::new("movie.report.json")).unwrap();
    assert!(Arc::ptr_eq(&movie.directory, &report.directory));
    let cancelled = AtomicBool::new(false);
    let mut report_file = sealed(&report, b"report");
    let mut movie_file = sealed(&movie, b"movie");
    report_file.commit(&cancelled, deadline()).unwrap();
    movie_file.commit(&cancelled, deadline()).unwrap();
    report_file.confirm_published().unwrap();
    assert_eq!(fs::read(report.path()).unwrap(), b"report");
    assert_eq!(fs::read(movie.path()).unwrap(), b"movie");
}

#[test]
fn replaced_partial_entry_never_writes_or_deletes_foreign_target() {
    let folder = tempfile::tempdir().unwrap();
    let destination = Destination::pin(folder.path(), OsStr::new("movie.mp4")).unwrap();
    let mut partial = sealed(&destination, b"our movie");
    let path = partial.path();
    let moved = folder.path().join("retained");
    fs::rename(&path, &moved).unwrap();
    let foreign = folder.path().join("foreign");
    fs::write(&foreign, b"foreign data").unwrap();
    symlink(&foreign, &path).unwrap();
    let error = partial
        .commit(&AtomicBool::new(false), deadline())
        .unwrap_err();
    assert_eq!(error.code(), "destination_changed");
    assert!(!error.published());
    drop(partial);
    assert_eq!(fs::read_link(&path).unwrap(), foreign);
    assert_eq!(fs::read(&foreign).unwrap(), b"foreign data");
    assert_eq!(fs::read(moved).unwrap(), b"our movie");
    assert!(!destination.path().exists());
}

#[test]
fn extra_hard_link_and_same_inode_mutation_reject_readback_and_commit() {
    for hard_link in [false, true] {
        let folder = tempfile::tempdir().unwrap();
        let destination = Destination::pin(folder.path(), OsStr::new("movie.mp4")).unwrap();
        let mut partial = sealed(&destination, b"our movie");
        if hard_link {
            fs::hard_link(partial.path(), folder.path().join("alias")).unwrap();
        } else {
            fs::OpenOptions::new()
                .write(true)
                .open(partial.path())
                .unwrap()
                .write_all_at(b"bad", 0)
                .unwrap();
        }
        let cancelled = AtomicBool::new(false);
        assert_eq!(
            partial.reader(&cancelled, deadline()).err().unwrap().code(),
            "destination_changed"
        );
        assert_eq!(
            partial.commit(&cancelled, deadline()).unwrap_err().code(),
            "destination_changed"
        );
        assert!(partial.path().exists());
        assert!(!destination.path().exists());
    }
}

#[test]
fn replaced_parent_or_ancestor_fails_even_if_final_directory_inode_survives() {
    for move_child_back in [false, true] {
        let folder = tempfile::tempdir().unwrap();
        let parent = folder.path().join("parent");
        let child = parent.join("child");
        fs::create_dir_all(&child).unwrap();
        let destination = Destination::pin(&child, OsStr::new("movie.mp4")).unwrap();
        let mut partial = sealed(&destination, b"our movie");
        let partial_name = partial.path().file_name().unwrap().to_owned();
        let moved = folder.path().join("moved");
        fs::rename(&parent, &moved).unwrap();
        fs::create_dir(&parent).unwrap();
        if move_child_back {
            fs::rename(moved.join("child"), &child).unwrap();
        } else {
            fs::create_dir(&child).unwrap();
        }
        let error = partial
            .commit(&AtomicBool::new(false), deadline())
            .unwrap_err();
        assert_eq!(error.code(), "destination_changed");
        assert!(!error.published());
        assert!(!destination.path().exists());
        let retained = if move_child_back {
            child
        } else {
            moved.join("child")
        };
        assert_eq!(fs::read(retained.join(partial_name)).unwrap(), b"our movie");
        assert!(destination.for_name(OsStr::new("report.json")).is_err());
    }
}

#[test]
fn initial_directory_alias_is_allowed_but_later_retargeting_is_rejected() {
    let folder = tempfile::tempdir().unwrap();
    let selected = folder.path().join("selected");
    let actual = folder.path().join("actual");
    let other = folder.path().join("other");
    fs::create_dir(&actual).unwrap();
    fs::create_dir(&other).unwrap();
    symlink(&actual, &selected).unwrap();
    let destination = Destination::pin(&selected, OsStr::new("movie.mp4")).unwrap();
    let mut partial = sealed(&destination, b"our movie");
    fs::remove_file(&selected).unwrap();
    symlink(&other, &selected).unwrap();
    assert_eq!(
        partial
            .commit(&AtomicBool::new(false), deadline())
            .unwrap_err()
            .code(),
        "destination_changed"
    );
    assert_eq!(fs::read(partial.path()).unwrap(), b"our movie");
    assert!(!other.join("movie.mp4").exists());
}

#[test]
fn every_sync_failure_reports_the_correct_side_of_the_commit_point() {
    for fail_at in [
        SyncPhase::BeforeRename,
        SyncPhase::PublishedFile,
        SyncPhase::PublishedDirectory,
        SyncPhase::AfterDirectory,
    ] {
        let folder = tempfile::tempdir().unwrap();
        let destination = Destination::pin(folder.path(), OsStr::new("movie.mp4")).unwrap();
        let mut partial = sealed(&destination, b"our movie");
        let error = partial
            .commit_with_sync(&AtomicBool::new(false), deadline(), |phase, _| {
                if phase == fail_at {
                    Err(io::Error::other("injected sync failure"))
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
        assert_eq!(error.code(), "publication_durability");
        let published = fail_at != SyncPhase::BeforeRename;
        assert_eq!(error.published(), published);
        assert_eq!(partial.is_published(), published);
        assert_eq!(destination.path().exists(), published);
        assert_eq!(partial.path().exists(), !published);
        let retained = if published {
            destination.path()
        } else {
            partial.path()
        };
        drop(partial);
        assert_eq!(fs::read(retained).unwrap(), b"our movie");
    }
}

#[test]
fn cancellation_before_rename_retains_partial_but_after_rename_does_not_undo_success() {
    for cancel_at in [SyncPhase::BeforeRename, SyncPhase::PublishedFile] {
        let folder = tempfile::tempdir().unwrap();
        let destination = Destination::pin(folder.path(), OsStr::new("movie.mp4")).unwrap();
        let mut partial = sealed(&destination, b"our movie");
        let cancelled = AtomicBool::new(false);
        let mut phases = Vec::new();
        let result = partial.commit_with_sync(&cancelled, deadline(), |phase, file| {
            phases.push(phase);
            if phase == cancel_at {
                cancelled.store(true, Ordering::Release);
            }
            if phase == SyncPhase::PublishedDirectory {
                fsync(file).map_err(Into::into)
            } else {
                full_sync(file)
            }
        });
        if cancel_at == SyncPhase::BeforeRename {
            assert_eq!(result.unwrap_err().code(), "cancelled");
            assert_eq!(phases, [SyncPhase::BeforeRename]);
            assert!(!destination.path().exists());
        } else {
            result.unwrap();
            assert_eq!(phases.len(), 4);
            partial.confirm_published().unwrap();
            assert_eq!(fs::read(destination.path()).unwrap(), b"our movie");
        }
    }
}

#[test]
fn final_name_replacement_and_later_report_mutation_are_observed_without_unlink() {
    let folder = tempfile::tempdir().unwrap();
    let destination = Destination::pin(folder.path(), OsStr::new("report.json")).unwrap();
    let mut partial = sealed(&destination, b"our report");
    let error = partial
        .commit_with_sync(&AtomicBool::new(false), deadline(), |phase, _| {
            if phase == SyncPhase::AfterDirectory {
                fs::rename(destination.path(), folder.path().join("retained-report"))?;
                fs::write(destination.path(), b"foreign report")?;
            }
            Ok(())
        })
        .unwrap_err();
    assert!(error.published());
    assert_eq!(error.code(), "destination_changed");
    assert_eq!(fs::read(destination.path()).unwrap(), b"foreign report");
    drop(partial);
    assert_eq!(
        fs::read(folder.path().join("retained-report")).unwrap(),
        b"our report"
    );

    let destination = Destination::pin(folder.path(), OsStr::new("other-report.json")).unwrap();
    let mut partial = sealed(&destination, b"stable report");
    partial.commit(&AtomicBool::new(false), deadline()).unwrap();
    fs::write(destination.path(), b"changed report").unwrap();
    let error = partial.confirm_published().unwrap_err();
    assert!(error.published());
    assert_eq!(error.code(), "destination_changed");
    assert_eq!(fs::read(destination.path()).unwrap(), b"changed report");
}

#[test]
fn published_hash_admission_rejects_same_length_mutation_with_restored_mtime() {
    let folder = tempfile::tempdir().unwrap();
    let destination = Destination::pin(folder.path(), OsStr::new("movie.mp4")).unwrap();
    let original = b"verified movie";
    let changed = b"tampered movie";
    assert_eq!(original.len(), changed.len());
    let mut partial = sealed(&destination, original);
    let uncancelled = AtomicBool::new(false);
    assert_eq!(
        partial
            .published_reader(&uncancelled, deadline())
            .err()
            .unwrap()
            .code(),
        "partial_state"
    );
    let original_mtime = partial.file.metadata().unwrap().modified().unwrap();
    partial
        .commit_with_sync(&uncancelled, deadline(), |phase, file| {
            if phase == SyncPhase::PublishedFile {
                file.write_all_at(changed, 0)?;
                file.set_modified(original_mtime)?;
            }
            if phase == SyncPhase::PublishedDirectory {
                fsync(file).map_err(Into::into)
            } else {
                full_sync(file)
            }
        })
        .unwrap();
    // Rename legitimately changes ctime. The filesystem commit cannot use the
    // old ctime to distinguish this mutation, so the host must rehash bytes.
    partial.confirm_published().unwrap();
    assert_eq!(
        partial.file.metadata().unwrap().modified().unwrap(),
        original_mtime
    );
    let mut observed = Vec::new();
    partial
        .published_reader(&uncancelled, deadline())
        .unwrap()
        .read_to_end(&mut observed)
        .unwrap();
    assert_eq!(observed, changed);
    let error = super::super::confirm_published_bytes(
        &partial,
        &super::super::digest(original),
        original.len() as u64,
        &uncancelled,
        deadline(),
    )
    .unwrap_err();
    assert_eq!(error.code, "published_hash_mismatch");
    assert!(partial.is_published());
    assert_eq!(fs::read(destination.path()).unwrap(), changed);
}
