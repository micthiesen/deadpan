use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::Path;

use deadpan_core::{
    BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate, HoldAudio,
    HoldRecipe, HoldVideo, NodeId, PresentationBasis, ProjectDocument, ProjectId, RevisionId,
    Subtree,
};
use deadpan_jobs::{
    AttemptId, CancellationAcknowledgement, CancellationToken, CandidateManifest, ConditioningMode,
    Diagnostic, FailureCode, HoldConstraints, HostFailure, HostFailureCode, JobFailure, JobState,
    MessageIdentity, MotionAmount, ProtocolVersion, ProviderPackId, ProviderPackVersion,
    ProviderSelection, RequestId, RuntimeId, RuntimeVersion, Sha256, StageProgress, VideoSpec,
    WorkerFailure, WorkerMessage, WorkerStage, WorkspaceArtifact, WorkspaceRef,
};
use deadpan_store::generation::{GenerationRequestInput, StoredGenerationRequest};
use deadpan_store::generation_attempts::{
    AttemptMutationOutcome, BeginGenerationAttempt, CandidateAvailability,
    CandidateValidationReceipt, ManagedCandidateRef, ValidatorIdentity,
};
use deadpan_store::{AccessMode, ProjectStore, StoreError};
use rusqlite::{Connection, params};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn rate() -> FrameRate {
    FrameRate::new(30, 1).unwrap()
}

