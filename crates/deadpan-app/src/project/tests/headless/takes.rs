use super::*;
use deadpan_store::takes::{Action, Request as TakeRequest, TakeId, TakeName};

fn take(workspace: &Workspace, request: TakeRequest) -> Operation {
    Operation::Execute {
        project_id: workspace.document.project_id().clone(),
        command: Box::new(ShortOperation::Take {
            request: Box::new(request),
            dry_run: false,
        }),
    }
}

#[test]
fn live_takes_refresh_native_catalog_and_refuse_unsaved_previews() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("live-takes.deadpan");
    let harness = Harness::new();
    let initial = opened(&harness, &path);
    let mut client = client(&path);
    let request = TakeRequest {
        project_id: initial.document.project_id().clone(),
        expected_revision: initial.document.revision_id().clone(),
        expected_version: 0,
        action: Action::Create {
            id: TakeId::new("first").unwrap(),
            name: TakeName::new("First version").unwrap(),
        },
    };
    harness.service.set_preview_active(true);
    let refused = live_project::request(&mut client, take(&initial, request.clone())).unwrap_err();
    assert_eq!(refused.code, "TakePreviewActive");
    let list = live_project::request(
        &mut client,
        Operation::Execute {
            project_id: initial.document.project_id().clone(),
            command: Box::new(ShortOperation::TakeCatalog),
        },
    )
    .unwrap();
    assert!(
        matches!(list, Reply::Completed { output, .. } if output["takes"]["entries"].as_array().unwrap().is_empty())
    );
    harness.service.set_preview_active(false);
    let saved = live_project::request(&mut client, take(&initial, request)).unwrap();
    assert!(
        matches!(saved, Reply::Completed { output, committed_revision: None, refresh_error: None, .. } if output["outcome"]["catalog"]["entries"][0]["name"] == "First version")
    );
    let update = wait(&harness.service, |update| update.take_catalog.is_some());
    assert_eq!(*update.workspace.unwrap().document, *initial.document);
    assert_eq!(
        update.take_catalog.unwrap().entries[0].name.as_str(),
        "First version"
    );
    committed(
        live_project::request(&mut client, change(&initial, "longer", 17)).unwrap(),
        "longer",
    );
    let changed = wait(&harness.service, |update| {
        update
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.document.revision_id().as_str() == "longer")
    })
    .workspace
    .unwrap();
    let restored = live_project::request(
        &mut client,
        take(
            &changed,
            TakeRequest {
                project_id: changed.document.project_id().clone(),
                expected_revision: changed.document.revision_id().clone(),
                expected_version: 1,
                action: Action::Restore {
                    id: TakeId::new("first").unwrap(),
                    expected_snapshot: initial.document.revision_id().clone(),
                    new_revision: RevisionId::new("take-opened").unwrap(),
                },
            },
        ),
    )
    .unwrap();
    assert!(
        matches!(restored, Reply::Completed { committed_revision: Some(revision), refresh_error: None, .. } if revision.as_str() == "take-opened")
    );
    let update = wait(&harness.service, |update| {
        update
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.document.revision_id().as_str() == "take-opened")
    });
    assert_eq!(
        update.workspace.unwrap().document.nodes(),
        initial.document.nodes()
    );
    assert_eq!(
        update.take_catalog.unwrap().revision_id.as_str(),
        "take-opened"
    );
    shutdown(&harness);
}
