use deadpan_cli::encoded_render::workflow::{
    PublicationRequest, ReconcileRender, RetryRender, StartRender, WorkflowIdentity,
    WorkflowOutcome, WorkflowStage,
};
use deadpan_jobs::{
    AttemptId, CancellationToken, RequestId,
    render::{RenderBFrames, RenderEncoder, RenderEngineeringPolicy, RenderSelection},
};

use super::*;
use crate::project::gain;

fn context(workspace: &Workspace) -> ProjectRenderContext {
    ProjectRenderContext {
        session: workspace.session,
        project: workspace.document.project_id().clone(),
    }
}

fn identity(suffix: &str) -> WorkflowIdentity {
    WorkflowIdentity {
        job_id: RequestId::new(format!("native-render-job-{suffix}")).unwrap(),
        attempt_id: AttemptId::new(format!("native-render-attempt-{suffix}")).unwrap(),
        cancellation_token: CancellationToken::new(format!("native-render-cancel-{suffix}"))
            .unwrap(),
    }
}

fn publication(path: &Path, suffix: &str) -> PublicationRequest {
    PublicationRequest {
        destination: path.join(format!("render-{suffix}.mp4")),
        publication_id: RequestId::new(format!("native-publication-{suffix}")).unwrap(),
        operation_id: AttemptId::new(format!("native-publication-operation-{suffix}")).unwrap(),
        cancellation_token: CancellationToken::new(format!("native-publication-cancel-{suffix}"))
            .unwrap(),
    }
}

fn limits() -> ProjectRenderLimits {
    ProjectRenderLimits {
        encode: deadpan_cli::encoded_render::EncodedWorkerLimits::default(),
        verification: deadpan_cli::encoded_render::verification::VerificationLimits::default(),
        media: deadpan_store::render_media::RenderMediaLimits::new(
            64 * 1024 * 1024,
            256 * 1024,
            64 * 1024 * 1024 + 256 * 1024,
            128 * 1024 * 1024,
            1024,
        )
        .unwrap(),
    }
}

fn start(
    workspace: &Workspace,
    destination: &Path,
    suffix: &str,
    ticket: u64,
) -> ProjectRenderRequest {
    ProjectRenderRequest {
        ticket,
        context: context(workspace),
        operation: ProjectRenderOperation::Start {
            request: StartRender {
                revision: workspace.document.revision_id().clone(),
                range: None,
                identity: identity(suffix),
                policy: RenderEngineeringPolicy {
                    schema_version: 1,
                    selection: RenderSelection::ExplicitEngineering,
                    encoder: RenderEncoder::Hardware,
                    b_frames: RenderBFrames::None,
                }
                .into(),
                publication: publication(destination, suffix),
                deadline: Instant::now() + TIMEOUT,
            },
            limits: limits(),
        },
    }
}

fn request(service: &ProjectService, request: ProjectRenderRequest) -> ProjectUpdate {
    let ticket = request.ticket;
    let captured_context = request.context.clone();
    service.submit(ProjectRequest::Render(request)).unwrap();
    let update = wait(service, |update| {
        !service.is_busy()
            && update
                .render
                .as_ref()
                .and_then(|render| render.command.as_ref())
                .is_some_and(|outcome| outcome.ticket == ticket)
    });
    assert_eq!(outcome(&update).context, captured_context);
    update
}

fn cancel(
    service: &ProjectService,
    workspace: &Workspace,
    run: WorkflowIdentity,
    ticket: u64,
) -> ProjectUpdate {
    request(
        service,
        ProjectRenderRequest {
            ticket,
            context: context(workspace),
            operation: ProjectRenderOperation::Cancel(run),
        },
    )
}

fn pause(harness: &Harness, paused: bool) {
    harness
        .service
        .shared
        .render_poll_paused
        .store(paused, Ordering::Release);
}

fn opened(harness: &Harness, path: &Path) -> Arc<Workspace> {
    drop(seed_holds(path, &["a"]));
    command(&harness.service, ProjectRequest::Open(path.into()))
        .workspace
        .unwrap()
}

fn outcome(update: &ProjectUpdate) -> &ProjectRenderCommandOutcome {
    update.render.as_ref().unwrap().command.as_ref().unwrap()
}

fn finished(service: &ProjectService) -> ProjectUpdate {
    wait(service, |update| {
        update
            .render
            .as_ref()
            .and_then(|render| render.workflow.as_ref())
            .is_some_and(|workflow| workflow.status.stage == WorkflowStage::Finished)
    })
}

