use super::*;
use crate::project::trim::{
    CapturedTarget, Event, MAX_EVENTS, Prepared, Proposal, ProposalId, Target,
};
use deadpan_core::{SourceTrimControl as Control, SourceTrimIntent, SourceTrimPolicy as Policy};

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
    assert!(target.right.is_some());
    (harness, workspace, target)
}

fn nudge(control: Control, frames: i64) -> Event {
    Event::Nudge { control, frames }
}
fn amount(control: Control, frames: i64) -> Event {
    Event::SetAmount { control, frames }
}
fn proposal(
    target: &Target,
    draft: u64,
    change: u64,
    previous_change: Option<u64>,
    events: Vec<Event>,
) -> Proposal {
    Proposal {
        target: target.clone(),
        draft,
        change,
        previous_change,
        events,
    }
}
fn prepared(update: &ProjectUpdate, id: &ProposalId) -> Arc<Prepared> {
    assert!(update.error.is_none(), "{:?}", update.error);
    let reply = update.trim.as_ref().unwrap();
    assert_eq!(&reply.id, id);
    let prepared = reply
        .result
        .as_ref()
        .unwrap_or_else(|error| panic!("{error}"))
        .clone();
    assert_eq!(
        prepared.accepted,
        reply.acknowledgment.as_ref().unwrap().accepted
    );
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
fn prepare(harness: &Harness, request: &Proposal) -> Arc<Prepared> {
    prepared(
        &command(
            &harness.service,
            ProjectRequest::PrepareTrim(request.clone()),
        ),
        &request.id(),
    )
}
fn unchanged(workspace: &Workspace, expected: (i64, i64)) {
    assert_eq!(counts(&workspace.path), expected);
    let store = ProjectStore::open(&workspace.path, AccessMode::ReadOnly).unwrap();
    assert_eq!(store.snapshot().unwrap(), *workspace.document);
}

#[test]
fn ordered_trim_clamp_then_reverse_preserves_the_applied_value_and_exact_prefix() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before, target) = fixture_at(&scratch.path().join("Documents"));
    let rows = counts(&before.path);
    let first = proposal(
        &target,
        1,
        1,
        None,
        vec![nudge(Control::Slip, 1000), nudge(Control::Slip, -1)],
    );
    let update = command(&harness.service, ProjectRequest::PrepareTrim(first.clone()));
    let exact = prepared(&update, &first.id());
    let ack = update.trim.unwrap().acknowledgment.unwrap();
    assert_eq!(ack.events.len(), 2);
    let first_adjustment = ack.events[0].adjustment.as_ref().unwrap();
    assert_eq!(first_adjustment.requested_value, 1000);
    assert_eq!(first_adjustment.applied_value, 96);
    assert!(first_adjustment.clamp.is_some());
    assert_eq!(
        ack.events[1].adjustment.as_ref().unwrap().previous_value,
        96
    );
    assert_eq!(ack.accepted.slip_frames, 95);
    assert!(ack.events.iter().all(|event| event.error.is_none()));
    assert!(Arc::ptr_eq(&exact.base, &before));
    let second = proposal(
        &target,
        1,
        2,
        Some(1),
        vec![nudge(Control::In, 1), nudge(Control::Slip, -1)],
    );
    let next = prepare(&harness, &second);
    assert_eq!(next.accepted.in_frames, 1);
    assert_eq!(next.accepted.slip_frames, 94);
    assert!(Arc::ptr_eq(&next.base, &exact.base));
    assert_ne!(
        next.snapshot.as_ref().unwrap().document.revision_id(),
        exact.snapshot.as_ref().unwrap().document.revision_id()
    );
    unchanged(&before, rows);
}

