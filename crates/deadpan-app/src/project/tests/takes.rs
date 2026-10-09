use super::*;
use crate::project::takes::{Operation, Receipt, Request};
use deadpan_store::takes::{Action, TakeCatalog, TakeId, TakeName};

fn send(
    service: &ProjectService,
    workspace: &Workspace,
    ticket: u64,
    operation: Operation,
) -> ProjectUpdate {
    command(
        service,
        ProjectRequest::Takes(Request {
            ticket,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            operation,
        }),
    )
}

fn receipt(update: &ProjectUpdate) -> &Receipt {
    update.takes.as_ref().unwrap().result.as_ref().unwrap()
}

fn request(catalog: &TakeCatalog, action: Action) -> Operation {
    Operation::Apply(deadpan_store::takes::Request {
        project_id: catalog.project_id.clone(),
        expected_revision: catalog.revision_id.clone(),
        expected_version: catalog.version,
        action,
    })
}

fn create() -> Action {
    Action::Create {
        id: TakeId::new("first-take").unwrap(),
        name: TakeName::new("First version").unwrap(),
    }
}

#[test]
fn named_takes_restore_as_one_edit_and_metadata_preserves_head_and_redo() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("takes.deadpan");
    drop(seed_holds(&path, &["a"]));
    let harness = Harness::new();
    let initial = command(&harness.service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    let list = send(&harness.service, &initial, 1, Operation::List);
    let saved = send(
        &harness.service,
        &initial,
        2,
        request(&receipt(&list).catalog, create()),
    );
    assert_eq!(
        *saved.workspace.as_ref().unwrap().document,
        *initial.document
    );
    assert!(saved.committed.is_none());
    let take = receipt(&saved).catalog.entries[0].clone();
    let edit = edited(
        &harness.service,
        &initial,
        ProjectEdit::Split {
            node: node("a"),
            at: FrameDuration::new(5).unwrap(),
        },
    )
    .workspace
    .unwrap();
    let list = send(&harness.service, &edit, 3, Operation::List);
    let restored = send(
        &harness.service,
        &edit,
        4,
        request(
            &receipt(&list).catalog,
            Action::Restore {
                id: take.id.clone(),
                expected_snapshot: take.revision_id.clone(),
                new_revision: RevisionId::new("restored-take").unwrap(),
            },
        ),
    );
    let restored_workspace = restored.workspace.as_ref().unwrap();
    assert_eq!(
        restored_workspace.document.nodes(),
        initial.document.nodes()
    );
    assert_eq!(
        receipt(&restored)
            .committed_revision
            .as_ref()
            .unwrap()
            .as_str(),
        "restored-take"
    );
    assert!(
        restored.committed.is_none(),
        "take restore must not invent a selected beat"
    );
    let undo = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: restored_workspace.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(undo.document.nodes(), edit.document.nodes());
    assert!(undo.can_redo);
    let list = send(&harness.service, &undo, 5, Operation::List);
    let rename = send(
        &harness.service,
        &undo,
        6,
        request(
            &receipt(&list).catalog,
            Action::Rename {
                id: take.id.clone(),
                expected_snapshot: take.revision_id.clone(),
                name: TakeName::new("Alternate").unwrap(),
            },
        ),
    );
    assert_eq!(
        rename.workspace.as_ref().unwrap().document.revision_id(),
        undo.document.revision_id()
    );
    assert!(rename.workspace.as_ref().unwrap().can_redo);
    let delete = send(
        &harness.service,
        &undo,
        7,
        request(
            &receipt(&rename).catalog,
            Action::Delete {
                id: take.id,
                expected_snapshot: take.revision_id,
            },
        ),
    );
    assert!(receipt(&delete).catalog.entries.is_empty());
    let redo = command(
        &harness.service,
        ProjectRequest::Redo {
            expected_revision: undo.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(redo.document.nodes(), initial.document.nodes());
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    assert!(
        receipt(&send(&harness.service, &reopened, 8, Operation::List))
            .catalog
            .entries
            .is_empty()
    );
}

#[test]
fn take_requests_refuse_stale_catalog_preview_and_session_without_writes() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("takes-refusal.deadpan");
    drop(seed_holds(&path, &["a"]));
    let harness = Harness::new();
    let initial = command(&harness.service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    let list = send(&harness.service, &initial, 1, Operation::List);
    let operation = request(&receipt(&list).catalog, create());
    harness.service.set_preview_active(true);
    let refused = send(&harness.service, &initial, 2, operation.clone());
    assert!(
        refused
            .takes
            .unwrap()
            .result
            .unwrap_err()
            .contains("preview")
    );
    assert!(
        receipt(&send(&harness.service, &initial, 3, Operation::List))
            .catalog
            .entries
            .is_empty()
    );
    harness.service.set_preview_active(false);
    let saved = send(&harness.service, &initial, 4, operation.clone());
    assert_eq!(receipt(&saved).catalog.entries.len(), 1);
    let stale = send(&harness.service, &initial, 5, operation.clone());
    assert!(stale.takes.unwrap().result.is_err());
    command(&harness.service, ProjectRequest::Close);
    let current = command(&harness.service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    assert_ne!(initial.session, current.session);
    let stale = send(&harness.service, &initial, 6, operation);
    assert!(
        stale
            .takes
            .unwrap()
            .result
            .unwrap_err()
            .contains("session changed")
    );
    assert_eq!(
        receipt(&send(&harness.service, &current, 7, Operation::List))
            .catalog
            .entries
            .len(),
        1
    );
}

#[test]
fn take_restore_retains_durable_receipt_when_workspace_refresh_fails() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("takes-refresh.deadpan");
    drop(seed_holds(&path, &["a"]));
    let harness = Harness::new();
    let initial = command(&harness.service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    let list = send(&harness.service, &initial, 1, Operation::List);
    let saved = send(
        &harness.service,
        &initial,
        2,
        request(&receipt(&list).catalog, create()),
    );
    let take = receipt(&saved).catalog.entries[0].clone();
    let edit = edited(
        &harness.service,
        &initial,
        ProjectEdit::Split {
            node: node("a"),
            at: FrameDuration::new(5).unwrap(),
        },
    )
    .workspace
    .unwrap();
    let list = send(&harness.service, &edit, 3, Operation::List);
    harness
        .service
        .shared
        .workspace_refresh_failure
        .store(true, Ordering::Release);
    let saved = send(
        &harness.service,
        &edit,
        4,
        request(
            &receipt(&list).catalog,
            Action::Restore {
                id: take.id,
                expected_snapshot: take.revision_id,
                new_revision: RevisionId::new("saved-despite-refresh").unwrap(),
            },
        ),
    );
    assert_eq!(
        receipt(&saved)
            .committed_revision
            .as_ref()
            .unwrap()
            .as_str(),
        "saved-despite-refresh"
    );
    assert!(
        receipt(&saved)
            .refresh_error
            .as_ref()
            .unwrap()
            .contains("Take opened and saved")
    );
    assert_eq!(
        saved.workspace.unwrap().document.revision_id(),
        edit.document.revision_id()
    );
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly).unwrap();
    assert_eq!(reader.snapshot().unwrap().nodes(), initial.document.nodes());
}