fn preview_gain(trim: i32) -> ProjectEdit {
    ProjectEdit::SetAudioTreatments {
        node: node("a"),
        treatments: deadpan_core::AudioTreatments::from_clip_gain(
            deadpan_core::ClipGain::new(
                deadpan_core::GainDb::new(trim).unwrap(),
                false,
                Vec::new(),
                Vec::new(),
            )
            .unwrap(),
        ),
    }
}

fn commit_and_start(
    workspace: &Workspace,
    destination: &Path,
    suffix: &str,
    ticket: u64,
    edit: ProjectEdit,
) -> ProjectRenderRequest {
    let mut render = start(workspace, destination, suffix, ticket);
    let ProjectRenderOperation::Start { request, limits } = render.operation else {
        unreachable!("start helper")
    };
    render.operation = ProjectRenderOperation::CommitAndStart {
        edit: Box::new(edit),
        cursor: ProjectFrame(4),
        scope: SequenceScope::default(),
        request,
        limits,
    };
    render
}

#[test]
fn preview_commit_renders_its_exact_receipt_and_busy_render_does_not_commit() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::new();
    let initial = opened(&harness, &scratch.path().join("preview-render.deadpan"));
    pause(&harness, true);
    let accepted = request(
        &harness.service,
        commit_and_start(&initial, scratch.path(), "preview", 1, preview_gain(-3000)),
    );
    assert!(outcome(&accepted).result.is_ok());
    let revision = outcome(&accepted).committed_revision.clone().unwrap();
    assert_ne!(&revision, initial.document.revision_id());
    assert_eq!(accepted.committed.as_ref().unwrap().revision, revision);
    assert_eq!(
        accepted.committed.as_ref().unwrap().cursor,
        Some(ProjectFrame(4))
    );
    let committed = accepted.workspace.unwrap();
    assert_eq!(committed.document.revision_id(), &revision);
    assert_eq!(
        accepted.render.unwrap().workflow.unwrap().revision,
        revision
    );
    let busy = request(
        &harness.service,
        commit_and_start(
            &committed,
            scratch.path(),
            "busy-preview",
            2,
            preview_gain(-6000),
        ),
    );
    assert_eq!(
        outcome(&busy).result.as_ref().unwrap_err().code,
        "RenderBusy"
    );
    assert!(outcome(&busy).committed_revision.is_none());
    assert_eq!(*busy.workspace.unwrap().document, *committed.document);

    // The render was admitted before this subsequent command could run, and
    // later authored edits cannot replace its acknowledged revision.
    let later = edited(&harness.service, &committed, preview_gain(-6000));
    let later = later.workspace.unwrap();
    cancel(&harness.service, &later, identity("preview"), 3);
    pause(&harness, false);
    let terminal = finished(&harness.service);
    assert_eq!(
        terminal
            .render
            .as_ref()
            .unwrap()
            .workflow
            .as_ref()
            .unwrap()
            .revision,
        revision
    );
    assert_eq!(*terminal.workspace.unwrap().document, *later.document);
}