fn document(hold_count: usize) -> Result<ProjectDocument> {
    let mut document = ProjectDocument::new(
        ProjectId::new("project")?,
        RevisionId::new("initial")?,
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: rate(),
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    for index in 0..hold_count {
        let node = NodeId::new(format!("hold-{index}"))?;
        let edit = deadpan_core::apply(
            &document,
            &CommandRequest {
                project_id: document.project_id().clone(),
                expected_revision: document.revision_id().clone(),
                new_revision: RevisionId::new(format!("setup-{index}"))?,
                command: Command::Insert {
                    parent: document.root().clone(),
                    index,
                    subtree: Subtree {
                        root: node.clone(),
                        nodes: BTreeMap::from([(
                            node,
                            BeatNode::hold(
                                "Pause",
                                HoldRecipe {
                                    duration: FrameDuration::new(12)?,
                                    video: HoldVideo::Background,
                                    audio: HoldAudio::Silence,
                                },
                            ),
                        )]),
                        overrides: BTreeMap::new(),
                    },
                },
            },
        )?;
        document = edit.forward.apply(&document)?;
    }
    Ok(document)
}

fn sha(character: char) -> Sha256 {
    Sha256::new(character.to_string().repeat(64)).unwrap()
}

fn provider(seed: u64) -> ProviderSelection {
    ProviderSelection {
        pack_id: ProviderPackId::new("pack").unwrap(),
        pack_version: ProviderPackVersion::new("v1").unwrap(),
        runtime_id: RuntimeId::new("runtime").unwrap(),
        runtime_version: RuntimeVersion::new("v1").unwrap(),
        seed,
    }
}

fn video() -> VideoSpec {
    VideoSpec::new(FrameDuration::new(12).unwrap(), rate(), 512, 320).unwrap()
}

fn allocate(
    store: &mut ProjectStore,
    request: &str,
    hold_index: usize,
    seed: u64,
) -> Result<StoredGenerationRequest> {
    let context_character = char::from(b"abcdef0123456789"[hold_index]);
    Ok(store.allocate_generation_request(GenerationRequestInput {
        request_id: RequestId::new(request)?,
        expected_revision: store.snapshot()?.revision_id().clone(),
        hold_id: NodeId::new(format!("hold-{hold_index}"))?,
        context_sha256: sha(context_character),
        constraints: HoldConstraints {
            video: video(),
            conditioning: ConditioningMode::Bridge,
            motion: MotionAmount::Still,
        },
        provider: provider(seed),
    })?)
}

fn identity(request: &StoredGenerationRequest, attempt: &str) -> MessageIdentity {
    MessageIdentity::new(request.request_id.clone(), AttemptId::new(attempt).unwrap())
}

fn begin(
    store: &mut ProjectStore,
    request: &StoredGenerationRequest,
    attempt: &str,
) -> Result<MessageIdentity> {
    let identity = identity(request, attempt);
    store.begin_generation_attempt(BeginGenerationAttempt {
        identity: identity.clone(),
        cancellation_token: CancellationToken::new(format!("cancel-{attempt}"))?,
    })?;
    Ok(identity)
}

fn stage(identity: &MessageIdentity, stage: WorkerStage) -> WorkerMessage {
    WorkerMessage::Stage {
        protocol: ProtocolVersion::V1,
        identity: identity.clone(),
        stage,
    }
}

fn run(store: &mut ProjectStore, identity: &MessageIdentity) -> Result {
    for worker_stage in [
        WorkerStage::Preflight,
        WorkerStage::ModelLoading,
        WorkerStage::Inference,
    ] {
        store.record_generation_worker_message(&stage(identity, worker_stage))?;
    }
    Ok(())
}

fn candidate(request: &StoredGenerationRequest, hash: Sha256) -> CandidateManifest {
    CandidateManifest {
        media: WorkspaceArtifact::new(
            WorkspaceRef::new("outputs/candidate.mp4").unwrap(),
            hash,
            8_192,
        )
        .unwrap(),
        video: request.constraints.video.clone(),
        provider: request.provider.clone(),
    }
}

fn complete(
    store: &mut ProjectStore,
    identity: &MessageIdentity,
    candidate: &CandidateManifest,
) -> Result {
    run(store, identity)?;
    store.record_generation_worker_message(&WorkerMessage::Completed {
        protocol: ProtocolVersion::V1,
        identity: identity.clone(),
        candidate: candidate.clone(),
    })?;
    Ok(())
}

fn receipt(path: &str, candidate: &CandidateManifest) -> CandidateValidationReceipt {
    CandidateValidationReceipt::new(
        ManagedCandidateRef::new(path).unwrap(),
        candidate.media.sha256().clone(),
        candidate.media.byte_length(),
        candidate.video.clone(),
        candidate.provider.clone(),
        ValidatorIdentity::new("deadpan-media", "1.0").unwrap(),
    )
    .unwrap()
}

#[test]
fn retries_retain_variants_and_selection_is_explicit() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("variants.deadpan");
    let mut store = ProjectStore::create(&path, &document(1)?)?;
    let request = allocate(&mut store, "request", 0, 7)?;
    let shared_hash = sha('f');

    let first = begin(&mut store, &request, "attempt-1")?;
    let first_candidate = candidate(&request, shared_hash.clone());
    complete(&mut store, &first, &first_candidate)?;
    assert_eq!(
        store.record_generation_candidate_ready(
            &first,
            &first_candidate,
            receipt("Candidates/request/attempt-1.mp4", &first_candidate),
        )?,
        AttemptMutationOutcome::Applied
    );
    assert_eq!(
        store
            .selected_generation_candidate(&request.request_id)?
            .unwrap()
            .identity,
        first
    );

    let second = begin(&mut store, &request, "attempt-2")?;
    let second_candidate = candidate(&request, shared_hash);
    complete(&mut store, &second, &second_candidate)?;
    assert!(matches!(
        store.record_generation_candidate_ready(
            &second,
            &second_candidate,
            receipt("Candidates/request/attempt-1.mp4", &second_candidate),
        ),
        Err(StoreError::GenerationAttempt(message)) if message.contains("already been used")
    ));
    assert_eq!(
        store.generation_attempt(&second)?.unwrap().checkpoint.state,
        JobState::Validating
    );
    let second_receipt = receipt("Candidates/request/attempt-2.mp4", &second_candidate);
    store.record_generation_candidate_ready(&second, &second_candidate, second_receipt.clone())?;
    assert_eq!(
        store.record_generation_candidate_ready(
            &second,
            &second_candidate,
            second_receipt.clone(),
        )?,
        AttemptMutationOutcome::Duplicate
    );
    assert_eq!(
        store
            .selected_generation_candidate(&request.request_id)?
            .unwrap()
            .identity,
        second
    );

    let attempts = store.generation_attempts(&request.request_id, 0, 10)?;
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0].ordinal, 1);
    assert_eq!(attempts[1].ordinal, 2);
    assert_eq!(attempts[1].receipt, Some(second_receipt));
    assert!(attempts[1].selected);

    store.select_generation_variant(&first)?;
    assert_eq!(
        store
            .selected_generation_candidate(&request.request_id)?
            .unwrap()
            .identity,
        first
    );
    store.mark_generation_candidate_evicted(&first)?;
    assert!(
        store
            .selected_generation_candidate(&request.request_id)?
            .is_none()
    );
    assert!(matches!(
        store.select_generation_variant(&first),
        Err(StoreError::GenerationAttempt(_))
    ));
    assert_eq!(
        store
            .generation_attempt(&first)?
            .unwrap()
            .receipt
            .unwrap()
            .availability(),
        CandidateAvailability::Evicted
    );
    store.validate()?;
    Ok(())
}