#[test]
fn refused_policy_events_retain_all_values_and_later_events_continue_in_order() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before, target) = fixture_at(&scratch.path().join("Documents"));
    let rows = counts(&before.path);
    let request = proposal(
        &target,
        1,
        1,
        None,
        vec![
            nudge(Control::In, -3),
            Event::TogglePolicy,
            Event::TogglePolicy,
            nudge(Control::Out, 1),
            amount(Control::In, 0),
            Event::TogglePolicy,
            Event::TogglePolicy,
        ],
    );
    let update = command(
        &harness.service,
        ProjectRequest::PrepareTrim(request.clone()),
    );
    let exact = prepared(&update, &request.id());
    let ack = update.trim.unwrap().acknowledgment.unwrap();
    assert_eq!(ack.events[0].accepted.in_frames, -3);
    for index in [1, 2] {
        assert!(ack.events[index].error.is_some());
        assert!(ack.events[index].adjustment.is_none());
        assert_eq!(ack.events[index].accepted, ack.events[0].accepted);
    }
    assert_eq!(ack.events[3].accepted.in_frames, -3);
    assert_eq!(ack.events[3].accepted.out_frames, 1);
    assert_eq!(ack.events[5].accepted.policy, Policy::Overwrite);
    assert_eq!(ack.events[6].accepted.policy, Policy::Ripple);
    assert_eq!(
        exact.accepted,
        SourceTrimIntent {
            out_frames: 1,
            ..SourceTrimIntent::default()
        }
    );
    unchanged(&before, rows);
}

#[test]
fn mixed_trim_commits_one_exact_revision_and_selects_its_final_wrapper_with_cursor_clamp() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before, mut target) = fixture_at(&scratch.path().join("Documents"));
    target.cursor = ProjectFrame(before.plan.duration().frames());
    let rows = counts(&before.path);
    let first = proposal(
        &target,
        1,
        1,
        None,
        vec![
            nudge(Control::In, 1),
            nudge(Control::Out, -1),
            nudge(Control::Slip, 2),
            nudge(Control::Roll, 2),
        ],
    );
    let mixed = prepare(&harness, &first);
    assert_eq!(
        mixed.accepted,
        SourceTrimIntent {
            in_frames: 1,
            out_frames: -1,
            slip_frames: 2,
            roll_frames: 2,
            policy: Policy::Ripple
        }
    );
    let overwrite = proposal(&target, 1, 2, Some(1), vec![Event::TogglePolicy]);
    let overlay = prepare(&harness, &overwrite);
    assert_eq!(
        overlay.accepted,
        SourceTrimIntent {
            policy: Policy::Overwrite,
            ..mixed.accepted
        }
    );
    assert_eq!(
        overlay.resolution.fillers,
        vec![
            FrameRange::new(ProjectFrame(0), ProjectFrame(1)).unwrap(),
            FrameRange::new(ProjectFrame(15), ProjectFrame(16)).unwrap(),
        ]
    );
    assert!(!overlay.cursor_clamped);
    let latest = proposal(&target, 1, 3, Some(2), vec![Event::TogglePolicy]);
    let exact = prepare(&harness, &latest);
    assert_eq!(exact.accepted, mixed.accepted);
    assert_eq!(
        exact.result.target_output,
        FrameRange::new(ProjectFrame(0), ProjectFrame(14)).unwrap()
    );
    assert_ne!(exact.result.target, target.node);
    assert!(exact.cursor_clamped);
    assert_eq!(exact.cursor_after, ProjectFrame(target.cursor.0 - 2));
    unchanged(&before, rows);
    assert!(
        command(&harness.service, ProjectRequest::CommitTrim(first.id()))
            .trim_commit
            .unwrap()
            .result
            .is_err()
    );
    command(&harness.service, ProjectRequest::AbandonTrim(first.id()));
    let update = command(&harness.service, ProjectRequest::CommitTrim(latest.id()));
    let receipt = update.trim_commit.unwrap().result.unwrap();
    assert_eq!(receipt.selected_node, Some(exact.result.target.clone()));
    assert_eq!(receipt.scope, target.scope);
    assert!(receipt.preserve_cursor);
    assert_eq!(receipt.cursor, Some(exact.cursor_after));
    assert!(receipt.range_selection.is_none());
    assert!(update.message.unwrap().contains("shorter project's end"));
    let after = update.workspace.unwrap();
    assert_eq!(*after.document, *exact.snapshot.as_ref().unwrap().document);
    assert_eq!(counts(&before.path), (rows.0 + 1, rows.1 + 1));
    assert_eq!(update.saved_trim.unwrap().committed, receipt);
    let duplicate = command(&harness.service, ProjectRequest::CommitTrim(latest.id()));
    assert_eq!(duplicate.trim_commit.unwrap().result.unwrap(), receipt);
    assert_eq!(counts(&before.path), (rows.0 + 1, rows.1 + 1));
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: after.document.revision_id().clone(),
        },
    );
    assert_eq!(undone.saved_trim.unwrap().committed, receipt);
    let undone = undone.workspace.unwrap();
    assert_eq!(undone.document.nodes(), before.document.nodes());
    assert_ne!(undone.document.revision_id(), before.document.revision_id());
    let undone_rows = counts(&before.path);
    assert_eq!(
        command(&harness.service, ProjectRequest::CommitTrim(latest.id()))
            .trim_commit
            .unwrap()
            .result
            .unwrap(),
        receipt
    );
    assert_eq!(counts(&before.path), undone_rows);
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
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(before.path.clone()))
        .workspace
        .unwrap();
    assert_eq!(*reopened.document, *redone.document);
}

