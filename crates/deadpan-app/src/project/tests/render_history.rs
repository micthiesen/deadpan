//! Real service/store tests. The retained checkpoint is deliberately opaque:
//! these fixtures exercise recovery identity and history, not media validity.

use deadpan_cli::encoded_render::workflow::{WorkflowIdentity, WorkflowStage};
use deadpan_core::{FrameRange, PresentationBasis};
use deadpan_jobs::{
    AttemptId, CancellationToken, RequestId,
    render::{
        RenderAttemptState, RenderAutomaticAlgorithm, RenderAutomaticPolicy,
        RenderAutomaticSelection, RenderBFrames, RenderDiagnostic, RenderEncoder,
        RenderEngineeringPolicy, RenderIntent, RenderPolicy, RenderSelection,
        RenderVerificationObservation,
        admission::RenderEncodingDecision,
        publication::{
            PreparedPublicationEvidence, PublicationCompletion, PublicationIntent,
            PublicationOutcome, PublicationPhase,
        },
    },
};
use deadpan_store::{
    render_jobs::{BeginRenderAttempt, RenderAttemptTransition, StoredRenderAttempt},
    render_media::RenderMediaLimits,
};
use sha2::{Digest, Sha256};

use super::*;
use crate::project::render_history::{Page, Query, Recovery, Request, Update};

fn context(workspace: &Workspace) -> ProjectRenderContext {
    ProjectRenderContext {
        session: workspace.session,
        project: workspace.document.project_id().clone(),
    }
}

fn measured() -> RenderEncodingDecision {
    RenderEncodingDecision::from_json(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../deadpan-jobs/src/render/admission/tests/measured-decision-v1.json"
    )))
    .unwrap()
}

fn seed_store(path: &Path) -> ProjectStore {
    let output = measured().output;
    let document = ProjectDocument::new(
        ProjectId::new("render-history-project").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: output.canvas[0],
            height: output.canvas[1],
            frame_rate: output.frame_rate,
            color_policy: output.color_policy,
        },
        node("root"),
    )
    .unwrap();
    let mut store = ProjectStore::create(path, &document).unwrap();
    seed_command(
        &mut store,
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("a"),
                nodes: BTreeMap::from([(
                    node("a"),
                    BeatNode::hold(
                        "saved picture",
                        hold(i64::try_from(output.frame_count).unwrap()),
                    ),
                )]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
        "saved-revision",
    );
    store
}

fn job(store: &mut ProjectStore, name: &str, automatic: bool) -> RenderIntent {
    let document = store.snapshot().unwrap();
    let policy = if automatic {
        RenderPolicy::Automatic(RenderAutomaticPolicy {
            schema_version: 1,
            selection: RenderAutomaticSelection::Automatic,
            algorithm: RenderAutomaticAlgorithm::AutomaticSdrV1,
        })
    } else {
        RenderEngineeringPolicy {
            schema_version: 1,
            selection: RenderSelection::ExplicitEngineering,
            encoder: RenderEncoder::Software,
            b_frames: RenderBFrames::None,
        }
        .into()
    };
    store
        .create_render_job(
            RenderIntent {
                schema_version: if automatic { 2 } else { 1 },
                job_id: RequestId::new(name).unwrap(),
                project_id: document.project_id().clone(),
                revision_id: document.revision_id().clone(),
                document_sha256: deadpan_jobs::render::document_sha256_for_validation(&document)
                    .unwrap(),
                range: FrameRange::new(ProjectFrame(0), ProjectFrame(128)).unwrap(),
                policy,
            },
            &AtomicBool::new(false),
            Instant::now() + TIMEOUT,
        )
        .unwrap()
}

fn attempt(store: &mut ProjectStore, job: &RenderIntent, ordinal: u64) -> StoredRenderAttempt {
    store
        .begin_render_attempt(BeginRenderAttempt {
            job_id: job.job_id.clone(),
            attempt_id: AttemptId::new(format!("{}-{ordinal}", job.job_id.as_str())).unwrap(),
            cancellation_token: CancellationToken::new(format!(
                "{}-cancel-{ordinal}",
                job.job_id.as_str()
            ))
            .unwrap(),
            checkpoint_attempt_id: None,
        })
        .unwrap()
}

