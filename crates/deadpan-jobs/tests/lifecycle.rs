use deadpan_core::{FrameDuration, FrameRate, NodeId, ProjectId};
use deadpan_jobs::{
    AttemptId, CancellationAcknowledgement, CancellationToken, CandidateDeclaration,
    CandidateManifest, Diagnostic, FailureCode, HostFailure, HostFailureCode, JobFailure,
    JobLifecycle, JobState, LifecycleError, MessageIdentity, NativeCandidateManifest,
    ProtocolVersion, ProviderPackId, ProviderPackVersion, ProviderSelection, Relevance, RequestId,
    RequestVersion, RuntimeId, RuntimeVersion, Sha256, StageProgress, TargetBinding, VideoSpec,
    WorkerEventOutcome, WorkerFailure, WorkerMessage, WorkerStage, WorkspaceArtifact, WorkspaceRef,
};

fn sha(character: char) -> Sha256 {
    Sha256::new(character.to_string().repeat(64)).unwrap()
}

fn identity() -> MessageIdentity {
    MessageIdentity::new(
        RequestId::new("request-1").unwrap(),
        AttemptId::new("attempt-1").unwrap(),
    )
}

fn token() -> CancellationToken {
    CancellationToken::new("cancel-1").unwrap()
}

fn binding() -> TargetBinding {
    TargetBinding {
        project_id: ProjectId::new("project-1").unwrap(),
        hold_id: NodeId::new("hold-1").unwrap(),
        request_version: RequestVersion::new(2).unwrap(),
        context_sha256: sha('a'),
    }
}

fn provider() -> ProviderSelection {
    ProviderSelection {
        pack_id: ProviderPackId::new("pack").unwrap(),
        pack_version: ProviderPackVersion::new("1.0").unwrap(),
        runtime_id: RuntimeId::new("runtime").unwrap(),
        runtime_version: RuntimeVersion::new("2.0").unwrap(),
        seed: 42,
    }
}

fn candidate_with_hash(hash: Sha256) -> CandidateManifest {
    CandidateManifest {
        media: WorkspaceArtifact::new(
            WorkspaceRef::new("outputs/candidate.mov").unwrap(),
            hash,
            5_000,
        )
        .unwrap(),
        video: VideoSpec::new(
            FrameDuration::new(45).unwrap(),
            FrameRate::new(30, 1).unwrap(),
            512,
            320,
        )
        .unwrap(),
        provider: provider(),
    }
}

fn candidate() -> CandidateManifest {
    candidate_with_hash(sha('b'))
}

fn native_candidate() -> NativeCandidateManifest {
    NativeCandidateManifest {
        native: WorkspaceArtifact::new(
            WorkspaceRef::new("outputs/native.mp4").unwrap(),
            sha('c'),
            8_000,
        )
        .unwrap(),
        provenance: WorkspaceArtifact::new(
            WorkspaceRef::new("outputs/provenance.json").unwrap(),
            sha('d'),
            800,
        )
        .unwrap(),
        video: VideoSpec::new(
            FrameDuration::new(49).unwrap(),
            FrameRate::new(24, 1).unwrap(),
            512,
            320,
        )
        .unwrap(),
        provider: provider(),
    }
}

fn stage(identity: &MessageIdentity, stage: WorkerStage) -> WorkerMessage {
    WorkerMessage::Stage {
        protocol: ProtocolVersion::V1,
        identity: identity.clone(),
        stage,
    }
}

fn running(job: &mut JobLifecycle) {
    let identity = job.identity().clone();
    for worker_stage in [
        WorkerStage::Preflight,
        WorkerStage::ModelLoading,
        WorkerStage::Inference,
    ] {
        job.apply_worker_message(&WorkerMessage::Stage {
            protocol: job.protocol(),
            identity: identity.clone(),
            stage: worker_stage,
        })
        .unwrap();
    }
}

#[test]
fn completion_requires_explicit_host_validation_before_ready() {
    let identity = identity();
    let candidate = candidate();
    let mut job = JobLifecycle::new(identity.clone(), token(), binding());
    running(&mut job);

    job.apply_worker_message(&WorkerMessage::Completed {
        protocol: ProtocolVersion::V1,
        identity: identity.clone(),
        candidate: candidate.clone(),
    })
    .unwrap();
    assert_eq!(job.state(), JobState::Validating);
    assert!(!job.can_authorize_acceptance());

    let different = candidate_with_hash(sha('c'));
    assert_eq!(
        job.host_validation_succeeded(&identity, &different),
        Err(LifecycleError::CandidateMismatch)
    );
    job.host_validation_succeeded(&identity, &candidate)
        .unwrap();
    assert_eq!(job.state(), JobState::Ready);
    assert!(job.can_authorize_acceptance());
}