#[test]
fn exact_duplicates_are_idempotent_and_progress_is_not_persisted() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("duplicates.deadpan");
    let mut store = ProjectStore::create(&path, &document(2)?)?;
    let request = allocate(&mut store, "request", 0, 1)?;
    let identity = begin(&mut store, &request, "attempt")?;
    let preflight = stage(&identity, WorkerStage::Preflight);
    assert_eq!(
        store.record_generation_worker_message(&preflight)?,
        AttemptMutationOutcome::Applied
    );
    assert_eq!(
        store.record_generation_worker_message(&preflight)?,
        AttemptMutationOutcome::Duplicate
    );
    assert!(matches!(
        store.record_generation_worker_message(&WorkerMessage::Progress {
            protocol: ProtocolVersion::V1,
            identity: identity.clone(),
            stage: WorkerStage::Preflight,
            progress: StageProgress::new(1, 2)?,
        }),
        Err(StoreError::GenerationProgressNotPersistent)
    ));
    store.record_generation_worker_message(&stage(&identity, WorkerStage::ModelLoading))?;
    assert!(matches!(
        store.record_generation_worker_message(&preflight),
        Err(StoreError::GenerationAttempt(_))
    ));
    store.record_generation_worker_message(&stage(&identity, WorkerStage::Inference))?;
    let completed = WorkerMessage::Completed {
        protocol: ProtocolVersion::V1,
        identity: identity.clone(),
        candidate: candidate(&request, sha('d')),
    };
    store.record_generation_worker_message(&completed)?;
    assert_eq!(
        store.record_generation_worker_message(&completed)?,
        AttemptMutationOutcome::Duplicate
    );
    let conflicting = WorkerMessage::Completed {
        protocol: ProtocolVersion::V1,
        identity: identity.clone(),
        candidate: candidate(&request, sha('e')),
    };
    assert!(matches!(
        store.record_generation_worker_message(&conflicting),
        Err(StoreError::GenerationAttempt(_))
    ));
    let host_failure = HostFailure {
        code: HostFailureCode::OutputValidationFailed,
        detail: Diagnostic::new("candidate failed output validation")?,
    };
    assert_eq!(
        store.fail_generation_attempt(&identity, host_failure.clone())?,
        AttemptMutationOutcome::Applied
    );
    assert_eq!(
        store.fail_generation_attempt(&identity, host_failure)?,
        AttemptMutationOutcome::Duplicate
    );
    assert!(matches!(
        store.fail_generation_attempt(
            &identity,
            HostFailure {
                code: HostFailureCode::ProtocolViolation,
                detail: Diagnostic::new("different terminal reason")?,
            },
        ),
        Err(StoreError::GenerationAttempt(_))
    ));
    let wrong = MessageIdentity::new(request.request_id.clone(), AttemptId::new("wrong-attempt")?);
    assert!(matches!(
        store.record_generation_worker_message(&stage(&wrong, WorkerStage::Preflight)),
        Err(StoreError::GenerationAttemptNotFound { .. })
    ));

    let cancel_request = allocate(&mut store, "cancel-request", 1, 2)?;
    let cancelling = begin(&mut store, &cancel_request, "attempt")?;
    let token = CancellationToken::new("cancel-attempt")?;
    let before_wrong_token = store.generation_attempt(&cancelling)?.unwrap();
    assert!(matches!(
        store.request_generation_attempt_cancel(
            &cancelling,
            &CancellationToken::new("wrong-token")?,
        ),
        Err(StoreError::GenerationAttempt(_))
    ));
    assert_eq!(
        store.generation_attempt(&cancelling)?.unwrap(),
        before_wrong_token
    );
    store.request_generation_attempt_cancel(&cancelling, &token)?;
    let discarded = WorkerMessage::Completed {
        protocol: ProtocolVersion::V1,
        identity: cancelling.clone(),
        candidate: candidate(&cancel_request, sha('a')),
    };
    assert_eq!(
        store.record_generation_worker_message(&discarded)?,
        AttemptMutationOutcome::CompletionDiscardedDuringCancellation
    );
    assert_eq!(
        store.record_generation_worker_message(&discarded)?,
        AttemptMutationOutcome::Duplicate
    );
    let before_reap = store.generation_attempt(&cancelling)?.unwrap();
    assert_eq!(before_reap.checkpoint.state, JobState::Cancelling);
    assert!(matches!(
        before_reap.checkpoint.cancellation_acknowledgement,
        Some(CancellationAcknowledgement::CompletionDiscarded(_))
    ));
    assert!(matches!(
        store.record_generation_worker_message(&WorkerMessage::Failed {
            protocol: ProtocolVersion::V1,
            identity: cancelling.clone(),
            failure: WorkerFailure {
                code: FailureCode::BackendFailure,
                detail: Diagnostic::new("conflicting terminal response")?,
            },
        }),
        Err(StoreError::GenerationAttempt(_))
    ));
    assert_eq!(store.generation_attempt(&cancelling)?.unwrap(), before_reap);
    store.finish_generation_attempt_cancelled(&cancelling, &token)?;
    assert_eq!(
        store.finish_generation_attempt_cancelled(&cancelling, &token)?,
        AttemptMutationOutcome::Duplicate
    );
    assert_eq!(
        store
            .generation_attempt(&cancelling)?
            .unwrap()
            .checkpoint
            .state,
        JobState::Cancelled
    );

    let overflow = begin(&mut store, &cancel_request, "attempt-2")?;
    let connection = Connection::open(path.join("project.sqlite"))?;
    connection.execute(
        "UPDATE generation_attempts SET transition_sequence=?1
         WHERE request_id=?2 AND attempt_id=?3",
        params![
            i64::MAX,
            overflow.request_id.as_str(),
            overflow.attempt_id.as_str()
        ],
    )?;
    drop(connection);
    assert!(matches!(
        store.record_generation_worker_message(&stage(&overflow, WorkerStage::Preflight)),
        Err(StoreError::GenerationAttempt(message)) if message.contains("sequence is exhausted")
    ));
    let unchanged = store.generation_attempt(&overflow)?.unwrap();
    assert_eq!(unchanged.checkpoint.state, JobState::Queued);
    assert_eq!(unchanged.transition_sequence, i64::MAX as u64);
    Ok(())
}