#[test]
fn preview_render_rejects_wrong_context_revision_scope_cursor_and_edit_without_commit() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::new();
    let initial = opened(&harness, &scratch.path().join("preview-rejections.deadpan"));
    for (case, code) in [
        ("ticket", "RenderInvalidRequest"),
        ("session", "RenderContextChanged"),
        ("project", "RenderContextChanged"),
        ("revision", "RenderRevisionChanged"),
        ("scope", "RenderPreviewCommitFailed"),
        ("cursor", "RenderPreviewCommitFailed"),
        ("node", "RenderPreviewCommitFailed"),
        ("structural", "RenderInvalidRequest"),
    ] {
        let mut captured = commit_and_start(&initial, scratch.path(), case, 1, preview_gain(-3000));
        match case {
            "ticket" => captured.ticket = 0,
            "session" => captured.context.session += 1,
            "project" => captured.context.project = ProjectId::new("foreign").unwrap(),
            _ => {
                let ProjectRenderOperation::CommitAndStart {
                    edit,
                    cursor,
                    scope,
                    request,
                    ..
                } = &mut captured.operation
                else {
                    unreachable!()
                };
                match case {
                    "revision" => request.revision = RevisionId::new("stale").unwrap(),
                    "scope" => *scope = SequenceScope::test_path(vec![node("a")]),
                    "cursor" => *cursor = ProjectFrame(11),
                    "node" => {
                        **edit = ProjectEdit::SetFraming {
                            node: node("missing"),
                            framing: None,
                        }
                    }
                    "structural" => {
                        **edit = ProjectEdit::Repeat {
                            node: node("a"),
                            plays: 3,
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
        let rejected = request(&harness.service, captured);
        assert_eq!(
            outcome(&rejected).result.as_ref().unwrap_err().code,
            code,
            "{case}"
        );
        assert!(outcome(&rejected).committed_revision.is_none(), "{case}");
        assert!(
            rejected.render.as_ref().unwrap().workflow.is_none(),
            "{case}"
        );
        assert_eq!(
            *rejected.workspace.unwrap().document,
            *initial.document,
            "{case}"
        );
    }
}

#[test]
fn unchanged_preview_cannot_reuse_an_earlier_commit_receipt() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::new();
    let initial = opened(&harness, &scratch.path().join("preview-unchanged.deadpan"));
    let earlier = edited(&harness.service, &initial, preview_gain(-3000));
    assert!(earlier.committed.is_some());
    let current = earlier.workspace.unwrap();
    let unchanged = request(
        &harness.service,
        commit_and_start(
            &current,
            scratch.path(),
            "unchanged",
            1,
            preview_gain(-3000),
        ),
    );
    assert_eq!(
        outcome(&unchanged).result.as_ref().unwrap_err().code,
        "RenderPreviewUnchanged"
    );
    assert!(outcome(&unchanged).committed_revision.is_none());
    assert!(unchanged.committed.is_none());
    assert!(unchanged.render.as_ref().unwrap().workflow.is_none());
    assert_eq!(*unchanged.workspace.unwrap().document, *current.document);

    for edit in [
        ProjectEdit::SetFraming {
            node: node("a"),
            framing: None,
        },
        ProjectEdit::HoldAudio {
            node: node("a"),
            audio: HoldAudio::Silence,
        },
    ] {
        let unchanged = request(
            &harness.service,
            commit_and_start(&current, scratch.path(), "unchanged-recipe", 3, edit),
        );
        assert_eq!(
            outcome(&unchanged).result.as_ref().unwrap_err().code,
            "RenderPreviewUnchanged"
        );
        assert!(outcome(&unchanged).committed_revision.is_none());
        assert!(unchanged.render.as_ref().unwrap().workflow.is_none());
        assert_eq!(*unchanged.workspace.unwrap().document, *current.document);
    }

    // A later error cannot manufacture a receipt from the matching target or
    // from the already committed workspace either.
    let mut invalid = commit_and_start(&current, scratch.path(), "invalid", 2, preview_gain(-6000));
    if let ProjectRenderOperation::CommitAndStart { cursor, .. } = &mut invalid.operation {
        *cursor = ProjectFrame(-1);
    }
    let rejected = request(&harness.service, invalid);
    assert!(outcome(&rejected).committed_revision.is_none());
    assert!(outcome(&rejected).result.is_err());
    assert_eq!(*rejected.workspace.unwrap().document, *current.document);
}

#[test]
fn render_admission_failure_reports_and_preserves_the_preview_commit() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("preview-admission-failure.deadpan");
    let harness = Harness::new();
    let initial = opened(&harness, &path);
    let mut captured = commit_and_start(
        &initial,
        scratch.path(),
        "bad-limits",
        1,
        preview_gain(-3000),
    );
    if let ProjectRenderOperation::CommitAndStart { limits, .. } = &mut captured.operation {
        limits.verification.maximum_packets = 0;
    }
    let rejected = request(&harness.service, captured);
    assert_eq!(
        outcome(&rejected).result.as_ref().unwrap_err().code,
        "RenderInvalidRequest"
    );
    let revision = outcome(&rejected).committed_revision.clone().unwrap();
    let committed = rejected.workspace.unwrap();
    assert_eq!(committed.document.revision_id(), &revision);
    assert!(rejected.render.unwrap().workflow.is_none());
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly).unwrap();
    assert_eq!(reader.snapshot().unwrap().revision_id(), &revision);
    assert!(reader.render_job(&identity("bad-limits").job_id).is_err());
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: revision,
        },
    );
    assert_eq!(
        undone.workspace.unwrap().document.nodes(),
        initial.document.nodes()
    );
}