#[test]
fn rejects_wrong_identity_regressions_and_post_terminal_messages() {
    let identity = identity();
    let mut job = JobLifecycle::new(identity.clone(), token(), binding());
    let wrong_request = MessageIdentity::new(
        RequestId::new("other").unwrap(),
        identity.attempt_id.clone(),
    );
    assert_eq!(
        job.apply_worker_message(&stage(&wrong_request, WorkerStage::Preflight)),
        Err(LifecycleError::WrongRequest)
    );
    let wrong_attempt = MessageIdentity::new(
        identity.request_id.clone(),
        AttemptId::new("other").unwrap(),
    );
    assert_eq!(
        job.apply_worker_message(&stage(&wrong_attempt, WorkerStage::Preflight)),
        Err(LifecycleError::WrongAttempt)
    );

    running(&mut job);
    assert!(matches!(
        job.apply_worker_message(&stage(&identity, WorkerStage::ModelLoading)),
        Err(LifecycleError::StageRegression { .. })
    ));
    job.apply_worker_message(&WorkerMessage::Failed {
        protocol: ProtocolVersion::V1,
        identity: identity.clone(),
        failure: WorkerFailure {
            code: FailureCode::BackendFailure,
            detail: Diagnostic::new("model process exited").unwrap(),
        },
    })
    .unwrap();
    assert_eq!(job.state(), JobState::Failed);
    assert!(matches!(job.failure(), Some(JobFailure::Worker(_))));
    assert_eq!(
        job.apply_worker_message(&stage(&identity, WorkerStage::Inference)),
        Err(LifecycleError::MessageAfterTerminal(JobState::Failed))
    );
}

#[test]
fn progress_is_monotonic_within_the_announced_stage() {
    let identity = identity();
    let mut job = JobLifecycle::new(identity.clone(), token(), binding());
    running(&mut job);

    let progress = |completed, total| WorkerMessage::Progress {
        protocol: ProtocolVersion::V1,
        identity: identity.clone(),
        stage: WorkerStage::Inference,
        progress: StageProgress::new(completed, total).unwrap(),
    };
    job.apply_worker_message(&progress(2, 10)).unwrap();
    assert_eq!(
        job.apply_worker_message(&progress(2, 10)).unwrap(),
        WorkerEventOutcome::Duplicate
    );
    assert!(matches!(
        job.apply_worker_message(&progress(1, 10)),
        Err(LifecycleError::ProgressRegression { .. })
    ));
    assert!(matches!(
        job.apply_worker_message(&progress(3, 11)),
        Err(LifecycleError::ProgressTotalChanged { .. })
    ));
}

#[test]
fn completion_during_cancellation_waits_for_host_reap_without_candidate() {
    let identity = identity();
    let token = token();
    let mut job = JobLifecycle::new(identity.clone(), token.clone(), binding());
    running(&mut job);
    assert_eq!(
        job.request_cancel(&identity, &CancellationToken::new("wrong-token").unwrap()),
        Err(LifecycleError::WrongCancellationToken)
    );
    assert_eq!(
        job.request_cancel(&identity, &token).unwrap(),
        WorkerEventOutcome::Applied
    );
    assert_eq!(
        job.request_cancel(&identity, &token).unwrap(),
        WorkerEventOutcome::Duplicate
    );
    assert_eq!(
        job.apply_worker_message(&WorkerMessage::Completed {
            protocol: ProtocolVersion::V1,
            identity: identity.clone(),
            candidate: candidate(),
        })
        .unwrap(),
        WorkerEventOutcome::CompletionDiscardedDuringCancellation
    );
    assert_eq!(job.state(), JobState::Cancelling);
    assert!(matches!(
        job.cancellation_acknowledgement(),
        Some(CancellationAcknowledgement::CompletionDiscarded(_))
    ));
    assert!(job.candidate().is_none());
    assert!(!job.can_authorize_acceptance());
    let restored = JobLifecycle::from_checkpoint(job.checkpoint(), Relevance::Current).unwrap();
    assert_eq!(restored, job);
    job.host_cancelled(&identity, &token).unwrap();
    assert_eq!(job.state(), JobState::Cancelled);
}

