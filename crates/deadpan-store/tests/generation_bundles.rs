#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::io::Cursor;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use deadpan_core::{
    BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate, GeneratedContentId,
    GeneratedObjectRef, HoldAudio, HoldRecipe, HoldVideo, NodeId, PresentationBasis,
    ProjectDocument, ProjectId, RevisionId, Subtree,
};
use deadpan_jobs::{
    AttemptId, AxisLimits, BridgeCapability, BridgeGenerationPlan, CancellationAcknowledgement,
    CancellationToken, CandidateManifest, ConditioningMode, DimensionLimits, FrameCountFormula,
    HoldConstraints, JobState, MessageIdentity, MotionAmount, NativeCandidateManifest,
    NativeDimensions, ProtocolVersion, ProviderPackId, ProviderPackVersion, ProviderSelection,
    RequestId, RuntimeId, RuntimeVersion, Sha256, VideoSpec, WorkerMessage, WorkerStage,
    WorkspaceArtifact, WorkspaceRef,
};
use deadpan_store::generated_media::GeneratedMediaLimits;
use deadpan_store::generation::{GenerationRequestInput, StoredGenerationRequest};
use deadpan_store::generation_attempts::{
    AttemptMutationOutcome, AttemptValueError, BeginGenerationAttempt, BundleValidationReceipt,
    CandidateAvailability, CandidateValidationReceipt, ManagedCandidateRef, ValidatorIdentity,
};
use deadpan_store::{AccessMode, ProjectStore, StoreError};
use rusqlite::Connection;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

const NATIVE_BYTES: &[u8] = b"canonical native fixture";
const SAMPLED_BYTES: &[u8] = b"canonical sampled fixture";
const PROVENANCE_BYTES: &[u8] = b"bounded provenance fixture";

fn rate() -> FrameRate {
    FrameRate::new(30, 1).unwrap()
}

