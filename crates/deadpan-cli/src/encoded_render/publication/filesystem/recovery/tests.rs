use super::*;
use std::{fs, time::Duration};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(20)
}

fn identity(mode: u32) -> DurableIdentity {
    DurableIdentity {
        volume_uuid: [1; 16],
        device: 2,
        inode: 3,
        birth_seconds: 4,
        birth_nanoseconds: 5,
        generation: None,
        owner: 501,
        group: 20,
        mode,
        flags: 0,
    }
}

fn file_evidence() -> FileEvidence {
    FileEvidence {
        schema_version: 1,
        identity: identity(0o100600),
        byte_length: 5,
        links: 1,
        modified_seconds: 6,
        modified_nanoseconds: 7,
    }
}

#[test]
fn strict_file_evidence_rejects_missing_identity_and_invalid_extents() {
    let original = serde_json::to_value(file_evidence()).unwrap();
    let decoded: FileEvidence = serde_json::from_value(original.clone()).unwrap();
    assert_eq!(decoded.byte_length(), 5);
    for (key, value) in [
        ("schema_version", serde_json::json!(2)),
        ("byte_length", serde_json::json!(0)),
        ("byte_length", serde_json::json!(MAX_BYTES + 1)),
        ("links", serde_json::json!(2)),
        ("modified_nanoseconds", serde_json::json!(-1)),
        ("modified_nanoseconds", serde_json::json!(1_000_000_000)),
        ("unexpected", serde_json::json!(true)),
    ] {
        let mut bad = original.clone();
        bad[key] = value;
        assert!(
            serde_json::from_value::<FileEvidence>(bad).is_err(),
            "accepted {key}"
        );
    }
    for (key, value) in [
        ("volume_uuid", serde_json::json!(vec![0; 16])),
        ("inode", serde_json::json!(0)),
        ("generation", serde_json::json!(0)),
        ("mode", serde_json::json!(0o100644)),
        ("mode", serde_json::json!(0o040600)),
        ("birth_nanoseconds", serde_json::json!(1_000_000_000)),
        ("unexpected", serde_json::json!(true)),
    ] {
        let mut bad = original.clone();
        bad["identity"][key] = value;
        assert!(
            serde_json::from_value::<FileEvidence>(bad).is_err(),
            "accepted identity {key}"
        );
    }
    let mut absent_birth = original;
    absent_birth["identity"]["birth_seconds"] = serde_json::json!(0);
    absent_birth["identity"]["birth_nanoseconds"] = serde_json::json!(0);
    assert!(serde_json::from_value::<FileEvidence>(absent_birth).is_err());
}

#[test]
fn directory_evidence_has_strict_paths_components_and_non_utf8_round_trip() {
    use std::os::unix::ffi::OsStringExt;
    let path = PathBuf::from(OsString::from_vec(b"/\xff".to_vec()));
    let evidence = DirectoryEvidence {
        schema_version: 1,
        selected: path.clone(),
        canonical: path.clone(),
        selected_entries: vec![
            SelectedEntry {
                path: PathBuf::from("/"),
                identity: identity(0o040755),
                symlink_target: None,
            },
            SelectedEntry {
                path,
                identity: identity(0o040755),
                symlink_target: None,
            },
        ],
        canonical_entries: vec![identity(0o040755); 2],
    };
    let serialized = serde_json::to_value(&evidence).unwrap();
    assert_eq!(
        serde_json::from_value::<DirectoryEvidence>(serialized.clone()).unwrap(),
        evidence
    );
    for (key, value) in [
        ("schema_version", serde_json::json!(0)),
        ("selected", serde_json::json!([47, 0])),
        ("canonical", serde_json::json!([97])),
        ("selected", serde_json::json!(vec![47; MAX_PATH_BYTES + 1])),
        ("selected_entries", serde_json::json!([])),
        ("canonical_entries", serde_json::json!([])),
        ("unexpected", serde_json::json!(1)),
    ] {
        let mut bad = serialized.clone();
        bad[key] = value;
        assert!(
            serde_json::from_value::<DirectoryEvidence>(bad).is_err(),
            "accepted {key}"
        );
    }
    let mut bad = serialized;
    bad["selected_entries"][0]["path"] = serde_json::json!([47, 97]);
    assert!(serde_json::from_value::<DirectoryEvidence>(bad).is_err());
}

