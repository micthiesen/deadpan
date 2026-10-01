use super::*;
use crate::project::slip::{CapturedTarget, Prepared, Proposal, ProposalId, Target};
use deadpan_core::{ExactRatio, SourceSlipClamp};

fn counts(path: &Path) -> (i64, i64) {
    rusqlite::Connection::open(path.join("project.sqlite"))
        .unwrap()
        .query_row(
            "SELECT (SELECT count(*) FROM revisions), (SELECT count(*) FROM history)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

fn fixture_at(documents: &Path) -> (Harness, Arc<Workspace>, Target) {
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(documents.into()).unwrap(),
    ));
    command(
        &harness.service,
        ProjectRequest::CreateFromSource {
            path: fixture("cfr-bframes.mp4"),
        },
    );
    harness.finish(harness.job());
    harness.finish(harness.job());
    let initial = complete(&harness.service);
    let original = initial
        .sources
        .values()
        .find(|source| source.video_index.is_some())
        .unwrap();
    let pasted = command(
        &harness.service,
        ProjectRequest::PasteMoment(MomentPaste {
            expected_session: initial.session,
            expected_revision: initial.document.revision_id().clone(),
            asset: original.asset.clone(),
            qualification: original.receipt.id().clone(),
            ordinals: 10..24,
            scope: SequenceScope::default(),
            parent: initial.document.root().clone(),
            destination: crate::project::splice::Destination::Slot(0),
        }),
    );
    assert!(pasted.error.is_none(), "{:?}", pasted.error);
    let node = pasted.committed.unwrap().selected_node.unwrap();
    let workspace = pasted.workspace.unwrap();
    let target = Target::capture(
        &workspace,
        SequenceScope::default(),
        Some(&node),
        ProjectFrame(20),
    )
    .unwrap();
    assert_eq!(
        target.range,
        FrameRange::new(ProjectFrame(0), ProjectFrame(14)).unwrap()
    );
    (harness, workspace, target)
}

fn proposal(target: &Target, draft: u64, change: u64, delta_frames: i64) -> Proposal {
    Proposal {
        target: target.clone(),
        draft,
        change,
        delta_frames,
    }
}

fn prepared(update: &ProjectUpdate, id: &ProposalId) -> Arc<Prepared> {
    assert!(update.error.is_none(), "{:?}", update.error);
    let reply = update.slip.as_ref().unwrap();
    assert_eq!(&reply.id, id);
    let prepared = reply
        .result
        .as_ref()
        .unwrap_or_else(|error| panic!("{error}"))
        .clone();
    assert_eq!(prepared.base.session, id.session);
    assert_eq!(prepared.base.document.project_id(), &id.project);
    assert_eq!(prepared.base.document.revision_id(), &id.base_revision);
    if let Some(snapshot) = &prepared.snapshot {
        snapshot.validate_original_proposal().unwrap();
        snapshot
            .validate_proposed_base(prepared.base.session, &prepared.base.document)
            .unwrap();
        assert_eq!(
            snapshot.content,
            deadpan_playback::ContentIdentity::Proposed {
                base_revision: id.base_revision.clone(),
                draft: id.draft,
                change: id.change,
            }
        );
    }
    prepared
}

fn prepare(harness: &Harness, proposal: &Proposal) -> Arc<Prepared> {
    prepared(
        &command(
            &harness.service,
            ProjectRequest::PrepareSlip(proposal.clone()),
        ),
        &proposal.id(),
    )
}

fn unchanged(workspace: &Workspace, expected: (i64, i64)) {
    assert_eq!(counts(&workspace.path), expected);
    let store = ProjectStore::open(&workspace.path, AccessMode::ReadOnly).unwrap();
    assert_eq!(store.snapshot().unwrap(), *workspace.document);
}