fn document() -> Result<ProjectDocument> {
    let mut document = ProjectDocument::new(
        ProjectId::new("project")?,
        RevisionId::new("initial")?,
        PresentationBasis {
            width: 512,
            height: 320,
            frame_rate: rate(),
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    let hold = NodeId::new("hold")?;
    document = deadpan_core::apply(
        &document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new("setup")?,
            command: Command::Insert {
                parent: document.root().clone(),
                index: 0,
                subtree: Subtree {
                    root: hold.clone(),
                    nodes: BTreeMap::from([(
                        hold,
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
    )?
    .forward
    .apply(&document)?;
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

fn constraints() -> HoldConstraints {
    HoldConstraints {
        video: VideoSpec::new(FrameDuration::new(12).unwrap(), rate(), 512, 320).unwrap(),
        conditioning: ConditioningMode::Bridge,
        motion: MotionAmount::Still,
    }
}

fn plan() -> BridgeGenerationPlan {
    BridgeGenerationPlan::new(
        FrameDuration::new(12).unwrap(),
        rate(),
        &BridgeCapability::new(
            true,
            FrameRate::new(24, 1).unwrap(),
            FrameCountFormula::new(1, 0, 2, 97).unwrap(),
            DimensionLimits::new(
                AxisLimits::new(512, 512, 1).unwrap(),
                AxisLimits::new(320, 320, 1).unwrap(),
            ),
        ),
        NativeDimensions::new(512, 320).unwrap(),
    )
    .unwrap()
}

fn same_contract_plan() -> BridgeGenerationPlan {
    let plan = BridgeGenerationPlan::new(
        FrameDuration::new(3).unwrap(),
        rate(),
        &BridgeCapability::new(
            true,
            rate(),
            FrameCountFormula::new(8, 3, 3, 11).unwrap(),
            DimensionLimits::new(
                AxisLimits::new(512, 512, 1).unwrap(),
                AxisLimits::new(320, 320, 1).unwrap(),
            ),
        ),
        NativeDimensions::new(512, 320).unwrap(),
    )
    .unwrap();
    assert_eq!(plan.native_frame_count(), 3);
    plan
}

fn allocate_bridge(
    store: &mut ProjectStore,
    request_id: &str,
    seed: u64,
) -> Result<StoredGenerationRequest> {
    Ok(store.record_bridge_generation_request(
        GenerationRequestInput {
            request_id: RequestId::new(request_id)?,
            expected_revision: store.snapshot()?.revision_id().clone(),
            hold_id: NodeId::new("hold")?,
            context_sha256: sha('a'),
            constraints: constraints(),
            provider: provider(seed),
        },
        plan(),
    )?)
}

fn allocate_legacy(store: &mut ProjectStore, request_id: &str) -> Result<StoredGenerationRequest> {
    Ok(store.allocate_generation_request(GenerationRequestInput {
        request_id: RequestId::new(request_id)?,
        expected_revision: store.snapshot()?.revision_id().clone(),
        hold_id: NodeId::new("hold")?,
        context_sha256: sha('b'),
        constraints: constraints(),
        provider: provider(99),
    })?)
}

fn begin(
    store: &mut ProjectStore,
    request: &StoredGenerationRequest,
    attempt: &str,
) -> Result<MessageIdentity> {
    let identity = MessageIdentity::new(request.request_id.clone(), AttemptId::new(attempt)?);
    store.begin_generation_attempt(BeginGenerationAttempt {
        identity: identity.clone(),
        cancellation_token: CancellationToken::new(format!("cancel-{attempt}"))?,
    })?;
    Ok(identity)
}

fn stage(
    store: &mut ProjectStore,
    identity: &MessageIdentity,
    protocol: ProtocolVersion,
    worker_stage: WorkerStage,
) -> Result<AttemptMutationOutcome> {
    Ok(
        store.record_generation_worker_message(&WorkerMessage::Stage {
            protocol,
            identity: identity.clone(),
            stage: worker_stage,
        })?,
    )
}

fn native_candidate(request: &StoredGenerationRequest) -> NativeCandidateManifest {
    let plan = request.bridge_plan.as_ref().unwrap();
    NativeCandidateManifest {
        native: WorkspaceArtifact::new(
            WorkspaceRef::new("outputs/native.mp4").unwrap(),
            sha('c'),
            101,
        )
        .unwrap(),
        provenance: WorkspaceArtifact::new(
            WorkspaceRef::new("outputs/provenance.json").unwrap(),
            sha('d'),
            202,
        )
        .unwrap(),
        video: VideoSpec::new(
            FrameDuration::new(i64::from(plan.native_frame_count())).unwrap(),
            plan.native_frame_rate(),
            plan.native_dimensions().width(),
            plan.native_dimensions().height(),
        )
        .unwrap(),
        provider: request.provider.clone(),
    }
}

fn same_contract_candidate() -> NativeCandidateManifest {
    NativeCandidateManifest {
        native: WorkspaceArtifact::new(
            WorkspaceRef::new("outputs/native-same.mp4").unwrap(),
            sha('7'),
            303,
        )
        .unwrap(),
        provenance: WorkspaceArtifact::new(
            WorkspaceRef::new("outputs/provenance-same.json").unwrap(),
            sha('8'),
            404,
        )
        .unwrap(),
        video: VideoSpec::new(FrameDuration::new(3).unwrap(), rate(), 512, 320).unwrap(),
        provider: provider(44),
    }
}

fn complete_bridge(
    store: &mut ProjectStore,
    identity: &MessageIdentity,
    candidate: &NativeCandidateManifest,
) -> Result {
    stage(store, identity, ProtocolVersion::V2, WorkerStage::Preflight)?;
    stage(store, identity, ProtocolVersion::V2, WorkerStage::Inference)?;
    store.record_generation_worker_message(&WorkerMessage::CompletedBridge {
        protocol: ProtocolVersion::V2,
        identity: identity.clone(),
        candidate: candidate.clone(),
    })?;
    Ok(())
}

fn object(bytes: &[u8]) -> GeneratedObjectRef {
    GeneratedObjectRef::new(
        GeneratedContentId::new(blake3::hash(bytes).to_hex().to_string()).unwrap(),
        bytes.len() as u64,
    )
    .unwrap()
}

fn media_limits() -> GeneratedMediaLimits {
    GeneratedMediaLimits::new(1024 * 1024).unwrap()
}

fn receipt(candidate: &NativeCandidateManifest) -> BundleValidationReceipt {
    BundleValidationReceipt::new(
        candidate,
        object(NATIVE_BYTES),
        object(SAMPLED_BYTES),
        object(PROVENANCE_BYTES),
        constraints().video,
        plan(),
        ValidatorIdentity::new("deadpan-media", "bridge-1").unwrap(),
    )
    .unwrap()
}

fn publish_bundle(store: &mut ProjectStore) -> Result {
    for bytes in [NATIVE_BYTES, SAMPLED_BYTES, PROVENANCE_BYTES] {
        store.promote_generated_object(&mut Cursor::new(bytes), &object(bytes), media_limits())?;
    }
    Ok(())
}

fn stored_path(package: &Path, reference: &GeneratedObjectRef) -> PathBuf {
    package
        .join("Media/Generated")
        .join(format!("blake3-{}", reference.content().digest()))
}

#[test]
fn modern_ready_requires_objects_and_eviction_survives_reopen() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("modern-ready.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let request = allocate_bridge(&mut store, "request", 1)?;
    let identity = begin(&mut store, &request, "attempt")?;
    let candidate = native_candidate(&request);
    complete_bridge(&mut store, &identity, &candidate)?;
    let receipt = receipt(&candidate);

    assert!(
        store
            .record_generation_bundle_ready(&identity, &candidate, receipt.clone(), media_limits(),)
            .is_err()
    );
    assert_eq!(
        store
            .generation_attempt(&identity)?
            .unwrap()
            .checkpoint
            .state,
        JobState::Validating
    );
    publish_bundle(&mut store)?;
    assert_eq!(
        store.record_generation_bundle_ready(
            &identity,
            &candidate,
            receipt.clone(),
            media_limits(),
        )?,
        AttemptMutationOutcome::Applied
    );
    assert_eq!(
        store.record_generation_bundle_ready(
            &identity,
            &candidate,
            receipt.clone(),
            media_limits(),
        )?,
        AttemptMutationOutcome::Duplicate
    );
    assert_eq!(
        store
            .selected_generation_bundle(&request.request_id)?
            .unwrap()
            .receipt,
        receipt
    );
    assert!(
        store
            .selected_generation_candidate(&request.request_id)?
            .is_none()
    );

    assert_eq!(
        store.mark_generation_bundle_evicted(&identity)?,
        AttemptMutationOutcome::Applied
    );
    assert_eq!(
        store.mark_generation_bundle_evicted(&identity)?,
        AttemptMutationOutcome::Duplicate
    );
    assert!(
        store
            .selected_generation_bundle(&request.request_id)?
            .is_none()
    );
    assert_eq!(
        store
            .generation_attempt(&identity)?
            .unwrap()
            .bundle_receipt
            .unwrap()
            .availability(),
        CandidateAvailability::Evicted
    );
    store.validate()?;
    drop(store);

    let reopened = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert!(
        reopened
            .selected_generation_bundle(&request.request_id)?
            .is_none()
    );
    assert_eq!(
        reopened
            .generation_attempt(&identity)?
            .unwrap()
            .bundle_receipt
            .unwrap()
            .availability(),
        CandidateAvailability::Evicted
    );
    reopened.validate()?;
    Ok(())
}

#[test]
fn protocol_kinds_cannot_cross_attempt_or_readiness_boundaries() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("cross-kind.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let bridge_request = allocate_bridge(&mut store, "bridge", 1)?;
    let bridge_identity = begin(&mut store, &bridge_request, "attempt")?;
    stage(
        &mut store,
        &bridge_identity,
        ProtocolVersion::V2,
        WorkerStage::Preflight,
    )?;
    stage(
        &mut store,
        &bridge_identity,
        ProtocolVersion::V2,
        WorkerStage::Inference,
    )?;
    let sampled = CandidateManifest {
        media: WorkspaceArtifact::new(WorkspaceRef::new("outputs/sampled.mp4")?, sha('e'), 300)?,
        video: constraints().video,
        provider: bridge_request.provider.clone(),
    };
    assert!(matches!(
        store.record_generation_worker_message(&WorkerMessage::Completed {
            protocol: ProtocolVersion::V1,
            identity: bridge_identity.clone(),
            candidate: sampled.clone(),
        }),
        Err(StoreError::GenerationAttempt(_))
    ));
    assert_eq!(
        store
            .generation_attempt(&bridge_identity)?
            .unwrap()
            .checkpoint
            .state,
        JobState::Running
    );
    assert!(matches!(
        store.record_generation_candidate_ready(
            &bridge_identity,
            &sampled,
            CandidateValidationReceipt::new(
                ManagedCandidateRef::new("Candidates/bridge/sampled.mp4")?,
                sampled.media.sha256().clone(),
                sampled.media.byte_length(),
                sampled.video.clone(),
                sampled.provider.clone(),
                ValidatorIdentity::new("deadpan-media", "legacy-1")?,
            )?,
        ),
        Err(StoreError::GenerationAttempt(_))
    ));

    let legacy_request = allocate_legacy(&mut store, "legacy")?;
    let legacy_identity = begin(&mut store, &legacy_request, "attempt")?;
    stage(
        &mut store,
        &legacy_identity,
        ProtocolVersion::V1,
        WorkerStage::Preflight,
    )?;
    stage(
        &mut store,
        &legacy_identity,
        ProtocolVersion::V1,
        WorkerStage::Inference,
    )?;
    assert!(matches!(
        store.record_generation_worker_message(&WorkerMessage::CompletedBridge {
            protocol: ProtocolVersion::V2,
            identity: legacy_identity.clone(),
            candidate: native_candidate(&bridge_request),
        }),
        Err(StoreError::GenerationAttempt(_))
    ));
    assert_eq!(
        store
            .generation_attempt(&legacy_identity)?
            .unwrap()
            .checkpoint
            .state,
        JobState::Running
    );
    Ok(())
}

#[test]
fn cancelled_and_stale_bridge_completions_never_select() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("cancel-stale.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;

    let cancelled_request = allocate_bridge(&mut store, "cancelled", 1)?;
    let cancelled = begin(&mut store, &cancelled_request, "attempt")?;
    stage(
        &mut store,
        &cancelled,
        ProtocolVersion::V2,
        WorkerStage::Preflight,
    )?;
    let token = CancellationToken::new("cancel-attempt")?;
    store.request_generation_attempt_cancel(&cancelled, &token)?;
    let candidate = native_candidate(&cancelled_request);
    assert_eq!(
        store.record_generation_worker_message(&WorkerMessage::CompletedBridge {
            protocol: ProtocolVersion::V2,
            identity: cancelled.clone(),
            candidate,
        })?,
        AttemptMutationOutcome::CompletionDiscardedDuringCancellation
    );
    let cancelling = store.generation_attempt(&cancelled)?.unwrap();
    assert_eq!(cancelling.checkpoint.state, JobState::Cancelling);
    assert!(matches!(
        cancelling.checkpoint.cancellation_acknowledgement,
        Some(CancellationAcknowledgement::CompletionDiscarded(_))
    ));
    store.finish_generation_attempt_cancelled(&cancelled, &token)?;
    assert_eq!(
        store
            .generation_attempt(&cancelled)?
            .unwrap()
            .checkpoint
            .state,
        JobState::Cancelled
    );

    let stale_request = allocate_bridge(&mut store, "stale", 2)?;
    let stale = begin(&mut store, &stale_request, "attempt")?;
    let stale_candidate = native_candidate(&stale_request);
    complete_bridge(&mut store, &stale, &stale_candidate)?;
    let replacement = allocate_bridge(&mut store, "replacement", 3)?;
    publish_bundle(&mut store)?;
    store.record_generation_bundle_ready(
        &stale,
        &stale_candidate,
        receipt(&stale_candidate),
        media_limits(),
    )?;
    assert_eq!(
        store.generation_attempt(&stale)?.unwrap().checkpoint.state,
        JobState::Ready
    );
    assert!(!store.generation_attempt(&stale)?.unwrap().selected);
    assert!(
        store
            .selected_generation_bundle(&stale_request.request_id)?
            .is_none()
    );
    assert_eq!(
        store
            .generation_request(&replacement.request_id)?
            .unwrap()
            .relevance,
        deadpan_jobs::Relevance::Current
    );
    Ok(())
}

#[test]
fn retry_supersedes_modern_selection_and_corrupt_objects_cannot_be_ready() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("retry-corrupt.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let request = allocate_bridge(&mut store, "request", 1)?;
    publish_bundle(&mut store)?;

    let first = begin(&mut store, &request, "attempt-1")?;
    let candidate = native_candidate(&request);
    complete_bridge(&mut store, &first, &candidate)?;
    let bundle_receipt = receipt(&candidate);
    store.record_generation_bundle_ready(
        &first,
        &candidate,
        bundle_receipt.clone(),
        media_limits(),
    )?;
    assert!(
        store
            .selected_generation_bundle(&request.request_id)?
            .is_some()
    );

    let second = begin(&mut store, &request, "attempt-2")?;
    assert_eq!(
        store
            .selected_generation_bundle(&request.request_id)?
            .unwrap()
            .identity,
        first
    );
    complete_bridge(&mut store, &second, &candidate)?;
    store.record_generation_bundle_ready(
        &second,
        &candidate,
        bundle_receipt.clone(),
        media_limits(),
    )?;
    assert_eq!(
        store
            .selected_generation_bundle(&request.request_id)?
            .unwrap()
            .identity,
        second
    );
    assert_eq!(
        store.select_generation_bundle_variant(&first)?,
        AttemptMutationOutcome::Applied
    );
    assert_eq!(
        store.select_generation_bundle_variant(&first)?,
        AttemptMutationOutcome::Duplicate
    );
    assert_eq!(
        store
            .selected_generation_bundle(&request.request_id)?
            .unwrap()
            .identity,
        first
    );
    store.mark_generation_bundle_evicted(&first)?;
    assert!(
        store
            .selected_generation_bundle(&request.request_id)?
            .is_none()
    );

    let third = begin(&mut store, &request, "attempt-3")?;
    complete_bridge(&mut store, &third, &candidate)?;
    let sampled_path = stored_path(&package, bundle_receipt.sampled_object());
    fs::set_permissions(&sampled_path, fs::Permissions::from_mode(0o600))?;
    fs::write(
        &sampled_path,
        vec![b'x'; bundle_receipt.sampled_object().byte_length() as usize],
    )?;
    fs::set_permissions(&sampled_path, fs::Permissions::from_mode(0o444))?;
    let error = store
        .record_generation_bundle_ready(&third, &candidate, bundle_receipt, media_limits())
        .unwrap_err();
    assert_eq!(error.code(), "GeneratedMediaHashMismatch");
    assert_eq!(
        store.generation_attempt(&third)?.unwrap().checkpoint.state,
        JobState::Validating
    );
    assert!(
        store
            .generation_attempt(&third)?
            .unwrap()
            .bundle_receipt
            .is_none()
    );
    Ok(())
}

#[test]
fn persisted_bundle_json_cannot_bypass_constructor_invariants() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("malformed-bundle.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let request = allocate_bridge(&mut store, "request", 1)?;
    let identity = begin(&mut store, &request, "attempt")?;
    let candidate = native_candidate(&request);
    complete_bridge(&mut store, &identity, &candidate)?;
    publish_bundle(&mut store)?;
    store.record_generation_bundle_ready(
        &identity,
        &candidate,
        receipt(&candidate),
        media_limits(),
    )?;

    let connection = Connection::open(package.join("project.sqlite"))?;
    let original: String =
        connection.query_row("SELECT bundle FROM generation_bundle_receipts", [], |row| {
            row.get(0)
        })?;
    for corruption in [
        "UPDATE generation_bundle_receipts
         SET bundle=json_set(bundle,'$.sampled_object',json_extract(bundle,'$.native_object'))",
        "UPDATE generation_bundle_receipts
         SET bundle=json_set(bundle,'$.provenance_object',json_extract(bundle,'$.native_object'))",
        "UPDATE generation_bundle_receipts
         SET bundle=json_set(bundle,'$.validator.id','')",
    ] {
        connection.execute(
            "UPDATE generation_bundle_receipts SET bundle=?1",
            [&original],
        )?;
        connection.execute_batch(corruption)?;
        assert!(matches!(
            store.validate(),
            Err(StoreError::Integrity(message))
                if message.contains("invalid bundle validation receipt")
        ));
    }
    for corruption in [
        format!(
            "UPDATE generation_bundle_receipts SET bundle=json_set(bundle,'$.native_sha256','{}')",
            "e".repeat(64)
        ),
        "UPDATE generation_bundle_receipts SET bundle=json_set(bundle,'$.native_byte_length',102)"
            .to_owned(),
        format!(
            "UPDATE generation_bundle_receipts SET bundle=json_set(bundle,'$.provenance_sha256','{}')",
            "f".repeat(64)
        ),
        "UPDATE generation_bundle_receipts SET bundle=json_set(bundle,'$.provenance_byte_length',203)"
            .to_owned(),
    ] {
        connection.execute(
            "UPDATE generation_bundle_receipts SET bundle=?1",
            [&original],
        )?;
        connection.execute_batch(&corruption)?;
        assert!(matches!(
            store.validate(),
            Err(StoreError::Integrity(message))
                if message.contains("bundle receipt does not match")
        ));
    }
    connection.execute(
        "UPDATE generation_bundle_receipts SET bundle=?1",
        [&original],
    )?;
    store.validate()?;
    Ok(())
}

#[test]
fn object_alias_requires_an_identical_video_contract() -> Result {
    let candidate = native_candidate(&StoredGenerationRequest {
        request_id: RequestId::new("request")?,
        origin_revision: RevisionId::new("origin")?,
        binding: deadpan_jobs::TargetBinding {
            project_id: ProjectId::new("project")?,
            hold_id: NodeId::new("hold")?,
            request_version: deadpan_jobs::RequestVersion::new(1)?,
            context_sha256: sha('a'),
        },
        constraints: constraints(),
        provider: provider(1),
        bridge_plan: Some(plan()),
        relevance: deadpan_jobs::Relevance::Current,
    });
    let shared = object(NATIVE_BYTES);
    assert_eq!(
        BundleValidationReceipt::new(
            &candidate,
            shared.clone(),
            shared.clone(),
            object(PROVENANCE_BYTES),
            constraints().video,
            plan(),
            ValidatorIdentity::new("deadpan-media", "bridge-1")?,
        )
        .unwrap_err(),
        AttemptValueError::BundleMetadataMismatch
    );

    let valid = receipt(&candidate);
    let mut invalid_wire = serde_json::to_value(&valid)?;
    invalid_wire["sampled_object"] = invalid_wire["native_object"].clone();
    assert!(serde_json::from_value::<BundleValidationReceipt>(invalid_wire).is_err());

    let same_contract_candidate = same_contract_candidate();
    let same_contract_video = same_contract_candidate.video.clone();
    let aliased = BundleValidationReceipt::new(
        &same_contract_candidate,
        shared.clone(),
        shared,
        object(PROVENANCE_BYTES),
        same_contract_video,
        same_contract_plan(),
        ValidatorIdentity::new("deadpan-media", "bridge-1")?,
    )?;
    assert_eq!(aliased.native_object(), aliased.sampled_object());
    assert_eq!(aliased.native_video(), aliased.sampled_video());
    let round_trip: BundleValidationReceipt =
        serde_json::from_value(serde_json::to_value(&aliased)?)?;
    assert_eq!(round_trip, aliased);
    Ok(())
}

#[test]
fn malformed_nonnull_plan_is_never_projected_as_a_legacy_request() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("malformed-plan.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let request = allocate_bridge(&mut store, "request", 1)?;
    let connection = Connection::open(package.join("project.sqlite"))?;
    connection.execute(
        "UPDATE generation_requests SET bridge_plan=42 WHERE request_id=?1",
        [request.request_id.as_str()],
    )?;
    assert!(matches!(
        store.generation_request(&request.request_id),
        Err(StoreError::Integrity(_))
    ));
    Ok(())
}
