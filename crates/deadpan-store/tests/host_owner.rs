#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt, symlink};
use std::path::Path;

use deadpan_core::{
    ColorPolicy, FrameRate, NodeId, PresentationBasis, ProjectDocument, ProjectId, RevisionId,
};
use deadpan_store::host_owner::{MAX_DISCOVERY_BYTES, PackageIdentity, read_discovery};
use deadpan_store::{AccessMode, ProjectStore, StoreError};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn document() -> Result<ProjectDocument> {
    Ok(ProjectDocument::new(
        ProjectId::new("project")?,
        RevisionId::new("r0")?,
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(30_000, 1_001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?)
}

fn private_file(path: &Path, bytes: &[u8]) -> Result {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    Ok(())
}

fn fifo(path: &Path) -> Result {
    #[cfg(target_os = "linux")]
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        path,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )?;
    #[cfg(target_os = "macos")]
    {
        let mut command = std::process::Command::new("mkfifo");
        command.arg(path);
        assert!(
            deadpan_native_process::spawn(&mut command)?
                .wait()?
                .success()
        );
    }
    Ok(())
}

#[test]
fn capabilities_identify_one_store_and_are_revoked_before_reopen() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("owner.deadpan");
    let other_path = scratch.path().join("other.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    let owner = store.writer_owner_handle()?;
    let clone = owner.clone();
    let second_handle = store.writer_owner_handle()?;
    store.check_writer_owner(&owner)?;
    store.check_writer_owner(&clone)?;
    store.check_writer_owner(&second_handle)?;
    let metadata = fs::metadata(&path)?;
    assert_eq!(
        owner.package_identity(),
        PackageIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
        }
    );
    let mut other = ProjectStore::create(&other_path, &document()?)?;
    let other_owner = other.writer_owner_handle()?;
    assert!(other.check_writer_owner(&owner).is_err());
    assert!(store.check_writer_owner(&other_owner).is_err());
    assert!(!owner.is_closed());
    drop(store);
    assert!(owner.is_closed());
    assert!(clone.is_closed());
    assert!(second_handle.is_closed());
    // Retaining every handle does not retain the writer lock.
    let mut reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let fresh = reopened.writer_owner_handle()?;
    assert_eq!(fresh.package_identity(), owner.package_identity());
    assert!(reopened.check_writer_owner(&owner).is_err());
    reopened.check_writer_owner(&fresh)?;
    Ok(())
}

#[test]
fn read_only_store_cannot_obtain_or_use_writer_capabilities() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("reader.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    let owner = store.writer_owner_handle()?;
    let mut reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert!(matches!(
        reader.writer_owner_handle(),
        Err(StoreError::ReadOnly)
    ));
    assert!(matches!(
        reader.check_writer_owner(&owner),
        Err(StoreError::ReadOnly)
    ));
    assert!(matches!(
        reader.publish_host_discovery(&owner, b"record"),
        Err(StoreError::ReadOnly)
    ));
    assert!(matches!(
        reader.unpublish_host_discovery(&owner),
        Err(StoreError::ReadOnly)
    ));
    assert!(!path.join(".host.json").exists());
    Ok(())
}

#[test]
fn explicit_registration_is_private_bounded_and_removed_before_unlock() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("discovery.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    assert!(read_discovery(&path)?.is_none());
    let owner = store.writer_owner_handle()?;
    assert!(read_discovery(&path)?.is_none());
    store.publish_host_discovery(&owner, b"opaque-record")?;
    let record = read_discovery(&path)?.ok_or("missing discovery")?;
    assert_eq!(record.package_identity, owner.package_identity());
    assert!(record.bytes == b"opaque-record");
    assert_eq!(
        fs::metadata(path.join(".host.json"))?.mode() & 0o7777,
        0o600
    );
    assert!(store.publish_host_discovery(&owner, b"second").is_err());
    store.unpublish_host_discovery(&owner)?;
    assert!(read_discovery(&path)?.is_none());
    store.unpublish_host_discovery(&owner)?;
    assert!(
        store
            .publish_host_discovery(&owner, &vec![0; MAX_DISCOVERY_BYTES + 1])
            .is_err()
    );
    assert!(read_discovery(&path)?.is_none());
    store.publish_host_discovery(&owner, &vec![0; MAX_DISCOVERY_BYTES])?;
    assert_eq!(
        read_discovery(&path)?
            .ok_or("missing discovery")?
            .bytes
            .len(),
        MAX_DISCOVERY_BYTES
    );
    drop(store);
    assert!(owner.is_closed());
    assert!(read_discovery(&path)?.is_none());
    assert!(path.join(".writer.lock").is_file());
    let reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert!(read_discovery(&path)?.is_none());
    drop(reopened);
    Ok(())
}

