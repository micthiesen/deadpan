use super::interior::{nested, prepare};
use super::*;
use deadpan_core::FrameRange;

fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}

fn assert_restored(actual: &ProjectDocument, expected: &ProjectDocument) {
    let mut expected = serde_json::to_value(expected).unwrap();
    expected["revision_id"] = serde_json::to_value(actual.revision_id()).unwrap();
    assert_eq!(serde_json::to_value(actual).unwrap(), expected);
}

#[test]
fn replacement_proposals_share_exact_commits_for_short_equal_long_and_full_ranges() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let mut before = initialize(&harness);
    for (serial, removed) in [range(30, 50), range(30, 44), range(30, 35), range(0, 120)]
        .into_iter()
        .enumerate()
    {
        let mut request = proposal(&before, serial as u64 + 1, 1);
        request.destination = Destination::Replace { range: removed };
        let exact = prepare(&harness, &request);
        assert_eq!(exact.removed, Some(removed));
        assert_eq!(
            exact.range,
            range(removed.start().0, removed.start().0 + 14)
        );
        assert_eq!(
            exact.plan.duration().frames(),
            120 - (removed.end().0 - removed.start().0) + 14
        );
        unchanged(&before);
        let saved = command(
            &harness.service,
            ProjectRequest::CommitSplice(request.id.clone()),
        );
        let committed = saved.workspace.unwrap();
        assert_eq!(*committed.document, *exact.snapshot.document);
        let receipt = saved.splice_commit.unwrap().result.unwrap();
        assert_eq!(receipt.cursor, Some(removed.start()));
        assert_eq!(receipt.selected_node.as_ref(), Some(&exact.node));
        let repeated = command(&harness.service, ProjectRequest::CommitSplice(request.id));
        assert_eq!(repeated.splice_commit.unwrap().result.unwrap(), receipt);
        assert_eq!(*repeated.workspace.unwrap().document, *committed.document);
        let restored = command(
            &harness.service,
            ProjectRequest::Undo {
                expected_revision: committed.document.revision_id().clone(),
            },
        )
        .workspace
        .unwrap();
        assert_restored(&restored.document, &before.document);
        before = restored;
    }
}

#[test]
fn nested_replacement_refines_only_source_and_fast_paste_uses_the_same_range() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let before = nested(&harness);
    let scope = SequenceScope::default()
        .descend(&before, &node("slice-group"))
        .unwrap();
    let mut request = proposal(&before, 1, 1);
    request.parent = node("slice-group");
    request.scope = scope.clone();
    request.destination = Destination::Replace {
        range: range(35, 60),
    };
    let first = prepare(&harness, &request);
    request.id.change += 1;
    set_ordinals(&mut request, 12..31);
    let exact = prepare(&harness, &request);
    assert_eq!(exact.removed, first.removed);
    assert_eq!(exact.range, range(35, 54));
    unchanged(&before);
    command(
        &harness.service,
        ProjectRequest::AbandonSplice(request.id.clone()),
    );
    let update = command(
        &harness.service,
        ProjectRequest::PasteMoment(MomentPaste {
            expected_session: before.session,
            expected_revision: before.document.revision_id().clone(),
            asset: original_source(&request).0.clone(),
            qualification: original_source(&request).1.clone(),
            ordinals: original_source(&request).2.clone(),
            scope: scope.clone(),
            parent: request.parent,
            destination: request.destination,
        }),
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    let commit = update.committed.unwrap();
    let after = update.workspace.unwrap();
    assert_eq!(commit.cursor, Some(ProjectFrame(35)));
    assert_eq!(commit.scope, scope);
    assert_eq!(after.plan.duration(), exact.plan.duration());
    assert_eq!(after.document.children(before.document.root()).count(), 2);
    assert_eq!(after.document.children(&node("slice-group")).count(), 4);
    let restored = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: after.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_restored(&restored.document, &before.document);
}

#[test]
fn replacement_rejects_wrong_scope_stale_revision_and_failed_cursor_write() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let before = nested(&harness);
    let scope = SequenceScope::default()
        .descend(&before, &node("slice-group"))
        .unwrap();
    let mut request = proposal(&before, 1, 1);
    request.parent = node("slice-group");
    request.scope = scope;
    request.destination = Destination::Replace {
        range: range(0, 20),
    };
    let rejected = command(
        &harness.service,
        ProjectRequest::PrepareSplice(request.clone()),
    );
    assert!(rejected.splice.unwrap().result.is_err());
    unchanged(&before);
    request.id.change += 1;
    request.destination = Destination::Replace {
        range: range(35, 60),
    };
    let exact = prepare(&harness, &request);
    let database = rusqlite::Connection::open(before.path.join("project.sqlite")).unwrap();
    database
        .execute_batch(
            "CREATE TRIGGER fail_replacement_cursor BEFORE UPDATE OF head_revision,cursor ON state
         BEGIN SELECT RAISE(ABORT, 'injected replacement failure'); END;",
        )
        .unwrap();
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
            .contains("injected replacement failure")
    );
    unchanged(&before);
    assert!(
        ProjectStore::open(&before.path, AccessMode::ReadOnly)
            .unwrap()
            .snapshot_at(exact.snapshot.document.revision_id())
            .is_err()
    );
    database
        .execute_batch("DROP TRIGGER fail_replacement_cursor")
        .unwrap();
    assert!(
        command(
            &harness.service,
            ProjectRequest::CommitSplice(request.id.clone())
        )
        .splice_commit
        .unwrap()
        .result
        .is_err()
    );
    request.id.change += 1;
    prepare(&harness, &request);
    let edited = command(
        &harness.service,
        edit_request_in(
            &before,
            request.scope.clone(),
            ProjectFrame(125),
            ProjectEdit::HoldDuration {
                node: node("group-hold"),
                duration: FrameDuration::new(9).unwrap(),
            },
        ),
    )
    .workspace
    .unwrap();
    let refused = command(&harness.service, ProjectRequest::CommitSplice(request.id));
    assert!(refused.splice_commit.unwrap().result.is_err());
    assert_eq!(*refused.workspace.unwrap().document, *edited.document);
}

