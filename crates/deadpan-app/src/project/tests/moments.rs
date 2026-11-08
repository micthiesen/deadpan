use super::*;
use crate::project::moments::{Operation, Receipt, Request};
use deadpan_store::moments::{Action, MomentCatalog, MomentId, MomentName, MomentTags};

pub(super) fn initialize(service: &ProjectService) -> Arc<Workspace> {
    service
        .submit(ProjectRequest::CreateFromSource {
            ownership: OriginalOwnership::Managed,
            path: fixture("cfr-bframes.mp4"),
        })
        .unwrap();
    let update = wait(service, |update| {
        update.import.as_ref().is_some_and(|status| {
            matches!(status.stage, ImportStage::Complete | ImportStage::Failed)
        })
    });
    assert_eq!(update.import.as_ref().unwrap().stage, ImportStage::Complete);
    update.workspace.unwrap()
}

fn send(
    service: &ProjectService,
    session: u64,
    ticket: u64,
    operation: Operation,
) -> ProjectUpdate {
    assert!(!service.is_busy());
    let _ = service.take_update();
    service
        .submit(ProjectRequest::Moments(Request {
            ticket,
            session,
            operation,
        }))
        .unwrap();
    wait(service, |update| {
        !service.is_busy()
            && update
                .moments
                .as_ref()
                .is_some_and(|result| result.ticket == ticket)
    })
}

fn receipt(update: &ProjectUpdate) -> &Receipt {
    update.moments.as_ref().unwrap().result.as_ref().unwrap()
}

fn apply(catalog: &MomentCatalog, action: Action) -> Operation {
    Operation::Apply(deadpan_store::moments::Request {
        project_id: catalog.project_id.clone(),
        expected_version: catalog.version,
        action,
    })
}

#[test]
fn named_moments_save_outside_history_and_refuse_stale_sessions() {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
    )
    .unwrap();
    let initial = initialize(&service);
    let list = send(&service, initial.session, 1, Operation::List);
    let catalog = receipt(&list).catalog.clone();
    assert!(catalog.entries.is_empty());
    let saved = send(
        &service,
        initial.session,
        2,
        apply(
            &catalog,
            Action::Create {
                id: MomentId::new("laugh").unwrap(),
                name: MomentName::new("Laugh").unwrap(),
                tags: MomentTags::from_input(["reaction"]).unwrap(),
                start: 10,
                end: 24,
            },
        ),
    );
    assert!(receipt(&saved).changed);
    assert!(saved.error.is_none(), "{:?}", saved.error);
    let workspace = saved.workspace.as_ref().unwrap();
    assert_eq!(
        workspace.document.revision_id(),
        initial.document.revision_id()
    );
    assert_eq!(workspace.can_undo, initial.can_undo);
    assert_eq!(
        saved.moment_catalog.as_ref().unwrap(),
        &receipt(&saved).catalog
    );

    // A stale catalog version and another session both refuse without writing.
    let stale = send(
        &service,
        initial.session,
        3,
        apply(
            &catalog,
            Action::Delete {
                id: MomentId::new("laugh").unwrap(),
            },
        ),
    );
    assert!(stale.moments.as_ref().unwrap().result.is_err());
    let foreign = send(&service, initial.session + 1, 4, Operation::List);
    assert!(foreign.moments.as_ref().unwrap().result.is_err());
    let after = send(&service, initial.session, 5, Operation::List);
    assert_eq!(receipt(&after).catalog.entries.len(), 1);
    assert_eq!(receipt(&after).catalog.version, 1);
}