#[test]
fn next_owner_can_replace_a_private_record_left_by_an_interrupted_host() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("stale.deadpan");
    drop(ProjectStore::create(&path, &document()?)?);
    private_file(&path.join(".host.json"), b"stale-record")?;
    let stale_inode = fs::metadata(path.join(".host.json"))?.ino();
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert!(read_discovery(&path)?.ok_or("missing stale record")?.bytes == b"stale-record");
    let owner = store.writer_owner_handle()?;
    store.publish_host_discovery(&owner, b"fresh-record")?;
    assert_ne!(fs::metadata(path.join(".host.json"))?.ino(), stale_inode);
    assert!(read_discovery(&path)?.ok_or("missing fresh record")?.bytes == b"fresh-record");
    drop(store);
    assert!(read_discovery(&path)?.is_none());
    Ok(())
}

#[test]
fn unsafe_discovery_entries_are_rejected_without_modifying_them() -> Result {
    for kind in [
        "symlink",
        "fifo",
        "hardlink",
        "public",
        "writable",
        "oversize",
        "directory",
    ] {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join("unsafe.deadpan");
        let mut store = ProjectStore::create(&path, &document()?)?;
        let owner = store.writer_owner_handle()?;
        let discovery = path.join(".host.json");
        match kind {
            "symlink" => {
                let outside = scratch.path().join("outside");
                private_file(&outside, b"outside")?;
                symlink(&outside, &discovery)?;
            }
            "fifo" => fifo(&discovery)?,
            "hardlink" => {
                private_file(&discovery, b"record")?;
                fs::hard_link(&discovery, scratch.path().join("linked"))?;
            }
            "public" | "writable" => {
                private_file(&discovery, b"record")?;
                let mode = if kind == "public" { 0o644 } else { 0o620 };
                fs::set_permissions(&discovery, fs::Permissions::from_mode(mode))?;
            }
            "oversize" => private_file(&discovery, &vec![0; MAX_DISCOVERY_BYTES + 1])?,
            "directory" => fs::create_dir(&discovery)?,
            _ => return Err("unknown fixture".into()),
        }
        let before = fs::symlink_metadata(&discovery)?;
        assert!(read_discovery(&path).is_err(), "accepted {kind}");
        assert!(
            store
                .publish_host_discovery(&owner, b"replacement")
                .is_err(),
            "replaced {kind}"
        );
        drop(store);
        let after = fs::symlink_metadata(&discovery)?;
        assert_eq!(before.ino(), after.ino());
        assert_eq!(before.mode(), after.mode());
    }
    Ok(())
}

#[test]
fn unsafe_lock_entries_fail_without_following_or_waiting_on_them() -> Result {
    for kind in [
        "symlink",
        "fifo",
        "hardlink",
        "shared-write",
        "nonempty",
        "directory",
    ] {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join("lock.deadpan");
        drop(ProjectStore::create(&path, &document()?)?);
        let lock = path.join(".writer.lock");
        fs::remove_file(&lock)?;
        match kind {
            "symlink" => {
                let outside = scratch.path().join("outside");
                private_file(&outside, b"")?;
                symlink(&outside, &lock)?;
            }
            "fifo" => fifo(&lock)?,
            "hardlink" => {
                private_file(&lock, b"")?;
                fs::hard_link(&lock, scratch.path().join("linked"))?;
            }
            "shared-write" => {
                private_file(&lock, b"")?;
                fs::set_permissions(&lock, fs::Permissions::from_mode(0o660))?;
            }
            "nonempty" => private_file(&lock, b"unexpected")?,
            "directory" => fs::create_dir(&lock)?,
            _ => return Err("unknown fixture".into()),
        }
        let before = fs::symlink_metadata(&lock)?;
        assert!(
            ProjectStore::open(&path, AccessMode::ReadWrite).is_err(),
            "accepted {kind}"
        );
        assert_eq!(before.ino(), fs::symlink_metadata(&lock)?.ino());
        assert_eq!(before.mode(), fs::symlink_metadata(&lock)?.mode());
    }
    Ok(())
}