fn fast_replacement(before: &Workspace) -> ProjectRequest {
    let request = proposal(before, 1, 1);
    ProjectRequest::PasteMoment(MomentPaste {
        expected_session: before.session,
        expected_revision: before.document.revision_id().clone(),
        asset: original_source(&request).0.clone(),
        qualification: original_source(&request).1.clone(),
        ordinals: original_source(&request).2.clone(),
        scope: request.scope,
        parent: request.parent,
        destination: Destination::Replace {
            range: range(30, 50),
        },
    })
}

fn history_counts(path: &Path) -> (i64, i64) {
    let database = rusqlite::Connection::open(path.join("project.sqlite")).unwrap();
    database
        .query_row(
            "SELECT (SELECT COUNT(*) FROM revisions), (SELECT COUNT(*) FROM history)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

fn fast_replacement_refresh_failure(cold: bool) {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let initial = initialize(&harness);
    let before = if cold {
        command(&harness.service, ProjectRequest::Close);
        command(&harness.service, ProjectRequest::Open(initial.path.clone()))
            .workspace
            .unwrap()
    } else {
        initial
    };
    let counts = history_counts(&before.path);
    harness
        .service
        .shared
        .splice_commit_refresh_failure
        .store(true, Ordering::Release);
    let requested = command(&harness.service, fast_replacement(&before));
    let applied = if cold {
        assert!(requested.error.is_none(), "{:?}", requested.error);
        assert!(requested.committed.is_none());
        assert_eq!(
            requested.import.unwrap().stage,
            ImportStage::PreparingInsertion
        );
        harness.finish(harness.job());
        wait(&harness.service, |update| {
            update.committed.is_some()
                || update
                    .import
                    .as_ref()
                    .is_some_and(|status| status.stage == ImportStage::Failed)
        })
    } else {
        requested
    };
    assert!(applied.error.is_none(), "{:?}", applied.error);
    let message = applied.message.as_deref().unwrap();
    assert!(message.contains("saved"), "{message}");
    assert!(message.contains("preview could not refresh"), "{message}");
    assert!(message.contains("Injected failure"), "{message}");
    assert_eq!(*applied.workspace.unwrap().document, *before.document);
    let receipt = applied
        .committed
        .expect("saved replacement retains its receipt");
    assert_eq!(receipt.cursor, Some(ProjectFrame(30)));
    assert_eq!(receipt.scope, SequenceScope::default());
    if cold {
        let status = applied.import.unwrap();
        assert_eq!(status.stage, ImportStage::Complete);
        assert!(status.error.is_none());
        assert!(status.asset.is_some());
    }
    let read = ProjectStore::open(&before.path, AccessMode::ReadOnly).unwrap();
    let saved = read.snapshot().unwrap();
    assert_eq!(saved.revision_id(), &receipt.revision);
    assert_eq!(saved.duration().unwrap().frames(), 114);
    let inserted = receipt.selected_node.as_ref().unwrap();
    let NodeKind::Source { source } = &saved.nodes()[inserted].kind else {
        panic!("replacement receipt must select its saved Source");
    };
    assert_eq!(source.duration.frames(), 14);
    assert_eq!(history_counts(&before.path), (counts.0 + 1, counts.1 + 1));
    drop(read);
    // A user retry against the still-visible old workspace must not repeat the
    // durable edit. The store rejects its captured old revision.
    let stale = command(&harness.service, fast_replacement(&before));
    assert!(stale.error.is_some());
    assert!(stale.committed.is_none());
    assert_eq!(history_counts(&before.path), (counts.0 + 1, counts.1 + 1));
    let restored = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: receipt.revision,
        },
    );
    assert!(restored.error.is_none(), "{:?}", restored.error);
    assert_restored(&restored.workspace.unwrap().document, &before.document);
}

#[test]
fn cached_fast_replacement_retains_saved_receipt_when_preview_refresh_fails() {
    fast_replacement_refresh_failure(false);
}

#[test]
fn cold_fast_replacement_finishes_saved_when_preview_refresh_fails() {
    fast_replacement_refresh_failure(true);
}
