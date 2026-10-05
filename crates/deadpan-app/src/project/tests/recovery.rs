//! Native recovery: unclean-session reports, a degraded open with a missing
//! Original and its relink, and the persistent alert after a real full disk.

use super::*;
use deadpan_store::original_media::OriginalAvailability;
use std::process::Command as ProcessCommand;

#[test]
fn opening_after_an_unclean_exit_reports_it_once_and_keeps_every_saved_edit() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("crashed.deadpan");
    drop(seed_holds(&path, &["first", "second"]));
    // The exact state a killed writer leaves behind (the store's own test
    // kills a real process): its session marker is still present.
    std::fs::write(
        path.join(".writer.session"),
        "deadpan writer pid=1 opened_unix=0",
    )
    .unwrap();
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let opened = command(&service, ProjectRequest::Open(path.clone()));
    assert!(opened.error.is_none(), "{:?}", opened.error);
    let workspace = opened.workspace.unwrap();
    assert_eq!(workspace.document.revision_id().as_str(), "insert-second");
    let report = opened.opened.expect("open report");
    assert_eq!(report.session, workspace.session);
    assert!(report.needs_attention());
    assert_eq!(
        report
            .recovery
            .unclean_previous_writer
            .as_ref()
            .map(|writer| writer.marker.as_str()),
        Some("deadpan writer pid=1 opened_unix=0")
    );
    assert!(report.originals.is_empty());
    // Unacknowledged findings survive another writer (a headless command).
    command(&service, ProjectRequest::Close);
    drop(ProjectStore::open(&path, AccessMode::ReadWrite).unwrap());
    let reopened = command(&service, ProjectRequest::Open(path.clone()));
    let workspace = reopened.workspace.unwrap();
    assert!(reopened.opened.unwrap().needs_attention());
    let acknowledged = command(
        &service,
        ProjectRequest::AcknowledgeRecovery {
            expected_session: workspace.session,
        },
    );
    assert!(acknowledged.error.is_none(), "{:?}", acknowledged.error);
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(path));
    assert!(!reopened.opened.unwrap().needs_attention());
}

/// Editing works with the Original missing: structural edits and Undo never
/// read media bytes, so they save normally and the report stays truthful.
#[test]
fn editing_with_the_original_missing_saves_and_undoes() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("edit-missing.deadpan");
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    create(&service, &path);
    import(&service, "cfr-bframes.mp4");
    let workspace = complete(&service);
    let asset = workspace.sources.keys().next().unwrap().clone();
    insert(&service, &workspace, &asset);
    let content = workspace.sources[&asset]
        .original
        .object()
        .content()
        .clone();
    command(&service, ProjectRequest::Close);
    std::fs::remove_file(
        path.join("Media/Originals")
            .join(format!("blake3-{}", content.digest())),
    )
    .unwrap();
    let opened = command(&service, ProjectRequest::Open(path.clone()));
    assert_eq!(opened.opened.unwrap().missing().count(), 1);
    let before = opened.workspace.unwrap();
    let first = |workspace: &Workspace| {
        workspace
            .document
            .children(workspace.document.root())
            .next()
            .unwrap()
            .clone()
    };
    let split = command(
        &service,
        edit_request(
            &before,
            ProjectEdit::Split {
                node: first(&before),
                at: FrameDuration::new(10).unwrap(),
            },
        ),
    );
    assert!(split.error.is_none(), "{:?}", split.error);
    let split = split.workspace.unwrap();
    assert_ne!(split.document.revision_id(), before.document.revision_id());
    let deleted = command(
        &service,
        edit_request(
            &split,
            ProjectEdit::Delete {
                node: first(&split),
            },
        ),
    );
    assert!(deleted.error.is_none(), "{:?}", deleted.error);
    let deleted = deleted.workspace.unwrap();
    let undone = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: deleted.document.revision_id().clone(),
        },
    );
    assert!(undone.error.is_none(), "{:?}", undone.error);
    assert!(undone.storage.is_none());
    command(&service, ProjectRequest::Close);
    ProjectStore::open(&path, AccessMode::ReadWrite)
        .unwrap()
        .validate_full()
        .unwrap();
}

#[test]
fn a_missing_original_opens_degraded_and_relinks_only_identical_bytes() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("missing.deadpan");
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    create(&service, &path);
    import(&service, "cfr-bframes.mp4");
    let workspace = complete(&service);
    let source = workspace.sources.values().next().unwrap().clone();
    let content = source.original.object().content().clone();
    command(&service, ProjectRequest::Close);
    let object = path
        .join("Media/Originals")
        .join(format!("blake3-{}", content.digest()));
    std::fs::remove_file(&object).unwrap();

    let opened = command(&service, ProjectRequest::Open(path.clone()));
    assert!(opened.error.is_none(), "{:?}", opened.error);
    let workspace = opened.workspace.unwrap();
    assert_eq!(workspace.sources.len(), 1, "the catalog is intact");
    let report = opened.opened.unwrap();
    let missing: Vec<_> = report.missing().collect();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].availability, OriginalAvailability::Missing);
    assert_eq!(missing[0].label, source.label);

    let relink = |ticket: u64, file: PathBuf| {
        service
            .submit(ProjectRequest::RelinkOriginal {
                ticket,
                expected_session: workspace.session,
                content: content.clone(),
                expected_version: source.original.version(),
                path: file,
            })
            .unwrap();
        wait(&service, |update| {
            update.relink.as_ref().is_some_and(|status| {
                status.ticket == ticket && status.state != RelinkState::Verifying
            })
        })
    };
    let refused = relink(1, fixture("vfr.mp4"));
    let RelinkState::Failed(reason) = refused.relink.unwrap().state else {
        panic!("a different file must be refused");
    };
    assert!(reason.contains("not this project's Original"), "{reason}");
    assert!(!object.exists(), "refused bytes were never published");
    assert_eq!(refused.opened.unwrap().missing().count(), 1);

    let restored = relink(2, fixture("cfr-bframes.mp4"));
    assert_eq!(restored.relink.unwrap().state, RelinkState::Restored);
    assert!(restored.error.is_none(), "{:?}", restored.error);
    assert_eq!(restored.opened.unwrap().missing().count(), 0);
    assert!(object.is_file());
    let after = restored.workspace.unwrap();
    assert_eq!(
        after.document.revision_id(),
        workspace.document.revision_id()
    );
}