#[test]
fn legacy_readable_lock_is_hardened_only_after_it_is_acquired() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("legacy.deadpan");
    drop(ProjectStore::create(&path, &document()?)?);
    let lock = path.join(".writer.lock");
    fs::set_permissions(&lock, fs::Permissions::from_mode(0o644))?;
    let inode = fs::metadata(&lock)?.ino();
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let owner = store.writer_owner_handle()?;
    store.check_writer_owner(&owner)?;
    assert_eq!(fs::metadata(&lock)?.mode() & 0o7777, 0o600);
    assert_eq!(fs::metadata(&lock)?.ino(), inode);
    Ok(())
}

#[test]
fn replaced_or_modified_registration_revokes_dispatch_and_survives_old_cleanup() -> Result {
    for replacement in [false, true] {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join("changed.deadpan");
        let mut store = ProjectStore::create(&path, &document()?)?;
        let owner = store.writer_owner_handle()?;
        store.publish_host_discovery(&owner, b"original")?;
        let discovery = path.join(".host.json");
        if replacement {
            fs::remove_file(&discovery)?;
            private_file(&discovery, b"replacement")?;
        } else {
            fs::write(&discovery, b"modified")?;
        }
        let before = fs::metadata(&discovery)?.ino();
        assert!(store.check_writer_owner(&owner).is_err());
        assert!(store.unpublish_host_discovery(&owner).is_err());
        drop(store);
        assert_eq!(fs::metadata(&discovery)?.ino(), before);
    }
    Ok(())
}

#[test]
fn replaced_lock_cannot_let_old_cleanup_erase_a_successor_registration() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("successor.deadpan");
    let mut old = ProjectStore::create(&path, &document()?)?;
    let old_owner = old.writer_owner_handle()?;
    old.publish_host_discovery(&old_owner, b"old")?;
    fs::remove_file(path.join(".writer.lock"))?;
    assert!(old.check_writer_owner(&old_owner).is_err());
    let mut successor = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let successor_owner = successor.writer_owner_handle()?;
    successor.publish_host_discovery(&successor_owner, b"successor")?;
    drop(old);
    assert!(old_owner.is_closed());
    successor.check_writer_owner(&successor_owner)?;
    assert!(read_discovery(&path)?.ok_or("successor was removed")?.bytes == b"successor");
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadWrite),
        Err(StoreError::AlreadyOpen)
    ));
    drop(successor);
    assert!(read_discovery(&path)?.is_none());
    Ok(())
}

#[test]
fn moved_package_fails_closed_even_before_lazy_owner_activation() -> Result {
    for activated in [false, true] {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join("package.deadpan");
        let moved = scratch.path().join("moved.deadpan");
        let mut store = ProjectStore::create(&path, &document()?)?;
        let owner = if activated {
            Some(store.writer_owner_handle()?)
        } else {
            None
        };
        fs::rename(&path, &moved)?;
        fs::create_dir(&path)?;
        private_file(&path.join(".host.json"), b"replacement")?;
        assert!(store.writer_owner_handle().is_err());
        if let Some(owner) = owner {
            assert!(store.check_writer_owner(&owner).is_err());
            assert!(store.publish_host_discovery(&owner, b"old").is_err());
        }
        drop(store);
        assert!(fs::read(path.join(".host.json"))? == b"replacement");
    }
    Ok(())
}

#[test]
fn shared_writable_or_symlinked_package_is_not_discovery_authority() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("private.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    let owner = store.writer_owner_handle()?;
    store.publish_host_discovery(&owner, b"record")?;
    let link = scratch.path().join("link.deadpan");
    symlink(&path, &link)?;
    assert!(read_discovery(&link).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o777))?;
    assert!(read_discovery(&path).is_err());
    assert!(store.check_writer_owner(&owner).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    store.check_writer_owner(&owner)?;
    Ok(())
}
