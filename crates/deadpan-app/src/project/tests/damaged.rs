use super::*;
use crate::project::damaged::{Action, Offer, Outcome, Request};
use deadpan_store::backups::{BackupLimits, BackupPolicy, BackupReason, create_backup};
use std::sync::atomic::AtomicBool;

fn damaged_project(path: &Path) -> deadpan_store::backups::BackupInfo {
    drop(seed_holds(path, &["one", "two"]));
    let backup = create_backup(
        path,
        BackupReason::Manual,
        &BackupPolicy::default(),
        BackupLimits::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    std::fs::write(path.join("project.sqlite"), b"broken project database").unwrap();
    backup.backup
}

fn send(service: &ProjectService, offer: &Offer, ticket: u64, action: Action) -> ProjectUpdate {
    command(
        service,
        ProjectRequest::Damaged(Request {
            offer: offer.id,
            ticket,
            action,
        }),
    )
}

#[test]
fn failed_open_offers_verified_recovery_and_opens_the_restored_backup_as_a_new_session() {
    let scratch = tempfile::tempdir().unwrap();
    let current = scratch.path().join("current.deadpan");
    let damaged = scratch.path().join("damaged.deadpan");
    let backup = damaged_project(&damaged);
    drop(seed_holds(&current, &["current"]));
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let prior = command(&service, ProjectRequest::Open(current))
        .workspace
        .unwrap();
    let failed = command(&service, ProjectRequest::Open(damaged.clone()));
    assert!(failed.error.is_some());
    assert_eq!(failed.workspace.as_ref().unwrap().session, prior.session);
    let offer = failed.damaged.unwrap().offer;
    let inspected = send(
        &service,
        &offer,
        1,
        Action::Inspect {
            backup: backup.id.clone(),
        },
    );
    let reply = inspected.damaged.unwrap().reply.unwrap();
    assert!(
        matches!(reply.result, Ok(Outcome::Inspected { .. })),
        "{:?}",
        reply.result
    );
    assert_eq!(
        std::fs::read(damaged.join("project.sqlite")).unwrap(),
        b"broken project database"
    );
    let restored = send(
        &service,
        &offer,
        2,
        Action::Restore {
            backup: backup.id,
            confirmed_project: None,
        },
    );
    let receipt = restored.damaged.unwrap().reply.unwrap().result.unwrap();
    let Outcome::Restored {
        quarantine,
        open_error,
        warnings,
    } = receipt
    else {
        panic!("expected a durable restore receipt")
    };
    assert!(open_error.is_none(), "{open_error:?}");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        std::fs::read(quarantine.join("project.sqlite")).unwrap(),
        b"broken project database"
    );
    let workspace = restored.workspace.unwrap();
    assert!(workspace.session > prior.session);
    assert_eq!(workspace.path, damaged.canonicalize().unwrap());
    assert_eq!(
        workspace
            .document
            .children(workspace.document.root())
            .count(),
        2
    );
    command(&service, ProjectRequest::Close);
    ProjectStore::open(&damaged, AccessMode::ReadOnly)
        .unwrap()
        .validate_full()
        .unwrap();
}

#[test]
fn recovery_cannot_retarget_after_another_open_or_an_edit() {
    let scratch = tempfile::tempdir().unwrap();
    let current = scratch.path().join("current.deadpan");
    let damaged = scratch.path().join("damaged.deadpan");
    let backup = damaged_project(&damaged);
    drop(seed_holds(&current, &["current"]));
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let prior = command(&service, ProjectRequest::Open(current.clone()))
        .workspace
        .unwrap();
    let offer = command(&service, ProjectRequest::Open(damaged.clone()))
        .damaged
        .unwrap()
        .offer;
    send(
        &service,
        &offer,
        1,
        Action::Inspect {
            backup: backup.id.clone(),
        },
    );
    let node = prior
        .document
        .children(prior.document.root())
        .next()
        .unwrap()
        .clone();
    let edit = edited(
        &service,
        &prior,
        ProjectEdit::Split {
            node,
            at: FrameDuration::new(6).unwrap(),
        },
    );
    assert!(edit.error.is_none());
    let refused = send(
        &service,
        &offer,
        2,
        Action::Restore {
            backup: backup.id.clone(),
            confirmed_project: None,
        },
    );
    assert!(refused.damaged.unwrap().reply.unwrap().result.is_err());
    assert_eq!(
        std::fs::read(damaged.join("project.sqlite")).unwrap(),
        b"broken project database"
    );
    // Even an Open that keeps the same current package retires the old offer.
    command(&service, ProjectRequest::Open(current));
    let refused = send(
        &service,
        &offer,
        3,
        Action::Restore {
            backup: backup.id,
            confirmed_project: None,
        },
    );
    assert!(
        refused
            .error
            .as_deref()
            .unwrap()
            .contains("no longer current")
    );
    assert!(refused.damaged.is_none());
    assert_eq!(
        std::fs::read(damaged.join("project.sqlite")).unwrap(),
        b"broken project database"
    );
}