#[test]
fn invalid_envelopes_do_not_consume_or_retarget_an_acknowledged_prefix() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before, target) = fixture_at(&scratch.path().join("Documents"));
    let first = proposal(&target, 1, 1, None, vec![nudge(Control::Slip, 2)]);
    let exact = prepare(&harness, &first);
    let rows = counts(&before.path);
    for case in 0..13 {
        let mut bad = proposal(&target, 1, 2, Some(1), vec![nudge(Control::Slip, 1)]);
        match case {
            0 => bad.change = 1,
            1 => bad.previous_change = None,
            2 => bad.previous_change = Some(9),
            3 => bad.target.cursor = ProjectFrame(0),
            4 => bad.target.right = None,
            5 => bad.target.session += 1,
            6 => bad.target.project = ProjectId::new("foreign").unwrap(),
            7 => bad.target.base_revision = RevisionId::new("stale").unwrap(),
            8 => bad.target.parent = target.node.clone(),
            9 => bad.target.node = node("missing"),
            10 => bad.events = vec![nudge(Control::Slip, 1); MAX_EVENTS + 1],
            11 => bad.target.range = FrameRange::new(ProjectFrame(1), ProjectFrame(14)).unwrap(),
            _ => bad.target.scope = SequenceScope::test_path(vec![node("missing")]),
        }
        let update = command(&harness.service, ProjectRequest::PrepareTrim(bad));
        let feedback = update.trim.unwrap();
        assert!(feedback.acknowledgment.is_none(), "case {case}");
        assert!(feedback.result.is_err(), "case {case}");
        unchanged(&before, rows);
    }
    let next = proposal(&target, 1, 2, Some(1), vec![nudge(Control::Slip, 1)]);
    let next = prepare(&harness, &next);
    assert_eq!(next.accepted.slip_frames, 3);
    assert!(Arc::ptr_eq(&next.base, &exact.base));
    unchanged(&before, rows);
}

