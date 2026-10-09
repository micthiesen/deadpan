//! Automatic, closing and explicit backups, restore as a new session, the
//! read-only view of a newer package and automatic relinking of a moved
//! linked Original (docs/BACKUPS.md, docs/RECOVERY.md).

use super::*;
use crate::project::backups::Request as BackupRequest;
use deadpan_store::backups::{BackupReason, list_backups};

fn backups_of(path: &Path) -> Vec<deadpan_store::backups::BackupInfo> {
    list_backups(path).unwrap()
}

fn split_first(service: &ProjectService, workspace: &Workspace, at: i64) -> Arc<Workspace> {
    let first = workspace
        .document
        .children(workspace.document.root())
        .next()
        .unwrap()
        .clone();
    let update = edited(
        service,
        workspace,
        ProjectEdit::Split {
            node: first,
            at: FrameDuration::new(at).unwrap(),
        },
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    update.workspace.unwrap()
}

#[test]
fn edits_are_backed_up_periodically_and_when_the_project_closes() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("periodic.deadpan");
    drop(seed_holds(&path, &["first"]));
    let path = path.canonicalize().unwrap();
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    service.set_backup_interval_for_check(Duration::from_millis(50));
    let opened = command(&service, ProjectRequest::Open(path.clone()));
    let workspace = opened.workspace.unwrap();
    // A project without backups gets its first one after the interval.
    let first = wait(&service, |update| update.backups.latest.is_some());
    let (info, revision) = first.backups.latest.clone().unwrap();
    assert_eq!(info.reason, BackupReason::Periodic);
    assert_eq!(
        revision.as_deref(),
        Some(workspace.document.revision_id().as_str())
    );
    // No edit, no new backup.
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(backups_of(&path).len(), 1);
    // An edit is backed up after the interval.
    let split = split_first(&service, &workspace, 6);
    let second = wait(&service, |update| {
        update.backups.latest.as_ref().is_some_and(|(_, revision)| {
            revision.as_deref() == Some(split.document.revision_id().as_str())
        })
    });
    assert!(second.backups.failure.is_none());
    // An edit made just before closing is backed up when it closes.
    service.set_backup_interval_for_check(Duration::from_secs(3600));
    let again = split_first(&service, &split, 3);
    command(&service, ProjectRequest::Close);
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let close = backups_of(&path)
            .into_iter()
            .find(|backup| backup.reason == BackupReason::Close);
        if let Some(close) = close {
            let preview = deadpan_store::backups::verify_backup(&close).unwrap();
            assert_eq!(&preview.revision_id, again.document.revision_id());
            break;
        }
        assert!(Instant::now() < deadline, "no closing backup");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn restoring_a_backup_starts_a_new_session_at_its_revision_and_is_reversible() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("restore.deadpan");
    drop(seed_holds(&path, &["first", "second"]));
    let path = path.canonicalize().unwrap();
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let opened = command(&service, ProjectRequest::Open(path.clone()));
    let workspace = opened.workspace.unwrap();
    // Back up now.
    let backed = command(
        &service,
        ProjectRequest::Backup(BackupRequest::Now {
            ticket: 1,
            expected_session: workspace.session,
        }),
    );
    assert!(backed.error.is_none(), "{:?}", backed.error);
    let backed = wait(&service, |update| {
        update
            .backups
            .reply
            .as_ref()
            .is_some_and(|reply| reply.ticket == 1)
    });
    assert!(backed.backups.reply.as_ref().unwrap().result.is_ok());
    let chosen = backed.backups.latest.clone().unwrap().0;
    assert_eq!(chosen.reason, BackupReason::Manual);
    let later = split_first(&service, &workspace, 6);
    // A stale session cannot restore.
    let stale = command(
        &service,
        ProjectRequest::Backup(BackupRequest::Restore {
            ticket: 2,
            expected_session: workspace.session + 7,
            id: chosen.id.clone(),
        }),
    );
    assert!(stale.backups.reply.unwrap().result.is_err());
    let restored = command(
        &service,
        ProjectRequest::Backup(BackupRequest::Restore {
            ticket: 3,
            expected_session: later.session,
            id: chosen.id.clone(),
        }),
    );
    let reply = restored.backups.reply.clone().unwrap();
    assert_eq!(reply.ticket, 3);
    assert!(reply.result.is_ok(), "{:?}", reply.result);
    let now = restored.workspace.unwrap();
    assert_eq!(
        now.session,
        later.session + 1,
        "a restore starts a new session"
    );
    assert_eq!(now.document.revision_id(), workspace.document.revision_id());
    assert_eq!(*now.document, *workspace.document);
    // The replaced state was backed up first and restores back.
    let safety = backups_of(&path)
        .into_iter()
        .find(|backup| backup.reason == BackupReason::BeforeRestore)
        .unwrap();
    let back = command(
        &service,
        ProjectRequest::Backup(BackupRequest::Restore {
            ticket: 4,
            expected_session: now.session,
            id: safety.id,
        }),
    );
    assert!(back.backups.reply.unwrap().result.is_ok());
    assert_eq!(
        back.workspace.unwrap().document.revision_id(),
        later.document.revision_id()
    );
    command(&service, ProjectRequest::Close);
    ProjectStore::open(&path, AccessMode::ReadOnly)
        .unwrap()
        .validate_full()
        .unwrap();
}

