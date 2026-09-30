use super::*;
use deadpan_core::*;
use deadpan_jobs::{
    Sha256,
    render::{RenderBFrames, RenderEncoder, RenderSelection, document_sha256},
};
use deadpan_store::AccessMode;
use std::{collections::BTreeMap, time::Duration};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn document() -> Result<ProjectDocument> {
    let empty = ProjectDocument::new(
        ProjectId::new("workflow-project")?,
        RevisionId::new("empty")?,
        PresentationBasis {
            width: 64,
            height: 48,
            frame_rate: FrameRate::new(30, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    let transaction = apply(
        &empty,
        &CommandRequest {
            project_id: empty.project_id().clone(),
            expected_revision: empty.revision_id().clone(),
            new_revision: RevisionId::new("baseline")?,
            command: deadpan_core::Command::Insert {
                parent: empty.root().clone(),
                index: 0,
                subtree: Subtree {
                    root: NodeId::new("hold")?,
                    nodes: BTreeMap::from([(
                        NodeId::new("hold")?,
                        BeatNode::hold(
                            "pause",
                            HoldRecipe {
                                picture_context: None,
                                duration: FrameDuration::new(12)?,
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                            },
                        ),
                    )]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        },
    )?;
    Ok(transaction.forward.apply(&empty)?)
}

fn config(package: PathBuf) -> Result<WorkflowConfig> {
    Ok(WorkflowConfig {
        package,
        runtime: RenderWorkerRuntime {
            executable: PathBuf::from("/nonexistent/deadpan-workflow-test-worker"),
            arguments: Vec::new(),
            environment: BTreeMap::new(),
        },
        encode_limits: EncodedWorkerLimits::default(),
        verification_limits: VerificationLimits::default(),
        media_limits: RenderMediaLimits::new(
            16 * 1024 * 1024,
            256 * 1024,
            17 * 1024 * 1024,
            64 * 1024 * 1024,
            128,
        )?,
    })
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
fn identity(name: &str) -> Result<WorkflowIdentity> {
    Ok(WorkflowIdentity {
        job_id: RequestId::new(format!("job-{name}"))?,
        attempt_id: AttemptId::new(format!("attempt-{name}"))?,
        cancellation_token: CancellationToken::new(format!("token-{name}"))?,
    })
}
fn publication(name: &str) -> Result<PublicationRequest> {
    Ok(PublicationRequest {
        destination: PathBuf::from(format!("/tmp/deadpan-workflow-unit-{name}.mp4")),
        publication_id: RequestId::new(format!("publication-{name}"))?,
        operation_id: AttemptId::new(format!("publish-{name}"))?,
        cancellation_token: CancellationToken::new(format!("publish-token-{name}"))?,
    })
}
fn policy() -> RenderEngineeringPolicy {
    RenderEngineeringPolicy {
        schema_version: 1,
        selection: RenderSelection::ExplicitEngineering,
        encoder: RenderEncoder::Software,
        b_frames: RenderBFrames::None,
    }
}
fn start(name: &str) -> Result<StartRender> {
    Ok(StartRender {
        revision: RevisionId::new("baseline")?,
        range: None,
        identity: identity(name)?,
        policy: policy(),
        publication: publication(name)?,
        deadline: deadline(),
    })
}

#[test]
fn stale_revision_and_foreign_writer_are_rejected_before_work() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("owner.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let mut foreign = ProjectStore::create(&scratch.path().join("foreign.deadpan"), &document()?)?;
    let mut workflow = RenderWorkflow::new(&store, config(package)?)?;
    let mut request = start("stale")?;
    request.revision = RevisionId::new("empty")?;
    assert!(matches!(
        workflow.start(&mut store, request),
        Err(WorkflowError::StaleRevision)
    ));
    assert!(matches!(
        workflow.start(&mut foreign, start("foreign")?),
        Err(WorkflowError::Store(_))
    ));
    assert!(matches!(
        workflow.poll(&mut foreign),
        Err(WorkflowError::Store(_))
    ));
    assert!(workflow.can_release_writer());
    assert!(store.render_jobs(None, 1)?.is_empty());
    Ok(())
}

#[test]
fn capture_cancel_drains_real_worker_without_creating_job_or_media() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("project.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let mut workflow = RenderWorkflow::new(&store, config(package)?)?;
    let request = start("cancel")?;
    let identity = request.identity.clone();
    workflow.start(&mut store, request)?;
    assert!(workflow.is_active());
    assert!(matches!(
        workflow.cancel(&mut store, &self::identity("stale")?),
        Err(WorkflowError::Identity)
    ));
    assert!(!workflow.status().cancellation_requested);
    workflow.cancel(&mut store, &identity)?;
    workflow.drain(&mut store)?;
    assert!(!workflow.is_active());
    assert!(workflow.can_release_writer());
    assert_eq!(workflow.status().outcome, Some(WorkflowOutcome::Cancelled));
    assert!(workflow.status().cleanup_confirmed);
    assert!(store.render_jobs(None, 1)?.is_empty());
    assert_eq!(store.snapshot()?, document()?);
    Ok(())
}

#[test]
fn separate_coordinators_share_slot_until_worker_release() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("project.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let mut first = RenderWorkflow::new(&store, config(package.clone())?)?;
    let mut second = RenderWorkflow::new(&store, config(package)?)?;
    first.start(&mut store, start("first")?)?;
    assert!(matches!(
        second.start(&mut store, start("second")?),
        Err(WorkflowError::Store(_))
    ));
    first.drain(&mut store)?;
    second.start(&mut store, start("second")?)?;
    second.drain(&mut store)?;
    assert_eq!(second.status().outcome, Some(WorkflowOutcome::Cancelled));
    Ok(())
}

#[test]
fn reopening_writer_never_rebinds_old_coordinator() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("project.deadpan");
    let store = ProjectStore::create(&package, &document()?)?;
    let mut workflow = RenderWorkflow::new(&store, config(package.clone())?)?;
    drop(store);
    let mut reopened = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    assert!(matches!(
        workflow.start(&mut reopened, start("reopened")?),
        Err(WorkflowError::Store(_))
    ));
    Ok(())
}

// The following controller tests supply explicit transport failure replies.
// They never construct a successful media capability or run a mock encoder.
struct Controlled {
    workflow: RenderWorkflow,
    commands: Receiver<worker::Command>,
}
fn controlled(store: &mut ProjectStore, package: PathBuf, name: &str) -> Result<Controlled> {
    let mut workflow = RenderWorkflow::new(store, config(package)?)?;
    let (commands, receiver) = mpsc::sync_channel(1);
    workflow.commands = Some(commands); // idle real worker exits without work
    let id = identity(name)?;
    workflow.begin(
        store,
        id.clone(),
        Destination::Publish(publication(name)?),
        deadline(),
    )?;
    let doc = store.snapshot()?;
    let intent = RenderIntent {
        schema_version: 1,
        job_id: id.job_id.clone(),
        project_id: doc.project_id().clone(),
        revision_id: doc.revision_id().clone(),
        document_sha256: document_sha256(&doc, &AtomicBool::new(false), deadline())?,
        range: FrameRange::new(ProjectFrame(0), ProjectFrame(12))?,
        policy: policy(),
    };
    store.create_render_job(intent.clone(), &AtomicBool::new(false), deadline())?;
    workflow.status.intent = Some(intent);
    workflow.status.attempt = Some(store.begin_render_attempt(BeginRenderAttempt {
        job_id: id.job_id,
        attempt_id: id.attempt_id,
        cancellation_token: id.cancellation_token,
        checkpoint_attempt_id: None,
    })?);
    workflow.transition(store, RenderAttemptTransition::Encoding)?;
    workflow
        .active
        .as_mut()
        .ok_or("missing active run")?
        .pending = true;
    workflow
        .active
        .as_mut()
        .ok_or("missing active run")?
        .expected_reply = Some(ExpectedReply::Retained);
    Ok(Controlled {
        workflow,
        commands: receiver,
    })
}
fn reply(controlled: &Controlled, stage: StageReply) -> Result<Reply> {
    Ok(Reply {
        identity: controlled
            .workflow
            .status
            .identity
            .clone()
            .ok_or("missing identity")?,
        stage,
    })
}

#[test]
fn unknown_cleanup_persists_cancelling_and_fences_every_coordinator() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("project.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let mut control = controlled(&mut store, package, "unresolved")?;
    let message = reply(
        &control,
        StageReply::Failed {
            diagnostic: diagnostic("cleanup_unconfirmed", "owned process group has not stopped"),
            cleanup_confirmed: false,
            retained: RetainedPublicationArtifacts::default(),
        },
    )?;
    assert!(matches!(
        control.workflow.receive(&mut store, message),
        Err(WorkflowError::Unresolved(_))
    ));
    let id = identity("unresolved")?;
    assert_eq!(
        store.render_attempt(&id.job_id, &id.attempt_id)?.state,
        RenderAttemptState::Cancelling
    );
    assert!(!control.workflow.can_release_writer());
    assert!(control.workflow.is_active());
    assert!(store.acquire_render_workflow().is_err());
    assert!(matches!(
        control.commands.try_recv(),
        Err(TryRecvError::Empty)
    ));
    assert!(control.workflow.drain(&mut store).is_err());
    Ok(())
}

#[test]
fn cancellation_is_durable_before_atomic_signal_and_terminal_waits_for_release() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("project.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let mut control = controlled(&mut store, package, "ordered")?;
    let id = identity("ordered")?;
    control.workflow.cancel(&mut store, &id)?;
    assert_eq!(
        store.render_attempt(&id.job_id, &id.attempt_id)?.state,
        RenderAttemptState::Cancelling
    );
    assert!(
        control
            .workflow
            .active
            .as_ref()
            .ok_or("missing active")?
            .cancelled
            .load(Ordering::Acquire)
    );
    let message = reply(
        &control,
        StageReply::Failed {
            diagnostic: diagnostic("cancelled", "stage stopped and all owned processes reaped"),
            cleanup_confirmed: true,
            retained: RetainedPublicationArtifacts::default(),
        },
    )?;
    control.workflow.receive(&mut store, message)?;
    assert_eq!(
        store.render_attempt(&id.job_id, &id.attempt_id)?.state,
        RenderAttemptState::Cancelled
    );
    assert!(control.workflow.is_active());
    assert!(!control.workflow.can_release_writer());
    assert!(matches!(control.commands.try_recv()?.kind, Work::Release));
    let released = reply(&control, StageReply::Released)?;
    control.workflow.receive(&mut store, released)?;
    assert!(!control.workflow.is_active());
    assert!(control.workflow.can_release_writer());
    store.acquire_render_workflow()?.release();
    Ok(())
}

#[test]
fn observed_movie_commit_survives_terminal_journal_failure() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("project.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let mut control = controlled(&mut store, package, "commit-observation")?;
    // Controller-only observation: intentionally no publication record. The
    // terminal write must fail, while the received commit knowledge survives.
    let receipt = PublicationReceipt {
        publication_id: "observed".into(),
        movie: "/tmp/observed.mp4".into(),
        report: "/tmp/observed.json".into(),
        movie_sha256: Sha256::new("1".repeat(64))?,
        movie_bytes: 1,
        report_sha256: Sha256::new("2".repeat(64))?,
        report_bytes: 1,
        contains_generated_pictures: false,
    };
    control
        .workflow
        .active
        .as_mut()
        .ok_or("missing active run")?
        .expected_reply = Some(ExpectedReply::Published);
    let message = reply(
        &control,
        StageReply::Published(PublicationOutcome::Published(receipt.clone())),
    )?;
    assert!(control.workflow.receive(&mut store, message).is_err());
    assert_eq!(control.workflow.status.receipt, Some(receipt));
    assert!(control.workflow.status.observed_movie_commit);
    assert_eq!(
        control.workflow.status.outcome,
        Some(WorkflowOutcome::PublishedUnconfirmed)
    );
    assert!(matches!(control.commands.try_recv()?.kind, Work::Release));
    let released = reply(&control, StageReply::Released)?;
    assert!(control.workflow.receive(&mut store, released).is_err());
    assert!(control.workflow.can_release_writer());
    assert_eq!(control.workflow.status.stage, WorkflowStage::Unresolved);
    Ok(())
}

#[test]
fn diagnostics_remain_valid_at_utf8_and_nul_bounds() -> Result {
    let result = diagnostic("test", format!("{}\0", "🦉".repeat(4096)));
    result.validate()?;
    assert_eq!(
        result.detail.len(),
        deadpan_jobs::render::MAX_RENDER_DIAGNOSTIC_BYTES
    );
    assert!(!result.detail.contains('\0'));
    Ok(())
}

#[test]
fn wrong_phase_reply_never_releases_an_encoding_slot() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("project.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let mut control = controlled(&mut store, package, "wrong-phase")?;
    let released = reply(&control, StageReply::Released)?;
    assert!(control.workflow.receive(&mut store, released).is_err());
    assert!(!control.workflow.can_release_writer());
    assert!(!control.workflow.status.cleanup_confirmed);
    let id = identity("wrong-phase")?;
    assert_eq!(
        store.render_attempt(&id.job_id, &id.attempt_id)?.state,
        RenderAttemptState::Cancelling
    );
    assert!(store.acquire_render_workflow().is_err());
    Ok(())
}

#[test]
fn publication_progress_serializes_as_a_status_snapshot() -> Result {
    let status = WorkflowStatus {
        stage: WorkflowStage::PreparingPublication,
        progress: Some(WorkflowProgress::Publication(
            PublicationStage::CopyingDestination,
        )),
        ..WorkflowStatus::default()
    };
    let value = serde_json::to_value(status)?;
    assert_eq!(value["progress"]["kind"], "publication");
    assert_eq!(value["progress"]["value"], "copying_destination");
    Ok(())
}

#[test]
fn stopped_reconciliation_keeps_destination_uncertain_during_verification() -> Result {
    for cancel in [false, true] {
        let scratch = tempfile::tempdir()?;
        let package = scratch.path().join("project.deadpan");
        let mut store = ProjectStore::create(&package, &document()?)?;
        let mut control = controlled(&mut store, package, "inspection")?;
        // Supply only controller recovery intent. No successful media or
        // destination admission is fabricated; the returned stage is a failure.
        control
            .workflow
            .active
            .as_mut()
            .ok_or("missing active run")?
            .destination = Destination::Reconcile {
            publication_id: RequestId::new("prior-publication")?,
            operation_id: AttemptId::new("inspection-operation")?,
            cancellation_token: CancellationToken::new("inspection-token")?,
        };
        if cancel {
            control
                .workflow
                .cancel(&mut store, &identity("inspection")?)?;
        }
        let stopped = reply(
            &control,
            StageReply::Failed {
                diagnostic: diagnostic("inspection_stopped", "Fresh verification did not finish"),
                cleanup_confirmed: true,
                retained: RetainedPublicationArtifacts::default(),
            },
        )?;
        control.workflow.receive(&mut store, stopped)?;
        assert_eq!(
            control.workflow.status.outcome,
            Some(WorkflowOutcome::Unresolved)
        );
        assert!(matches!(control.commands.try_recv()?.kind, Work::Release));
        let released = reply(&control, StageReply::Released)?;
        control.workflow.receive(&mut store, released)?;
        assert!(control.workflow.can_release_writer());
    }
    Ok(())
}

#[test]
fn unknown_cleanup_preserves_previously_observed_movie_commit() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("project.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let mut control = controlled(&mut store, package, "committed-cleanup")?;
    // Reconciliation starts with this positive knowledge from its prior journal.
    control.workflow.status.observed_movie_commit = true;
    let stopped = reply(
        &control,
        StageReply::Failed {
            diagnostic: diagnostic(
                "cleanup_unconfirmed",
                "Verifier process teardown is unknown",
            ),
            cleanup_confirmed: false,
            retained: RetainedPublicationArtifacts::default(),
        },
    )?;
    assert!(control.workflow.receive(&mut store, stopped).is_err());
    assert_eq!(
        control.workflow.status.outcome,
        Some(WorkflowOutcome::PublishedUnconfirmed)
    );
    assert!(control.workflow.status.observed_movie_commit);
    assert!(!control.workflow.status.cleanup_confirmed);
    assert!(!control.workflow.can_release_writer());
    assert!(store.acquire_render_workflow().is_err());
    Ok(())
}

#[test]
fn disconnected_release_channel_does_not_erase_commit_knowledge() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("project.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let control = controlled(&mut store, package, "release-channel")?;
    let Controlled {
        mut workflow,
        commands,
    } = control;
    drop(commands);
    workflow
        .active
        .as_mut()
        .ok_or("missing active run")?
        .pending = false;
    workflow.status.observed_movie_commit = true;
    workflow.status.cleanup_confirmed = true;
    assert!(workflow.send(Work::Release).is_err());
    assert_eq!(
        workflow.status.outcome,
        Some(WorkflowOutcome::PublishedUnconfirmed)
    );
    assert!(workflow.status.observed_movie_commit);
    assert!(!workflow.can_release_writer());
    assert!(store.acquire_render_workflow().is_err());
    Ok(())
}