#[test]
fn zero_round_trip_and_abandon_never_author_or_reuse_a_ready_trim() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before, target) = fixture_at(&scratch.path().join("Documents"));
    let rows = counts(&before.path);
    let ready = proposal(&target, 1, 1, None, vec![nudge(Control::Slip, 3)]);
    prepare(&harness, &ready);
    let zero = proposal(
        &target,
        1,
        2,
        Some(1),
        vec![
            amount(Control::Slip, 0),
            Event::SetPolicy(Policy::Overwrite),
        ],
    );
    let report = prepare(&harness, &zero);
    assert!(report.snapshot.is_none());
    assert!(report.accepted.is_zero());
    assert_eq!(report.accepted.policy, Policy::Overwrite);
    assert_eq!(*report.base.document, *before.document);
    assert!(
        command(&harness.service, ProjectRequest::CommitTrim(ready.id()))
            .trim_commit
            .unwrap()
            .result
            .is_err()
    );
    assert!(
        command(&harness.service, ProjectRequest::CommitTrim(zero.id()))
            .trim_commit
            .unwrap()
            .result
            .unwrap_err()
            .contains("no change")
    );
    let final_request = proposal(&target, 1, 3, Some(2), vec![nudge(Control::Slip, 2)]);
    prepare(&harness, &final_request);
    command(
        &harness.service,
        ProjectRequest::AbandonTrim(final_request.id()),
    );
    assert!(
        command(
            &harness.service,
            ProjectRequest::CommitTrim(final_request.id())
        )
        .trim_commit
        .unwrap()
        .result
        .is_err()
    );
    let lost = command(
        &harness.service,
        ProjectRequest::PrepareTrim(proposal(&target, 1, 4, Some(3), vec![])),
    )
    .trim
    .unwrap();
    assert!(lost.acknowledgment.is_none());
    assert!(lost.result.is_err());
    unchanged(&before, rows);
}