#[test]
fn replacement_receipt_survives_a_failure_to_open_the_recovered_project() {
    let scratch = tempfile::tempdir().unwrap();
    let current = scratch.path().join("current.deadpan");
    let path = scratch.path().join("damaged.deadpan");
    let backup = damaged_project(&path);
    drop(seed_holds(&current, &["current"]));
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let prior = command(&service, ProjectRequest::Open(current))
        .workspace
        .unwrap();
    let offer = command(&service, ProjectRequest::Open(path.clone()))
        .damaged
        .unwrap()
        .offer;
    let inspected = send(
        &service,
        &offer,
        1,
        Action::Inspect {
            backup: backup.id.clone(),
        },
    );
    assert!(inspected.damaged.unwrap().reply.unwrap().result.is_ok());
    // Prevent host discovery publication after the database has been replaced.
    // This is outside the captured database and leaves its backup fully valid.
    std::fs::create_dir(path.join(".host.json")).unwrap();
    let restored = send(
        &service,
        &offer,
        2,
        Action::Restore {
            backup: backup.id,
            confirmed_project: None,
        },
    );
    let Outcome::Restored {
        quarantine,
        open_error,
        warnings,
    } = restored.damaged.unwrap().reply.unwrap().result.unwrap()
    else {
        panic!("replacement must be reported even when opening fails")
    };
    assert!(open_error.is_some());
    assert!(warnings.is_empty());
    assert_eq!(restored.workspace.unwrap().session, prior.session);
    assert_eq!(
        std::fs::read(quarantine.join("project.sqlite")).unwrap(),
        b"broken project database"
    );
    ProjectStore::open(&path, AccessMode::ReadOnly)
        .unwrap()
        .validate_full()
        .unwrap();
}

#[test]
fn recovery_after_close_replaces_the_same_package_without_retaining_old_readers() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("close-before-recovery.deadpan");
    drop(seed_holds(&path, &["current"]));
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let opened = command(&service, ProjectRequest::Open(path.clone()));
    let workspace = opened.workspace.unwrap();
    let node = workspace
        .document
        .children(workspace.document.root())
        .next()
        .unwrap()
        .clone();
    let changed = edited(
        &service,
        &workspace,
        ProjectEdit::Split {
            node,
            at: FrameDuration::new(6).unwrap(),
        },
    );
    assert!(changed.error.is_none());
    let closed = command(&service, ProjectRequest::Close);
    if closed.backups.owned_workers_active_for_check {
        wait(&service, |update| {
            !update.backups.owned_workers_active_for_check
        });
    }
    // A last read-only backup connection may leave a valid WAL after closing.
    // Damage the complete disposable database, not just its recoverable main file.
    for name in [
        "project.sqlite-wal",
        "project.sqlite-shm",
        "project.sqlite-journal",
    ] {
        match std::fs::remove_file(path.join(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("{error}"),
        }
    }
    std::fs::write(path.join("project.sqlite"), b"broken after Close").unwrap();
    let failed = command(&service, ProjectRequest::Open(path.clone()));
    assert!(failed.workspace.is_none());
    assert!(failed.error.is_some());
    let offer = failed.damaged.unwrap().offer;
    let id = offer.backups[0].id.clone();
    let inspected = send(&service, &offer, 1, Action::Inspect { backup: id.clone() });
    assert!(inspected.damaged.unwrap().reply.unwrap().result.is_ok());
    let restored = send(
        &service,
        &offer,
        2,
        Action::Restore {
            backup: id,
            confirmed_project: None,
        },
    );
    assert!(matches!(
        restored.damaged.unwrap().reply.unwrap().result,
        Ok(Outcome::Restored {
            open_error: None,
            ..
        })
    ));
    assert_eq!(
        restored.workspace.unwrap().document.revision_id(),
        changed.workspace.unwrap().document.revision_id()
    );
    let closed = command(&service, ProjectRequest::Close);
    if closed.backups.owned_workers_active_for_check {
        wait(&service, |update| {
            !update.backups.owned_workers_active_for_check
        });
    }
    ProjectStore::open(&path, AccessMode::ReadOnly)
        .unwrap()
        .validate_full()
        .unwrap();
}
