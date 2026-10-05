use super::*;
use crate::project::splice::{Destination, Operation, Prepared, Proposal, ProposalId, Source};

#[path = "splice_equivalence.rs"]
mod equivalence;

#[path = "splice/interior.rs"]
mod interior;

#[path = "splice/replacement.rs"]
mod replacement;

fn initialize(harness: &Harness) -> Arc<Workspace> {
    command(
        &harness.service,
        ProjectRequest::CreateFromSource {
            path: fixture("cfr-bframes.mp4"),
        },
    );
    harness.finish(harness.job());
    harness.finish(harness.job());
    complete(&harness.service)
}

fn proposal(workspace: &Workspace, draft: u64, change: u64) -> Proposal {
    let source = workspace
        .sources
        .values()
        .find(|source| source.video_index.is_some())
        .unwrap();
    Proposal {
        operation: Operation::Copy,
        id: ProposalId {
            session: workspace.session,
            project: workspace.document.project_id().clone(),
            base_revision: workspace.document.revision_id().clone(),
            draft,
            change,
        },
        source: Source::Original {
            asset: source.asset.clone(),
            qualification: source.receipt.id().clone(),
            ordinals: 10..24,
        },
        scope: SequenceScope::default(),
        parent: workspace.document.root().clone(),
        destination: Destination::Slot(0),
    }
}

fn set_ordinals(proposal: &mut Proposal, range: std::ops::Range<u64>) {
    let Source::Original { ordinals, .. } = &mut proposal.source else {
        panic!("Original proposal")
    };
    *ordinals = range;
}

fn original_source(
    proposal: &Proposal,
) -> (&AssetId, &SourceQualificationId, &std::ops::Range<u64>) {
    let Source::Original {
        asset,
        qualification,
        ordinals,
    } = &proposal.source
    else {
        panic!("Original proposal")
    };
    (asset, qualification, ordinals)
}

fn prepared(update: &ProjectUpdate, id: &ProposalId) -> Arc<Prepared> {
    assert!(update.error.is_none(), "{:?}", update.error);
    let reply = update.splice.as_ref().expect("slice reply");
    assert_eq!(&reply.id, id);
    let prepared = reply
        .result
        .as_ref()
        .map(Arc::clone)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(prepared.base.session, id.session);
    assert_eq!(prepared.base.document.project_id(), &id.project);
    assert_eq!(prepared.base.document.revision_id(), &id.base_revision);
    prepared
        .snapshot
        .validate_proposed_base(prepared.base.session, &prepared.base.document)
        .unwrap();
    prepared
}

fn unchanged(workspace: &Workspace) {
    let store = ProjectStore::open(&workspace.path, AccessMode::ReadOnly).unwrap();
    assert_eq!(store.snapshot().unwrap(), *workspace.document);
}