#[test]
fn worker_cancellation_acknowledgement_is_durable_but_not_reaped() {
    let identity = identity();
    let token = token();
    let mut job = JobLifecycle::new(identity.clone(), token.clone(), binding());
    job.request_cancel(&identity, &token).unwrap();
    let cancelled = WorkerMessage::Cancelled {
        protocol: ProtocolVersion::V1,
        identity: identity.clone(),
    };
    assert_eq!(
        job.apply_worker_message(&cancelled).unwrap(),
        WorkerEventOutcome::CancellationAcknowledged
    );
    assert_eq!(job.state(), JobState::Cancelling);
    assert_eq!(
        job.apply_worker_message(&cancelled).unwrap(),
        WorkerEventOutcome::Duplicate
    );
    let before_conflict = job.clone();
    assert_eq!(
        job.apply_worker_message(&WorkerMessage::Failed {
            protocol: ProtocolVersion::V1,
            identity: identity.clone(),
            failure: WorkerFailure {
                code: FailureCode::BackendFailure,
                detail: Diagnostic::new("conflicting failure").unwrap(),
            },
        }),
        Err(LifecycleError::ConflictingCancellationAcknowledgement)
    );
    assert_eq!(job, before_conflict);
    let restored = JobLifecycle::from_checkpoint(job.checkpoint(), Relevance::Current).unwrap();
    assert_eq!(restored, job);
    job.host_cancelled(&identity, &token).unwrap();
    assert_eq!(job.state(), JobState::Cancelled);
}

#[test]
fn buffered_stage_and_progress_are_ignored_during_cancellation() {
    let identity = identity();
    let token = token();
    let mut job = JobLifecycle::new(identity.clone(), token.clone(), binding());
    running(&mut job);
    job.request_cancel(&identity, &token).unwrap();

    assert_eq!(
        job.apply_worker_message(&stage(&identity, WorkerStage::Inference))
            .unwrap(),
        WorkerEventOutcome::IgnoredDuringCancellation
    );
    assert_eq!(
        job.apply_worker_message(&WorkerMessage::Progress {
            protocol: ProtocolVersion::V1,
            identity: identity.clone(),
            stage: WorkerStage::Inference,
            progress: StageProgress::new(9, 10).unwrap(),
        })
        .unwrap(),
        WorkerEventOutcome::IgnoredDuringCancellation
    );
    assert_eq!(job.state(), JobState::Cancelling);
    assert!(!job.can_authorize_acceptance());

    let wrong_identity = MessageIdentity::new(
        RequestId::new("other-request").unwrap(),
        identity.attempt_id.clone(),
    );
    assert_eq!(
        job.apply_worker_message(&stage(&wrong_identity, WorkerStage::Inference)),
        Err(LifecycleError::WrongRequest)
    );
    assert_eq!(job.state(), JobState::Cancelling);
}

#[test]
fn worker_messages_after_completion_cannot_bypass_host_validation() {
    let identity = identity();
    let candidate = candidate();
    let mut job = JobLifecycle::new(identity.clone(), token(), binding());
    running(&mut job);
    job.apply_worker_message(&WorkerMessage::Completed {
        protocol: ProtocolVersion::V1,
        identity: identity.clone(),
        candidate: candidate.clone(),
    })
    .unwrap();

    let late_messages = [
        stage(&identity, WorkerStage::Inference),
        WorkerMessage::Progress {
            protocol: ProtocolVersion::V1,
            identity: identity.clone(),
            stage: WorkerStage::Inference,
            progress: StageProgress::new(10, 10).unwrap(),
        },
        WorkerMessage::Failed {
            protocol: ProtocolVersion::V1,
            identity: identity.clone(),
            failure: WorkerFailure {
                code: FailureCode::BackendFailure,
                detail: Diagnostic::new("late worker failure").unwrap(),
            },
        },
        WorkerMessage::Cancelled {
            protocol: ProtocolVersion::V1,
            identity: identity.clone(),
        },
    ];
    for message in late_messages {
        assert_eq!(
            job.apply_worker_message(&message),
            Err(LifecycleError::InvalidTransition {
                from: JobState::Validating,
                event: "worker message after completion",
            })
        );
        assert_eq!(job.state(), JobState::Validating);
        assert!(!job.can_authorize_acceptance());
    }

    job.host_validation_succeeded(&identity, &candidate)
        .unwrap();
    assert_eq!(job.state(), JobState::Ready);
    assert!(job.can_authorize_acceptance());
}

