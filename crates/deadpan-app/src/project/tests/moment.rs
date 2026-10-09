use super::*;

fn request(workspace: &Workspace, scope: SequenceScope, index: usize) -> ProjectRequest {
    let source = workspace
        .sources
        .values()
        .find(|source| source.video_index.is_some())
        .unwrap();
    let parent = scope.resolve(workspace).unwrap().owner.clone();
    ProjectRequest::PasteMoment(MomentPaste {
        expected_session: workspace.session,
        expected_revision: workspace.document.revision_id().clone(),
        asset: source.asset.clone(),
        qualification: source.receipt.id().clone(),
        ordinals: 10..24,
        scope,
        parent,
        destination: crate::project::splice::Destination::Slot(index),
    })
}

fn initialize(service: &ProjectService) -> Arc<Workspace> {
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
    assert!(update.error.is_none(), "{:?}", update.error);
    assert_eq!(update.import.as_ref().unwrap().stage, ImportStage::Complete);
    update.workspace.unwrap()
}

#[test]
fn native_moment_is_one_undo_and_rejects_stale_or_changed_source_identity() {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
    )
    .unwrap();
    let before = initialize(&service);
    let update = command(&service, request(&before, SequenceScope::default(), 0));
    assert!(update.error.is_none(), "{:?}", update.error);
    let commit = update.committed.unwrap();
    assert_eq!(commit.cursor, Some(ProjectFrame(0)));
    let pasted = update.workspace.unwrap();
    assert_eq!(
        pasted.plan.duration().frames(),
        before.plan.duration().frames() + 14
    );
    assert_eq!(pasted.document.assets(), before.document.assets());
    assert_eq!(pasted.single_source, before.single_source);
    let NodeKind::Source { source } =
        &pasted.document.nodes()[commit.selected_node.as_ref().unwrap()].kind
    else {
        panic!("Source paste")
    };
    assert_eq!(source.duration.frames(), 14);
    let stale = command(&service, request(&before, SequenceScope::default(), 1));
    assert!(stale.error.is_some());
    assert!(stale.committed.is_none());
    assert_eq!(*stale.workspace.unwrap().document, *pasted.document);

    let mut wrong = request(&pasted, SequenceScope::default(), 1);
    if let ProjectRequest::PasteMoment(request) = &mut wrong {
        request.qualification = deadpan_core::SourceQualificationId::new("b".repeat(64)).unwrap();
    }
    let rejected = command(&service, wrong);
    assert!(rejected.error.unwrap().contains("qualification"));
    assert_eq!(*rejected.workspace.unwrap().document, *pasted.document);
    let undone = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: pasted.document.revision_id().clone(),
        },
    );
    let undone = undone.workspace.unwrap();
    assert_eq!(undone.document.nodes(), before.document.nodes());
    assert_eq!(undone.plan.duration(), before.plan.duration());
    let redone = command(
        &service,
        ProjectRequest::Redo {
            expected_revision: undone.document.revision_id().clone(),
        },
    );
    assert_eq!(
        redone.workspace.unwrap().document.nodes(),
        pasted.document.nodes()
    );
}

#[test]
fn uncached_moment_preparation_keeps_nested_group_edge_and_explicit_cursor() {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
    )
    .unwrap();
    let initial = initialize(&service);
    let path = initial.path.clone();
    command(&service, ProjectRequest::Close);
    {
        let mut store = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
        seed_command(
            &mut store,
            Command::Group {
                parent: initial.document.root().clone(),
                start: 0,
                end: 1,
                id: node("group"),
                label: "Original group".into(),
            },
            "group-original",
        );
    }
    let opened = command(&service, ProjectRequest::Open(path));
    let before = opened.workspace.unwrap();
    let scope = SequenceScope::default()
        .descend(&before, &node("group"))
        .unwrap();
    service.submit(request(&before, scope.clone(), 1)).unwrap();
    let pasted = wait(&service, |update| {
        update.committed.is_some()
            || update
                .import
                .as_ref()
                .is_some_and(|status| status.stage == ImportStage::Failed)
    });
    assert!(pasted.error.is_none(), "{:?}", pasted.error);
    let commit = pasted
        .committed
        .as_ref()
        .expect("prepared paste must commit");
    assert_eq!(commit.scope, scope);
    assert_eq!(
        commit.cursor,
        Some(ProjectFrame(before.plan.duration().frames()))
    );
    let workspace = pasted.workspace.unwrap();
    assert_eq!(
        workspace
            .document
            .children(workspace.document.root())
            .count(),
        1
    );
    let children: Vec<_> = workspace.document.children(&node("group")).collect();
    assert_eq!(children.len(), 2);
    assert_eq!(Some(children[1]), commit.selected_node.as_ref());
}