#[test]
fn cached_refinement_and_abandon_leave_history_unchanged_and_exact_commit_undo_once() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let before = initialize(&harness);
    assert!(!before.can_undo);
    let first = proposal(&before, 1, 1);
    let first_update = command(
        &harness.service,
        ProjectRequest::PrepareSplice(first.clone()),
    );
    let first_prepared = prepared(&first_update, &first.id);
    assert!(Arc::ptr_eq(&first_prepared.base, &before));
    assert_eq!(first_prepared.range.start(), ProjectFrame(0));
    assert_eq!(first_prepared.range.end(), ProjectFrame(14));
    assert_eq!(
        first_prepared.snapshot.document.assets(),
        before.document.assets()
    );
    assert_eq!(
        first_prepared.snapshot.content,
        deadpan_playback::ContentIdentity::Proposed {
            base_revision: before.document.revision_id().clone(),
            draft: 1,
            change: 1,
        }
    );
    let original_receipt = first_update.committed.clone();
    unchanged(&before);

    let mut refined = proposal(&before, 1, 2);
    set_ordinals(&mut refined, 12..31);
    refined.destination = Destination::Slot(1);
    let update = command(
        &harness.service,
        ProjectRequest::PrepareSplice(refined.clone()),
    );
    let second = prepared(&update, &refined.id);
    assert_ne!(
        first_prepared.snapshot.document.revision_id(),
        second.snapshot.document.revision_id()
    );
    assert_ne!(first_prepared.node, second.node);
    assert_eq!(
        second.range.start(),
        ProjectFrame(before.plan.duration().frames())
    );
    assert_eq!(
        second.range.end(),
        ProjectFrame(before.plan.duration().frames() + 19)
    );
    assert_eq!(update.committed, original_receipt);
    assert!(Arc::ptr_eq(update.workspace.as_ref().unwrap(), &before));
    unchanged(&before);

    // A delayed abandon from an older refinement cannot discard the latest one.
    command(&harness.service, ProjectRequest::AbandonSplice(first.id));
    let abandoned = command(
        &harness.service,
        ProjectRequest::AbandonSplice(refined.id.clone()),
    );
    assert!(abandoned.splice.unwrap().result.is_err());
    let refused = command(&harness.service, ProjectRequest::CommitSplice(refined.id));
    assert!(refused.splice_commit.unwrap().result.is_err());
    unchanged(&before);

    let final_proposal = proposal(&before, 2, 1);
    let ready = command(
        &harness.service,
        ProjectRequest::PrepareSplice(final_proposal.clone()),
    );
    let exact = prepared(&ready, &final_proposal.id);
    let committed = command(
        &harness.service,
        ProjectRequest::CommitSplice(final_proposal.id.clone()),
    );
    assert!(committed.error.is_none(), "{:?}", committed.error);
    let receipt = committed.splice_commit.unwrap().result.unwrap();
    assert_eq!(&receipt.revision, exact.snapshot.document.revision_id());
    assert_eq!(receipt.selected_node, Some(exact.node.clone()));
    assert_eq!(receipt.cursor, Some(exact.range.start()));
    let after = committed.workspace.unwrap();
    assert_eq!(*after.document, *exact.snapshot.document);
    assert!(after.can_undo);
    let repeated = command(
        &harness.service,
        ProjectRequest::CommitSplice(final_proposal.id),
    );
    assert_eq!(repeated.splice_commit.unwrap().result.unwrap(), receipt);
    assert_eq!(*repeated.workspace.unwrap().document, *after.document);
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: after.document.revision_id().clone(),
        },
    );
    let undone = undone.workspace.unwrap();
    assert_eq!(undone.document.nodes(), before.document.nodes());
    assert_eq!(undone.plan.duration(), before.plan.duration());
    assert!(!undone.can_undo);
}

#[test]
fn uncached_qualification_prepares_latest_exact_nested_destination_without_committing() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let initial = initialize(&harness);
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
                label: "Slice destination".into(),
            },
            "group-before-slice",
        );
    }
    let before = command(&harness.service, ProjectRequest::Open(initial.path.clone()))
        .workspace
        .unwrap();
    let first = proposal(&before, 1, 1);
    let waiting = command(
        &harness.service,
        ProjectRequest::PrepareSplice(first.clone()),
    );
    assert!(waiting.splice.is_none());
    assert!(
        waiting.import.is_none(),
        "slice preparation must not look like asset registration"
    );
    let job = harness.job();
    let mut latest = proposal(&before, 1, 2);
    set_ordinals(&mut latest, 31..45);
    latest.scope = SequenceScope::default()
        .descend(&before, &node("slice-group"))
        .unwrap();
    latest.parent = node("slice-group");
    latest.destination = Destination::Slot(1);
    command(
        &harness.service,
        ProjectRequest::PrepareSplice(latest.clone()),
    );
    assert!(!job.cancelled.load(Ordering::Acquire));
    // A spurious earlier worker identity cannot consume the active qualification.
    harness
        .replies
        .send(worker::Reply {
            id: job.id + 1,
            result: Err("unrelated reply".into()),
        })
        .unwrap();
    harness.finish(job);
    let ready = wait(&harness.service, |update| {
        update
            .splice
            .as_ref()
            .is_some_and(|reply| reply.id == latest.id)
    });
    let exact = prepared(&ready, &latest.id);
    assert!(
        ready.import.is_none(),
        "slice completion must not select a registered source"
    );
    assert_eq!(
        exact.range.start(),
        ProjectFrame(before.plan.duration().frames())
    );
    unchanged(&before);
    let stale = command(&harness.service, ProjectRequest::CommitSplice(first.id));
    assert!(stale.splice_commit.unwrap().result.is_err());
    let applied = command(&harness.service, ProjectRequest::CommitSplice(latest.id));
    assert_eq!(
        *applied.workspace.unwrap().document,
        *exact.snapshot.document
    );
    assert_eq!(
        applied.splice_commit.unwrap().result.unwrap().scope,
        latest.scope
    );
}