#[test]
fn receipt_mismatch_and_late_selection_roll_back_atomically() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("receipt-rollback.deadpan");
    let mut store = ProjectStore::create(&path, &document(1)?)?;
    let request = allocate(&mut store, "request", 0, 1)?;
    let identity = begin(&mut store, &request, "attempt")?;
    let candidate = candidate(&request, sha('a'));
    complete(&mut store, &identity, &candidate)?;

    let wrong = CandidateValidationReceipt::new(
        ManagedCandidateRef::new("Candidates/request/wrong.mp4")?,
        sha('b'),
        candidate.media.byte_length(),
        candidate.video.clone(),
        candidate.provider.clone(),
        ValidatorIdentity::new("deadpan-media", "1.0")?,
    )?;
    assert!(matches!(
        store.record_generation_candidate_ready(&identity, &candidate, wrong),
        Err(StoreError::GenerationAttempt(_))
    ));
    let unchanged = store.generation_attempt(&identity)?.unwrap();
    assert_eq!(unchanged.checkpoint.state, JobState::Validating);
    assert!(unchanged.receipt.is_none());

    let fault = Connection::open(path.join("project.sqlite"))?;
    fault.execute_batch(
        "CREATE TRIGGER fail_ready_selection
         BEFORE UPDATE OF selected_ready_attempt_id ON generation_attempt_heads
         BEGIN SELECT RAISE(ABORT,'injected selection failure'); END;",
    )?;
    let valid = receipt("Candidates/request/valid.mp4", &candidate);
    assert!(matches!(
        store.record_generation_candidate_ready(&identity, &candidate, valid),
        Err(StoreError::Database(_))
    ));
    fault.execute_batch("DROP TRIGGER fail_ready_selection")?;
    let rolled_back = store.generation_attempt(&identity)?.unwrap();
    assert_eq!(rolled_back.checkpoint.state, JobState::Validating);
    assert!(rolled_back.receipt.is_none());
    Ok(())
}

