use super::*;
use std::io::{Seek, Write};

#[test]
fn returns_exact_opaque_bytes_without_parsing_or_changing_the_file() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("keymap.json");
    let bytes = b"not JSON\0\xff\n";
    std::fs::write(&path, bytes).unwrap();
    let loaded = read_from(path.clone());
    assert_eq!(loaded.path.as_ref(), Some(&path));
    assert_eq!(loaded.contents.unwrap().unwrap(), bytes);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(std::fs::read_dir(scratch.path()).unwrap().count(), 1);
}

#[test]
fn missing_file_and_parents_remain_missing_without_directory_creation() {
    let scratch = tempfile::tempdir().unwrap();
    for path in [
        scratch.path().join("keymap.json"),
        scratch.path().join("uncreated/Deadpan/keymap.json"),
    ] {
        let loaded = read_from(path.clone());
        assert_eq!(loaded.path, Some(path));
        assert_eq!(loaded.contents.unwrap(), None);
    }
    assert_eq!(std::fs::read_dir(scratch.path()).unwrap().count(), 0);
}

#[test]
fn empty_file_is_distinct_from_an_absent_keymap() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("keymap.json");
    std::fs::write(&path, []).unwrap();
    assert_eq!(read_from(path).contents.unwrap(), Some(vec![]));
}

#[test]
fn relative_paths_are_rejected_without_cwd_resolution() {
    let cwd = std::env::current_dir().unwrap();
    for relative in ["", "keymap.json", "Cargo.toml", "../keymap.json"] {
        let path = PathBuf::from(relative);
        let loaded = read_from(path.clone());
        assert_eq!(loaded.path, Some(path));
        assert!(loaded.contents.unwrap_err().contains("must be absolute"));
    }
    assert_eq!(std::env::current_dir().unwrap(), cwd);
}

#[test]
fn directory_and_non_directory_parent_fail_truthfully_without_replacement() {
    let scratch = tempfile::tempdir().unwrap();
    let error = read_from(scratch.path().to_owned()).contents.unwrap_err();
    assert!(error.contains("must be a regular file"));
    let parent = scratch.path().join("file-parent");
    std::fs::write(&parent, b"keep").unwrap();
    let path = parent.join("keymap.json");
    let loaded = read_from(path.clone());
    assert_eq!(loaded.path, Some(path.clone()));
    let error = loaded.contents.unwrap_err();
    assert!(error.contains("Cannot open keymap") && error.contains(&path.display().to_string()));
    assert_eq!(std::fs::read(&parent).unwrap(), b"keep");
    assert_eq!(std::fs::read_dir(scratch.path()).unwrap().count(), 1);
}

#[test]
fn accepts_exact_limit_and_rejects_larger_metadata_without_writing() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("keymap.json");
    let bytes = vec![b' '; MAX_BYTES];
    std::fs::write(&path, &bytes).unwrap();
    assert_eq!(read_from(path.clone()).contents.unwrap().unwrap(), bytes);
    let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_len((MAX_BYTES + 1) as u64).unwrap();
    assert!(
        read_from(path.clone())
            .contents
            .unwrap_err()
            .contains(TOO_LARGE)
    );
    assert_eq!(
        std::fs::metadata(path).unwrap().len(),
        (MAX_BYTES + 1) as u64
    );
}

#[test]
fn descriptor_growth_after_metadata_is_bounded_to_one_excess_byte() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("keymap.json");
    std::fs::write(&path, b"{}").unwrap();
    let mut reader = File::open(&path).unwrap();
    assert_eq!(reader.metadata().unwrap().len(), 2);
    // Deterministically model growth after the production descriptor check.
    let mut writer = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writer.write_all(&vec![b' '; MAX_BYTES + 32]).unwrap();
    drop(writer);
    assert_eq!(read_bounded(&mut reader).unwrap_err(), TOO_LARGE);
    assert_eq!(reader.stream_position().unwrap(), (MAX_BYTES + 1) as u64);
}