#[test]
fn cancelled_and_revision_stale_worker_replies_cannot_restore_or_commit_a_draft() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let initial = initialize(&harness);
    command(&harness.service, ProjectRequest::Close);
    let before = command(&harness.service, ProjectRequest::Open(initial.path.clone()))
        .workspace
        .unwrap();
    let abandoned = proposal(&before, 1, 1);
    command(
        &harness.service,
        ProjectRequest::PrepareSplice(abandoned.clone()),
    );
    let job = harness.job();
    let worker_id = job.id;
    let cancellation = job.cancelled.clone();
    let result = worker::prepare(job);
    assert!(result.is_ok());
    command(
        &harness.service,
        ProjectRequest::AbandonSplice(abandoned.id.clone()),
    );
    assert!(cancellation.load(Ordering::Acquire));
    harness
        .replies
        .send(worker::Reply {
            id: worker_id,
            result,
        })
        .unwrap();
    // Two actor turns form a drain barrier without depending on decoder timing.
    command(
        &harness.service,
        ProjectRequest::AbandonSplice(abandoned.id.clone()),
    );
    command(
        &harness.service,
        ProjectRequest::AbandonSplice(abandoned.id.clone()),
    );
    let refused = command(&harness.service, ProjectRequest::CommitSplice(abandoned.id));
    assert!(refused.splice_commit.unwrap().result.is_err());
    unchanged(&before);

    let stale = proposal(&before, 2, 1);
    command(
        &harness.service,
        ProjectRequest::PrepareSplice(stale.clone()),
    );
    let job = harness.job();
    let worker_id = job.id;
    let cancellation = job.cancelled.clone();
    let result = worker::prepare(job);
    assert!(result.is_ok());
    let original = before
        .document
        .children(before.document.root())
        .next()
        .unwrap()
        .clone();
    let edited = edited(
        &harness.service,
        &before,
        ProjectEdit::Split {
            node: original,
            at: FrameDuration::new(60).unwrap(),
        },
    );
    let after = edited.workspace.unwrap();
    assert!(cancellation.load(Ordering::Acquire));
    harness
        .replies
        .send(worker::Reply {
            id: worker_id,
            result,
        })
        .unwrap();
    command(
        &harness.service,
        ProjectRequest::AbandonSplice(stale.id.clone()),
    );
    let refused = command(&harness.service, ProjectRequest::CommitSplice(stale.id));
    assert!(refused.splice_commit.unwrap().result.is_err());
    assert_eq!(*refused.workspace.unwrap().document, *after.document);
    unchanged(&after);
}

#[test]
fn invalid_identity_qualification_scope_and_reopened_session_fail_without_retargeting() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let before = initialize(&harness);
    for (draft, change) in [(0, 1), (1, 0)] {
        let request = proposal(&before, draft, change);
        let update = command(&harness.service, ProjectRequest::PrepareSplice(request));
        assert!(update.splice.unwrap().result.is_err());
    }
    let mut foreign = proposal(&before, 1, 1);
    foreign.id.project = ProjectId::new("another-project").unwrap();
    assert!(
        command(&harness.service, ProjectRequest::PrepareSplice(foreign))
            .splice
            .unwrap()
            .result
            .is_err()
    );
    let mut wrong = proposal(&before, 1, 1);
    let Source::Original { qualification, .. } = &mut wrong.source else {
        panic!("Original proposal")
    };
    *qualification = SourceQualificationId::new("b".repeat(64)).unwrap();
    assert!(
        command(&harness.service, ProjectRequest::PrepareSplice(wrong))
            .splice
            .unwrap()
            .result
            .is_err()
    );
    let mut outside = proposal(&before, 2, 1);
    outside.destination = Destination::Slot(2);
    assert!(
        command(&harness.service, ProjectRequest::PrepareSplice(outside))
            .splice
            .unwrap()
            .result
            .is_err()
    );
    let request = proposal(&before, 3, 1);
    let ready = command(
        &harness.service,
        ProjectRequest::PrepareSplice(request.clone()),
    );
    prepared(&ready, &request.id);
    let mut reused = request.clone();
    set_ordinals(&mut reused, 40..55);
    assert!(
        command(&harness.service, ProjectRequest::PrepareSplice(reused))
            .splice
            .unwrap()
            .result
            .is_err()
    );
    unchanged(&before);
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(before.path.clone()))
        .workspace
        .unwrap();
    assert_ne!(reopened.session, before.session);
    let rejected = command(&harness.service, ProjectRequest::CommitSplice(request.id));
    assert!(rejected.splice_commit.unwrap().result.is_err());
    assert_eq!(*rejected.workspace.unwrap().document, *before.document);
}