#[test]
fn writable_reopen_interrupts_every_nonterminal_state_but_read_only_does_not() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("recovery.deadpan");
    let mut store = ProjectStore::create(&path, &document(8)?)?;
    let mut identities = Vec::new();
    for index in 0..8 {
        let request = allocate(&mut store, &format!("request-{index}"), index, index as u64)?;
        let identity = begin(&mut store, &request, "attempt")?;
        identities.push((request, identity));
    }
    store.record_generation_worker_message(&stage(&identities[1].1, WorkerStage::Preflight))?;
    for worker_stage in [WorkerStage::Preflight, WorkerStage::ModelLoading] {
        store.record_generation_worker_message(&stage(&identities[2].1, worker_stage))?;
    }
    run(&mut store, &identities[3].1)?;
    let validating_candidate = candidate(&identities[4].0, sha('e'));
    complete(&mut store, &identities[4].1, &validating_candidate)?;
    let cancelling_token = CancellationToken::new("cancel-attempt")?;
    store.request_generation_attempt_cancel(&identities[5].1, &cancelling_token)?;

    let ready_candidate = candidate(&identities[6].0, sha('9'));
    complete(&mut store, &identities[6].1, &ready_candidate)?;
    store.record_generation_candidate_ready(
        &identities[6].1,
        &ready_candidate,
        receipt("Candidates/recovery/ready.mp4", &ready_candidate),
    )?;
    let cancelled_token = CancellationToken::new("cancel-attempt")?;
    store.request_generation_attempt_cancel(&identities[7].1, &cancelled_token)?;
    store.finish_generation_attempt_cancelled(&identities[7].1, &cancelled_token)?;
    drop(store);

    let mut read_only = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    for (_, identity) in &identities[..6] {
        assert!(
            !read_only
                .generation_attempt(identity)?
                .unwrap()
                .checkpoint
                .state
                .is_terminal()
        );
    }
    assert!(matches!(
        read_only.begin_generation_attempt(BeginGenerationAttempt {
            identity: MessageIdentity::new(
                identities[0].0.request_id.clone(),
                AttemptId::new("read-only-attempt")?,
            ),
            cancellation_token: CancellationToken::new("read-only-token")?,
        }),
        Err(StoreError::ReadOnly)
    ));
    drop(read_only);

    let mut recovered = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    for (_, identity) in &identities[..6] {
        let attempt = recovered.generation_attempt(identity)?.unwrap();
        assert_eq!(attempt.checkpoint.state, JobState::Failed);
        assert!(matches!(
            attempt.checkpoint.failure,
            Some(JobFailure::Host(ref failure)) if failure.code == HostFailureCode::Interrupted
        ));
    }
    assert_eq!(
        recovered
            .generation_attempt(&identities[6].1)?
            .unwrap()
            .checkpoint
            .state,
        JobState::Ready
    );
    assert_eq!(
        recovered
            .generation_attempt(&identities[7].1)?
            .unwrap()
            .checkpoint
            .state,
        JobState::Cancelled
    );
    let retry = begin(&mut recovered, &identities[0].0, "attempt-2")?;
    assert_eq!(recovered.generation_attempt(&retry)?.unwrap().ordinal, 2);
    assert!(matches!(
        recovered.begin_generation_attempt(BeginGenerationAttempt {
            identity: identities[0].1.clone(),
            cancellation_token: CancellationToken::new("new-token")?,
        }),
        Err(StoreError::GenerationAttemptReused { .. })
    ));
    Ok(())
}

