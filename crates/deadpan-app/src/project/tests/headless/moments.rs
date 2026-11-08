use super::*;
use deadpan_store::moments::{Action, MomentId, MomentName, MomentTags, Request as MomentRequest};

#[test]
fn live_moments_refresh_the_native_catalog_without_an_edit() {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
    )
    .unwrap();
    let initial = super::super::moments::initialize(&service);
    let mut client = client(&initial.path);
    let list = live_project::request(
        &mut client,
        Operation::Execute {
            project_id: initial.document.project_id().clone(),
            command: Box::new(ShortOperation::MomentCatalog),
        },
    )
    .unwrap();
    assert!(
        matches!(list, Reply::Completed { output, .. } if output["moments"]["entries"].as_array().unwrap().is_empty())
    );
    let saved = live_project::request(
        &mut client,
        Operation::Execute {
            project_id: initial.document.project_id().clone(),
            command: Box::new(ShortOperation::Moment {
                request: Box::new(MomentRequest {
                    project_id: initial.document.project_id().clone(),
                    expected_version: 0,
                    action: Action::Create {
                        id: MomentId::new("remote").unwrap(),
                        name: MomentName::new("Remote stare").unwrap(),
                        tags: MomentTags::from_input(["detail"]).unwrap(),
                        start: 4,
                        end: 9,
                    },
                }),
                dry_run: false,
            }),
        },
    )
    .unwrap();
    assert!(
        matches!(
            saved,
            Reply::Completed {
                committed_revision: None,
                ..
            }
        ),
        "{saved:?}"
    );
    let update = wait(&service, |update| update.moment_catalog.is_some());
    assert_eq!(
        update.moment_catalog.unwrap().entries[0].name.as_str(),
        "Remote stare"
    );
    assert_eq!(
        update.workspace.unwrap().document.revision_id(),
        initial.document.revision_id()
    );
    service.shutdown();
    until(|| service.is_shutdown_complete());
}