#[test]
fn native_slip_previews_are_history_neutral_and_commit_one_exact_revision_with_preserved_cursor() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before, target) = fixture_at(&scratch.path().join("Documents"));
    let rows = counts(&before.path);
    let first = proposal(&target, 1, 1, 5);
    let first_prepared = prepare(&harness, &first);
    assert!(Arc::ptr_eq(&first_prepared.base, &before));
    assert_eq!(first_prepared.resolution.applied_delta_frames, 5);
    assert_eq!(
        first_prepared.resolution.after.video_mapping.start_frames(),
        ExactRatio::integer(-15)
    );
    let latest = proposal(&target, 1, 2, -100);
    let exact = prepare(&harness, &latest);
    assert_eq!(exact.resolution.applied_delta_frames, -10);
    assert_eq!(exact.resolution.clamp, Some(SourceSlipClamp::PictureStart));
    assert_ne!(
        first_prepared
            .snapshot
            .as_ref()
            .unwrap()
            .document
            .revision_id(),
        exact.snapshot.as_ref().unwrap().document.revision_id()
    );
    unchanged(&before, rows);
    // An old commit/abandon must not consume the latest ready amount.
    assert!(
        command(&harness.service, ProjectRequest::CommitSlip(first.id()))
            .slip_commit
            .unwrap()
            .result
            .is_err()
    );
    command(&harness.service, ProjectRequest::AbandonSlip(first.id()));
    let update = command(&harness.service, ProjectRequest::CommitSlip(latest.id()));
    let receipt = update.slip_commit.unwrap().result.unwrap();
    assert_eq!(receipt.selected_node, Some(target.node.clone()));
    assert_eq!(receipt.scope, target.scope);
    assert!(receipt.preserve_cursor);
    assert_eq!(receipt.cursor, Some(ProjectFrame(20)));
    assert!(receipt.range_selection.is_none());
    let after = update.workspace.unwrap();
    assert_eq!(*after.document, *exact.snapshot.as_ref().unwrap().document);
    assert_eq!(after.plan.duration(), before.plan.duration());
    assert_eq!(counts(&before.path), (rows.0 + 1, rows.1 + 1));
    assert_eq!(update.saved_slip.unwrap().committed, receipt);
    let duplicate = command(&harness.service, ProjectRequest::CommitSlip(latest.id()));
    assert_eq!(duplicate.slip_commit.unwrap().result.unwrap(), receipt);
    assert_eq!(counts(&before.path), (rows.0 + 1, rows.1 + 1));
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: after.document.revision_id().clone(),
        },
    );
    assert_eq!(undone.saved_slip.as_ref().unwrap().committed, receipt);
    let undone = undone.workspace.unwrap();
    assert_eq!(undone.document.nodes(), before.document.nodes());
    assert_ne!(undone.document.revision_id(), before.document.revision_id());
    let unchanged_rows = counts(&before.path);
    let replay = command(&harness.service, ProjectRequest::CommitSlip(latest.id()));
    assert_eq!(replay.slip_commit.unwrap().result.unwrap(), receipt);
    assert_eq!(
        replay.workspace.unwrap().document.revision_id(),
        undone.document.revision_id()
    );
    assert_eq!(counts(&before.path), unchanged_rows);
    let redone = command(
        &harness.service,
        ProjectRequest::Redo {
            expected_revision: undone.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(redone.document.nodes(), after.document.nodes());
    assert_ne!(redone.document.revision_id(), after.document.revision_id());
}

#[test]
fn zero_abandon_and_failed_refinement_never_reuse_a_previous_ready_slip() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before, target) = fixture_at(&scratch.path().join("Documents"));
    let rows = counts(&before.path);
    let zero = proposal(&target, 1, 1, 0);
    let report = prepare(&harness, &zero);
    assert!(report.snapshot.is_none());
    assert_eq!(report.resolution.after, report.resolution.before);
    assert!(
        command(&harness.service, ProjectRequest::CommitSlip(zero.id()))
            .slip_commit
            .unwrap()
            .result
            .unwrap_err()
            .contains("no change")
    );
    let ready = proposal(&target, 2, 1, 3);
    prepare(&harness, &ready);
    let mut bad = proposal(&target, 2, 2, 4);
    bad.target.range = FrameRange::new(ProjectFrame(1), ProjectFrame(14)).unwrap();
    let rejected = command(&harness.service, ProjectRequest::PrepareSlip(bad));
    assert!(rejected.slip.unwrap().result.is_err());
    assert!(
        command(&harness.service, ProjectRequest::CommitSlip(ready.id()))
            .slip_commit
            .unwrap()
            .result
            .is_err()
    );
    let reused = command(
        &harness.service,
        ProjectRequest::PrepareSlip(proposal(&target, 2, 2, 4)),
    );
    assert!(reused.slip.unwrap().result.is_err());
    let final_proposal = proposal(&target, 3, 1, -2);
    prepare(&harness, &final_proposal);
    command(
        &harness.service,
        ProjectRequest::AbandonSlip(final_proposal.id()),
    );
    assert!(
        command(
            &harness.service,
            ProjectRequest::CommitSlip(final_proposal.id())
        )
        .slip_commit
        .unwrap()
        .result
        .is_err()
    );
    unchanged(&before, rows);
}