#[test]
fn literal_empty_neighbor_and_captured_absence_never_skip_to_a_roll_target() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, initial, target) = fixture_at(&scratch.path().join("Documents"));
    let absent: CapturedTarget =
        Target::capture(&initial, SequenceScope::default(), None, ProjectFrame(0));
    assert!(absent.unwrap_err().contains("no target was captured"));
    command(&harness.service, ProjectRequest::Close);
    let mut store = ProjectStore::open(&initial.path, AccessMode::ReadWrite).unwrap();
    seed_command(
        &mut store,
        Command::Insert {
            parent: target.parent.clone(),
            index: 1,
            subtree: Subtree {
                root: node("trim-empty"),
                nodes: BTreeMap::from([(node("trim-empty"), BeatNode::sequence("empty", vec![]))]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
        "trim-insert-empty",
    );
    drop(store);
    let before = command(&harness.service, ProjectRequest::Open(initial.path.clone()))
        .workspace
        .unwrap();
    let target = Target::capture(
        &before,
        SequenceScope::default(),
        Some(&target.node),
        ProjectFrame(20),
    )
    .unwrap();
    assert_eq!(target.right, Some(node("trim-empty")));
    let rows = counts(&before.path);
    let request = proposal(
        &target,
        1,
        1,
        None,
        vec![nudge(Control::Roll, 1), nudge(Control::Slip, 2)],
    );
    let update = command(
        &harness.service,
        ProjectRequest::PrepareTrim(request.clone()),
    );
    let exact = prepared(&update, &request.id());
    assert_eq!(exact.accepted.roll_frames, 0);
    assert_eq!(exact.accepted.slip_frames, 2);
    let ack = update.trim.unwrap().acknowledgment.unwrap();
    assert!(ack.events[0].error.is_some());
    assert!(ack.events[1].error.is_none());
    let mut skipped = proposal(&target, 1, 2, Some(1), vec![nudge(Control::Roll, 1)]);
    skipped.target.right = initial
        .document
        .children(initial.document.root())
        .next_back()
        .cloned();
    let rejected = command(&harness.service, ProjectRequest::PrepareTrim(skipped))
        .trim
        .unwrap();
    assert!(rejected.acknowledgment.is_none());
    assert!(rejected.result.is_err());
    let last = before
        .document
        .children(before.document.root())
        .next_back()
        .unwrap();
    let no_right = Target::capture(
        &before,
        SequenceScope::default(),
        Some(last),
        ProjectFrame(20),
    )
    .unwrap();
    assert!(no_right.right.is_none());
    let no_right_request = proposal(&no_right, 2, 1, None, vec![nudge(Control::Roll, 1)]);
    let update = command(
        &harness.service,
        ProjectRequest::PrepareTrim(no_right_request.clone()),
    );
    let exact = prepared(&update, &no_right_request.id());
    assert!(exact.snapshot.is_none());
    assert!(
        update.trim.unwrap().acknowledgment.unwrap().events[0]
            .error
            .is_some()
    );
    unchanged(&before, rows);
}

#[test]
fn receipt_failure_acknowledges_input_and_revokes_old_apply_until_fresh_admission() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before, target) = fixture_at(&scratch.path().join("Documents"));
    let rows = counts(&before.path);
    let ready = proposal(&target, 1, 1, None, vec![nudge(Control::Slip, 3)]);
    prepare(&harness, &ready);
    let database = rusqlite::Connection::open(before.path.join("project.sqlite")).unwrap();
    let receipt: (String, String, String, Vec<u8>) = database.query_row(
        "SELECT id,original_content_id,original_ref,snapshot FROM source_qualifications LIMIT 1", [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ).unwrap();
    database
        .execute("DELETE FROM source_qualifications", [])
        .unwrap();
    let failed = proposal(&target, 1, 2, Some(1), vec![nudge(Control::Slip, 1)]);
    let rejected = command(
        &harness.service,
        ProjectRequest::PrepareTrim(failed.clone()),
    )
    .trim
    .unwrap();
    assert!(rejected.result.is_err());
    assert_eq!(rejected.acknowledgment.unwrap().accepted.slip_frames, 4);
    for id in [ready.id(), failed.id()] {
        assert!(
            command(&harness.service, ProjectRequest::CommitTrim(id))
                .trim_commit
                .unwrap()
                .result
                .is_err()
        );
    }
    let zero = proposal(&target, 1, 3, Some(2), vec![amount(Control::Slip, 0)]);
    let rejected_zero = command(&harness.service, ProjectRequest::PrepareTrim(zero))
        .trim
        .unwrap();
    assert!(rejected_zero.acknowledgment.unwrap().accepted.is_zero());
    assert!(rejected_zero.result.is_err());
    assert_eq!(counts(&before.path), rows);
    database.execute(
        "INSERT INTO source_qualifications(id,original_content_id,original_ref,snapshot) VALUES(?1,?2,?3,?4)",
        rusqlite::params![receipt.0, receipt.1, receipt.2, receipt.3],
    ).unwrap();
    let retry = proposal(&target, 1, 4, Some(3), vec![nudge(Control::Slip, 2)]);
    let exact = prepare(&harness, &retry);
    assert_eq!(exact.accepted.slip_frames, 2);
    // A receipt can disappear after preparation too; commit rechecks it and the
    // consumed attempt remains unavailable when the stored evidence returns.
    database
        .execute("DELETE FROM source_qualifications", [])
        .unwrap();
    assert!(
        command(&harness.service, ProjectRequest::CommitTrim(retry.id()))
            .trim_commit
            .unwrap()
            .result
            .is_err()
    );
    database.execute(
        "INSERT INTO source_qualifications(id,original_content_id,original_ref,snapshot) VALUES(?1,?2,?3,?4)",
        rusqlite::params![receipt.0, receipt.1, receipt.2, receipt.3],
    ).unwrap();
    assert!(
        command(&harness.service, ProjectRequest::CommitTrim(retry.id()))
            .trim_commit
            .unwrap()
            .result
            .is_err()
    );
    let retry = proposal(&target, 1, 5, Some(4), vec![]);
    assert_eq!(prepare(&harness, &retry).accepted.slip_frames, 2);
    unchanged(&before, rows);
}

#[test]
fn saved_trim_survives_refresh_failure_later_errors_queries_and_session_close() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before, target) = fixture_at(&scratch.path().join("Documents"));
    let request = proposal(&target, 1, 1, None, vec![nudge(Control::In, 1)]);
    let exact = prepare(&harness, &request);
    harness
        .service
        .shared
        .trim_commit_refresh_failure
        .store(true, Ordering::Release);
    let update = command(&harness.service, ProjectRequest::CommitTrim(request.id()));
    assert!(update.error.is_none());
    assert!(update.message.as_ref().unwrap().contains("saved, but"));
    assert!(update.message.as_ref().unwrap().contains("Reopen"));
    assert_eq!(*update.workspace.unwrap().document, *before.document);
    let saved = update.saved_trim.unwrap();
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
    let later = proposal(&target, 2, 1, None, vec![nudge(Control::Slip, 4)]);
    let failed_preview = command(&harness.service, ProjectRequest::PrepareTrim(later.clone()));
    assert!(failed_preview.trim.unwrap().result.is_err());
    assert_eq!(failed_preview.saved_trim, Some(saved.clone()));
    let failed_commit = command(&harness.service, ProjectRequest::CommitTrim(later.id()));
    assert!(failed_commit.trim_commit.unwrap().result.is_err());
    assert_eq!(failed_commit.saved_trim, Some(saved.clone()));
    let duplicate = command(&harness.service, ProjectRequest::CommitTrim(request.id()));
    assert_eq!(
        duplicate.trim_commit.unwrap().result.unwrap(),
        saved.committed
    );
    assert_eq!(duplicate.saved_trim, Some(saved.clone()));
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
    assert_eq!(query.saved_trim, Some(saved));
    assert_eq!(counts(&before.path), rows);
    let closed = command(&harness.service, ProjectRequest::Close);
    assert!(closed.saved_trim.is_none());
    assert!(closed.trim_commit.is_none());
    assert!(closed.trim.is_none());
    let opened = command(&harness.service, ProjectRequest::Open(before.path.clone()))
        .workspace
        .unwrap();
    assert_ne!(opened.session, before.session);
    assert_eq!(*opened.document, *exact.snapshot.as_ref().unwrap().document);
    assert!(
        command(&harness.service, ProjectRequest::CommitTrim(request.id()))
            .trim_commit
            .unwrap()
            .result
            .is_err()
    );
    assert_eq!(counts(&before.path), rows);
}

