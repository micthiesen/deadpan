use super::*;

use deadpan_core::{FrameRange, IterationOrder, PitchPolicy, RetimePurpose, Subtree};
use deadpan_store::original_media::OriginalOwnership;

fn nested_document(path: &Path) -> ProjectStore {
    let mut store = seed_holds(path, &[]);
    let inner = node("inner");
    let repeat_body = node("repeat-body");
    let retime_body = node("retime-body");
    let nodes = BTreeMap::from([
        (
            node("outer"),
            BeatNode::sequence(
                "outer",
                vec![
                    inner.clone(),
                    node("outer-tail"),
                    node("repeat"),
                    node("retime"),
                ],
            ),
        ),
        (
            inner,
            BeatNode::sequence("inner", vec![node("a"), node("b"), node("empty")]),
        ),
        (node("a"), BeatNode::hold("a", hold(10))),
        (node("b"), BeatNode::hold("b", hold(12))),
        (node("empty"), BeatNode::sequence("empty", vec![])),
        (
            node("repeat"),
            BeatNode {
                audio_treatments: Default::default(),
                label: "repeat".into(),
                kind: NodeKind::Repeat {
                    child: repeat_body.clone(),
                    iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 2).unwrap(),
                    gap: None,
                },
                audio_edges: Default::default(),
                framing: None,
            },
        ),
        (
            repeat_body,
            BeatNode::sequence("repeat body", vec![node("repeat-hold")]),
        ),
        (node("repeat-hold"), BeatNode::hold("repeat hold", hold(2))),
        (
            node("retime"),
            BeatNode {
                audio_treatments: Default::default(),
                label: "retime".into(),
                kind: NodeKind::Retime {
                    child: retime_body.clone(),
                    duration: FrameDuration::new(3).unwrap(),
                    mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(3)).unwrap(),
                    pitch: PitchPolicy::Preserve,
                    purpose: RetimePurpose::Edit,
                },
                audio_edges: Default::default(),
                framing: None,
            },
        ),
        (
            retime_body,
            BeatNode::sequence("retime body", vec![node("retime-hold")]),
        ),
        (node("retime-hold"), BeatNode::hold("retime hold", hold(3))),
        (node("outer-tail"), BeatNode::hold("outer tail", hold(5))),
    ]);
    seed_command(
        &mut store,
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("outer"),
                nodes,
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
        "nested",
    );
    seed_command(
        &mut store,
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("before"),
                nodes: BTreeMap::from([(node("before"), BeatNode::hold("before", hold(5)))]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
        "before",
    );
    seed_command(
        &mut store,
        Command::Insert {
            parent: node("root"),
            index: 2,
            subtree: Subtree {
                root: node("after"),
                nodes: BTreeMap::from([(node("after"), BeatNode::hold("after", hold(7)))]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
        "after",
    );
    store
}

fn open_nested(path: &Path) -> (ProjectService, Arc<Workspace>) {
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let workspace = command(&service, ProjectRequest::Open(path.to_path_buf()))
        .workspace
        .unwrap();
    (service, workspace)
}

fn nested_scope(workspace: &Workspace) -> SequenceScope {
    SequenceScope::default()
        .descend(workspace, &node("outer"))
        .unwrap()
        .descend(workspace, &node("inner"))
        .unwrap()
}

#[test]
fn scope_resolves_absolute_edges_and_blocks_composite_descent_and_pause_seams() {
    let scratch = tempfile::tempdir().unwrap();
    drop(nested_document(&scratch.path().join("scope.deadpan")));
    let (service, workspace) = open_nested(&scratch.path().join("scope.deadpan"));

    let root = SequenceScope::default();
    let root_view = root.resolve(&workspace).unwrap();
    assert_eq!((root_view.start, root_view.end), (0, 46));
    assert_eq!(
        root_view.children,
        [node("before"), node("outer"), node("after")]
    );

    let outer = root.descend(&workspace, &node("outer")).unwrap();
    let outer_view = outer.resolve(&workspace).unwrap();
    assert_eq!((outer_view.start, outer_view.end), (5, 39));
    assert!(outer.descend(&workspace, &node("repeat")).is_err());
    assert!(outer.descend(&workspace, &node("retime")).is_err());

    let inner = nested_scope(&workspace);
    let inner_view = inner.resolve(&workspace).unwrap();
    assert_eq!((inner_view.start, inner_view.end), (5, 27));
    let empty = inner.descend(&workspace, &node("empty")).unwrap();
    let empty_view = empty.resolve(&workspace).unwrap();
    assert_eq!((empty_view.start, empty_view.end), (27, 27));
    assert!(empty_view.children.is_empty());
    assert!(matches!(inner.parent(), Some(parent) if parent == outer));
    assert!(root.parent().is_none());

    assert_eq!(inner.enclosing_cursor(&workspace, 6), inner);
    assert_eq!(inner.enclosing_cursor(&workspace, 27), outer);
    assert_eq!(inner.enclosing_cursor(&workspace, 39), root);
    assert_eq!(root.enclosing_cursor(&workspace, 46), root);
    assert_eq!(empty.enclosing_cursor(&workspace, 27), outer);

    inner.check_pause(&workspace, ProjectFrame(6)).unwrap();
    assert!(
        inner
            .check_pause(&workspace, ProjectFrame(5))
            .unwrap_err()
            .contains("Backspace")
    );
    assert!(
        inner
            .check_pause(&workspace, ProjectFrame(27))
            .unwrap_err()
            .contains("Backspace")
    );
    // Ordinary Hold interiors and seams before a composite remain legal.
    outer.check_pause(&workspace, ProjectFrame(28)).unwrap();
    outer.check_pause(&workspace, ProjectFrame(32)).unwrap();
    // The scoped UI cannot address the interior of a Repeat play or Retime.
    assert!(outer.check_pause(&workspace, ProjectFrame(33)).is_err());
    assert!(outer.check_pause(&workspace, ProjectFrame(37)).is_err());
    command(&service, ProjectRequest::Close);
}

#[test]
fn nested_edits_validate_direct_membership_and_rebuild_sibling_selection_in_scope() {
    use deadpan_core::{Framing, FramingPose};

    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("nested-edits.deadpan");
    drop(nested_document(&path));
    let (service, mut workspace) = open_nested(&path);
    let scope = nested_scope(&workspace);

    let update = command(
        &service,
        edit_request_in(
            &workspace,
            scope.clone(),
            ProjectFrame(18),
            ProjectEdit::HoldDuration {
                node: node("b"),
                duration: FrameDuration::new(14).unwrap(),
            },
        ),
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    let committed = update.committed.unwrap();
    assert_eq!(committed.scope, scope);
    assert_eq!(committed.cursor, None);
    workspace = update.workspace.unwrap();
    assert_eq!(
        workspace
            .plan
            .node_duration(&node("inner"))
            .unwrap()
            .frames(),
        24
    );

    let update = command(
        &service,
        edit_request_in(
            &workspace,
            scope.clone(),
            ProjectFrame(16),
            ProjectEdit::SetFraming {
                node: node("a"),
                framing: Some(Framing::static_pose(FramingPose::default()).unwrap()),
            },
        ),
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    let committed = update.committed.unwrap();
    assert!(committed.preserve_cursor);
    assert_eq!(committed.cursor, Some(ProjectFrame(16)));
    assert_eq!(committed.scope, scope);
    workspace = update.workspace.unwrap();

    let update = command(
        &service,
        edit_request_in(
            &workspace,
            scope.clone(),
            ProjectFrame(9),
            ProjectEdit::Split {
                node: node("a"),
                at: FrameDuration::new(4).unwrap(),
            },
        ),
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    let right = update.committed.unwrap().selected_node.unwrap();
    workspace = update.workspace.unwrap();
    let children = scope.resolve(&workspace).unwrap().children;
    assert_eq!(children[1], right);
    assert_eq!(
        workspace
            .plan
            .node_duration(&node("inner"))
            .unwrap()
            .frames(),
        24
    );

    let update = command(
        &service,
        edit_request_in(
            &workspace,
            scope.clone(),
            ProjectFrame(20),
            ProjectEdit::Delete { node: node("b") },
        ),
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    let committed = update.committed.unwrap();
    assert_eq!(committed.scope, scope);
    assert_eq!(committed.selected_node, Some(node("empty")));
    workspace = update.workspace.unwrap();
    assert!(scope.resolve(&workspace).is_ok());

    let hidden = command(
        &service,
        edit_request_in(
            &workspace,
            scope.clone(),
            ProjectFrame(20),
            ProjectEdit::Delete {
                node: node("outer-tail"),
            },
        ),
    );
    assert!(
        hidden
            .error
            .unwrap()
            .contains("direct child of the active Sequence")
    );
    assert!(hidden.committed.is_none());
    assert_eq!(*hidden.workspace.unwrap().document, *workspace.document);
    command(&service, ProjectRequest::Close);
}

#[test]
fn stale_nested_scope_and_revision_are_rejected_and_reconcile_to_nearest_parent() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("stale-scope.deadpan");
    drop(nested_document(&path));
    let (service, workspace) = open_nested(&path);
    let stale_scope = nested_scope(&workspace);

    let stale_revision = command(
        &service,
        ProjectRequest::Edit {
            expected_session: workspace.session,
            expected_revision: RevisionId::new("stale-revision").unwrap(),
            cursor: ProjectFrame(6),
            scope: stale_scope.clone(),
            edit: ProjectEdit::HoldDuration {
                node: node("a"),
                duration: FrameDuration::new(11).unwrap(),
            },
        },
    );
    assert!(stale_revision.error.unwrap().contains("changed before"));
    assert!(stale_revision.committed.is_none());
    assert_eq!(
        *stale_revision.workspace.unwrap().document,
        *workspace.document
    );

    let deleted = command(
        &service,
        edit_request(
            &workspace,
            ProjectEdit::Delete {
                node: node("outer"),
            },
        ),
    );
    assert!(deleted.error.is_none(), "{:?}", deleted.error);
    let updated = deleted.workspace.unwrap();
    assert!(stale_scope.resolve(&updated).is_err());
    let stale_path = command(
        &service,
        edit_request_in(
            &updated,
            stale_scope.clone(),
            ProjectFrame(0),
            ProjectEdit::HoldDuration {
                node: node("a"),
                duration: FrameDuration::new(11).unwrap(),
            },
        ),
    );
    assert!(stale_path.error.is_some());
    assert!(stale_path.committed.is_none());
    assert_eq!(*stale_path.workspace.unwrap().document, *updated.document);

    let mut reconciled = stale_scope;
    reconciled.reconcile(&updated);
    assert_eq!(reconciled, SequenceScope::default());
    command(&service, ProjectRequest::Close);
}

#[test]
fn nested_insert_checks_parent_and_retains_captured_scope_for_cached_and_async_paths() {
    use std::path::PathBuf;

    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("nested-insert.deadpan");
    drop(nested_document(&path));
    let harness = Harness::new();
    let service = &harness.service;
    let opened = command(service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    let scope = nested_scope(&opened);
    let source_a_path = fixture("cfr-bframes.mp4");
    let import_a = command(
        service,
        ProjectRequest::Import {
            path: source_a_path.clone(),
            media: ImportMedia::Video,
            ownership: OriginalOwnership::Managed,
        },
    );
    assert!(import_a.error.is_none(), "{:?}", import_a.error);
    harness.finish(harness.job());
    harness.finish(harness.job());
    let import_a = wait(service, |update| {
        update.import.as_ref().is_some_and(|status| {
            status.path == source_a_path && status.stage == ImportStage::Complete
        })
    });
    let workspace = import_a.workspace.unwrap();
    let asset_a = import_a.import.unwrap().asset.unwrap();
    let source_a_label = workspace.sources[&asset_a].label.clone();

    let invalid_parent = command(
        service,
        ProjectRequest::Insert {
            expected_session: workspace.session,
            expected_revision: workspace.document.revision_id().clone(),
            asset: asset_a.clone(),
            scope: scope.clone(),
            parent: node("root"),
            index: 0,
        },
    );
    assert!(
        invalid_parent
            .error
            .unwrap()
            .contains("active Sequence scope")
    );
    assert!(invalid_parent.committed.is_none());
    assert_eq!(
        *invalid_parent.workspace.unwrap().document,
        *workspace.document
    );

    let cached = command(
        service,
        ProjectRequest::Insert {
            expected_session: workspace.session,
            expected_revision: workspace.document.revision_id().clone(),
            asset: asset_a.clone(),
            scope: scope.clone(),
            parent: node("inner"),
            index: scope.resolve(&workspace).unwrap().children.len(),
        },
    );
    assert!(cached.error.is_none(), "{:?}", cached.error);
    let cached_commit = cached.committed.unwrap();
    assert_eq!(cached_commit.scope, scope);
    let workspace = cached.workspace.unwrap();
    assert_eq!(
        scope.resolve(&workspace).unwrap().children.last(),
        cached_commit.selected_node.as_ref()
    );

    let source_b_path = fixture("offset-bframes.mp4");
    command(
        service,
        ProjectRequest::Import {
            path: source_b_path.clone(),
            media: ImportMedia::Video,
            ownership: OriginalOwnership::Managed,
        },
    );
    harness.finish(harness.job());
    harness.finish(harness.job());
    let second_import = wait(service, |update| {
        update.import.as_ref().is_some_and(|status| {
            status.path == source_b_path && status.stage == ImportStage::Complete
        })
    });
    assert!(second_import.error.is_none(), "{:?}", second_import.error);
    let workspace = second_import.workspace.unwrap();
    let async_insert = command(
        service,
        ProjectRequest::Insert {
            expected_session: workspace.session,
            expected_revision: workspace.document.revision_id().clone(),
            asset: asset_a,
            scope: scope.clone(),
            parent: node("inner"),
            index: scope.resolve(&workspace).unwrap().children.len(),
        },
    );
    assert!(async_insert.error.is_none(), "{:?}", async_insert.error);
    assert!(async_insert.committed.is_none());
    // Hold the real preparation result until after observing the pending state.
    // A live worker can otherwise finish before command() reads the coalesced
    // mailbox, legitimately returning the completed insert as its first update.
    harness.finish(harness.job());
    let source_label_path = PathBuf::from(source_a_label);
    let completed = wait(service, |update| {
        update.import.as_ref().is_some_and(|status| {
            status.stage == ImportStage::Complete && status.path == source_label_path
        })
    });
    assert!(completed.error.is_none(), "{:?}", completed.error);
    let committed = completed.committed.unwrap();
    assert_eq!(committed.scope, scope);
    let selected = committed.selected_node.unwrap();
    let workspace = completed.workspace.unwrap();
    assert_eq!(
        scope.resolve(&workspace).unwrap().children.last(),
        Some(&selected)
    );
    command(service, ProjectRequest::Close);
}
