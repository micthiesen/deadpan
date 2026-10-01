use super::*;
use deadpan_core::SliceCaptureSelection;

fn child(workspace: &Workspace, serial: u64, name: &str) -> CaptureRequest {
    let mut request = capture_request(workspace, serial, range(0, 1));
    request.selection = SliceCaptureSelection::Child { node: node(name) };
    request
}

fn empty_setup(path: &Path) -> (Harness, Arc<Workspace>) {
    let mut store = seed_holds(path, &["a", "b"]);
    for (index, name) in [(1, "empty-left"), (2, "empty"), (3, "empty-right")] {
        seed_command(
            &mut store,
            Command::Insert {
                parent: node("root"),
                index,
                subtree: Subtree {
                    root: node(name),
                    nodes: BTreeMap::from([(node(name), BeatNode::sequence(name, vec![]))]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
            &format!("seed-{name}"),
        );
    }
    drop(store);
    let harness = Harness::new();
    let workspace = command(&harness.service, ProjectRequest::Open(path.into()))
        .workspace
        .unwrap();
    (harness, workspace)
}

#[test]
fn cut_range_publishes_one_saved_copy_and_retains_it_through_queries_and_undo() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before) = setup(&scratch.path().join("cut.deadpan"));
    let request = capture_request(&before, 1, range(3, 16));
    let initial_counts = counts(&before.path);
    let saved = command(
        &harness.service,
        ProjectRequest::CutEditSlice(request.clone()),
    );
    assert!(saved.error.is_none());
    let cut = saved.cut_slice.unwrap();
    assert_eq!(cut.request, request);
    let receipt = cut.result.unwrap();
    assert_eq!(saved.committed.as_ref(), Some(&receipt.committed));
    assert_eq!(receipt.copied.slice().range(), range(3, 16));
    assert_eq!(
        receipt.copied.slice().revision_id(),
        before.document.revision_id()
    );
    assert_eq!(receipt.committed.cursor, Some(ProjectFrame(3)));
    let after = saved.workspace.unwrap();
    assert!(receipt.needs_refresh(Some(&before)));
    assert!(!receipt.needs_refresh(Some(&after)));
    assert_eq!(after.plan.duration().frames(), 17);
    assert_eq!(
        counts(&before.path),
        (initial_counts.0 + 1, initial_counts.1 + 1)
    );

    let queried = command(
        &harness.service,
        ProjectRequest::CaptureEditSlice(capture_request(&before, 2, range(0, 10))),
    );
    assert_eq!(
        queried.cut_slice.unwrap().result.unwrap().committed,
        receipt.committed
    );
    let duplicate = command(
        &harness.service,
        ProjectRequest::CutEditSlice(request.clone()),
    );
    assert_eq!(
        duplicate.cut_slice.unwrap().result.unwrap().committed,
        receipt.committed
    );
    assert_eq!(
        counts(&before.path),
        (initial_counts.0 + 1, initial_counts.1 + 1)
    );

    let mut collision = request.clone();
    collision.selection = SliceCaptureSelection::Child { node: node("a") };
    assert!(
        command(&harness.service, ProjectRequest::CutEditSlice(collision))
            .cut_slice
            .unwrap()
            .result
            .is_err()
    );
    assert_eq!(
        command(&harness.service, ProjectRequest::CutEditSlice(request))
            .cut_slice
            .unwrap()
            .result
            .unwrap()
            .committed,
        receipt.committed
    );
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: after.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    restored(&undone.document, &before.document);
    assert!(
        !receipt.needs_refresh(Some(&undone)),
        "Undo has a fresh revision, not a stale pre-cut view"
    );
    let pasted = command(
        &harness.service,
        paste(&undone, receipt.copied, Destination::Slot(3)),
    );
    assert!(pasted.error.is_none(), "{:?}", pasted.error);
    assert_eq!(pasted.workspace.unwrap().plan.duration().frames(), 43);
}

#[test]
fn cut_empty_child_preserves_equal_time_siblings_and_preview_selects_exact_pasted_slot() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before) = empty_setup(&scratch.path().join("empty.deadpan"));
    let initial_counts = counts(&before.path);
    let saved = command(
        &harness.service,
        ProjectRequest::CutEditSlice(child(&before, 1, "empty")),
    );
    let receipt = saved.cut_slice.unwrap().result.unwrap();
    assert_eq!(receipt.copied.child_label(), Some("empty"));
    assert_eq!(receipt.copied.source_path(), &["Your edit"]);
    assert_eq!(receipt.copied.slice().duration(), FrameDuration::ZERO);
    let after = saved.workspace.unwrap();
    assert_eq!(after.plan.duration(), before.plan.duration());
    assert!(!after.document.nodes().contains_key(&node("empty")));
    assert!(after.document.nodes().contains_key(&node("empty-left")));
    assert!(after.document.nodes().contains_key(&node("empty-right")));
    assert_eq!(receipt.committed.selected_node, Some(node("empty-right")));
    assert_eq!(
        counts(&before.path),
        (initial_counts.0 + 1, initial_counts.1 + 1)
    );
    for slot in [1, 2, 3] {
        let proposed = proposal(
            &after,
            receipt.copied.clone(),
            slot as u64,
            1,
            Destination::Slot(slot),
        );
        let ready = command(
            &harness.service,
            ProjectRequest::PrepareSplice(proposed.clone()),
        );
        let prepared = ready.splice.unwrap().result.unwrap();
        assert_eq!(prepared.empty_slot, Some(slot));
        assert_eq!(prepared.range, range(10, 10));
        prepared.validate_result().unwrap();
        let NodeKind::Sequence { children } =
            &prepared.snapshot.document.nodes()[&node("root")].kind
        else {
            panic!()
        };
        assert_eq!(children.get(slot), Some(&prepared.node));
        assert_eq!(
            counts(&before.path),
            (initial_counts.0 + 1, initial_counts.1 + 1)
        );
        if slot == 2 {
            let placed = command(&harness.service, ProjectRequest::CommitSplice(proposed.id));
            let commit = placed.committed.unwrap();
            assert_eq!(commit.selected_node, Some(prepared.node.clone()));
            assert_eq!(commit.cursor, Some(ProjectFrame(10)));
            assert!(commit.range_selection.is_none());
            let placed = placed.workspace.unwrap();
            assert_eq!(placed.plan.duration(), before.plan.duration());
            let undo = command(
                &harness.service,
                ProjectRequest::Undo {
                    expected_revision: placed.document.revision_id().clone(),
                },
            )
            .workspace
            .unwrap();
            restored(&undo.document, &after.document);
            // Undo changes revision, so later previews use an explicit current base.
            break;
        }
    }
}