fn diagnostic() -> RenderDiagnostic {
    RenderDiagnostic {
        code: "Fixture".into(),
        detail: "Controlled operational history fixture".into(),
    }
}

fn verified(store: &mut ProjectStore, job: &RenderIntent) -> StoredRenderAttempt {
    let queued = attempt(store, job, 1);
    let mut decision = measured();
    decision.job_id = job.job_id.clone();
    decision.encoding_attempt_id = queued.attempt_id.clone();
    decision.document_sha256 = job.document_sha256.clone();
    decision.output.project_id = job.project_id.clone();
    decision.output.revision_id = job.revision_id.clone();
    decision.output.range = job.range;
    for probe in &mut decision.probes {
        probe.identity.request_id = job.job_id.clone();
    }
    let encoding = store
        .begin_render_encoding(&queued.identity(), &decision)
        .unwrap();
    let movie = b"opaque recovery fixture, not a playable movie";
    let hash = deadpan_jobs::Sha256::new(
        Sha256::digest(movie)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
    )
    .unwrap();
    let retained = store
        .render_write_handle()
        .unwrap()
        .prepare_retention(
            &encoding.identity(),
            &mut &movie[..],
            u64::try_from(movie.len()).unwrap(),
            &hash,
            b"{\"opaque_fixture\":true}",
            RenderMediaLimits::new(1024, 1024, 2048, 8192, 16).unwrap(),
            &AtomicBool::new(false),
            Instant::now() + TIMEOUT,
        )
        .unwrap();
    let retained = store
        .retain_render_checkpoint(
            &encoding.identity(),
            &retained,
            &AtomicBool::new(false),
            Instant::now() + TIMEOUT,
        )
        .unwrap();
    let verifying = store
        .transition_render_attempt(&retained.identity(), RenderAttemptTransition::Verifying)
        .unwrap();
    store
        .record_render_verification(
            &verifying.identity(),
            RenderVerificationObservation {
                schema_version: 1,
                validator_id: "history-fixture".into(),
                validator_version: "1".into(),
                movie_sha256: hash,
                movie_byte_length: u64::try_from(movie.len()).unwrap(),
                report: serde_json::json!({"fixture": "x".repeat(64 * 1024)}),
            },
        )
        .unwrap()
}

fn publication(store: &mut ProjectStore, attempt: &StoredRenderAttempt, id: &str, path: &Path) {
    let permit = store
        .begin_render_publication(
            PublicationIntent {
                schema_version: 1,
                publication_id: RequestId::new(id).unwrap(),
                job_id: attempt.job_id.clone(),
                verified_attempt_id: attempt.attempt_id.clone(),
                destination: path.join(format!("{id}.mp4")),
            },
            AttemptId::new(format!("{id}-operation")).unwrap(),
            CancellationToken::new(format!("{id}-cancellation")).unwrap(),
        )
        .unwrap();
    if id != "publication-00" {
        return;
    }
    let evidence = PreparedPublicationEvidence {
        schema_version: 1,
        movie_sha256: permit.record().movie_sha256.clone(),
        movie_bytes: permit.record().movie_bytes,
        report_sha256: deadpan_jobs::Sha256::new("a".repeat(64)).unwrap(),
        report_bytes: 10,
        contains_generated_pictures: false,
        filesystem: serde_json::json!({"opaque_fixture":true}),
    };
    let mut permit = store
        .record_prepared_publication(&permit.identity(), evidence)
        .unwrap();
    for phase in [
        PublicationPhase::ReportCommitting,
        PublicationPhase::ReportCommitted,
        PublicationPhase::MovieCommitting,
    ] {
        permit = store
            .advance_publication(&permit.identity(), phase)
            .unwrap();
    }
    store
        .finish_publication(
            &permit.identity(),
            PublicationCompletion::PublishedUnconfirmed(diagnostic()),
        )
        .unwrap();
}