#[test]
fn captured_absence_and_wrong_context_fail_without_cursor_based_retargeting() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before, target) = fixture_at(&scratch.path().join("Documents"));
    let captured: CapturedTarget =
        Target::capture(&before, SequenceScope::default(), None, ProjectFrame(0));
    assert!(captured.unwrap_err().contains("no target was captured"));
    assert!(
        Target::capture(
            &before,
            SequenceScope::default(),
            Some(&node("missing")),
            ProjectFrame(0)
        )
        .is_err()
    );
    let rows = counts(&before.path);
    for case in 0..6 {
        let mut bad = proposal(&target, 10 + case, 1, 1);
        match case {
            0 => bad.target.session += 1,
            1 => bad.target.project = ProjectId::new("foreign").unwrap(),
            2 => bad.target.base_revision = RevisionId::new("stale").unwrap(),
            3 => bad.target.parent = target.node.clone(),
            4 => bad.target.node = node("missing"),
            _ => bad.target.cursor = ProjectFrame(-1),
        }
        assert!(
            command(&harness.service, ProjectRequest::PrepareSlip(bad))
                .slip
                .unwrap()
                .result
                .is_err()
        );
    }
    unchanged(&before, rows);
    // The valid selected beat remains the same despite an Edit cursor outside it.
    assert!(target.cursor.0 > target.range.end().0);
    let exact = prepare(&harness, &proposal(&target, 20, 1, 1));
    assert_eq!(exact.target, target);
}

#[test]
fn stored_qualification_is_rechecked_for_zero_previews_and_after_ready_proposals() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before, target) = fixture_at(&scratch.path().join("Documents"));
    let rows = counts(&before.path);
    let ready = proposal(&target, 1, 1, 3);
    prepare(&harness, &ready);
    let database = rusqlite::Connection::open(before.path.join("project.sqlite")).unwrap();
    let receipt: (String, String, String, Vec<u8>) = database.query_row(
        "SELECT id,original_content_id,original_ref,snapshot FROM source_qualifications LIMIT 1", [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ).unwrap();
    database
        .execute("DELETE FROM source_qualifications", [])
        .unwrap();
    let rejected = command(&harness.service, ProjectRequest::CommitSlip(ready.id()));
    assert!(rejected.slip_commit.unwrap().result.is_err());
    let zero = command(
        &harness.service,
        ProjectRequest::PrepareSlip(proposal(&target, 2, 1, 0)),
    );
    assert!(zero.slip.unwrap().result.is_err());
    assert_eq!(counts(&before.path), rows);
    database.execute("INSERT INTO source_qualifications(id,original_content_id,original_ref,snapshot) VALUES(?1,?2,?3,?4)", rusqlite::params![receipt.0,receipt.1,receipt.2,receipt.3]).unwrap();
    // Recovery requires fresh intent; the consumed request cannot later retry.
    assert!(
        command(&harness.service, ProjectRequest::CommitSlip(ready.id()))
            .slip_commit
            .unwrap()
            .result
            .is_err()
    );
    prepare(&harness, &proposal(&target, 3, 1, 1));
    unchanged(&before, rows);
}