/// A private attached APFS image, detached on drop (including on panic).
#[cfg(target_os = "macos")]
struct DiskImage {
    mount: PathBuf,
    _scratch: tempfile::TempDir,
}

#[cfg(target_os = "macos")]
impl DiskImage {
    fn new() -> Self {
        let scratch = tempfile::tempdir().unwrap();
        let image = scratch.path().join("volume.dmg");
        let mount = scratch.path().join("mount");
        std::fs::create_dir(&mount).unwrap();
        for arguments in [
            vec![
                "create",
                "-quiet",
                "-size",
                "16m",
                "-fs",
                "APFS",
                "-layout",
                "NONE",
                "-volname",
                "deadpan-app-test",
            ],
            vec![
                "attach",
                "-quiet",
                "-nobrowse",
                "-noautoopen",
                "-mountpoint",
            ],
        ] {
            let mut command = ProcessCommand::new("hdiutil");
            command.args(arguments.iter());
            if arguments[0] == "create" {
                command.arg(&image);
            } else {
                command.arg(&mount).arg(&image);
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Self {
            mount: mount.canonicalize().unwrap(),
            _scratch: scratch,
        }
    }

    /// Fills the volume until a new 4 KiB file is refused. APFS can release
    /// space shortly after a write fails, so keep filling until it holds.
    fn fill(&self) -> PathBuf {
        use std::io::Write as _;
        let path = self.mount.join("filler");
        // Append so topping up never frees what is already filled; a volume
        // too full to even open the filler is already full.
        let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        else {
            return path;
        };
        // APFS can keep releasing space under load; give it time to settle.
        for round in 0..64 {
            if round > 0 {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            for chunk in [1 << 20, 64 << 10, 4 << 10, 512] {
                let bytes = vec![0x5a_u8; chunk];
                while file
                    .write_all(&bytes)
                    .and_then(|()| file.sync_data())
                    .is_ok()
                {}
            }
            let probe = self.mount.join("probe");
            let refused = std::fs::File::create(&probe)
                .and_then(|mut file| file.write_all(&[0; 4096]).and_then(|()| file.sync_all()));
            let _ = std::fs::remove_file(&probe);
            if matches!(&refused, Err(error) if error.kind() == std::io::ErrorKind::StorageFull) {
                return path;
            }
        }
        // Under heavy parallel I/O APFS may keep releasing space; callers
        // top the volume up again before each step instead of failing here.
        path
    }
}

#[cfg(target_os = "macos")]
impl Drop for DiskImage {
    fn drop(&mut self) {
        let _ = ProcessCommand::new("hdiutil")
            .args(["detach", "-quiet", "-force"])
            .arg(&self.mount)
            .output();
    }
}

#[cfg(target_os = "macos")]
#[test]
fn a_full_disk_never_reports_saved_and_its_alert_clears_after_a_real_save() {
    let image = DiskImage::new();
    let path = image.mount.join("full.deadpan");
    let names: Vec<String> = (0..64).map(|index| format!("hold-{index}")).collect();
    drop(seed_holds(
        &path,
        &names.iter().map(String::as_str).collect::<Vec<_>>(),
    ));
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let mut workspace = command(&service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    let mut filler = image.fill();
    let mut refused = None;
    for name in &names {
        // Keep the volume full even if APFS released space since the last step.
        filler = image.fill();
        let before = workspace.document.revision_id().clone();
        let update = command(
            &service,
            edit_request(&workspace, ProjectEdit::Delete { node: node(name) }),
        );
        workspace = update.workspace.clone().unwrap();
        if let Some(error) = &update.error {
            assert!(error.starts_with("Not saved: the disk is full."), "{error}");
            assert!(update.committed.is_none());
            assert!(
                !update.message.as_deref().unwrap_or("").contains("saved"),
                "{:?}",
                update.message
            );
            assert_eq!(workspace.document.revision_id(), &before);
            let alert = update.storage.clone().expect("persistent alert");
            assert_eq!(alert.code, "DiskFull");
            assert_eq!(alert.revision.as_ref(), Some(&before));
            refused = Some(name.clone());
            break;
        }
    }
    let refused = refused.expect("the full volume refused a commit");

    // The alert persists through another refusal until a save succeeds.
    let still = command(
        &service,
        edit_request(
            &workspace,
            ProjectEdit::Delete {
                node: node(&refused),
            },
        ),
    );
    assert!(still.error.is_some());
    assert_eq!(still.storage.unwrap().code, "DiskFull");
    let workspace = still.workspace.unwrap();
    std::fs::remove_file(&filler).unwrap();
    let saved = command(
        &service,
        edit_request(
            &workspace,
            ProjectEdit::Delete {
                node: node(&refused),
            },
        ),
    );
    assert!(saved.error.is_none(), "{:?}", saved.error);
    assert!(
        saved.storage.is_none(),
        "a successful save clears the alert"
    );
    command(&service, ProjectRequest::Close);
    ProjectStore::open(&path, AccessMode::ReadWrite)
        .unwrap()
        .validate_full()
        .unwrap();
}