fn opened(service: &ProjectService, path: &Path) -> Arc<Workspace> {
    let update = command(service, ProjectRequest::Open(path.into()));
    assert!(update.error.is_none(), "{:?}", update.error);
    update.workspace.unwrap()
}

fn history(
    service: &ProjectService,
    context: ProjectRenderContext,
    ticket: u64,
    query: Query,
) -> ProjectUpdate {
    service
        .submit(ProjectRequest::RenderHistory(Request {
            ticket,
            context: context.clone(),
            query: query.clone(),
        }))
        .unwrap();
    let update = wait(service, |update| {
        !service.is_busy()
            && update.render_history.as_ref().is_some_and(|result| {
                result.ticket == ticket && result.context == context && result.query == query
            })
    });
    assert!(update.render_history.is_some());
    update
}

fn response(update: &ProjectUpdate) -> &Update {
    update.render_history.as_ref().unwrap()
}

fn recovery(
    service: &ProjectService,
    context: ProjectRenderContext,
    ticket: u64,
    operation: Recovery,
) -> ProjectUpdate {
    service
        .submit(ProjectRequest::Render(ProjectRenderRequest {
            ticket,
            context: context.clone(),
            operation: ProjectRenderOperation::Recover(operation),
        }))
        .unwrap();
    wait(service, |update| {
        !service.is_busy()
            && update
                .render
                .as_ref()
                .and_then(|render| render.command.as_ref())
                .is_some_and(|outcome| outcome.ticket == ticket && outcome.context == context)
    })
}

fn cancel_and_drain(
    service: &ProjectService,
    context: ProjectRenderContext,
    identity: WorkflowIdentity,
    ticket: u64,
) {
    let cancelled = command(
        service,
        ProjectRequest::Render(ProjectRenderRequest {
            ticket,
            context,
            operation: ProjectRenderOperation::Cancel(identity),
        }),
    );
    assert!(
        cancelled
            .render
            .as_ref()
            .unwrap()
            .command
            .as_ref()
            .unwrap()
            .result
            .is_ok()
    );
    service
        .shared
        .render_poll_paused
        .store(false, Ordering::Release);
    wait(service, |update| {
        update
            .render
            .as_ref()
            .and_then(|render| render.workflow.as_ref())
            .is_some_and(|workflow| {
                workflow.status.stage == WorkflowStage::Finished
                    && workflow.status.cleanup_confirmed
            })
    });
}