#[test]
fn saved_slip_survives_refresh_failure_rejected_commands_and_queries_until_session_replacement() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before, target) = fixture_at(&scratch.path().join("Documents"));
    let request = proposal(&target, 1, 1, 3);
    let exact = prepare(&harness, &request);
    harness
        .service
        .shared
        .slip_commit_refresh_failure
        .store(true, Ordering::Release);
    let update = command(&harness.service, ProjectRequest::CommitSlip(request.id()));
    assert!(update.error.is_none());
    assert!(update.message.as_ref().unwrap().contains("saved, but"));
    assert!(update.message.as_ref().unwrap().contains("Reopen"));
    assert_eq!(*update.workspace.unwrap().document, *before.document);
    let saved = update.saved_slip.unwrap();
    assert!(
        saved
            .refresh_error
            .as_ref()
            .unwrap()
            .contains("Injected failure")
    );
    assert_eq!(update.committed.unwrap(), saved.committed);
    let reader = ProjectStore::open(&before.path, AccessMode::ReadOnly).unwrap();
    assert_eq!(
        reader.snapshot().unwrap(),
        *exact.snapshot.as_ref().unwrap().document
    );
    drop(reader);
    let rows = counts(&before.path);
    let failed_preview = command(
        &harness.service,
        ProjectRequest::PrepareSlip(proposal(&target, 2, 1, 4)),
    );
    assert!(failed_preview.slip.unwrap().result.is_err());
    assert_eq!(failed_preview.saved_slip, Some(saved.clone()));
    let failed_commit = command(
        &harness.service,
        ProjectRequest::CommitSlip(proposal(&target, 2, 1, 4).id()),
    );
    assert!(failed_commit.slip_commit.unwrap().result.is_err());
    assert_eq!(failed_commit.saved_slip, Some(saved.clone()));
    let duplicate = command(&harness.service, ProjectRequest::CommitSlip(request.id()));
    assert_eq!(
        duplicate.slip_commit.unwrap().result.unwrap(),
        saved.committed
    );
    assert_eq!(duplicate.saved_slip, Some(saved.clone()));
    let query = command(
        &harness.service,
        ProjectRequest::RenderHistory(crate::project::render_history::Request {
            ticket: 1,
            context: ProjectRenderContext {
                session: before.session,
                project: before.document.project_id().clone(),
            },
            query: crate::project::render_history::Query::Jobs { after: None },
        }),
    );
    assert_eq!(query.saved_slip, Some(saved));
    assert_eq!(counts(&before.path), rows);
    let closed = command(&harness.service, ProjectRequest::Close);
    assert!(closed.saved_slip.is_none());
    assert!(closed.slip_commit.is_none());
    assert!(closed.slip.is_none());
    let opened = command(&harness.service, ProjectRequest::Open(before.path.clone()))
        .workspace
        .unwrap();
    assert_ne!(opened.session, before.session);
    assert_eq!(*opened.document, *exact.snapshot.as_ref().unwrap().document);
    assert!(
        command(&harness.service, ProjectRequest::CommitSlip(request.id()))
            .slip_commit
            .unwrap()
            .result
            .is_err()
    );
    assert_eq!(counts(&before.path), rows);
}

#[test]
fn intervening_edit_invalidates_ready_slip_and_undo_does_not_revive_it() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before, target) = fixture_at(&scratch.path().join("Documents"));
    let request = proposal(&target, 1, 1, 2);
    prepare(&harness, &request);
    let changed = edited(
        &harness.service,
        &before,
        ProjectEdit::SetFraming {
            node: target.node.clone(),
            framing: Some(
                deadpan_core::Framing::static_pose(deadpan_core::FramingPose::identity()).unwrap(),
            ),
        },
    );
    assert!(changed.slip.unwrap().result.is_err());
    let changed = changed.workspace.unwrap();
    let rows = counts(&before.path);
    assert!(
        command(&harness.service, ProjectRequest::CommitSlip(request.id()))
            .slip_commit
            .unwrap()
            .result
            .is_err()
    );
    assert_eq!(counts(&before.path), rows);
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: changed.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_ne!(undone.document.revision_id(), before.document.revision_id());
    assert!(
        command(&harness.service, ProjectRequest::PrepareSlip(request))
            .slip
            .unwrap()
            .result
            .is_err()
    );
    assert_eq!(undone.document.nodes(), before.document.nodes());
}