#[test]
fn a_package_from_a_newer_deadpan_opens_read_only_and_never_saves() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("future.deadpan");
    drop(seed_holds(&path, &["first"]));
    let newer = deadpan_store::DATABASE_SCHEMA_VERSION + 1;
    {
        let connection = rusqlite::Connection::open(path.join("project.sqlite")).unwrap();
        connection
            .execute_batch("CREATE TABLE future_feature(id INTEGER PRIMARY KEY) STRICT;")
            .unwrap();
        connection
            .pragma_update(None, "user_version", newer)
            .unwrap();
        connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .unwrap();
    }
    let before = std::fs::read(path.join("project.sqlite")).unwrap();
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    service.set_backup_interval_for_check(Duration::from_millis(1));
    let opened = command(&service, ProjectRequest::Open(path.clone()));
    assert!(opened.error.is_none(), "{:?}", opened.error);
    assert!(opened.message.as_deref().unwrap().contains("read-only"));
    let workspace = opened.workspace.unwrap();
    let reason = workspace.read_only.clone().expect("read-only session");
    assert!(reason.contains("newer Deadpan"), "{reason}");
    assert_eq!(workspace.document.revision_id().as_str(), "insert-first");
    // Edits, Undo, backups and restores are refused with the explanation.
    let first = workspace
        .document
        .children(workspace.document.root())
        .next()
        .unwrap()
        .clone();
    let refused = command(
        &service,
        edit_request(
            &workspace,
            ProjectEdit::Split {
                node: first,
                at: FrameDuration::new(4).unwrap(),
            },
        ),
    );
    assert!(refused.error.as_deref().unwrap().starts_with("Not saved"));
    let undo = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: workspace.document.revision_id().clone(),
        },
    );
    assert!(undo.error.as_deref().unwrap().contains("newer Deadpan"));
    let backup = command(
        &service,
        ProjectRequest::Backup(BackupRequest::Now {
            ticket: 1,
            expected_session: workspace.session,
        }),
    );
    assert!(backup.error.is_some());
    // No automatic backup of a read-only view, and no writer lock taken.
    std::thread::sleep(Duration::from_millis(100));
    assert!(backups_of(&path).is_empty());
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadWrite),
        Err(deadpan_store::StoreError::NewerSchema { .. })
    ));
    command(&service, ProjectRequest::Close);
    assert_eq!(std::fs::read(path.join("project.sqlite")).unwrap(), before);
}

#[test]
fn a_moved_linked_original_is_found_by_its_bookmark_and_relinked_on_open() {
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path().canonicalize().unwrap();
    let source = root.join("linked.mp4");
    std::fs::copy(fixture("cfr-bframes.mp4"), &source).unwrap();
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let path = root.join("moved.deadpan");
    create(&service, &path);
    service
        .submit(ProjectRequest::Import {
            path: source.clone(),
            media: ImportMedia::Video,
            ownership: OriginalOwnership::Linked { bookmark: None },
        })
        .unwrap();
    let registered = complete(&service);
    let original = registered.sources.values().next().unwrap().original.clone();
    assert!(
        original
            .linked()
            .and_then(|linked| linked.bookmark())
            .is_some_and(|bookmark| !bookmark.is_empty()),
        "linked imports record a bookmark"
    );
    command(&service, ProjectRequest::Close);
    std::fs::create_dir(root.join("Footage")).unwrap();
    let moved = root.join("Footage/renamed.mp4");
    std::fs::rename(&source, &moved).unwrap();
    let opened = command(&service, ProjectRequest::Open(path.clone()));
    let report = opened.opened.clone().unwrap();
    assert_eq!(report.originals.len(), 1);
    assert_eq!(
        report.originals[0].moved_to.as_deref(),
        Some(moved.as_path())
    );
    // Found, so not reported missing while it verifies.
    assert_eq!(report.missing().count(), 0);
    let relinked = wait(&service, |update| {
        update
            .relink
            .as_ref()
            .is_some_and(|status| status.state == RelinkState::Restored)
    });
    let workspace = relinked.workspace.unwrap();
    let source = workspace.sources.values().next().unwrap();
    assert_eq!(
        source.original.linked().map(|linked| linked.path()),
        Some(moved.as_path())
    );
    assert_eq!(source.original.version(), original.version() + 1);
    let report = relinked.opened.unwrap();
    assert!(matches!(
        report.originals[0].availability,
        deadpan_store::original_media::OriginalAvailability::Present
    ));
    assert!(relinked.message.unwrap().contains("bookmark"));
}

