use super::*;

pub(super) fn prepare(harness: &Harness, request: &Proposal) -> Arc<Prepared> {
    let update = command(
        &harness.service,
        ProjectRequest::PrepareSplice(request.clone()),
    );
    let update = if update.splice.is_none() {
        harness.finish(harness.job());
        wait(&harness.service, |update| {
            update
                .splice
                .as_ref()
                .is_some_and(|reply| reply.id == request.id)
        })
    } else {
        update
    };
    prepared(&update, &request.id)
}

pub(super) fn nested(harness: &Harness) -> Arc<Workspace> {
    let initial = initialize(harness);
    command(&harness.service, ProjectRequest::Close);
    {
        let mut store = ProjectStore::open(&initial.path, AccessMode::ReadWrite).unwrap();
        seed_command(
            &mut store,
            Command::Group {
                parent: initial.document.root().clone(),
                start: 0,
                end: 1,
                id: node("slice-group"),
                label: "Interior destination".into(),
            },
            "group-before-interior",
        );
        for (parent, index, name, frames) in [
            (initial.document.root().clone(), 0, "lead-in", 5),
            (node("slice-group"), 1, "group-hold", 8),
        ] {
            seed_command(
                &mut store,
                Command::Insert {
                    parent,
                    index,
                    subtree: Subtree {
                        root: node(name),
                        nodes: BTreeMap::from([(node(name), BeatNode::hold(name, hold(frames)))]),
                        overrides: BTreeMap::new(),
                        gap_overrides: BTreeMap::new(),
                    },
                },
                name,
            );
        }
    }
    command(&harness.service, ProjectRequest::Open(initial.path.clone()))
        .workspace
        .unwrap()
}

fn request(workspace: &Workspace, draft: u64, change: u64, target: NodeId, at: i64) -> Proposal {
    let mut request = proposal(workspace, draft, change);
    request.scope = SequenceScope::default()
        .descend(workspace, &node("slice-group"))
        .unwrap();
    request.parent = node("slice-group");
    request.destination = Destination::Interior {
        target,
        at: FrameDuration::new(at).unwrap(),
    };
    request
}

#[test]
fn nested_source_and_hold_interiors_preview_unsaved_and_commit_one_exact_undo() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let mut before = nested(&harness);
    let original = before
        .document
        .children(&node("slice-group"))
        .next()
        .unwrap()
        .clone();
    let original_duration = before.document.node_duration(&original).unwrap().frames();
    for (draft, target, offset, expected_cursor) in [
        (1, original, 30, 35),
        (2, node("group-hold"), 4, 5 + original_duration + 4),
    ] {
        let first = request(&before, draft, 1, target.clone(), offset);
        let old = prepare(&harness, &first);
        let latest = request(&before, draft, 2, target, offset + 1);
        let exact = prepare(&harness, &latest);
        assert_ne!(old.node, exact.node);
        assert_eq!(exact.range.start(), ProjectFrame(expected_cursor + 1));
        assert_eq!(exact.range.end(), ProjectFrame(expected_cursor + 15));
        assert_eq!(
            exact
                .snapshot
                .document
                .children(before.document.root())
                .count(),
            2
        );
        assert_eq!(
            exact
                .snapshot
                .document
                .children(&node("slice-group"))
                .count(),
            4
        );
        unchanged(&before);
        assert!(
            ProjectStore::open(&before.path, AccessMode::ReadOnly)
                .unwrap()
                .snapshot_at(exact.snapshot.document.revision_id())
                .is_err()
        );
        let refused = command(&harness.service, ProjectRequest::CommitSplice(first.id));
        assert!(refused.splice_commit.unwrap().result.is_err());
        unchanged(&before);

        let committed = command(
            &harness.service,
            ProjectRequest::CommitSplice(latest.id.clone()),
        );
        let receipt = committed.splice_commit.unwrap().result.unwrap();
        assert_eq!(receipt.scope, latest.scope);
        assert_eq!(receipt.selected_node.as_ref(), Some(&exact.node));
        assert_eq!(receipt.cursor, Some(exact.range.start()));
        let after = committed.workspace.unwrap();
        assert_eq!(*after.document, *exact.snapshot.document);
        let repeated = command(&harness.service, ProjectRequest::CommitSplice(latest.id));
        assert_eq!(repeated.splice_commit.unwrap().result.unwrap(), receipt);
        assert_eq!(*repeated.workspace.unwrap().document, *after.document);
        let undone = command(
            &harness.service,
            ProjectRequest::Undo {
                expected_revision: after.document.revision_id().clone(),
            },
        )
        .workspace
        .unwrap();
        assert_eq!(undone.document.nodes(), before.document.nodes());
        assert_eq!(
            undone.document.audio_bindings(),
            before.document.audio_bindings()
        );
        assert_eq!(undone.plan.duration(), before.plan.duration());
        assert_eq!(undone.can_undo, before.can_undo);
        before = undone;
    }
}