#[test]
fn nested_partition_slip_keeps_wrapper_selection_and_other_fragment_pictures() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, initial, target) = fixture_at(&scratch.path().join("Documents"));
    let split = edited(
        &harness.service,
        &initial,
        ProjectEdit::Split {
            node: target.node,
            at: FrameDuration::new(5).unwrap(),
        },
    );
    let right = split.committed.unwrap().selected_node.unwrap();
    let split = split.workspace.unwrap();
    assert!(matches!(
        split.document.nodes()[&right].kind,
        NodeKind::Retime {
            purpose: deadpan_core::RetimePurpose::Partition,
            ..
        }
    ));
    command(&harness.service, ProjectRequest::Close);
    let mut store = ProjectStore::open(&split.path, AccessMode::ReadWrite).unwrap();
    seed_command(
        &mut store,
        Command::Group {
            parent: split.document.root().clone(),
            start: 0,
            end: 2,
            id: node("slip-group"),
            label: "Slip scope".into(),
        },
        "group-slip-fragments",
    );
    drop(store);
    let before = command(&harness.service, ProjectRequest::Open(split.path.clone()))
        .workspace
        .unwrap();
    let scope = SequenceScope::default()
        .descend(&before, &node("slip-group"))
        .unwrap();
    let target = Target::capture(&before, scope.clone(), Some(&right), ProjectFrame(20)).unwrap();
    assert_eq!(
        target.range,
        FrameRange::new(ProjectFrame(5), ProjectFrame(14)).unwrap()
    );
    let request = proposal(&target, 1, 1, 2);
    let exact = prepare(&harness, &request);
    assert_eq!(exact.resolution.target, right);
    assert_ne!(exact.resolution.physical_source, right);
    let committed = command(&harness.service, ProjectRequest::CommitSlip(request.id()));
    let receipt = committed.slip_commit.unwrap().result.unwrap();
    assert_eq!(receipt.selected_node, Some(right.clone()));
    assert_eq!(receipt.scope, scope);
    assert_eq!(receipt.cursor, Some(ProjectFrame(20)));
    let after = committed.workspace.unwrap();
    let mut expected_right = before.document.nodes()[&right].clone();
    expected_right.audio_editorial_edges = deadpan_core::AudioEditorialEdges {
        start: true,
        end: true,
    };
    assert_eq!(after.document.nodes()[&right], expected_right);
    let NodeKind::Sequence { children } = &before.document.nodes()[&node("slip-group")].kind else {
        unreachable!()
    };
    let left = &children[0];
    let mut expected_left = before.document.nodes()[left].clone();
    expected_left.audio_editorial_edges.end = true;
    assert_eq!(after.document.nodes()[left], expected_left);
    let NodeKind::Sequence { children } = &before.document.nodes()[before.document.root()].kind
    else {
        unreachable!()
    };
    let following = &children[1];
    let mut expected_following = before.document.nodes()[following].clone();
    expected_following.audio_editorial_edges.start = true;
    assert_eq!(after.document.nodes()[following], expected_following);
    assert_eq!(after.plan.duration(), before.plan.duration());
    for frame in 0..5 {
        assert_eq!(
            after.plan.picture(ProjectFrame(frame)).unwrap().picture,
            before.plan.picture(ProjectFrame(frame)).unwrap().picture
        );
    }
    assert_ne!(
        after.plan.picture(ProjectFrame(5)).unwrap().picture,
        before.plan.picture(ProjectFrame(5)).unwrap().picture
    );
    let restored = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: after.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(restored.document.nodes(), before.document.nodes());
}

#[test]
fn failed_store_transaction_consumes_only_its_draft_and_fresh_intent_can_retry() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before, target) = fixture_at(&scratch.path().join("Documents"));
    let rows = counts(&before.path);
    let request = proposal(&target, 1, 1, 2);
    let exact = prepare(&harness, &request);
    let database = rusqlite::Connection::open(before.path.join("project.sqlite")).unwrap();
    database.execute_batch("CREATE TRIGGER fail_native_slip BEFORE UPDATE OF head_revision,cursor ON state BEGIN SELECT RAISE(ABORT, 'injected native Slip transaction failure'); END;").unwrap();
    let failed = command(&harness.service, ProjectRequest::CommitSlip(request.id()));
    assert!(
        failed
            .slip_commit
            .unwrap()
            .result
            .unwrap_err()
            .contains("injected native Slip")
    );
    assert!(failed.saved_slip.is_none());
    unchanged(&before, rows);
    assert!(
        ProjectStore::open(&before.path, AccessMode::ReadOnly)
            .unwrap()
            .snapshot_at(exact.snapshot.as_ref().unwrap().document.revision_id())
            .is_err()
    );
    database
        .execute_batch("DROP TRIGGER fail_native_slip")
        .unwrap();
    assert!(
        command(&harness.service, ProjectRequest::CommitSlip(request.id()))
            .slip_commit
            .unwrap()
            .result
            .is_err()
    );
    unchanged(&before, rows);
    let retry = proposal(&target, 1, 2, 2);
    prepare(&harness, &retry);
    assert!(
        command(&harness.service, ProjectRequest::CommitSlip(retry.id()))
            .slip_commit
            .unwrap()
            .result
            .is_ok()
    );
    assert_eq!(counts(&before.path), (rows.0 + 1, rows.1 + 1));
}