#[test]
fn bounded_history_pages_survive_reopen_and_preserve_publication_commit_knowledge() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("history.deadpan");
    let mut store = seed_store(&path);
    let first = job(&mut store, "job-00", true);
    let original = verified(&mut store, &first);
    for index in 1..9 {
        job(&mut store, &format!("job-{index:02}"), true);
    }
    for index in 0..9 {
        publication(
            &mut store,
            &original,
            &format!("publication-{index:02}"),
            scratch.path(),
        );
    }
    for ordinal in 2..10 {
        let pending = attempt(&mut store, &first, ordinal);
        store
            .transition_render_attempt(
                &pending.identity(),
                RenderAttemptTransition::Failed(diagnostic()),
            )
            .unwrap();
    }
    attempt(&mut store, &first, 10);
    drop(store);
    let harness = Harness::new();
    let workspace = opened(&harness.service, &path);
    let initial = history(
        &harness.service,
        context(&workspace),
        1,
        Query::Jobs { after: None },
    );
    let Page::Jobs { items, next_after } = response(&initial).result.as_ref().unwrap() else {
        panic!("jobs page")
    };
    assert_eq!(items.len(), 8);
    assert_eq!(next_after.as_ref().unwrap().as_str(), "job-07");
    assert!(
        items
            .iter()
            .all(|job| job.revision_id == first.revision_id && job.automatic)
    );
    let next = history(
        &harness.service,
        context(&workspace),
        2,
        Query::Jobs {
            after: next_after.clone(),
        },
    );
    assert!(
        matches!(response(&next).result.as_ref().unwrap(), Page::Jobs { items, next_after: None } if items.len() == 1)
    );

    let attempts = history(
        &harness.service,
        context(&workspace),
        3,
        Query::Attempts {
            job: first.job_id.clone(),
            after_ordinal: 0,
        },
    );
    let Page::Attempts {
        items,
        next_after_ordinal,
        ..
    } = response(&attempts).result.as_ref().unwrap()
    else {
        panic!("attempt page")
    };
    assert_eq!(items.len(), 8);
    assert_eq!(*next_after_ordinal, Some(8));
    assert!(items[0].verification_recorded);
    assert_eq!(
        items[0].checkpoint_attempt_id,
        Some(original.attempt_id.clone())
    );
    let next = history(
        &harness.service,
        context(&workspace),
        4,
        Query::Attempts {
            job: first.job_id,
            after_ordinal: 8,
        },
    );
    let Page::Attempts {
        items,
        next_after_ordinal,
        ..
    } = response(&next).result.as_ref().unwrap()
    else {
        panic!("attempt page")
    };
    assert_eq!(items.len(), 2);
    assert_eq!(*next_after_ordinal, None);
    assert_eq!(items[1].state, RenderAttemptState::Interrupted);

    let destinations = history(
        &harness.service,
        context(&workspace),
        5,
        Query::Publications { after: None },
    );
    let Page::Publications { items, next_after } = response(&destinations).result.as_ref().unwrap()
    else {
        panic!("publication page")
    };
    assert_eq!(items.len(), 8);
    assert_eq!(items[0].outcome, PublicationOutcome::PublishedUnconfirmed);
    assert!(items[0].observed_movie_commit && items[0].can_reconcile());
    assert_eq!(items[1].outcome, PublicationOutcome::Interrupted);
    assert!(items[1].can_reconcile());
    let next = history(
        &harness.service,
        context(&workspace),
        6,
        Query::Publications {
            after: next_after.clone(),
        },
    );
    assert!(
        matches!(response(&next).result.as_ref().unwrap(), Page::Publications { items, next_after: None } if items.len() == 1)
    );
    command(&harness.service, ProjectRequest::Close);
}