#[test]
fn a_moved_file_with_different_bytes_is_reported_missing_not_relinked() {
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path().canonicalize().unwrap();
    let source = root.join("linked.mp4");
    std::fs::copy(fixture("cfr-bframes.mp4"), &source).unwrap();
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let path = root.join("changed.deadpan");
    create(&service, &path);
    service
        .submit(ProjectRequest::Import {
            path: source.clone(),
            media: ImportMedia::Video,
            ownership: OriginalOwnership::Linked { bookmark: None },
        })
        .unwrap();
    let registered = complete(&service);
    let original = registered.sources.values().next().unwrap().original.clone();
    command(&service, ProjectRequest::Close);
    let moved = root.join("elsewhere.mp4");
    std::fs::rename(&source, &moved).unwrap();
    // Same file identity (the bookmark still finds it), different bytes.
    let mut bytes = std::fs::read(&moved).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    std::fs::write(&moved, bytes).unwrap();
    command(&service, ProjectRequest::Open(path.clone()));
    let failed = wait(&service, |update| {
        update
            .relink
            .as_ref()
            .is_some_and(|status| matches!(status.state, RelinkState::Failed(_)))
    });
    assert!(failed.error.is_none(), "not editor feedback");
    assert!(matches!(
        &failed.relink.as_ref().unwrap().state,
        RelinkState::Failed(error) if error.contains("not the same content")
    ));
    let report = failed.opened.unwrap();
    assert_eq!(report.missing().count(), 1, "missing again, for :relink");
    let workspace = failed.workspace.unwrap();
    assert_eq!(
        workspace.sources.values().next().unwrap().original,
        original,
        "the record is unchanged"
    );
}

/// Older unused development formats are refused before authored state,
/// operational tables or backup history can be changed.
#[test]
fn opening_a_schema66_package_refuses_without_rewriting_or_backing_it_up() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("previous.deadpan");
    drop(seed_holds(&path, &["first"]));
    let path = path.canonicalize().unwrap();
    {
        let connection = rusqlite::Connection::open(path.join("project.sqlite")).unwrap();
        connection
            .execute_batch(
                "DROP TABLE retired_identities;
                 DROP TABLE generation_variant_retention;
                 DROP TABLE generation_retention_state;",
            )
            .unwrap();
        connection.pragma_update(None, "user_version", 66).unwrap();
        connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .unwrap();
    }
    let database = path.join("project.sqlite");
    let before = std::fs::read(&database).unwrap();
    assert!(list_backups(&path).unwrap().is_empty());
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    service.set_backup_interval_for_check(Duration::from_millis(1));
    let opened = command(&service, ProjectRequest::Open(path.clone()));
    assert_eq!(
        opened.error,
        Some(format!(
            "Could not open {}: {}",
            path.display(),
            crate::recovery::describe_store_error(&deadpan_store::StoreError::UnsupportedSchema(
                66
            ))
        ))
    );
    assert!(
        opened
            .error
            .as_deref()
            .unwrap()
            .contains("Nothing was changed and no backup was made")
    );
    assert!(opened.workspace.is_none());
    assert!(opened.committed.is_none());
    assert!(opened.backups.latest.is_none() && opened.backups.running.is_none());
    command(&service, ProjectRequest::Close);
    assert!(list_backups(&path).unwrap().is_empty());
    assert_eq!(std::fs::read(&database).unwrap(), before);
    let connection = rusqlite::Connection::open_with_flags(
        &database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
            .unwrap(),
        66
    );
    let absent: i64 = connection.query_row(
        "SELECT count(*) FROM sqlite_schema WHERE name IN ('retired_identities', 'generation_variant_retention', 'generation_retention_state')",
        [], |row| row.get(0),
    ).unwrap();
    assert_eq!(
        absent, 0,
        "refusal must not recreate newer operational tables"
    );
}