#[test]
fn relevance_is_independent_and_stale_requests_cannot_select() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("stale.deadpan");
    let mut store = ProjectStore::create(&path, &document(1)?)?;
    let request = allocate(&mut store, "request", 0, 1)?;
    let identity = begin(&mut store, &request, "attempt")?;
    let candidate = candidate(&request, sha('a'));
    complete(&mut store, &identity, &candidate)?;
    let replacement = allocate(&mut store, "replacement-request", 0, 2)?;
    store.record_generation_candidate_ready(
        &identity,
        &candidate,
        receipt("Candidates/stale/candidate.mp4", &candidate),
    )?;
    assert_eq!(
        store
            .generation_attempt(&identity)?
            .unwrap()
            .checkpoint
            .state,
        JobState::Ready
    );
    assert!(!store.generation_attempt(&identity)?.unwrap().selected);
    assert!(
        store
            .selected_generation_candidate(&request.request_id)?
            .is_none()
    );
    assert!(matches!(
        store.select_generation_variant(&identity),
        Err(StoreError::GenerationAttempt(_))
    ));
    assert!(matches!(
        begin(&mut store, &request, "attempt-2"),
        Err(error) if matches!(error.downcast_ref::<StoreError>(), Some(StoreError::GenerationAttempt(_)))
    ));
    let replacement_attempt = begin(&mut store, &replacement, "attempt-1")?;
    assert_eq!(
        store
            .generation_attempt(&replacement_attempt)?
            .unwrap()
            .checkpoint
            .state,
        JobState::Queued
    );
    Ok(())
}

#[test]
fn recovery_of_multiple_attempts_rolls_back_if_one_sequence_is_exhausted() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("recovery-rollback.deadpan");
    let mut store = ProjectStore::create(&path, &document(2)?)?;
    let first_request = allocate(&mut store, "first-request", 0, 1)?;
    let second_request = allocate(&mut store, "second-request", 1, 2)?;
    let first = begin(&mut store, &first_request, "attempt")?;
    let second = begin(&mut store, &second_request, "attempt")?;
    drop(store);

    let connection = Connection::open(path.join("project.sqlite"))?;
    connection.execute(
        "UPDATE generation_attempts SET transition_sequence=?1
         WHERE request_id=?2 AND attempt_id=?3",
        params![
            i64::MAX,
            first.request_id.as_str(),
            first.attempt_id.as_str()
        ],
    )?;
    drop(connection);
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadWrite),
        Err(StoreError::Database(_))
    ));

    let read_only = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(
        read_only
            .generation_attempt(&first)?
            .unwrap()
            .checkpoint
            .state,
        JobState::Queued
    );
    assert_eq!(
        read_only
            .generation_attempt(&second)?
            .unwrap()
            .checkpoint
            .state,
        JobState::Queued
    );
    Ok(())
}