#[test]
fn refresh_failure_keeps_the_durable_preview_receipt_without_starting_render() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("preview-refresh-failure.deadpan");
    let harness = Harness::new();
    let initial = opened(&harness, &path);
    harness
        .service
        .shared
        .render_commit_refresh_failure
        .store(true, Ordering::Release);
    let rejected = request(
        &harness.service,
        commit_and_start(
            &initial,
            scratch.path(),
            "refresh-failure",
            1,
            preview_gain(-3000),
        ),
    );
    assert_eq!(
        outcome(&rejected).result.as_ref().unwrap_err().code,
        "RenderPreviewCommitFailed"
    );
    let revision = outcome(&rejected).committed_revision.clone().unwrap();
    assert_ne!(&revision, initial.document.revision_id());
    assert_eq!(rejected.committed.as_ref().unwrap().revision, revision);
    // The old workspace deliberately failed to refresh. It is not used as the
    // source of the acknowledged revision and no workflow was started from it.
    assert_eq!(*rejected.workspace.unwrap().document, *initial.document);
    assert!(rejected.render.unwrap().workflow.is_none());
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly).unwrap();
    let durable = reader.snapshot().unwrap();
    assert_eq!(durable.revision_id(), &revision);
    assert_ne!(
        durable.nodes()[&node("a")],
        initial.document.nodes()[&node("a")]
    );
    assert!(
        reader
            .render_job(&identity("refresh-failure").job_id)
            .is_err()
    );
}

#[test]
fn render_capture_pending_does_not_block_edits_or_retarget_its_revision() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::new();
    let initial = opened(&harness, &scratch.path().join("render-edit.deadpan"));
    // The production coordinator performs capture. Only owner admission of its
    // result is held, so this test makes no encoder-throughput claim.
    pause(&harness, true);
    let accepted = request(&harness.service, start(&initial, scratch.path(), "edit", 1));
    assert!(outcome(&accepted).result.is_ok());
    assert!(!harness.service.is_busy());
    let edited = edited(
        &harness.service,
        &initial,
        ProjectEdit::Repeat {
            node: node("a"),
            plays: 3,
        },
    );
    let marker = edited.committed.clone().unwrap();
    let current = edited.workspace.unwrap();
    let wrong_cancel = cancel(&harness.service, &current, identity("foreign"), 2);
    assert_eq!(
        outcome(&wrong_cancel).result.as_ref().unwrap_err().code,
        "RenderIdentityChanged"
    );
    assert_eq!(wrong_cancel.committed, Some(marker));
    let error = command(
        &harness.service,
        ProjectRequest::Open(scratch.path().join("missing.deadpan")),
    );
    let editor_error = error.error.unwrap();
    assert!(
        !error
            .render
            .as_ref()
            .unwrap()
            .workflow
            .as_ref()
            .unwrap()
            .status
            .cancellation_requested
    );
    let cancelled = cancel(&harness.service, &current, identity("edit"), 3);
    assert!(outcome(&cancelled).result.is_ok());
    assert_eq!(cancelled.error.as_deref(), Some(editor_error.as_str()));
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: current.document.revision_id().clone(),
        },
    );
    let undone = undone.workspace.unwrap();
    assert_eq!(undone.document.duration().unwrap().frames(), 10);
    pause(&harness, false);
    let terminal = finished(&harness.service);
    let render = terminal.render.as_ref().unwrap();
    let workflow = render.workflow.as_ref().unwrap();
    assert_eq!(workflow.revision, *initial.document.revision_id());
    assert_eq!(workflow.status.outcome, Some(WorkflowOutcome::Cancelled));
    assert!(workflow.status.cleanup_confirmed);
    assert_eq!(
        *terminal.workspace.as_ref().unwrap().document,
        *undone.document
    );
    let later = command(
        &harness.service,
        ProjectRequest::Redo {
            expected_revision: undone.document.revision_id().clone(),
        },
    );
    assert_eq!(
        later.render.unwrap().workflow.unwrap().status.outcome,
        Some(WorkflowOutcome::Cancelled)
    );
}