#[test]
fn abandon_does_not_cancel_unrelated_import_and_commit_receipt_survives_refresh_failure() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let before = initialize(&harness);
    let request = proposal(&before, 1, 1);
    let ready = command(
        &harness.service,
        ProjectRequest::PrepareSplice(request.clone()),
    );
    prepared(&ready, &request.id);
    command(
        &harness.service,
        ProjectRequest::ImportSound {
            expected_session: before.session,
            expected_revision: before.document.revision_id().clone(),
            path: fixture("../audio-fixtures/pcm-stereo-48000.wav"),
            stream: None,
            interpretation: Some(AudioLayoutInterpretation::StereoLeftRight),
            ownership: OriginalOwnership::Managed,
        },
    );
    let unrelated = harness.job();
    command(&harness.service, ProjectRequest::AbandonSplice(request.id));
    assert!(!unrelated.cancelled.load(Ordering::Acquire));
    harness.finish(unrelated);
    harness.finish(harness.job());
    let before = complete(&harness.service);
    let request = proposal(&before, 2, 1);
    let waiting = command(
        &harness.service,
        ProjectRequest::PrepareSplice(request.clone()),
    );
    let ready = if waiting.splice.is_none() {
        harness.finish(harness.job());
        wait(&harness.service, |update| {
            update
                .splice
                .as_ref()
                .is_some_and(|reply| reply.id == request.id)
        })
    } else {
        waiting
    };
    let exact = prepared(&ready, &request.id);
    harness
        .service
        .shared
        .splice_commit_refresh_failure
        .store(true, Ordering::Release);
    let applied = command(
        &harness.service,
        ProjectRequest::CommitSplice(request.id.clone()),
    );
    assert!(applied.error.is_none());
    let warning = applied.message.as_ref().unwrap();
    assert!(warning.contains("saved, but"));
    assert!(warning.contains("Injected failure"));
    assert!(warning.contains("Reopen"));
    let receipt = applied.splice_commit.unwrap().result.unwrap();
    assert_eq!(&receipt.revision, exact.snapshot.document.revision_id());
    assert_eq!(applied.committed.unwrap(), receipt);
    assert_eq!(*applied.workspace.unwrap().document, *before.document);
    let read = ProjectStore::open(&before.path, AccessMode::ReadOnly).unwrap();
    assert_eq!(read.snapshot().unwrap(), *exact.snapshot.document);
    drop(read);
    let duplicate = command(
        &harness.service,
        ProjectRequest::CommitSplice(request.id.clone()),
    );
    assert_eq!(duplicate.splice_commit.unwrap().result.unwrap(), receipt);
    let abandoned = command(&harness.service, ProjectRequest::AbandonSplice(request.id));
    assert_eq!(abandoned.splice_commit.unwrap().result.unwrap(), receipt);
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
    assert_eq!(query.splice_commit.unwrap().result.unwrap(), receipt);
    let failed_preview = command(
        &harness.service,
        ProjectRequest::PrepareSplice(proposal(&before, 3, 1)),
    );
    assert!(failed_preview.splice.unwrap().result.is_err());
    assert_eq!(
        failed_preview.splice_commit.unwrap().result.unwrap(),
        receipt
    );
}