#[test]
fn history_rejects_stale_sessions_and_invalid_queries_without_erasing_receipts() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("receipts.deadpan");
    let mut store = seed_store(&path);
    let saved = job(&mut store, "saved", true);
    drop(store);
    let harness = Harness::new();
    let workspace = opened(&harness.service, &path);
    let edit = edited(
        &harness.service,
        &workspace,
        ProjectEdit::HoldDuration {
            node: node("a"),
            duration: FrameDuration::new(130).unwrap(),
        },
    );
    let commit = edit.committed.unwrap();
    let workspace = edit.workspace.unwrap();
    let bad_cursor = history(
        &harness.service,
        context(&workspace),
        1,
        Query::Attempts {
            job: saved.job_id,
            after_ordinal: u64::MAX,
        },
    );
    assert_eq!(
        response(&bad_cursor).result.as_ref().unwrap_err().code,
        "RenderHistoryQueryFailed"
    );
    assert_eq!(bad_cursor.committed.as_ref(), Some(&commit));
    for (ticket, captured) in [
        (0, context(&workspace)),
        (
            2,
            ProjectRenderContext {
                session: workspace.session + 1,
                ..context(&workspace)
            },
        ),
        (
            3,
            ProjectRenderContext {
                project: ProjectId::new("foreign").unwrap(),
                ..context(&workspace)
            },
        ),
    ] {
        let rejected = history(
            &harness.service,
            captured,
            ticket,
            Query::Jobs { after: None },
        );
        assert_eq!(
            response(&rejected).result.as_ref().unwrap_err().code,
            if ticket == 0 {
                "RenderHistoryInvalidRequest"
            } else {
                "RenderContextChanged"
            }
        );
        assert_eq!(rejected.committed.as_ref(), Some(&commit));
    }
    let proposal = crate::project::gain::Proposal {
        target: crate::project::gain::Target {
            scoped: None,
            session: workspace.session,
            project: workspace.document.project_id().clone(),
            revision: workspace.document.revision_id().clone(),
            scope: SequenceScope::default(),
            node: node("a"),
            cursor: ProjectFrame(0),
            entry: workspace.document.nodes()[&node("a")]
                .audio_treatments
                .clone(),
        },
        draft: 1,
        change: 1,
        treatments: workspace.document.nodes()[&node("a")]
            .audio_treatments
            .clone(),
    };
    let gain = command(&harness.service, ProjectRequest::PrepareGain(proposal));
    let queried = history(
        &harness.service,
        context(&workspace),
        4,
        Query::Jobs { after: None },
    );
    assert_eq!(
        queried.gain.as_ref().unwrap().id,
        gain.gain.as_ref().unwrap().id
    );
    let room_tone = command(
        &harness.service,
        ProjectRequest::PrepareRoomTone {
            expected_session: workspace.session,
            expected_revision: workspace.document.revision_id().clone(),
            ticket: 5,
            selection: RoomToneSelection::Original {
                asset: AssetId::new("missing").unwrap(),
                qualification: SourceQualificationId::new("c".repeat(64)).unwrap(),
                ordinals: 0..1,
            },
        },
    );
    assert!(room_tone.room_tone_error.is_some());
    let queried = history(
        &harness.service,
        context(&workspace),
        6,
        Query::Jobs { after: None },
    );
    assert_eq!(queried.room_tone_error, room_tone.room_tone_error);
    let old_context = context(&workspace);
    command(&harness.service, ProjectRequest::Close);
    let reopened = opened(&harness.service, &path);
    let rejected = history(
        &harness.service,
        old_context,
        7,
        Query::Jobs { after: None },
    );
    assert_eq!(
        response(&rejected).result.as_ref().unwrap_err().code,
        "RenderContextChanged"
    );
    assert_ne!(reopened.session, workspace.session);
    command(&harness.service, ProjectRequest::Close);
}

#[test]
fn captured_recovery_targets_keep_historical_revision_and_fresh_attempts() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("recovery.deadpan");
    let mut store = seed_store(&path);
    let saved = job(&mut store, "saved", true);
    let original = verified(&mut store, &saved);
    publication(&mut store, &original, "publication-00", scratch.path());
    drop(store);
    let harness = Harness::new();
    let before = opened(&harness.service, &path);
    let edited = edited(
        &harness.service,
        &before,
        ProjectEdit::HoldDuration {
            node: node("a"),
            duration: FrameDuration::new(130).unwrap(),
        },
    );
    let workspace = edited.workspace.unwrap();
    assert_ne!(workspace.document.revision_id(), &saved.revision_id);
    for (index, checkpoint) in [Some(original.attempt_id.clone()), None]
        .into_iter()
        .enumerate()
    {
        harness
            .service
            .shared
            .render_poll_paused
            .store(true, Ordering::Release);
        let ticket = 10 + u64::try_from(index).unwrap() * 3;
        let accepted = recovery(
            &harness.service,
            context(&workspace),
            ticket,
            Recovery::Retry {
                job_id: saved.job_id.clone(),
                checkpoint_attempt_id: checkpoint.clone(),
                destination: scratch.path().join(format!("retry-{index}.mp4")),
            },
        );
        let render = accepted.render.as_ref().unwrap();
        let identity = render
            .command
            .as_ref()
            .unwrap()
            .result
            .as_ref()
            .unwrap()
            .clone();
        assert_eq!(identity.job_id, saved.job_id);
        assert_ne!(identity.attempt_id, original.attempt_id);
        let workflow = render.workflow.as_ref().unwrap();
        assert_eq!(workflow.revision, saved.revision_id);
        assert_eq!(workflow.status.intent.as_ref().unwrap(), &saved);
        assert_eq!(
            workflow
                .status
                .attempt
                .as_ref()
                .unwrap()
                .checkpoint_attempt_id,
            checkpoint
        );
        let queried = history(
            &harness.service,
            context(&workspace),
            ticket + 1,
            Query::Jobs { after: None },
        );
        assert_eq!(
            queried
                .render
                .as_ref()
                .unwrap()
                .command
                .as_ref()
                .unwrap()
                .ticket,
            ticket
        );
        assert_eq!(queried.committed, edited.committed);
        cancel_and_drain(&harness.service, context(&workspace), identity, ticket + 2);
    }
    harness
        .service
        .shared
        .render_poll_paused
        .store(true, Ordering::Release);
    let accepted = recovery(
        &harness.service,
        context(&workspace),
        20,
        Recovery::Reconcile {
            publication_id: RequestId::new("publication-00").unwrap(),
        },
    );
    let render = accepted.render.as_ref().unwrap();
    let identity = render
        .command
        .as_ref()
        .unwrap()
        .result
        .as_ref()
        .unwrap()
        .clone();
    let workflow = render.workflow.as_ref().unwrap();
    assert_eq!(workflow.revision, saved.revision_id);
    assert_eq!(
        workflow
            .status
            .attempt
            .as_ref()
            .unwrap()
            .checkpoint_attempt_id,
        Some(original.attempt_id)
    );
    assert_eq!(
        workflow
            .status
            .publication
            .as_ref()
            .unwrap()
            .intent
            .destination,
        scratch.path().join("publication-00.mp4")
    );
    assert!(workflow.status.observed_movie_commit);
    cancel_and_drain(&harness.service, context(&workspace), identity, 21);
    command(&harness.service, ProjectRequest::Close);
}