#[test]
fn validation_rejects_indexless_live_duplicates_and_orphan_receipts() -> Result {
    let scratch = tempfile::tempdir()?;
    let duplicate_path = scratch.path().join("duplicate.deadpan");
    let mut store = ProjectStore::create(&duplicate_path, &document(1)?)?;
    let request = allocate(&mut store, "request", 0, 1)?;
    begin(&mut store, &request, "attempt-1")?;
    let connection = Connection::open(duplicate_path.join("project.sqlite"))?;
    connection.execute_batch("DROP INDEX one_nonterminal_generation_attempt_per_request")?;
    drop(connection);
    assert!(matches!(
        begin(&mut store, &request, "attempt-2"),
        Err(error) if matches!(error.downcast_ref::<StoreError>(), Some(StoreError::GenerationAttempt(message)) if message.contains("nonterminal"))
    ));
    drop(store);
    let connection = Connection::open(duplicate_path.join("project.sqlite"))?;
    connection.execute(
        "INSERT INTO generation_attempts(
            request_id,attempt_id,ordinal,cancellation_token,state,transition_sequence
         ) VALUES (?1,'attempt-2',2,'cancel-2','queued',1)",
        [request.request_id.as_str()],
    )?;
    connection.execute(
        "UPDATE generation_attempt_heads SET high_water=2,latest_attempt_id='attempt-2'
         WHERE request_id=?1",
        [request.request_id.as_str()],
    )?;
    drop(connection);
    assert!(matches!(
        ProjectStore::open(&duplicate_path, AccessMode::ReadOnly),
        Err(StoreError::Integrity(message)) if message.contains("multiple nonterminal")
    ));

    let orphan_path = scratch.path().join("orphan.deadpan");
    let store = ProjectStore::create(&orphan_path, &document(1)?)?;
    drop(store);
    let connection = Connection::open(orphan_path.join("project.sqlite"))?;
    connection.execute_batch(
        "ALTER TABLE generation_candidate_receipts RENAME TO old_generation_candidate_receipts;
         CREATE TABLE generation_candidate_receipts (
            request_id TEXT NOT NULL, attempt_id TEXT NOT NULL, staged_ref TEXT NOT NULL,
            sha256 TEXT NOT NULL, byte_length INTEGER NOT NULL, video TEXT NOT NULL,
            provider TEXT NOT NULL, validator_id TEXT NOT NULL, validator_version TEXT NOT NULL,
            availability TEXT NOT NULL
         ) STRICT;",
    )?;
    connection.execute(
        "INSERT INTO generation_candidate_receipts VALUES(
            'missing-request','missing-attempt','Candidates/orphan/file.mp4',?1,1,?2,?3,
            'deadpan-media','1.0','present')",
        params![
            sha('a').as_str(),
            serde_json::to_string(&video())?,
            serde_json::to_string(&provider(1))?
        ],
    )?;
    connection.execute_batch("DROP TABLE old_generation_candidate_receipts")?;
    drop(connection);
    assert!(matches!(
        ProjectStore::open(&orphan_path, AccessMode::ReadOnly),
        Err(StoreError::Integrity(message)) if message.contains("receipt has no attempt")
    ));

    let optional_path = scratch.path().join("invalid-optional.deadpan");
    let mut store = ProjectStore::create(&optional_path, &document(1)?)?;
    let request = allocate(&mut store, "request", 0, 1)?;
    let identity = begin(&mut store, &request, "attempt")?;
    drop(store);
    let connection = Connection::open(optional_path.join("project.sqlite"))?;
    connection.execute_batch("PRAGMA ignore_check_constraints=ON")?;
    connection.execute(
        "UPDATE generation_attempts SET worker_stage='unknown_stage'
         WHERE request_id=?1 AND attempt_id=?2",
        params![identity.request_id.as_str(), identity.attempt_id.as_str()],
    )?;
    drop(connection);
    assert!(matches!(
        ProjectStore::open(&optional_path, AccessMode::ReadOnly),
        Err(StoreError::Integrity(message)) if message.contains("wrong type")
    ));

    let declaration_path = scratch.path().join("early-candidate.deadpan");
    let mut store = ProjectStore::create(&declaration_path, &document(1)?)?;
    let request = allocate(&mut store, "request", 0, 1)?;
    let identity = begin(&mut store, &request, "attempt")?;
    drop(store);
    let connection = Connection::open(declaration_path.join("project.sqlite"))?;
    connection.execute(
        "UPDATE generation_attempts SET worker_candidate=?1
         WHERE request_id=?2 AND attempt_id=?3",
        params![
            serde_json::to_string(&candidate(&request, sha('a')))?,
            identity.request_id.as_str(),
            identity.attempt_id.as_str()
        ],
    )?;
    drop(connection);
    assert!(matches!(
        ProjectStore::open(&declaration_path, AccessMode::ReadOnly),
        Err(StoreError::Integrity(message)) if message.contains("before completion")
    ));
    Ok(())
}

#[test]
fn copied_project_never_infers_live_process_ownership() -> Result {
    let scratch = tempfile::tempdir()?;
    let source = scratch.path().join("source.deadpan");
    let copy = scratch.path().join("copy.deadpan");
    let mut store = ProjectStore::create(&source, &document(1)?)?;
    let request = allocate(&mut store, "request", 0, 1)?;
    let identity = begin(&mut store, &request, "attempt")?;
    run(&mut store, &identity)?;
    drop(store);
    copy_directory(&source, &copy)?;

    let copied_read_only = ProjectStore::open(&copy, AccessMode::ReadOnly)?;
    assert_eq!(
        copied_read_only
            .generation_attempt(&identity)?
            .unwrap()
            .checkpoint
            .state,
        JobState::Running
    );
    drop(copied_read_only);
    let copied_writer = ProjectStore::open(&copy, AccessMode::ReadWrite)?;
    assert_eq!(
        copied_writer
            .generation_attempt(&identity)?
            .unwrap()
            .checkpoint
            .state,
        JobState::Failed
    );
    drop(copied_writer);

    let source_read_only = ProjectStore::open(&source, AccessMode::ReadOnly)?;
    assert_eq!(
        source_read_only
            .generation_attempt(&identity)?
            .unwrap()
            .checkpoint
            .state,
        JobState::Running
    );
    Ok(())
}

fn copy_directory(source: &Path, destination: &Path) -> Result {
    fs::create_dir(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_directory(&source_path, &destination_path)?;
        } else {
            fs::copy(source_path, destination_path)?;
        }
    }
    Ok(())
}