#[test]
fn native_render_admission_preserves_proposals_and_rejects_stale_targets() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::new();
    let initial = opened(&harness, &scratch.path().join("render-admission.deadpan"));
    let proposal = gain::Proposal {
        target: gain::Target {
            session: initial.session,
            project: initial.document.project_id().clone(),
            revision: initial.document.revision_id().clone(),
            scope: SequenceScope::default(),
            node: node("a"),
            cursor: ProjectFrame(0),
            entry: initial.document.nodes()[&node("a")]
                .audio_treatments
                .clone(),
        },
        draft: 1,
        change: 1,
        treatments: initial.document.nodes()[&node("a")]
            .audio_treatments
            .clone(),
    };
    let proposal_id = proposal.id();
    let prepared = command(&harness.service, ProjectRequest::PrepareGain(proposal));
    assert!(prepared.gain.as_ref().unwrap().result.is_ok());
    let mut stale = start(&initial, scratch.path(), "stale", 1);
    stale.context.session += 1;
    let rejected = request(&harness.service, stale);
    assert_eq!(
        outcome(&rejected).result.as_ref().unwrap_err().code,
        "RenderContextChanged"
    );
    assert_eq!(rejected.gain.as_ref().unwrap().id, proposal_id);
    let mut stale = start(&initial, scratch.path(), "stale-revision", 2);
    if let ProjectRenderOperation::Start { request, .. } = &mut stale.operation {
        request.revision = RevisionId::new("never-current").unwrap();
    }
    let rejected = request(&harness.service, stale);
    assert_eq!(
        outcome(&rejected).result.as_ref().unwrap_err().code,
        "RenderRevisionChanged"
    );
    pause(&harness, true);
    let accepted = request(
        &harness.service,
        start(&initial, scratch.path(), "admission", 3),
    );
    assert!(outcome(&accepted).result.is_ok());
    assert_eq!(accepted.gain.as_ref().unwrap().id, proposal_id);
    let occupied = request(
        &harness.service,
        start(&initial, scratch.path(), "second", 4),
    );
    assert_eq!(
        outcome(&occupied).result.as_ref().unwrap_err().code,
        "RenderBusy"
    );
    assert_eq!(
        occupied
            .render
            .as_ref()
            .unwrap()
            .workflow
            .as_ref()
            .unwrap()
            .status
            .identity,
        Some(identity("admission"))
    );
    cancel(&harness.service, &initial, identity("admission"), 5);
    pause(&harness, false);
    finished(&harness.service);
    let mut foreign = context(&initial);
    foreign.project = ProjectId::new("other-project").unwrap();
    let retry = request(
        &harness.service,
        ProjectRenderRequest {
            ticket: 6,
            context: foreign.clone(),
            operation: ProjectRenderOperation::Retry {
                request: RetryRender {
                    identity: identity("retry"),
                    checkpoint_attempt_id: None,
                    publication: publication(scratch.path(), "retry"),
                    deadline: Instant::now() + TIMEOUT,
                },
                limits: limits(),
            },
        },
    );
    assert_eq!(
        outcome(&retry).result.as_ref().unwrap_err().code,
        "RenderContextChanged"
    );
    let reconcile = request(
        &harness.service,
        ProjectRenderRequest {
            ticket: 7,
            context: foreign,
            operation: ProjectRenderOperation::Reconcile {
                request: ReconcileRender {
                    publication_id: RequestId::new("unknown-publication").unwrap(),
                    identity: identity("reconcile"),
                    operation_id: AttemptId::new("reconcile-op").unwrap(),
                    cancellation_token: CancellationToken::new("reconcile-cancel").unwrap(),
                    deadline: Instant::now() + TIMEOUT,
                },
                limits: limits(),
            },
        },
    );
    assert_eq!(
        outcome(&reconcile).result.as_ref().unwrap_err().code,
        "RenderContextChanged"
    );
}

#[test]
fn close_retains_the_render_writer_until_checked_result_admission() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("render-close.deadpan");
    let harness = Harness::new();
    let initial = opened(&harness, &path);
    pause(&harness, true);
    request(
        &harness.service,
        start(&initial, scratch.path(), "close", 1),
    );
    harness.service.submit(ProjectRequest::Close).unwrap();
    let closing = wait(&harness.service, |update| {
        update
            .render
            .as_ref()
            .unwrap()
            .workflow
            .as_ref()
            .unwrap()
            .status
            .cancellation_requested
    });
    assert!(harness.service.is_busy());
    assert_eq!(closing.workspace.as_ref().unwrap().session, initial.session);
    assert!(ProjectStore::open(&path, AccessMode::ReadWrite).is_err());
    pause(&harness, false);
    let closed = wait(&harness.service, |update| {
        update.workspace.is_none() && !harness.service.is_busy()
    });
    assert_eq!(
        closed.render.unwrap().workflow.unwrap().status.outcome,
        Some(WorkflowOutcome::Cancelled)
    );
    let writer = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
    assert_eq!(writer.snapshot().unwrap(), *initial.document);
}