#[test]
fn failed_store_transaction_consumes_its_attempt_and_empty_fresh_prefix_can_retry() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before, target) = fixture_at(&scratch.path().join("Documents"));
    let rows = counts(&before.path);
    let request = proposal(&target, 1, 1, None, vec![nudge(Control::Slip, 2)]);
    let exact = prepare(&harness, &request);
    let database = rusqlite::Connection::open(before.path.join("project.sqlite")).unwrap();
    database.execute_batch("CREATE TRIGGER fail_native_trim BEFORE UPDATE OF head_revision,cursor ON state BEGIN SELECT RAISE(ABORT, 'injected native Trim transaction failure'); END;").unwrap();
    let failed = command(&harness.service, ProjectRequest::CommitTrim(request.id()));
    assert!(
        failed
            .trim_commit
            .unwrap()
            .result
            .unwrap_err()
            .contains("injected native Trim")
    );
    assert!(failed.saved_trim.is_none());
    unchanged(&before, rows);
    assert!(
        ProjectStore::open(&before.path, AccessMode::ReadOnly)
            .unwrap()
            .snapshot_at(exact.snapshot.as_ref().unwrap().document.revision_id())
            .is_err()
    );
    database
        .execute_batch("DROP TRIGGER fail_native_trim")
        .unwrap();
    assert!(
        command(&harness.service, ProjectRequest::CommitTrim(request.id()))
            .trim_commit
            .unwrap()
            .result
            .is_err()
    );
    unchanged(&before, rows);
    let retry = proposal(&target, 1, 2, Some(1), vec![]);
    assert_eq!(prepare(&harness, &retry).accepted, exact.accepted);
    assert!(
        command(&harness.service, ProjectRequest::CommitTrim(retry.id()))
            .trim_commit
            .unwrap()
            .result
            .is_ok()
    );
    assert_eq!(counts(&before.path), (rows.0 + 1, rows.1 + 1));
}