#[test]
fn host_failure_and_cancellation_remain_available_during_validation() {
    let identity = identity();
    let token = token();
    let completed = WorkerMessage::Completed {
        protocol: ProtocolVersion::V1,
        identity: identity.clone(),
        candidate: candidate(),
    };

    let mut cancelled = JobLifecycle::new(identity.clone(), token.clone(), binding());
    running(&mut cancelled);
    cancelled.apply_worker_message(&completed).unwrap();
    assert_eq!(
        cancelled.request_cancel(&identity, &token).unwrap(),
        WorkerEventOutcome::Applied
    );
    assert_eq!(cancelled.state(), JobState::Cancelling);
    assert!(!cancelled.can_authorize_acceptance());

    let mut failed = JobLifecycle::new(identity.clone(), token, binding());
    running(&mut failed);
    failed.apply_worker_message(&completed).unwrap();
    failed
        .host_failed(
            &identity,
            HostFailure {
                code: HostFailureCode::OutputValidationFailed,
                detail: Diagnostic::new("candidate hash did not match media").unwrap(),
            },
        )
        .unwrap();
    assert_eq!(failed.state(), JobState::Failed);
    assert!(!failed.can_authorize_acceptance());
}

#[test]
fn host_can_finalize_killed_worker_or_record_process_failure() {
    let identity = identity();
    let token = token();
    let mut cancelled = JobLifecycle::new(identity.clone(), token.clone(), binding());
    cancelled.request_cancel(&identity, &token).unwrap();
    cancelled.host_cancelled(&identity, &token).unwrap();
    cancelled.host_cancelled(&identity, &token).unwrap();
    assert_eq!(cancelled.state(), JobState::Cancelled);

    let mut failed = JobLifecycle::new(identity.clone(), token, binding());
    failed
        .host_failed(
            &identity,
            HostFailure {
                code: HostFailureCode::WorkerExited,
                detail: Diagnostic::new("worker exited with status 9").unwrap(),
            },
        )
        .unwrap();
    assert!(matches!(failed.failure(), Some(JobFailure::Host(_))));
}

#[test]
fn stale_or_detached_binding_never_silently_revives_or_authorizes_edit() {
    let identity = identity();
    let candidate = candidate();
    let original = binding();
    let mut job = JobLifecycle::new(identity.clone(), token(), original.clone());
    running(&mut job);
    job.apply_worker_message(&WorkerMessage::Completed {
        protocol: ProtocolVersion::V1,
        identity: identity.clone(),
        candidate: candidate.clone(),
    })
    .unwrap();

    let mut changed = original.clone();
    changed.request_version = RequestVersion::new(3).unwrap();
    job.observe_target(Some(&changed));
    assert_eq!(job.relevance(), Relevance::Stale);
    job.observe_target(Some(&original));
    assert_eq!(job.relevance(), Relevance::Stale);
    job.host_validation_succeeded(&identity, &candidate)
        .unwrap();
    assert_eq!(job.state(), JobState::Ready);
    assert!(!job.can_authorize_acceptance());

    job.observe_target(None);
    assert_eq!(job.relevance(), Relevance::Detached);
    job.observe_target(Some(&original));
    assert_eq!(job.relevance(), Relevance::Detached);
}

#[test]
fn context_hash_changes_stale_a_job_but_unrelated_revision_is_not_a_binding() {
    let original = binding();
    let mut job = JobLifecycle::new(identity(), token(), original.clone());
    job.observe_target(Some(&original));
    assert_eq!(job.relevance(), Relevance::Current);

    let mut changed_context = original;
    changed_context.context_sha256 = sha('d');
    job.observe_target(Some(&changed_context));
    assert_eq!(job.relevance(), Relevance::Stale);
}

#[test]
fn durable_checkpoint_rejects_impossible_state_payload_combinations() {
    let identity = identity();
    let base = JobLifecycle::new(identity.clone(), token(), binding());

    let mut candidate_while_queued = base.checkpoint();
    candidate_while_queued.completion = Some(CandidateDeclaration::SampledV1(candidate()));
    assert!(matches!(
        JobLifecycle::from_checkpoint(candidate_while_queued, Relevance::Current),
        Err(LifecycleError::InvalidCheckpoint(_))
    ));

    let mut failed_without_reason = base.checkpoint();
    failed_without_reason.state = JobState::Failed;
    assert!(matches!(
        JobLifecycle::from_checkpoint(failed_without_reason, Relevance::Current),
        Err(LifecycleError::InvalidCheckpoint(_))
    ));

    let mut acknowledgement_while_queued = base.checkpoint();
    acknowledgement_while_queued.cancellation_acknowledgement =
        Some(CancellationAcknowledgement::Cancelled);
    assert!(matches!(
        JobLifecycle::from_checkpoint(acknowledgement_while_queued, Relevance::Current),
        Err(LifecycleError::InvalidCheckpoint(_))
    ));

    let completed_candidate = candidate();
    let mut completed = base;
    running(&mut completed);
    completed
        .apply_worker_message(&WorkerMessage::Completed {
            protocol: ProtocolVersion::V1,
            identity: identity.clone(),
            candidate: completed_candidate,
        })
        .unwrap();
    let mut impossible_stage = completed.checkpoint();
    impossible_stage.worker_stage = Some(WorkerStage::ModelLoading);
    assert!(matches!(
        JobLifecycle::from_checkpoint(impossible_stage, Relevance::Current),
        Err(LifecycleError::InvalidCheckpoint(_))
    ));
}