#[test]
fn open_validates_before_cancelling_and_switches_only_after_render_release() {
    let scratch = tempfile::tempdir().unwrap();
    let old_path = scratch.path().join("render-old.deadpan");
    let new_path = scratch.path().join("render-new.deadpan");
    drop(seed_holds(&new_path, &["b"]));
    let harness = Harness::new();
    let initial = opened(&harness, &old_path);
    pause(&harness, true);
    request(
        &harness.service,
        start(&initial, scratch.path(), "switch", 1),
    );
    let invalid = command(
        &harness.service,
        ProjectRequest::Open(scratch.path().join("missing.deadpan")),
    );
    assert!(invalid.error.is_some());
    assert!(
        !invalid
            .render
            .as_ref()
            .unwrap()
            .workflow
            .as_ref()
            .unwrap()
            .status
            .cancellation_requested
    );
    assert_eq!(invalid.workspace.unwrap().session, initial.session);
    harness
        .service
        .submit(ProjectRequest::Open(new_path.clone()))
        .unwrap();
    let pending = wait(&harness.service, |update| {
        update
            .render
            .as_ref()
            .unwrap()
            .workflow
            .as_ref()
            .unwrap()
            .status
            .cancellation_requested
    });
    assert_eq!(pending.workspace.unwrap().session, initial.session);
    assert!(ProjectStore::open(&old_path, AccessMode::ReadWrite).is_err());
    pause(&harness, false);
    let switched = wait(&harness.service, |update| {
        update
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.session != initial.session)
            && !harness.service.is_busy()
    });
    let current = switched.workspace.unwrap();
    assert_eq!(current.path, new_path.canonicalize().unwrap());
    assert_eq!(current.session, initial.session + 1);
    assert_eq!(
        switched.render.unwrap().workflow.unwrap().context,
        context(&initial)
    );
    assert!(ProjectStore::open(&old_path, AccessMode::ReadWrite).is_ok());
    let stale = cancel(&harness.service, &initial, identity("switch"), 2);
    assert_eq!(
        outcome(&stale).result.as_ref().unwrap_err().code,
        "RenderContextChanged"
    );
    assert_eq!(stale.workspace.unwrap().session, current.session);
}

#[test]
fn shutdown_finishes_admitted_edit_and_pumps_render_before_releasing_writer() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("render-shutdown.deadpan");
    let harness = Harness::new();
    let initial = opened(&harness, &path);
    pause(&harness, true);
    request(
        &harness.service,
        start(&initial, scratch.path(), "shutdown", 1),
    );
    harness
        .service
        .submit(edit_request(
            &initial,
            ProjectEdit::Repeat {
                node: node("a"),
                plays: 2,
            },
        ))
        .unwrap();
    harness.service.shutdown();
    assert!(harness.service.submit(ProjectRequest::Close).is_err());
    let stopping = wait(&harness.service, |update| {
        update
            .render
            .as_ref()
            .unwrap()
            .workflow
            .as_ref()
            .unwrap()
            .status
            .cancellation_requested
            && update.committed.is_some()
    });
    assert_eq!(
        stopping
            .workspace
            .as_ref()
            .unwrap()
            .document
            .duration()
            .unwrap()
            .frames(),
        20
    );
    assert!(!harness.service.is_shutdown_complete());
    assert!(ProjectStore::open(&path, AccessMode::ReadWrite).is_err());
    pause(&harness, false);
    let deadline = Instant::now() + TIMEOUT;
    while !harness.service.is_shutdown_complete() {
        assert!(
            Instant::now() < deadline,
            "render shutdown did not complete"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    let final_update = harness.service.take_update().unwrap();
    assert_eq!(
        final_update
            .workspace
            .as_ref()
            .unwrap()
            .document
            .duration()
            .unwrap()
            .frames(),
        20
    );
    assert_eq!(
        final_update
            .render
            .unwrap()
            .workflow
            .unwrap()
            .status
            .outcome,
        Some(WorkflowOutcome::Cancelled)
    );
    let writer = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
    assert_eq!(writer.snapshot().unwrap().duration().unwrap().frames(), 20);
}
