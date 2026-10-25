use super::*;

fn extension_job() -> JobLifecycle {
    JobLifecycle::new_with_protocol(identity(), token(), binding(), ProtocolVersion::V3)
}

fn completed() -> WorkerMessage {
    WorkerMessage::CompletedExtension {
        protocol: ProtocolVersion::V3,
        identity: identity(),
        candidate: native_candidate(),
    }
}

#[test]
fn extension_completion_requires_exact_extension_validation_and_round_trips_checkpoint() {
    let mut job = extension_job();
    running(&mut job);
    job.apply_worker_message(&completed()).unwrap();
    assert_eq!(job.state(), JobState::Validating);
    assert!(!job.can_authorize_acceptance());
    assert!(job.candidate().is_none());
    assert!(job.candidate_bundle().is_none());
    assert_eq!(job.extension_candidate_bundle(), Some(&native_candidate()));
    assert_eq!(
        job.completion(),
        Some(&CandidateDeclaration::NativeExtensionV3(native_candidate()))
    );
    assert_eq!(
        job.host_bundle_validation_succeeded(&identity(), &native_candidate()),
        Err(LifecycleError::CandidateMismatch)
    );
    let mut changed = native_candidate();
    changed.provider.seed += 1;
    assert_eq!(
        job.host_extension_validation_succeeded(&identity(), &changed),
        Err(LifecycleError::CandidateMismatch)
    );
    let mut restored = JobLifecycle::from_checkpoint(job.checkpoint(), Relevance::Current).unwrap();
    assert_eq!(restored, job);
    restored
        .host_extension_validation_succeeded(&identity(), &native_candidate())
        .unwrap();
    assert_eq!(restored.state(), JobState::Ready);
    assert!(restored.can_authorize_acceptance());
    let ready = JobLifecycle::from_checkpoint(restored.checkpoint(), Relevance::Current).unwrap();
    assert_eq!(ready, restored);
    let stale = JobLifecycle::from_checkpoint(restored.checkpoint(), Relevance::Stale).unwrap();
    assert!(!stale.can_authorize_acceptance());
    assert_eq!(
        stale.extension_candidate_bundle(),
        Some(&native_candidate())
    );
}

#[test]
fn extension_late_completion_stays_discarded_through_checkpoint_and_reap() {
    let mut job = extension_job();
    running(&mut job);
    job.request_cancel(&identity(), &token()).unwrap();
    assert_eq!(
        job.apply_worker_message(&completed()).unwrap(),
        WorkerEventOutcome::CompletionDiscardedDuringCancellation
    );
    assert_eq!(
        job.apply_worker_message(&completed()).unwrap(),
        WorkerEventOutcome::Duplicate
    );
    assert!(job.extension_candidate_bundle().is_none());
    assert_eq!(
        job.cancellation_acknowledgement(),
        Some(&CancellationAcknowledgement::CompletionDiscarded(Box::new(
            CandidateDeclaration::NativeExtensionV3(native_candidate())
        )))
    );
    let mut restored = JobLifecycle::from_checkpoint(job.checkpoint(), Relevance::Current).unwrap();
    assert_eq!(restored, job);
    restored.host_cancelled(&identity(), &token()).unwrap();
    assert_eq!(restored.state(), JobState::Cancelled);
    assert!(!restored.can_authorize_acceptance());
    assert_eq!(
        JobLifecycle::from_checkpoint(restored.checkpoint(), Relevance::Current).unwrap(),
        restored
    );
    let mut crossed = job.checkpoint();
    crossed.protocol = ProtocolVersion::V2;
    assert!(matches!(
        JobLifecycle::from_checkpoint(crossed, Relevance::Current),
        Err(LifecycleError::InvalidCheckpoint(_))
    ));
}

#[test]
fn extension_lifecycle_rejects_crossed_completion_versions_and_attempts() {
    for message in [
        WorkerMessage::CompletedBridge {
            protocol: ProtocolVersion::V2,
            identity: identity(),
            candidate: native_candidate(),
        },
        WorkerMessage::CompletedBridge {
            protocol: ProtocolVersion::V3,
            identity: identity(),
            candidate: native_candidate(),
        },
        WorkerMessage::Completed {
            protocol: ProtocolVersion::V1,
            identity: identity(),
            candidate: candidate(),
        },
        WorkerMessage::CompletedExtension {
            protocol: ProtocolVersion::V2,
            identity: identity(),
            candidate: native_candidate(),
        },
        WorkerMessage::CompletedExtension {
            protocol: ProtocolVersion::V3,
            identity: MessageIdentity::new(
                RequestId::new("other-request").unwrap(),
                identity().attempt_id,
            ),
            candidate: native_candidate(),
        },
    ] {
        let mut job = extension_job();
        running(&mut job);
        let before = job.clone();
        assert!(job.apply_worker_message(&message).is_err());
        assert_eq!(job, before);
    }
    let mut job = extension_job();
    running(&mut job);
    job.apply_worker_message(&completed()).unwrap();
    for protocol in [ProtocolVersion::V1, ProtocolVersion::V2] {
        let mut crossed = job.checkpoint();
        crossed.protocol = protocol;
        assert!(matches!(
            JobLifecycle::from_checkpoint(crossed, Relevance::Current),
            Err(LifecycleError::InvalidCheckpoint(_))
        ));
    }
    let mut crossed = job.checkpoint();
    crossed.completion = Some(CandidateDeclaration::NativeBridgeV2(native_candidate()));
    assert!(matches!(
        JobLifecycle::from_checkpoint(crossed, Relevance::Current),
        Err(LifecycleError::InvalidCheckpoint(_))
    ));
}