#[test]
fn recovery_rejects_reopened_session_and_missing_or_engineering_targets() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("guard.deadpan");
    let mut store = seed_store(&path);
    let saved = job(&mut store, "saved", true);
    let engineering = job(&mut store, "engineering", false);
    drop(store);
    let harness = Harness::new();
    let old = opened(&harness.service, &path);
    command(&harness.service, ProjectRequest::Close);
    let current = opened(&harness.service, &path);
    for (ticket, captured, operation, expected) in [
        (
            0,
            context(&current),
            Recovery::Retry {
                job_id: saved.job_id.clone(),
                checkpoint_attempt_id: None,
                destination: scratch.path().join("zero.mp4"),
            },
            "RenderInvalidRequest",
        ),
        (
            1,
            context(&old),
            Recovery::Retry {
                job_id: saved.job_id.clone(),
                checkpoint_attempt_id: None,
                destination: scratch.path().join("stale.mp4"),
            },
            "RenderContextChanged",
        ),
        (
            2,
            context(&old),
            Recovery::Reconcile {
                publication_id: RequestId::new("missing").unwrap(),
            },
            "RenderContextChanged",
        ),
        (
            3,
            context(&current),
            Recovery::Retry {
                job_id: saved.job_id.clone(),
                checkpoint_attempt_id: Some(AttemptId::new("missing").unwrap()),
                destination: scratch.path().join("missing.mp4"),
            },
            "RenderRecoveryRequestFailed",
        ),
        (
            4,
            context(&current),
            Recovery::Retry {
                job_id: engineering.job_id,
                checkpoint_attempt_id: None,
                destination: scratch.path().join("engineering.mp4"),
            },
            "RenderEngineeringJob",
        ),
    ] {
        let rejected = recovery(&harness.service, captured, ticket, operation);
        assert_eq!(
            rejected
                .render
                .as_ref()
                .unwrap()
                .command
                .as_ref()
                .unwrap()
                .result
                .as_ref()
                .unwrap_err()
                .code,
            expected
        );
        assert_eq!(
            rejected.workspace.as_ref().unwrap().document.revision_id(),
            current.document.revision_id()
        );
    }
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly).unwrap();
    assert!(
        reader
            .render_attempts(&saved.job_id, 0, 8)
            .unwrap()
            .is_empty()
    );
    assert!(reader.render_publications(None, 8).unwrap().is_empty());
    command(&harness.service, ProjectRequest::Close);
}
