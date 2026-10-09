use super::*;
use deadpan_core::{FrameRange, PitchPolicy, RetimePurpose};

fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}

fn same_authored(actual: &ProjectDocument, expected: &ProjectDocument) {
    let mut actual = serde_json::to_value(actual).unwrap();
    actual["revision_id"] = serde_json::to_value(expected.revision_id()).unwrap();
    assert_eq!(actual, serde_json::to_value(expected).unwrap());
}

#[test]
fn native_range_delete_nested_partitions_commit_once_and_reopen_with_undo_redo() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("nested-fragments.deadpan");
    let mut store = seed_holds(&path, &["leading", "tail"]);
    let mut nodes = BTreeMap::from([
        (
            node("group"),
            BeatNode::sequence("Group", vec![node("outer")]),
        ),
        (node("held"), BeatNode::hold("Retained input", hold(10))),
    ]);
    for (name, child, start, end) in [
        ("inner", "held", 2, 9),
        ("middle", "inner", 1, 6),
        ("outer", "middle", 1, 4),
    ] {
        nodes.insert(
            node(name),
            BeatNode {
                label: name.into(),
                audio_treatments: Default::default(),
                audio_editorial_edges: Default::default(),
                audio_edges: Default::default(),
                framing: None,
                kind: NodeKind::Retime {
                    child: node(child),
                    duration: FrameDuration::new(end - start).unwrap(),
                    mapping: range(start, end),
                    pitch: PitchPolicy::FollowSpeed,
                    purpose: RetimePurpose::Partition,
                },
                cutaways: Vec::new(),
                captions: Vec::new(),
            },
        );
    }
    seed_command(
        &mut store,
        Command::Insert {
            parent: node("root"),
            index: 1,
            subtree: Subtree {
                root: node("group"),
                nodes,
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
        "insert-fragments",
    );
    drop(store);
    let database = rusqlite::Connection::open(path.join("project.sqlite")).unwrap();
    let history_before: i64 = database
        .query_row("SELECT count(*) FROM history", [], |row| row.get(0))
        .unwrap();
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let before = command(&service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    let scope = SequenceScope::default()
        .descend(&before, &node("group"))
        .unwrap();
    let selected = range(11, 12);
    let required_ids = before
        .document
        .range_deletion(&node("group"), selected)
        .unwrap()
        .required_ids;
    let update = command(
        &service,
        edit_request_in(
            &before,
            scope.clone(),
            ProjectFrame(12),
            ProjectEdit::DeleteRange {
                parent: node("group"),
                range: selected,
            },
        ),
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    let receipt = update.committed.unwrap();
    let after = update.workspace.unwrap();
    let view = scope.resolve(&after).unwrap();
    assert_eq!(after.document.duration().unwrap().frames(), 22);
    assert_eq!((view.start, view.end, view.children.len()), (10, 12, 2));
    assert_eq!(receipt.scope, scope);
    assert_eq!(receipt.cursor, Some(ProjectFrame(11)));
    assert_eq!(receipt.selected_node.as_ref(), view.children.get(1));
    let history_after: i64 = database
        .query_row("SELECT count(*) FROM history", [], |row| row.get(0))
        .unwrap();
    assert_eq!(history_after, history_before + 1);
    let stored: String = database
        .query_row(
            "SELECT request FROM history WHERE revision_id=?1",
            [after.document.revision_id().as_str()],
            |row| row.get(0),
        )
        .unwrap();
    let request: CommandRequest = serde_json::from_str(&stored).unwrap();
    let Command::DeleteRange {
        parent,
        range: actual,
        identities,
        timing,
    } = request.command
    else {
        panic!("nested deletion must store one typed range command")
    };
    assert_eq!(parent, node("group"));
    assert_eq!(actual, selected);
    assert_eq!(identities.nodes.len(), required_ids);
    assert_eq!(timing.allocation, request.new_revision);
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    assert_eq!(*reopened.document, *after.document);
    let restored = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: reopened.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    same_authored(&restored.document, &before.document);
    let redone = command(
        &service,
        ProjectRequest::Redo {
            expected_revision: restored.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    same_authored(&redone.document, &after.document);
    assert_ne!(redone.document.revision_id(), after.document.revision_id());
}

#[test]
fn native_range_delete_captures_nested_owner_and_returns_exact_join_once() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("range.deadpan");
    let mut store = seed_holds(&path, &["leading", "left", "right", "tail"]);
    seed_command(
        &mut store,
        Command::Group {
            parent: node("root"),
            start: 1,
            end: 3,
            id: node("group"),
            label: "Group".into(),
        },
        "group",
    );
    drop(store);
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let before = command(&service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    let scope = SequenceScope::default()
        .descend(&before, &node("group"))
        .unwrap();
    let selected = range(12, 23);
    let required_ids = before
        .document
        .range_deletion(&node("group"), selected)
        .unwrap()
        .required_ids;
    let update = command(
        &service,
        edit_request_in(
            &before,
            scope.clone(),
            ProjectFrame(29),
            ProjectEdit::DeleteRange {
                parent: node("group"),
                range: selected,
            },
        ),
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    let receipt = update.committed.unwrap();
    assert_eq!(receipt.scope, scope);
    assert_eq!(receipt.cursor, Some(ProjectFrame(12)));
    let after = update.workspace.unwrap();
    assert_eq!(after.document.duration().unwrap().frames(), 29);
    let view = scope.resolve(&after).unwrap();
    assert_eq!((view.start, view.end), (10, 19));
    assert_eq!(receipt.selected_node.as_ref(), view.children.get(1));
    let database = rusqlite::Connection::open(path.join("project.sqlite")).unwrap();
    let stored: String = database
        .query_row(
            "SELECT request FROM history WHERE revision_id=?1",
            [after.document.revision_id().as_str()],
            |row| row.get(0),
        )
        .unwrap();
    let request: CommandRequest = serde_json::from_str(&stored).unwrap();
    let Command::DeleteRange {
        parent,
        range: actual,
        identities,
        timing,
    } = request.command
    else {
        panic!("native deletion must store one typed range command")
    };
    assert_eq!(parent, node("group"));
    assert_eq!(actual, selected);
    assert_eq!(identities.nodes.len(), required_ids);
    assert_eq!(timing.allocation, request.new_revision);
    assert_eq!(timing.ordinal, 0);
    let stale = edit_request_in(
        &before,
        scope.clone(),
        ProjectFrame(20),
        ProjectEdit::DeleteRange {
            parent: node("group"),
            range: selected,
        },
    );
    let denied = command(&service, stale);
    assert!(
        denied
            .error
            .as_deref()
            .is_some_and(|error| error.contains("changed"))
    );
    assert_eq!(*denied.workspace.unwrap().document, *after.document);
    let restored = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: after.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    same_authored(&restored.document, &before.document);
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    let stale_session = command(
        &service,
        edit_request_in(
            &restored,
            scope.clone(),
            ProjectFrame(20),
            ProjectEdit::DeleteRange {
                parent: node("group"),
                range: selected,
            },
        ),
    );
    assert!(
        stale_session
            .error
            .as_deref()
            .is_some_and(|error| error.contains("session changed"))
    );
    assert_eq!(
        *stale_session.workspace.unwrap().document,
        *reopened.document
    );
    for invalid in [range(20, 20), range(0, 23), range(20, 31)] {
        let denied = command(
            &service,
            edit_request_in(
                &reopened,
                scope.clone(),
                ProjectFrame(20),
                ProjectEdit::DeleteRange {
                    parent: node("group"),
                    range: invalid,
                },
            ),
        );
        assert!(denied.error.is_some());
        assert!(denied.committed.is_none());
        assert_eq!(*denied.workspace.unwrap().document, *reopened.document);
    }
    let wrong_owner = command(
        &service,
        edit_request_in(
            &reopened,
            scope,
            ProjectFrame(20),
            ProjectEdit::DeleteRange {
                parent: node("root"),
                range: selected,
            },
        ),
    );
    assert!(wrong_owner.error.is_some());
    assert_eq!(*wrong_owner.workspace.unwrap().document, *reopened.document);
}

#[test]
fn native_range_delete_terminal_and_full_original_keep_the_protected_baseline() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    command(
        &harness.service,
        ProjectRequest::CreateFromSource {
            ownership: OriginalOwnership::Managed,
            path: fixture("cfr-bframes.mp4"),
        },
    );
    harness.finish(harness.job());
    harness.finish(harness.job());
    let baseline = complete(&harness.service);
    command(&harness.service, ProjectRequest::Close);
    for selected in [range(100, 120), range(0, 120)] {
        let before = command(
            &harness.service,
            ProjectRequest::Open(baseline.path.clone()),
        )
        .workspace
        .unwrap();
        let update = command(
            &harness.service,
            edit_request_in(
                &before,
                SequenceScope::default(),
                ProjectFrame(120),
                ProjectEdit::DeleteRange {
                    parent: before.document.root().clone(),
                    range: selected,
                },
            ),
        );
        assert!(update.error.is_none(), "{:?}", update.error);
        let receipt = update.committed.unwrap();
        assert_eq!(receipt.cursor, Some(selected.start()));
        let after = update.workspace.unwrap();
        assert_eq!(
            after.document.duration().unwrap().frames(),
            120 - selected.duration().frames()
        );
        assert_eq!(after.single_source, baseline.single_source);
        assert_eq!(after.document.assets(), baseline.document.assets());
        assert_eq!(receipt.selected_node.is_none(), selected.start().0 == 0);
        command(&harness.service, ProjectRequest::Close);
        let reopened = command(
            &harness.service,
            ProjectRequest::Open(baseline.path.clone()),
        )
        .workspace
        .unwrap();
        assert_eq!(*reopened.document, *after.document);
        let restored = command(
            &harness.service,
            ProjectRequest::Undo {
                expected_revision: reopened.document.revision_id().clone(),
            },
        )
        .workspace
        .unwrap();
        same_authored(&restored.document, &baseline.document);
        assert!(!restored.can_undo);
        command(&harness.service, ProjectRequest::Close);
    }
}

#[test]
fn native_range_delete_reports_saved_when_refresh_fails_then_reopen_undo_restores() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("range-refresh.deadpan");
    drop(seed_holds(&path, &["beat"]));
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let before = command(&service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    service
        .shared
        .render_commit_refresh_failure
        .store(true, Ordering::Release);
    let update = command(
        &service,
        edit_request_in(
            &before,
            SequenceScope::default(),
            ProjectFrame(6),
            ProjectEdit::DeleteRange {
                parent: node("root"),
                range: range(2, 6),
            },
        ),
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    let receipt = update.committed.unwrap();
    assert_ne!(&receipt.revision, before.document.revision_id());
    assert_eq!(receipt.cursor, Some(ProjectFrame(2)));
    assert_eq!(
        receipt.selected_node, None,
        "no refreshed selection is invented"
    );
    assert_eq!(*update.workspace.unwrap().document, *before.document);
    let message = update.message.unwrap();
    assert!(
        message.contains("cut and saved")
            && message.contains("could not refresh")
            && message.contains("Reopen this project before editing or undoing"),
        "{message}"
    );
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly).unwrap();
    let durable = reader.snapshot().unwrap();
    assert_eq!(durable.revision_id(), &receipt.revision);
    assert_eq!(durable.duration().unwrap().frames(), 6);
    drop(reader);
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    assert_eq!(*reopened.document, durable);
    let restored = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: reopened.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    same_authored(&restored.document, &before.document);
}