#[test]
fn interior_invalid_targets_and_changed_revision_cannot_retarget_or_publish_split() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let before = nested(&harness);
    let original = before
        .document
        .children(&node("slice-group"))
        .next()
        .unwrap()
        .clone();
    let duration = before.document.node_duration(&original).unwrap().frames();
    for (index, (target, offset)) in [
        (original.clone(), 0),
        (original.clone(), duration),
        (node("missing"), 1),
        (node("lead-in"), 1),
        (node("slice-group"), 1),
    ]
    .into_iter()
    .enumerate()
    {
        let request = request(&before, index as u64 + 1, 1, target, offset);
        let update = command(
            &harness.service,
            ProjectRequest::PrepareSplice(request.clone()),
        );
        assert!(update.splice.unwrap().result.is_err());
        let commit = command(&harness.service, ProjectRequest::CommitSplice(request.id));
        assert!(commit.splice_commit.unwrap().result.is_err());
        unchanged(&before);
    }
    let mut group = proposal(&before, 6, 1);
    group.destination = Destination::Interior {
        target: node("slice-group"),
        at: FrameDuration::new(1).unwrap(),
    };
    assert!(
        command(&harness.service, ProjectRequest::PrepareSplice(group))
            .splice
            .unwrap()
            .result
            .is_err()
    );
    unchanged(&before);

    let captured = request(&before, 7, 1, original, 30);
    prepare(&harness, &captured);
    let after = command(
        &harness.service,
        edit_request_in(
            &before,
            captured.scope.clone(),
            ProjectFrame(5 + duration),
            ProjectEdit::HoldDuration {
                node: node("group-hold"),
                duration: FrameDuration::new(9).unwrap(),
            },
        ),
    )
    .workspace
    .unwrap();
    assert_ne!(after.document.revision_id(), before.document.revision_id());
    let refused = command(&harness.service, ProjectRequest::CommitSplice(captured.id));
    assert!(refused.splice_commit.unwrap().result.is_err());
    assert_eq!(*refused.workspace.unwrap().document, *after.document);
    unchanged(&after);
}

#[test]
fn failed_interior_commit_consumes_draft_without_leaking_split_or_insertion() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let before = initialize(&harness);
    let mut request = proposal(&before, 1, 1);
    request.destination = Destination::Interior {
        target: before
            .document
            .children(before.document.root())
            .next()
            .unwrap()
            .clone(),
        at: FrameDuration::new(30).unwrap(),
    };
    let exact = prepare(&harness, &request);
    let database = rusqlite::Connection::open(before.path.join("project.sqlite")).unwrap();
    database.execute_batch(
        "CREATE TRIGGER fail_slice_interior_cursor BEFORE UPDATE OF head_revision,cursor ON state
         BEGIN SELECT RAISE(ABORT, 'injected interior commit failure'); END;",
    ).unwrap();
    let failed = command(
        &harness.service,
        ProjectRequest::CommitSplice(request.id.clone()),
    );
    assert!(
        failed
            .splice_commit
            .unwrap()
            .result
            .unwrap_err()
            .contains("injected interior commit failure")
    );
    unchanged(&before);
    assert!(
        ProjectStore::open(&before.path, AccessMode::ReadOnly)
            .unwrap()
            .snapshot_at(exact.snapshot.document.revision_id())
            .is_err()
    );
    database
        .execute_batch("DROP TRIGGER fail_slice_interior_cursor")
        .unwrap();
    let retry = command(
        &harness.service,
        ProjectRequest::CommitSplice(request.id.clone()),
    );
    assert!(
        retry.splice_commit.unwrap().result.is_err(),
        "failure consumed the exact draft"
    );
    unchanged(&before);
    request.id.change += 1;
    let replacement = prepare(&harness, &request);
    let committed = command(&harness.service, ProjectRequest::CommitSplice(request.id));
    assert!(committed.splice_commit.unwrap().result.is_ok());
    assert_eq!(
        *committed.workspace.unwrap().document,
        *replacement.snapshot.document
    );
}
