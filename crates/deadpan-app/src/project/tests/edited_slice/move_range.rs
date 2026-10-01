//! Native service authority and durable selection, without decoder/UI work.
use super::*;
use crate::project::splice::{Movement, Prepared};

fn moving(
    workspace: &Workspace,
    copied: Arc<Captured>,
    draft: u64,
    destination: Destination,
) -> Proposal {
    let mut request = proposal(workspace, copied, draft, 1, destination);
    request.operation = Operation::Move;
    request
}

fn ready(harness: &Harness, request: &Proposal) -> Arc<Prepared> {
    let update = command(
        &harness.service,
        ProjectRequest::PrepareSplice(request.clone()),
    );
    let reply = update.splice.unwrap();
    assert_eq!(reply.id, request.id);
    assert!(reply.source_view.unwrap().result.is_ok());
    let prepared = reply.result.unwrap();
    prepared.validate_result().unwrap();
    prepared
}

#[test]
fn move_forest_preview_is_history_neutral_and_commits_one_retained_command_and_range() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before) = setup(&scratch.path().join("forest.deadpan"));
    let copied = capture(&harness.service, capture_request(&before, 1, range(0, 20)));
    let request = moving(&before, copied, 1, Destination::Slot(3));
    let cells = counts(&before.path);
    let prepared = ready(&harness, &request);
    assert_eq!(counts(&before.path), cells);
    assert_eq!(prepared.node, node("a"));
    assert_eq!(prepared.parent, node("root"));
    assert_eq!(prepared.range, range(10, 30));
    assert_eq!(
        prepared
            .plan
            .node_duration(&prepared.node)
            .unwrap()
            .frames(),
        10
    );
    assert!(prepared.removed.is_none());
    assert_eq!(
        prepared.movement,
        Some(Movement {
            source_parent: node("root"),
            source_before: range(0, 20),
            destination_before: ProjectFrame(30),
            removal_after: ProjectFrame(0),
        })
    );
    assert_eq!(
        prepared.snapshot.document.nodes().len(),
        before.document.nodes().len()
    );
    assert_eq!(
        prepared
            .snapshot
            .document
            .children(&node("root"))
            .cloned()
            .collect::<Vec<_>>(),
        [node("c"), node("a"), node("b")]
    );
    let mut forged = prepared.as_ref().clone();
    forged.node = node("b");
    assert!(forged.validate_result().is_err());
    forged = prepared.as_ref().clone();
    forged.range = range(11, 30);
    assert!(forged.validate_result().is_err());
    let saved = command(
        &harness.service,
        ProjectRequest::CommitSplice(request.id.clone()),
    );
    let receipt = saved.committed.unwrap();
    assert_eq!(receipt.selected_node, Some(node("a")));
    assert_eq!(receipt.cursor, Some(ProjectFrame(10)));
    assert_eq!(
        receipt.range_selection,
        Some(CommittedRangeSelection {
            session: before.session,
            project: before.document.project_id().clone(),
            parent: node("root"),
            range: range(10, 30),
        })
    );
    let after = saved.workspace.unwrap();
    assert_eq!(*after.document, *prepared.snapshot.document);
    assert_eq!(counts(&before.path), (cells.0 + 1, cells.1 + 1));
    let database = rusqlite::Connection::open(before.path.join("project.sqlite")).unwrap();
    let wire: String = database
        .query_row(
            "SELECT request FROM history ORDER BY rowid DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let retained: deadpan_core::CommandRequest = serde_json::from_str(&wire).unwrap();
    assert!(matches!(retained.command, Command::MoveRange { .. }));
    assert_eq!(retained.new_revision, receipt.revision);
    let duplicate = command(&harness.service, ProjectRequest::CommitSplice(request.id));
    assert_eq!(duplicate.splice_commit.unwrap().result.unwrap(), receipt);
    assert_eq!(counts(&before.path), (cells.0 + 1, cells.1 + 1));
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

#[test]
fn rejected_moves_keep_endpoints_and_cannot_commit_or_replace_a_previous_ready_draft() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before) = setup(&scratch.path().join("reject-move.deadpan"));
    let copied = capture(&harness.service, capture_request(&before, 1, range(0, 20)));
    let request = moving(&before, copied.clone(), 1, Destination::Slot(3));
    ready(&harness, &request);
    let cells = counts(&before.path);
    for (index, destination, reason) in [
        (0, Destination::Slot(0), "Already at this position"),
        (1, Destination::Slot(2), "Already at this position"),
        (
            2,
            Destination::Interior {
                target: node("b"),
                at: FrameDuration::new(5).unwrap(),
            },
            "interior",
        ),
        (
            3,
            Destination::Replace {
                range: range(20, 30),
            },
            "cannot replace",
        ),
    ] {
        let mut rejected = request.clone();
        rejected.id.change = index + 2;
        rejected.destination = destination;
        let reply = command(
            &harness.service,
            ProjectRequest::PrepareSplice(rejected.clone()),
        )
        .splice
        .unwrap();
        assert!(reply.source_view.unwrap().result.is_ok());
        assert!(reply.result.err().unwrap().contains(reason));
        assert!(
            command(&harness.service, ProjectRequest::CommitSplice(rejected.id))
                .splice_commit
                .unwrap()
                .result
                .is_err()
        );
        assert_eq!(counts(&before.path), cells);
    }
    assert!(
        command(&harness.service, ProjectRequest::CommitSplice(request.id))
            .splice_commit
            .unwrap()
            .result
            .is_err()
    );
    let mut original = moving(&before, copied, 2, Destination::Slot(3));
    original.source = Source::Original {
        asset: AssetId::new("original").unwrap(),
        qualification: SourceQualificationId::new("a".repeat(64)).unwrap(),
        ordinals: 0..5,
    };
    let rejected = command(&harness.service, ProjectRequest::PrepareSplice(original))
        .splice
        .unwrap();
    assert!(
        rejected
            .result
            .err()
            .unwrap()
            .contains("Original always copies")
    );
    assert_eq!(counts(&before.path), cells);
}