#[test]
fn read_errors_never_return_partial_contents() {
    struct Failure;
    impl Read for Failure {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("injected descriptor read failure"))
        }
    }
    let reader = b"partial".as_slice().chain(Failure);
    assert_eq!(
        read_bounded(reader).unwrap_err(),
        "injected descriptor read failure"
    );
}

#[cfg(unix)]
#[test]
fn ordinary_symlinks_follow_regular_targets_but_not_directories() {
    let scratch = tempfile::tempdir().unwrap();
    let target = scratch.path().join("actual.json");
    let link = scratch.path().join("keymap.json");
    std::fs::write(&target, b"{}").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let loaded = read_from(link.clone());
    assert_eq!(loaded.path, Some(link));
    assert_eq!(loaded.contents.unwrap(), Some(b"{}".to_vec()));
    let directory_link = scratch.path().join("directory-link");
    std::os::unix::fs::symlink(scratch.path(), &directory_link).unwrap();
    assert!(
        read_from(directory_link)
            .contents
            .unwrap_err()
            .contains("regular file")
    );
}

#[cfg(unix)]
#[test]
fn missing_symlink_target_is_absent_and_a_symlink_loop_is_an_error() {
    let scratch = tempfile::tempdir().unwrap();
    let missing = scratch.path().join("missing-link");
    std::os::unix::fs::symlink(scratch.path().join("absent.json"), &missing).unwrap();
    assert_eq!(read_from(missing).contents.unwrap(), None);
    let cycle = scratch.path().join("cycle");
    std::os::unix::fs::symlink(&cycle, &cycle).unwrap();
    assert!(
        read_from(cycle)
            .contents
            .unwrap_err()
            .contains("Cannot open keymap")
    );
    assert_eq!(std::fs::read_dir(scratch.path()).unwrap().count(), 2);
}

#[cfg(unix)]
#[test]
fn fifo_without_a_writer_is_rejected_without_waiting_for_a_peer() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("keymap.fifo");
    #[cfg(not(target_os = "macos"))]
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        &path,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .unwrap();
    #[cfg(target_os = "macos")]
    {
        let mut command = std::process::Command::new("mkfifo");
        command.arg(&path);
        assert!(
            deadpan_native_process::spawn(&mut command)
                .unwrap()
                .wait()
                .unwrap()
                .success()
        );
    }
    let (send, receive) = std::sync::mpsc::channel();
    let reader_path = path.clone();
    let thread = std::thread::spawn(move || {
        let _ = send.send(read_from(reader_path));
    });
    let result = receive.recv_timeout(std::time::Duration::from_secs(2));
    if result.is_err() {
        // Unblock a regressed blocking open before failing the witness.
        let peer = rustix::fs::open(
            &path,
            rustix::fs::OFlags::RDWR | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .unwrap();
        let _ = receive.recv_timeout(std::time::Duration::from_secs(2));
        drop(peer);
    }
    let loaded = result.expect("Keymap reader waited for a FIFO peer");
    thread.join().unwrap();
    assert!(
        loaded
            .contents
            .unwrap_err()
            .contains("must be a regular file")
    );
    assert_eq!(std::fs::read_dir(scratch.path()).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn device_and_unix_socket_paths_cannot_supply_keymap_bytes() {
    assert!(
        read_from(PathBuf::from("/dev/null"))
            .contents
            .unwrap_err()
            .contains("regular file")
    );
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("keymap.socket");
    let _socket = std::os::unix::net::UnixListener::bind(&path).unwrap();
    assert!(read_from(path).contents.is_err());
}

#[cfg(target_os = "macos")]
#[test]
fn native_location_is_absolute_and_comes_from_user_application_support() {
    let directory = application_support_directory().unwrap();
    assert!(directory.is_absolute());
    assert_eq!(directory.file_name().unwrap(), "Application Support");
    let keymap = directory.join("Deadpan/keymap.json");
    assert!(keymap.is_absolute());
    assert!(keymap.ends_with("Deadpan/keymap.json"));
}

#[cfg(not(target_os = "macos"))]
#[test]
fn unsupported_native_location_reports_no_path_instead_of_a_fallback() {
    let loaded = load();
    assert!(loaded.path.is_none());
    assert!(loaded.contents.unwrap_err().contains("only on macOS"));
}