#[test]
fn intervening_edit_and_fresh_undo_revision_never_revive_trim() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before, target) = fixture_at(&scratch.path().join("Documents"));
    let request = proposal(&target, 1, 1, None, vec![nudge(Control::Slip, 2)]);
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
    assert!(changed.trim.unwrap().result.is_err());
    let changed = changed.workspace.unwrap();
    let rows = counts(&before.path);
    assert!(
        command(&harness.service, ProjectRequest::CommitTrim(request.id()))
            .trim_commit
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
    let stale = command(&harness.service, ProjectRequest::PrepareTrim(request))
        .trim
        .unwrap();
    assert!(stale.result.is_err());
    assert!(stale.acknowledgment.is_none());
    assert_eq!(undone.document.nodes(), before.document.nodes());
}

#[test]
fn trim_event_capacity_and_amount_overflow_are_explicit_without_losing_accepted_values() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before, target) = fixture_at(&scratch.path().join("Documents"));
    let rows = counts(&before.path);
    let maximum = proposal(
        &target,
        1,
        1,
        None,
        vec![nudge(Control::Slip, 0); MAX_EVENTS],
    );
    let update = command(
        &harness.service,
        ProjectRequest::PrepareTrim(maximum.clone()),
    );
    assert!(prepared(&update, &maximum.id()).snapshot.is_none());
    assert_eq!(
        update.trim.unwrap().acknowledgment.unwrap().events.len(),
        MAX_EVENTS
    );
    let overflow = proposal(
        &target,
        1,
        9,
        Some(1),
        vec![
            nudge(Control::Slip, 2),
            amount(Control::Slip, i64::MIN),
            nudge(Control::Slip, -1),
        ],
    );
    let update = command(
        &harness.service,
        ProjectRequest::PrepareTrim(overflow.clone()),
    );
    assert_eq!(prepared(&update, &overflow.id()).accepted.slip_frames, 1);
    let ack = update.trim.unwrap().acknowledgment.unwrap();
    assert_eq!(ack.events[1].accepted.slip_frames, 2);
    assert!(ack.events[1].error.as_ref().unwrap().contains("overflow"));
    assert!(ack.events[1].adjustment.is_none());
    unchanged(&before, rows);
}

#[test]
fn nested_partition_trim_keeps_its_captured_scope_wrapper_and_absolute_cursor() {
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
    command(&harness.service, ProjectRequest::Close);
    let mut store = ProjectStore::open(&split.path, AccessMode::ReadWrite).unwrap();
    seed_command(
        &mut store,
        Command::Group {
            parent: split.document.root().clone(),
            start: 0,
            end: 2,
            id: node("trim-group"),
            label: "Trim scope".into(),
        },
        "group-trim-fragments",
    );
    drop(store);
    let before = command(&harness.service, ProjectRequest::Open(split.path.clone()))
        .workspace
        .unwrap();
    let scope = SequenceScope::default()
        .descend(&before, &node("trim-group"))
        .unwrap();
    let target = Target::capture(&before, scope.clone(), Some(&right), ProjectFrame(20)).unwrap();
    assert!(target.right.is_none());
    assert_eq!(
        target.range,
        FrameRange::new(ProjectFrame(5), ProjectFrame(14)).unwrap()
    );
    let request = proposal(&target, 1, 1, None, vec![nudge(Control::In, 1)]);
    let exact = prepare(&harness, &request);
    assert_eq!(exact.result.target, right);
    assert_eq!(
        exact.result.target_output,
        FrameRange::new(ProjectFrame(5), ProjectFrame(13)).unwrap()
    );
    assert_eq!(exact.cursor_after, target.cursor);
    assert!(!exact.cursor_clamped);
    let update = command(&harness.service, ProjectRequest::CommitTrim(request.id()));
    let receipt = update.trim_commit.unwrap().result.unwrap();
    assert_eq!(receipt.selected_node, Some(right));
    assert_eq!(receipt.scope, scope);
    assert_eq!(receipt.cursor, Some(ProjectFrame(20)));
    let after = update.workspace.unwrap();
    assert_eq!(*after.document, *exact.snapshot.as_ref().unwrap().document);
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