#[test]
fn cut_failures_keep_history_and_saved_refresh_failure_keeps_copy_and_receipt() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before) = setup(&scratch.path().join("failure.deadpan"));
    let count = counts(&before.path);
    for request in [
        capture_request(&before, 1, range(5, 5)),
        child(&before, 2, "missing"),
    ] {
        let failed = command(&harness.service, ProjectRequest::CutEditSlice(request));
        assert!(failed.cut_slice.unwrap().result.is_err());
        assert_eq!(*failed.workspace.unwrap().document, *before.document);
        assert_eq!(counts(&before.path), count);
    }
    harness
        .service
        .shared
        .render_commit_refresh_failure
        .store(true, Ordering::Release);
    let request = child(&before, 3, "b");
    let saved = command(
        &harness.service,
        ProjectRequest::CutEditSlice(request.clone()),
    );
    let receipt = saved.cut_slice.unwrap().result.unwrap();
    assert!(receipt.refresh_error.as_ref().unwrap().contains("Reopen"));
    assert!(Arc::ptr_eq(saved.workspace.as_ref().unwrap(), &before));
    assert_eq!(counts(&before.path), (count.0 + 1, count.1 + 1));
    let queried = command(
        &harness.service,
        ProjectRequest::CaptureEditSlice(capture_request(&before, 4, range(0, 10))),
    );
    assert!(queried.captured_slice.unwrap().result.is_ok());
    let rejected_undo = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: before.document.revision_id().clone(),
        },
    );
    assert!(rejected_undo.error.is_some());
    assert!(
        rejected_undo.committed.is_none(),
        "ordinary command cleared generic feedback"
    );
    let retained = rejected_undo.saved_cut.unwrap();
    assert_eq!(retained.committed, receipt.committed);
    assert!(retained.needs_refresh(rejected_undo.workspace.as_deref()));
    assert_eq!(retained.refresh_error, receipt.refresh_error);
    let replay = command(&harness.service, ProjectRequest::CutEditSlice(request));
    assert_eq!(
        replay.cut_slice.unwrap().result.unwrap().committed,
        receipt.committed
    );
    assert_eq!(counts(&before.path), (count.0 + 1, count.1 + 1));
    let stale = command(
        &harness.service,
        ProjectRequest::CutEditSlice(child(&before, 5, "a")),
    );
    assert!(stale.cut_slice.unwrap().result.is_err());
    assert_eq!(stale.saved_cut.unwrap().committed, receipt.committed);
    assert_eq!(counts(&before.path), (count.0 + 1, count.1 + 1));
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(before.path.clone()))
        .workspace
        .unwrap();
    assert_eq!(reopened.document.revision_id(), &receipt.committed.revision);
    assert_eq!(reopened.plan.duration().frames(), 20);
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: reopened.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    restored(&undone.document, &before.document);
}

#[test]
fn positive_child_move_keeps_adjacent_empty_siblings_and_the_exact_child_identity() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before) = empty_setup(&scratch.path().join("whole-child-move.deadpan"));
    let copied = capture(&harness.service, child(&before, 1, "b"));
    let mut proposed = proposal(&before, copied, 1, 1, Destination::Slot(0));
    proposed.operation = Operation::Move;
    let initial_counts = counts(&before.path);
    let ready = command(
        &harness.service,
        ProjectRequest::PrepareSplice(proposed.clone()),
    );
    let prepared = ready.splice.unwrap().result.unwrap();
    assert_eq!(prepared.node, node("b"));
    assert_eq!(prepared.range, range(0, 10));
    assert_eq!(counts(&before.path), initial_counts);
    let expected = vec![
        node("b"),
        node("a"),
        node("empty-left"),
        node("empty"),
        node("empty-right"),
    ];
    let NodeKind::Sequence { children } = &prepared.snapshot.document.nodes()[&node("root")].kind
    else {
        panic!()
    };
    assert_eq!(children, &expected);
    let saved = command(&harness.service, ProjectRequest::CommitSplice(proposed.id));
    assert_eq!(
        saved.committed.as_ref().unwrap().selected_node,
        Some(node("b"))
    );
    let after = saved.workspace.unwrap();
    let NodeKind::Sequence { children } = &after.document.nodes()[&node("root")].kind else {
        panic!()
    };
    assert_eq!(children, &expected);
    assert_eq!(after.plan.duration(), before.plan.duration());
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: after.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    restored(&undone.document, &before.document);
}