#[test]
fn native_bridge_completion_requires_exact_bundle_validation() {
    let identity = identity();
    let candidate = native_candidate();
    let mut job =
        JobLifecycle::new_with_protocol(identity.clone(), token(), binding(), ProtocolVersion::V2);
    running(&mut job);
    job.apply_worker_message(&WorkerMessage::CompletedBridge {
        protocol: ProtocolVersion::V2,
        identity: identity.clone(),
        candidate: candidate.clone(),
    })
    .unwrap();
    assert_eq!(job.state(), JobState::Validating);
    assert!(job.candidate().is_none());
    assert_eq!(job.candidate_bundle(), Some(&candidate));
    assert_eq!(
        job.completion(),
        Some(&CandidateDeclaration::NativeBridgeV2(candidate.clone()))
    );
    assert!(
        job.host_validation_succeeded(&identity, &candidate_with_hash(sha('e')))
            .is_err()
    );
    job.host_bundle_validation_succeeded(&identity, &candidate)
        .unwrap();
    assert_eq!(job.state(), JobState::Ready);
    assert!(job.can_authorize_acceptance());

    let restored = JobLifecycle::from_checkpoint(job.checkpoint(), Relevance::Current).unwrap();
    assert_eq!(restored.protocol(), ProtocolVersion::V2);
    assert_eq!(restored.candidate_bundle(), Some(&candidate));
}

#[test]
fn v2_late_completion_during_cancellation_is_discarded_until_reap() {
    let identity = identity();
    let token = token();
    let candidate = native_candidate();
    let mut job = JobLifecycle::new_with_protocol(
        identity.clone(),
        token.clone(),
        binding(),
        ProtocolVersion::V2,
    );
    running(&mut job);
    job.request_cancel(&identity, &token).unwrap();
    assert_eq!(
        job.apply_worker_message(&WorkerMessage::CompletedBridge {
            protocol: ProtocolVersion::V2,
            identity: identity.clone(),
            candidate: candidate.clone(),
        })
        .unwrap(),
        WorkerEventOutcome::CompletionDiscardedDuringCancellation
    );
    assert_eq!(job.state(), JobState::Cancelling);
    assert!(job.completion().is_none());
    assert_eq!(
        job.cancellation_acknowledgement(),
        Some(&CancellationAcknowledgement::CompletionDiscarded(Box::new(
            CandidateDeclaration::NativeBridgeV2(candidate)
        )))
    );
    assert!(!job.can_authorize_acceptance());
    job.host_cancelled(&identity, &token).unwrap();
    assert_eq!(job.state(), JobState::Cancelled);
}

#[test]
fn lifecycle_rejects_message_and_checkpoint_protocol_crossovers() {
    let identity = identity();
    let mut job =
        JobLifecycle::new_with_protocol(identity.clone(), token(), binding(), ProtocolVersion::V2);
    assert!(matches!(
        job.apply_worker_message(&WorkerMessage::Stage {
            protocol: ProtocolVersion::V1,
            identity: identity.clone(),
            stage: WorkerStage::Preflight,
        }),
        Err(LifecycleError::WrongProtocol)
    ));

    let legacy = JobLifecycle::new(identity.clone(), token(), binding());
    let mut checkpoint = legacy.checkpoint();
    checkpoint.state = JobState::Validating;
    checkpoint.worker_stage = Some(WorkerStage::Inference);
    checkpoint.completion = Some(CandidateDeclaration::NativeBridgeV2(native_candidate()));
    assert!(matches!(
        JobLifecycle::from_checkpoint(checkpoint, Relevance::Current),
        Err(LifecycleError::InvalidCheckpoint(_))
    ));
}