#[test]
fn old_copy_still_pastes_but_move_requires_current_source_even_after_undo() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before) = setup(&scratch.path().join("stale-move.deadpan"));
    let copied = capture(&harness.service, capture_request(&before, 1, range(0, 20)));
    let changed = edited(
        &harness.service,
        &before,
        ProjectEdit::HoldDuration {
            node: node("c"),
            duration: FrameDuration::new(12).unwrap(),
        },
    )
    .workspace
    .unwrap();
    let current = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: changed.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    let cells = counts(&before.path);
    let old = moving(&current, copied.clone(), 1, Destination::Slot(3));
    let rejected = command(&harness.service, ProjectRequest::PrepareSplice(old.clone()))
        .splice
        .unwrap();
    assert!(rejected.source_view.unwrap().result.is_ok());
    assert!(rejected.result.err().unwrap().contains("older revision"));
    let mut copy = old;
    copy.id.change += 1;
    copy.operation = Operation::Copy;
    assert!(ready(&harness, &copy).movement.is_none());
    let fresh = capture(&harness.service, capture_request(&current, 2, range(0, 20)));
    ready(&harness, &moving(&current, fresh, 2, Destination::Slot(3)));
    assert_eq!(counts(&before.path), cells);
}

#[test]
fn move_refinement_uses_current_source_range_and_saved_refresh_failure_retains_its_exact_selection()
{
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before) = setup(&scratch.path().join("move-refresh.deadpan"));
    let copied = capture(&harness.service, capture_request(&before, 1, range(0, 10)));
    let mut request = moving(
        &before,
        copied.clone(),
        1,
        Destination::Interior {
            target: node("a"),
            at: FrameDuration::new(7).unwrap(),
        },
    );
    request.source = Source::Edited {
        copied: copied.clone(),
        range: range(2, 4),
    };
    let prepared = ready(&harness, &request);
    assert_eq!(prepared.range, range(5, 7));
    assert_eq!(
        prepared.movement.as_ref().unwrap().source_before,
        range(2, 4)
    );
    assert_eq!(copied.slice().range(), range(0, 10));
    assert_eq!(
        prepared.snapshot.document.nodes().len(),
        before.document.nodes().len() + 7
    );
    let cells = counts(&before.path);
    harness
        .service
        .shared
        .splice_commit_refresh_failure
        .store(true, Ordering::Release);
    let saved = command(
        &harness.service,
        ProjectRequest::CommitSplice(request.id.clone()),
    );
    let receipt = saved.committed.clone().unwrap();
    assert_eq!(receipt.range_selection.as_ref().unwrap().range, range(5, 7));
    assert_eq!(receipt.selected_node, Some(prepared.node.clone()));
    assert!(Arc::ptr_eq(saved.workspace.as_ref().unwrap(), &before));
    assert!(saved.message.as_ref().unwrap().contains("saved, but"));
    assert!(saved.message.as_ref().unwrap().contains("Reopen"));
    let repeated = command(&harness.service, ProjectRequest::CommitSplice(request.id));
    assert_eq!(repeated.committed, Some(receipt.clone()));
    assert_eq!(repeated.message, saved.message);
    assert_eq!(counts(&before.path), (cells.0 + 1, cells.1 + 1));
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(before.path.clone()))
        .workspace
        .unwrap();
    assert_eq!(*reopened.document, *prepared.snapshot.document);
    assert_eq!(reopened.document.revision_id(), &receipt.revision);
}

#[test]
fn boundary_reparenting_keeps_time_but_returns_the_destination_scope_and_whole_forest() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("ownership.deadpan");
    let mut store = seed_holds(&path, &["a", "b", "c"]);
    seed_command(
        &mut store,
        Command::Group {
            parent: node("root"),
            start: 0,
            end: 2,
            id: node("group"),
            label: "Group".into(),
        },
        "group",
    );
    drop(store);
    let harness = Harness::new();
    let before = command(&harness.service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    let copied = capture(&harness.service, capture_request(&before, 1, range(20, 30)));
    let mut request = moving(&before, copied, 1, Destination::Slot(2));
    request.parent = node("group");
    request.scope = SequenceScope::default()
        .descend(&before, &node("group"))
        .unwrap();
    let prepared = ready(&harness, &request);
    assert_eq!(prepared.range, range(20, 30));
    assert_eq!(
        prepared.movement.as_ref().unwrap().destination_before,
        ProjectFrame(20)
    );
    assert_eq!(
        prepared
            .snapshot
            .document
            .children(&node("group"))
            .cloned()
            .collect::<Vec<_>>(),
        [node("a"), node("b"), node("c")]
    );
    let saved = command(&harness.service, ProjectRequest::CommitSplice(request.id));
    let receipt = saved.committed.unwrap();
    assert_eq!(receipt.scope, request.scope);
    assert_eq!(receipt.range_selection.unwrap().parent, node("group"));
    assert_eq!(
        saved.workspace.unwrap().plan.duration(),
        before.plan.duration()
    );
}