#[cfg(target_os = "macos")]
fn staged(destination: &Destination, name: &str, bytes: &[u8]) -> PartialFile {
    let cancelled = AtomicBool::new(false);
    let mut partial = destination
        .create_partial_named(OsStr::new(name), bytes.len() as u64)
        .unwrap();
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

#[cfg(target_os = "macos")]
#[test]
fn unchanged_partial_and_renamed_file_reopen_with_original_evidence() {
    for publish in [false, true] {
        let folder = tempfile::tempdir().unwrap();
        let destination = Destination::pin(folder.path(), OsStr::new("movie.mp4")).unwrap();
        let mut partial = staged(&destination, ".recorded.partial", b"owned movie");
        let directory = destination.evidence().unwrap();
        let evidence = partial.evidence().unwrap();
        if publish {
            partial.commit(&AtomicBool::new(false), deadline()).unwrap();
        }
        let name = if publish {
            "movie.mp4"
        } else {
            ".recorded.partial"
        };
        let recovered = RecoveredDirectory::open(&directory).unwrap();
        assert_eq!(
            recovered
                .open_file(OsStr::new(name), &evidence)
                .err()
                .unwrap()
                .code(),
            "publication_locked"
        );
        drop(partial);
        let file = recovered
            .open_file(OsStr::new(name), &evidence)
            .unwrap()
            .unwrap();
        let mut bytes = Vec::new();
        file.reader(&AtomicBool::new(false), deadline())
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, b"owned movie");
        file.sync_verified().unwrap();
        file.confirm().unwrap();
        assert_eq!(
            recovered
                .open_file(OsStr::new(name), &evidence)
                .err()
                .unwrap()
                .code(),
            "publication_locked"
        );
        assert!(
            recovered
                .open_file(OsStr::new("absent"), &evidence)
                .unwrap()
                .is_none()
        );
        drop(file);
        assert!(
            recovered
                .open_file(OsStr::new(name), &evidence)
                .unwrap()
                .is_some()
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn byte_identical_replacement_is_rejected_even_with_restored_mtime() {
    let folder = tempfile::tempdir().unwrap();
    let destination = Destination::pin(folder.path(), OsStr::new("movie.mp4")).unwrap();
    let partial = staged(&destination, ".recorded.partial", b"owned movie");
    let directory = destination.evidence().unwrap();
    let evidence = partial.evidence().unwrap();
    let path = partial.path();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    drop(partial);
    fs::rename(&path, folder.path().join("old-owned-object")).unwrap();
    let file = File::options()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    fchmod(&file, Mode::RUSR | Mode::WUSR).unwrap();
    file.write_all_at(b"owned movie", 0).unwrap();
    file.set_modified(modified).unwrap();
    let recovered = RecoveredDirectory::open(&directory).unwrap();
    assert_eq!(
        recovered
            .open_file(OsStr::new(".recorded.partial"), &evidence)
            .err()
            .unwrap()
            .code(),
        "destination_changed"
    );
    assert_eq!(fs::read(&path).unwrap(), b"owned movie");
    assert_eq!(
        fs::read(folder.path().join("old-owned-object")).unwrap(),
        b"owned movie"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn foreign_links_symlinks_modes_and_types_are_never_read_or_modified() {
    use std::os::unix::fs::symlink;
    for variant in ["hardlink", "symlink", "mode", "directory"] {
        let folder = tempfile::tempdir().unwrap();
        let destination = Destination::pin(folder.path(), OsStr::new("movie.mp4")).unwrap();
        let partial = staged(&destination, ".recorded.partial", b"owned movie");
        let directory = destination.evidence().unwrap();
        let evidence = partial.evidence().unwrap();
        let path = partial.path();
        drop(partial);
        match variant {
            "hardlink" => fs::hard_link(&path, folder.path().join("alias")).unwrap(),
            "symlink" => {
                fs::rename(&path, folder.path().join("retained")).unwrap();
                symlink("retained", &path).unwrap();
            }
            "mode" => fchmod(
                File::open(&path).unwrap(),
                Mode::RUSR | Mode::WUSR | Mode::RGRP,
            )
            .unwrap(),
            "directory" => {
                fs::rename(&path, folder.path().join("retained")).unwrap();
                fs::create_dir(&path).unwrap();
            }
            _ => unreachable!(),
        }
        let recovered = RecoveredDirectory::open(&directory).unwrap();
        assert_eq!(
            recovered
                .open_file(OsStr::new(".recorded.partial"), &evidence)
                .err()
                .unwrap()
                .code(),
            "destination_changed",
            "{variant}"
        );
        assert!(path.exists());
    }
}

#[cfg(target_os = "macos")]
#[test]
fn selected_symlink_replacement_and_canonical_ancestor_replacement_fail() {
    use std::os::unix::fs::symlink;
    let folder = tempfile::tempdir().unwrap();
    let actual = folder.path().join("actual");
    fs::create_dir(&actual).unwrap();
    let selected = folder.path().join("selected");
    symlink(&actual, &selected).unwrap();
    let destination = Destination::pin(&selected, OsStr::new("movie.mp4")).unwrap();
    let evidence = destination.evidence().unwrap();
    fs::remove_file(&selected).unwrap();
    symlink(&actual, &selected).unwrap();
    assert_eq!(
        RecoveredDirectory::open(&evidence).err().unwrap().code(),
        "destination_changed"
    );

    let parent = folder.path().join("parent");
    let child = parent.join("child");
    fs::create_dir_all(&child).unwrap();
    let destination = Destination::pin(&child, OsStr::new("movie.mp4")).unwrap();
    let evidence = destination.evidence().unwrap();
    fs::rename(&parent, folder.path().join("old-parent")).unwrap();
    fs::create_dir(&parent).unwrap();
    fs::rename(folder.path().join("old-parent/child"), &child).unwrap();
    assert_eq!(
        RecoveredDirectory::open(&evidence).err().unwrap().code(),
        "destination_changed"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn recovery_reads_are_bounded_controlled_and_detect_same_inode_mutation() {
    let folder = tempfile::tempdir().unwrap();
    let destination = Destination::pin(folder.path(), OsStr::new("movie.mp4")).unwrap();
    let partial = staged(&destination, ".recorded.partial", &vec![7; IO_BYTES + 1]);
    let directory = destination.evidence().unwrap();
    let evidence = partial.evidence().unwrap();
    let path = partial.path();
    drop(partial);
    let recovered = RecoveredDirectory::open(&directory).unwrap();
    let file = recovered
        .open_file(OsStr::new(".recorded.partial"), &evidence)
        .unwrap()
        .unwrap();
    assert_eq!(
        file.reader(&AtomicBool::new(true), deadline())
            .err()
            .unwrap()
            .code(),
        "cancelled"
    );
    assert_eq!(
        file.reader(&AtomicBool::new(false), Instant::now())
            .err()
            .unwrap()
            .code(),
        "deadline_exceeded"
    );
    let cancelled = AtomicBool::new(false);
    let mut reader = file.reader(&cancelled, deadline()).unwrap();
    assert_eq!(reader.read(&mut vec![0; IO_BYTES + 1]).unwrap(), IO_BYTES);
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let foreign = File::options().write(true).open(&path).unwrap();
    foreign.write_all_at(b"x", 0).unwrap();
    foreign.set_modified(modified).unwrap();
    let error = reader.read(&mut [0; 1]).unwrap_err();
    assert_eq!(
        error
            .get_ref()
            .unwrap()
            .downcast_ref::<FsError>()
            .unwrap()
            .code(),
        "destination_changed"
    );
    assert!(file.sync_verified().is_err());
    assert_eq!(fs::read(&path).unwrap()[0], b'x');
}

#[cfg(not(target_os = "macos"))]
#[test]
fn durable_identity_is_explicitly_unavailable_off_macos() {
    let folder = tempfile::tempdir().unwrap();
    let destination = Destination::pin(folder.path(), OsStr::new("movie.mp4")).unwrap();
    assert_eq!(
        destination.evidence().unwrap_err().code(),
        "unsupported_recovery_identity"
    );
}
