//! Failures while opening another package keep the current save state intact.

use super::*;
use std::os::unix::fs::PermissionsExt as _;

#[test]
fn denied_open_does_not_mark_current_project_unsaved_or_hide_a_later_save_failure() {
    let scratch = tempfile::tempdir().unwrap();
    let current = scratch.path().join("saved.deadpan");
    let other = scratch.path().join("denied.deadpan");
    drop(seed_holds(&current, &["first", "second"]));
    drop(seed_holds(&other, &["other"]));
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let before = command(&service, ProjectRequest::Open(current.clone()))
        .workspace
        .unwrap();

    // This is an actual kernel EACCES from the candidate's writer lock.
    let lock = other.join(".writer.lock");
    let permissions = std::fs::metadata(&lock).unwrap().permissions();
    std::fs::set_permissions(&lock, std::fs::Permissions::from_mode(0o000)).unwrap();
    let failed = command(&service, ProjectRequest::Open(other.clone()));
    std::fs::set_permissions(&lock, permissions).unwrap();
    let error = failed.error.as_ref().expect("candidate Open must fail");
    assert!(error.contains(&other.display().to_string()), "{error}");
    assert_eq!(
        crate::recovery::storage_code(error),
        Some("PermissionDenied")
    );
    let retained = failed.workspace.as_ref().unwrap();
    assert_eq!(retained.session, before.session);
    assert_eq!(
        retained.document.revision_id(),
        before.document.revision_id()
    );
    assert!(
        failed.storage.is_none(),
        "the retained project is still saved"
    );

    // Read-only follow-up must not reinterpret the standing Open error.
    let listed = command(
        &service,
        ProjectRequest::Takes(crate::project::takes::Request {
            ticket: 1,
            session: before.session,
            revision: before.document.revision_id().clone(),
            operation: crate::project::takes::Operation::List,
        }),
    );
    assert!(listed.storage.is_none());

    // A subsequent real project failure still raises, then a saved edit clears.
    service
        .shared
        .storage_failure
        .store(true, Ordering::Release);
    let request = || {
        edit_request(
            &before,
            ProjectEdit::Delete {
                node: node("first"),
            },
        )
    };
    let refused = command(&service, request());
    assert_eq!(refused.storage.unwrap().code, "DiskFull");
    assert_eq!(
        refused.workspace.unwrap().document.revision_id(),
        before.document.revision_id()
    );
    let saved = command(&service, request());
    assert!(saved.error.is_none(), "{:?}", saved.error);
    assert!(saved.storage.is_none());
    assert_ne!(
        saved.workspace.unwrap().document.revision_id(),
        before.document.revision_id()
    );
    let opened = command(&service, ProjectRequest::Open(other.clone()));
    assert!(opened.error.is_none(), "{:?}", opened.error);
    assert!(opened.storage.is_none());
    assert_ne!(opened.workspace.unwrap().session, before.session);
    command(&service, ProjectRequest::Close);
    for path in [current, other] {
        ProjectStore::open(&path, AccessMode::ReadOnly)
            .unwrap()
            .validate_full()
            .unwrap();
    }
}
