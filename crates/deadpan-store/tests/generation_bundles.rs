#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::io::Cursor;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use deadpan_core::{
    AssetId, BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate,
    GeneratedContentId, GeneratedObjectRef, HoldAudio, HoldRecipe, HoldVideo, NodeId, NodeKind,
    PresentationBasis, ProjectDocument, ProjectId, RevisionId, SourceSpan, SourceTimeBase,
    SourceTimestamp, Subtree,
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
use deadpan_store::generation::{
    ContextObservation, GenerationRequestInput, RelevanceObservation, RelevancePlan,
    StoredGenerationRequest,
};
use deadpan_store::generation_acceptance::GenerationAcceptance;
use deadpan_store::generation_attempts::{
    AttemptMutationOutcome, AttemptValueError, BeginGenerationAttempt, BundleAdmissionEvidence,
    BundleInputObjects, BundleValidationReceipt, CandidateAvailability, CandidateValidationReceipt,
    ManagedCandidateRef, ValidatorIdentity,
};
use deadpan_store::{AccessMode, ProjectStore, StoreError};
use rusqlite::Connection;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

#[path = "generation_bundles/edited_slice.rs"]
mod edited_slice;
#[path = "generation_bundles/move_range.rs"]
mod move_range;
#[path = "generation_bundles/preparations.rs"]
mod preparations;
#[path = "generation_bundles/retention.rs"]
mod retention;
#[path = "generation_bundles/storage.rs"]
mod storage;

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
        instructions: None,
        region_target: None,
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
    native_candidate_for(request, 1)
}

/// The declaration of attempt `ordinal`, a seeded variant of the request.
fn native_candidate_for(
    request: &StoredGenerationRequest,
    ordinal: u64,
) -> NativeCandidateManifest {
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
        provider: request.provider.for_attempt(ordinal),
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
    let second_candidate = native_candidate_for(&request, 2);
    complete_bridge(&mut store, &second, &second_candidate)?;
    store.record_generation_bundle_ready(
        &second,
        &second_candidate,
        receipt(&second_candidate),
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
    let candidate = native_candidate_for(&request, 3);
    complete_bridge(&mut store, &third, &candidate)?;
    let bundle_receipt = receipt(&candidate);
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
        scope_id: deadpan_store::generation::GenerationScopeId::from_first_request(RequestId::new(
            "request",
        )?),
        origin_target: deadpan_core::ScopedNodeTarget {
            node: NodeId::new("hold")?,
            repeats: vec![],
        },
        target: deadpan_core::ScopedNodeTarget {
            node: NodeId::new("hold")?,
            repeats: vec![],
        },
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

const INPUT_BYTES: [&[u8]; 3] = [
    b"context manifest fixture",
    b"left prepared fixture",
    b"right prepared fixture",
];

#[test]
fn admission_wire_rejects_alias_conflicts_and_unchecked_spans() -> Result {
    let inputs = BundleInputObjects::new(
        sha('a'),
        object(INPUT_BYTES[0]),
        object(INPUT_BYTES[1]),
        object(INPUT_BYTES[1]),
    )?;
    assert_eq!(inputs.left(), inputs.right());
    assert!(
        BundleInputObjects::new(
            sha('a'),
            object(INPUT_BYTES[0]),
            object(INPUT_BYTES[0]),
            object(INPUT_BYTES[1])
        )
        .is_err()
    );
    let candidate = same_contract_candidate();
    let base = BundleValidationReceipt::new(
        &candidate,
        object(NATIVE_BYTES),
        object(NATIVE_BYTES),
        object(PROVENANCE_BYTES),
        candidate.video.clone(),
        same_contract_plan(),
        ValidatorIdentity::new("validator", "1")?,
    )?;
    assert!(
        base.clone()
            .with_admission(BundleAdmissionEvidence::new(
                measured_span(100),
                measured_span(101),
                inputs.clone()
            )?)
            .is_err()
    );
    let valid = base.clone().with_admission(BundleAdmissionEvidence::new(
        measured_span(100),
        measured_span(100),
        inputs,
    )?)?;
    assert_eq!(
        serde_json::from_str::<BundleValidationReceipt>(&serde_json::to_string(&valid)?)?,
        valid
    );
    for path in ["manifest", "left", "right"] {
        let mut bad = serde_json::to_value(&valid)?;
        bad["admission"]["inputs"][path] = serde_json::to_value(valid.native_object())?;
        assert!(serde_json::from_value::<BundleValidationReceipt>(bad).is_err());
    }
    let mut bad = serde_json::to_value(&valid)?;
    bad["admission"]["native_span"]["end"]["ticks"] = serde_json::json!(0);
    assert!(serde_json::from_value::<BundleValidationReceipt>(bad).is_err());
    let mut bad = serde_json::to_value(&valid)?;
    bad["admission"]["extra"] = serde_json::json!(true);
    assert!(serde_json::from_value::<BundleValidationReceipt>(bad).is_err());
    assert!(base.admission().is_none());
    assert!(serde_json::to_value(base)?.get("admission").is_none());
    Ok(())
}

#[test]
fn identical_masters_can_share_one_fresh_asset_identity() -> Result {
    let scratch = tempfile::tempdir()?;
    let before = document()?;
    let short = deadpan_core::apply(
        &before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: RevisionId::new("short")?,
            command: Command::SetHoldDuration {
                node: NodeId::new("hold")?,
                duration: FrameDuration::new(3)?,
            },
        },
    )?
    .forward
    .apply(&before)?;
    let mut store = ProjectStore::create(&scratch.path().join("aliased.deadpan"), &short)?;
    let candidate = same_contract_candidate();
    let request = store.record_bridge_generation_request(
        GenerationRequestInput {
            request_id: RequestId::new("aliased")?,
            expected_revision: short.revision_id().clone(),
            hold_id: NodeId::new("hold")?,
            context_sha256: sha('a'),
            provider: candidate.provider.clone(),
            constraints: HoldConstraints {
                video: candidate.video.clone(),
                conditioning: ConditioningMode::Bridge,
                motion: MotionAmount::Still,
                instructions: None,
                region_target: None,
            },
        },
        same_contract_plan(),
    )?;
    let identity = begin(&mut store, &request, "attempt")?;
    complete_bridge(&mut store, &identity, &candidate)?;
    for bytes in [NATIVE_BYTES, PROVENANCE_BYTES]
        .into_iter()
        .chain(INPUT_BYTES)
    {
        store.promote_generated_object(&mut Cursor::new(bytes), &object(bytes), media_limits())?;
    }
    let receipt = BundleValidationReceipt::new(
        &candidate,
        object(NATIVE_BYTES),
        object(NATIVE_BYTES),
        object(PROVENANCE_BYTES),
        candidate.video.clone(),
        same_contract_plan(),
        ValidatorIdentity::new("validator", "1")?,
    )?
    .with_admission(BundleAdmissionEvidence::new(
        measured_span(100),
        measured_span(100),
        admission(sha('a')).inputs().clone(),
    )?)?;
    store.record_generation_bundle_ready(&identity, &candidate, receipt.clone(), media_limits())?;
    let mut input = GenerationAcceptance {
        expected_revision: short.revision_id().clone(),
        new_revision: RevisionId::new("accepted")?,
        identity,
        expected_receipt: receipt,
        sampled_asset: AssetId::new("shared")?,
        native_asset: AssetId::new("shared")?,
    };
    store.accept_generation_bundle(
        &input,
        &unchanged_relevance(&store, &input.new_revision)?,
        media_limits(),
    )?;
    assert_eq!(store.snapshot()?.assets().len(), 1);
    input.expected_revision = input.new_revision;
    input.new_revision = RevisionId::new("reused-assets")?;
    assert!(
        matches!(store.preview_generation_acceptance(&input, media_limits()), Err(StoreError::GenerationAcceptance(message)) if message.contains("fresh asset"))
    );
    store.validate()?;
    Ok(())
}

fn measured_span(end: i64) -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 1000).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: end,
            time_base,
        },
    )
    .unwrap()
}

fn admission(context: Sha256) -> BundleAdmissionEvidence {
    BundleAdmissionEvidence::new(
        measured_span(458),
        measured_span(400),
        BundleInputObjects::new(
            context,
            object(INPUT_BYTES[0]),
            object(INPUT_BYTES[1]),
            object(INPUT_BYTES[2]),
        )
        .unwrap(),
    )
    .unwrap()
}

fn ready_for_acceptance(store: &mut ProjectStore) -> Result<GenerationAcceptance> {
    let request = allocate_bridge(store, "request", 1)?;
    let identity = begin(store, &request, "attempt")?;
    let candidate = native_candidate(&request);
    complete_bridge(store, &identity, &candidate)?;
    publish_bundle(store)?;
    for bytes in INPUT_BYTES {
        store.promote_generated_object(&mut Cursor::new(bytes), &object(bytes), media_limits())?;
    }
    let receipt = receipt(&candidate).with_admission(admission(request.binding.context_sha256))?;
    store.record_generation_bundle_ready(&identity, &candidate, receipt.clone(), media_limits())?;
    Ok(GenerationAcceptance {
        expected_revision: store.snapshot()?.revision_id().clone(),
        new_revision: RevisionId::new("accepted")?,
        identity,
        expected_receipt: receipt,
        native_asset: AssetId::new("native")?,
        sampled_asset: AssetId::new("sampled")?,
    })
}

fn unchanged_relevance(store: &ProjectStore, next: &RevisionId) -> Result<RelevancePlan> {
    Ok(RelevancePlan {
        from_revision: store.snapshot()?.revision_id().clone(),
        to_revision: next.clone(),
        observations: store
            .current_generation_requests()?
            .into_iter()
            .map(|request| RelevanceObservation {
                request_id: request.request_id,
                target: request.target,
                after_context: ContextObservation::Resolved(request.binding.context_sha256.clone()),
                binding: request.binding,
            })
            .collect(),
    })
}

fn edit_reconciled(store: &mut ProjectStore, revision: &str, command: Command) -> Result {
    let current = store.snapshot()?;
    let next = RevisionId::new(revision)?;
    let relevance = unchanged_relevance(store, &next)?;
    store.commit_reconciled(
        &CommandRequest {
            project_id: current.project_id().clone(),
            expected_revision: current.revision_id().clone(),
            new_revision: next,
            command,
        },
        &relevance,
    )?;
    Ok(())
}

#[test]
fn explicit_acceptance_derives_assets_and_survives_history_without_worker() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("acceptance.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let input = ready_for_acceptance(&mut store)?;
    let before = store.snapshot()?;
    let preview = store.preview_generation_acceptance(&input, media_limits())?;
    assert_eq!(store.snapshot()?, before);
    let accepted = preview.forward.apply(&before)?;
    assert_eq!(
        accepted.assets()[&input.native_asset].video,
        Some(measured_span(458))
    );
    assert_eq!(
        accepted.assets()[&input.sampled_asset].video,
        Some(measured_span(400))
    );
    for (id, object, video) in [
        (
            &input.native_asset,
            input.expected_receipt.native_object(),
            input.expected_receipt.native_video(),
        ),
        (
            &input.sampled_asset,
            input.expected_receipt.sampled_object(),
            input.expected_receipt.sampled_video(),
        ),
    ] {
        let asset = &accepted.assets()[id];
        assert_eq!(asset.content_hash, object.content().to_string());
        assert_eq!(asset.frame_count, Some(video.frames()));
        assert_eq!(asset.audio, None);
        assert!(!asset.still_image);
    }
    let outcome = store.accept_generation_bundle(
        &input,
        &unchanged_relevance(&store, &input.new_revision)?,
        media_limits(),
    )?;
    assert_eq!(outcome.edit, preview);
    assert_eq!(store.snapshot()?, accepted);
    store.validate()?;
    // The artifact records the canvas its conditioning was fitted for.
    let NodeKind::Hold { recipe } = &accepted.nodes()[&NodeId::new("hold")?].kind else {
        panic!()
    };
    let HoldVideo::Generated {
        accepted: generated,
    } = &recipe.video
    else {
        panic!("accepted Hold is generated")
    };
    let basis = accepted.presentation_basis();
    assert_eq!(
        generated.artifact.content_aspect,
        Some([basis.width, basis.height])
    );
    drop(store);

    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let undo = RevisionId::new("acceptance-undo")?;
    store.undo_reconciled(
        &input.new_revision,
        undo.clone(),
        &unchanged_relevance(&store, &undo)?,
    )?;
    assert!(store.snapshot()?.assets().is_empty());
    let redo = RevisionId::new("acceptance-redo")?;
    store.redo_reconciled(&undo, redo.clone(), &unchanged_relevance(&store, &redo)?)?;
    edit_reconciled(
        &mut store,
        "reverted",
        Command::RevertGeneratedHold {
            node: NodeId::new("hold")?,
        },
    )?;
    let reverted = store.snapshot()?;
    let NodeKind::Hold { recipe } = &reverted.nodes()[&NodeId::new("hold")?].kind else {
        panic!()
    };
    assert_eq!(recipe.video, HoldVideo::Background);
    assert_eq!(store.snapshot()?.assets().len(), 2);
    for bytes in [NATIVE_BYTES, SAMPLED_BYTES, PROVENANCE_BYTES]
        .into_iter()
        .chain(INPUT_BYTES)
    {
        assert_eq!(
            store
                .snapshot_generated_object(&object(bytes), media_limits())?
                .reference(),
            &object(bytes)
        );
    }
    let undo_revert = RevisionId::new("undo-revert")?;
    store.undo_reconciled(
        &RevisionId::new("reverted")?,
        undo_revert.clone(),
        &unchanged_relevance(&store, &undo_revert)?,
    )?;
    edit_reconciled(
        &mut store,
        "branch",
        Command::Rename {
            node: NodeId::new("hold")?,
            label: "New branch".into(),
        },
    )?;
    assert!(matches!(
        store.preview_redo(&RevisionId::new("branch")?, RevisionId::new("cannot-redo")?),
        Err(StoreError::NothingToRedo)
    ));
    let mut reused = input.clone();
    reused.expected_revision = store.snapshot()?.revision_id().clone();
    reused.native_asset = AssetId::new("fresh-native")?;
    reused.sampled_asset = AssetId::new("fresh-sampled")?;
    assert!(matches!(
        store.preview_generation_acceptance(&reused, media_limits()),
        Err(StoreError::RevisionReused(_))
    ));
    store.validate()?;
    Ok(())
}

#[test]
fn admission_requires_all_six_immutable_objects_and_matching_context() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("dependencies.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let request = allocate_bridge(&mut store, "request", 1)?;
    let identity = begin(&mut store, &request, "attempt")?;
    let candidate = native_candidate(&request);
    complete_bridge(&mut store, &identity, &candidate)?;
    publish_bundle(&mut store)?;
    let qualified = receipt(&candidate).with_admission(admission(sha('a')))?;
    assert!(
        store
            .record_generation_bundle_ready(
                &identity,
                &candidate,
                qualified.clone(),
                media_limits()
            )
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
    for bytes in INPUT_BYTES {
        store.promote_generated_object(&mut Cursor::new(bytes), &object(bytes), media_limits())?;
    }
    let mismatch = receipt(&candidate).with_admission(admission(sha('f')))?;
    assert!(
        store
            .record_generation_bundle_ready(&identity, &candidate, mismatch, media_limits())
            .is_err()
    );
    store.record_generation_bundle_ready(
        &identity,
        &candidate,
        qualified.clone(),
        media_limits(),
    )?;
    let input = GenerationAcceptance {
        expected_revision: store.snapshot()?.revision_id().clone(),
        new_revision: RevisionId::new("accepted")?,
        identity,
        expected_receipt: qualified,
        sampled_asset: AssetId::new("sampled")?,
        native_asset: AssetId::new("native")?,
    };
    let before = store.snapshot()?;
    for bytes in [NATIVE_BYTES, SAMPLED_BYTES, PROVENANCE_BYTES]
        .into_iter()
        .chain(INPUT_BYTES)
    {
        let path = stored_path(&package, &object(bytes));
        fs::remove_file(&path)?;
        assert!(
            store
                .accept_generation_bundle(
                    &input,
                    &unchanged_relevance(&store, &input.new_revision)?,
                    media_limits()
                )
                .is_err()
        );
        fs::write(&path, vec![b'!'; bytes.len()])?;
        assert!(
            store
                .preview_generation_acceptance(&input, media_limits())
                .is_err()
        );
        fs::remove_file(&path)?;
        store.promote_generated_object(&mut Cursor::new(bytes), &object(bytes), media_limits())?;
        assert_eq!(store.snapshot()?, before);
    }
    Ok(())
}

#[test]
fn acceptance_rechecks_selection_receipt_revision_and_complete_relevance() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("optimistic.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let input = ready_for_acceptance(&mut store)?;
    let before = store.snapshot()?;
    let mut wrong = input.clone();
    wrong.expected_revision = RevisionId::new("old")?;
    assert!(matches!(
        store.preview_generation_acceptance(&wrong, media_limits()),
        Err(StoreError::RevisionConflict { .. })
    ));
    let mut plan = unchanged_relevance(&store, &input.new_revision)?;
    plan.observations.clear();
    assert!(
        store
            .accept_generation_bundle(&input, &plan, media_limits())
            .is_err()
    );
    plan = unchanged_relevance(&store, &input.new_revision)?;
    plan.observations[0].after_context = ContextObservation::Unresolved;
    assert!(
        store
            .accept_generation_bundle(&input, &plan, media_limits())
            .is_err()
    );
    wrong = input.clone();
    wrong.sampled_asset = wrong.native_asset.clone();
    assert!(
        store
            .preview_generation_acceptance(&wrong, media_limits())
            .is_err()
    );

    let connection = Connection::open(package.join("project.sqlite"))?;
    connection.execute("UPDATE generation_bundle_receipts SET bundle=json_set(bundle,'$.validator.version','different')", [])?;
    assert!(
        store
            .preview_generation_acceptance(&input, media_limits())
            .is_err()
    );
    connection.execute(
        "UPDATE generation_bundle_receipts SET bundle=?1",
        [serde_json::to_string(&input.expected_receipt)?],
    )?;
    let request = store
        .generation_request(&input.identity.request_id)?
        .unwrap();
    let next = begin(&mut store, &request, "next-attempt")?;
    let candidate = native_candidate_for(&request, 2);
    complete_bridge(&mut store, &next, &candidate)?;
    store.record_generation_bundle_ready(
        &next,
        &candidate,
        receipt(&candidate).with_admission(admission(request.binding.context_sha256.clone()))?,
        media_limits(),
    )?;
    assert!(
        store
            .preview_generation_acceptance(&input, media_limits())
            .is_err()
    );
    store.select_generation_bundle_variant(&input.identity)?;
    store.mark_generation_bundle_evicted(&input.identity)?;
    assert!(
        store
            .preview_generation_acceptance(&input, media_limits())
            .is_err()
    );
    assert_eq!(store.snapshot()?, before);
    store.validate()?;
    Ok(())
}

#[test]
fn acceptance_rollback_read_only_and_legacy_receipt_are_safe() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("rollback.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let input = ready_for_acceptance(&mut store)?;
    let plan = unchanged_relevance(&store, &input.new_revision)?;
    let before = store.snapshot()?;
    let connection = Connection::open(package.join("project.sqlite"))?;
    connection.execute_batch("CREATE TRIGGER reject_acceptance BEFORE INSERT ON history BEGIN SELECT RAISE(ABORT,'injected'); END;")?;
    assert!(
        store
            .accept_generation_bundle(&input, &plan, media_limits())
            .is_err()
    );
    assert_eq!(store.snapshot()?, before);
    assert_eq!(
        connection.query_row(
            "SELECT COUNT(*) FROM revisions WHERE id='accepted'",
            [],
            |row| row.get::<_, i64>(0)
        )?,
        0
    );
    store.validate()?;
    connection.execute_batch("DROP TRIGGER reject_acceptance;")?;
    drop(store);
    let mut reader = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    reader.preview_generation_acceptance(&input, media_limits())?;
    assert!(matches!(
        reader.accept_generation_bundle(&input, &plan, media_limits()),
        Err(StoreError::ReadOnly)
    ));
    drop(reader);
    let legacy = receipt(&native_candidate(
        &ProjectStore::open(&package, AccessMode::ReadOnly)?
            .generation_request(&input.identity.request_id)?
            .unwrap(),
    ));
    connection.execute(
        "UPDATE generation_bundle_receipts SET bundle=?1",
        [serde_json::to_string(&legacy)?],
    )?;
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let legacy_input = GenerationAcceptance {
        expected_receipt: legacy,
        ..input
    };
    assert!(
        matches!(store.accept_generation_bundle(&legacy_input, &plan, media_limits()), Err(StoreError::GenerationAcceptance(message)) if message.contains("legacy"))
    );
    assert_eq!(store.snapshot()?, before);
    Ok(())
}

#[test]
fn stale_and_detached_bundles_never_revive_through_undo() -> Result {
    for detach in [false, true] {
        let scratch = tempfile::tempdir()?;
        let mut store = ProjectStore::create(&scratch.path().join("stale.deadpan"), &document()?)?;
        let mut input = ready_for_acceptance(&mut store)?;
        let command = if detach {
            Command::Delete {
                node: NodeId::new("hold")?,
            }
        } else {
            Command::SetHoldDuration {
                node: NodeId::new("hold")?,
                duration: FrameDuration::new(11)?,
            }
        };
        edit_reconciled(&mut store, "changed", command)?;
        input.expected_revision = store.snapshot()?.revision_id().clone();
        assert!(
            store
                .preview_generation_acceptance(&input, media_limits())
                .is_err()
        );
        let undo = RevisionId::new("restored")?;
        store.undo_reconciled(
            &input.expected_revision,
            undo.clone(),
            &unchanged_relevance(&store, &undo)?,
        )?;
        input.expected_revision = undo;
        assert!(
            store
                .preview_generation_acceptance(&input, media_limits())
                .is_err()
        );
        assert_eq!(
            store
                .generation_request(&input.identity.request_id)?
                .unwrap()
                .relevance,
            if detach {
                deadpan_jobs::Relevance::Detached
            } else {
                deadpan_jobs::Relevance::Stale
            }
        );
        store.validate()?;
    }
    Ok(())
}

#[test]
fn scoped_acceptance_isolates_one_play_and_keeps_default_request_current() -> Result {
    use deadpan_core::{RepeatEditBranch, RepeatEditStep, ScopedNodeTarget};
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("repeated.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    edit_reconciled(
        &mut store,
        "repeat",
        Command::WrapRepeat {
            node: NodeId::new("hold")?,
            id: NodeId::new("repeat")?,
            plays: 3,
            gap: None,
            anchor_policy: Default::default(),
        },
    )?;
    assert!(allocate_bridge(&mut store, "ambiguous", 2).is_err());
    let repeated = store.snapshot()?;
    let NodeKind::Repeat { iterations, .. } = &repeated.nodes()[&NodeId::new("repeat")?].kind
    else {
        panic!()
    };
    let iteration = iterations.at(1).unwrap();
    let target = |branch| ScopedNodeTarget {
        node: NodeId::new("hold").unwrap(),
        repeats: vec![RepeatEditStep {
            repeat: NodeId::new("repeat").unwrap(),
            branch,
        }],
    };
    let mut allocate = |name, scope| {
        store.record_scoped_bridge_generation_request(
            GenerationRequestInput {
                request_id: RequestId::new(name).unwrap(),
                expected_revision: RevisionId::new("repeat").unwrap(),
                hold_id: NodeId::new("hold").unwrap(),
                context_sha256: sha('a'),
                constraints: constraints(),
                provider: provider(3),
            },
            scope,
            plan(),
        )
    };
    let default = allocate("default", target(RepeatEditBranch::Default))?;
    let selected = allocate("second", target(RepeatEditBranch::Play { iteration }))?;
    publish_bundle(&mut store)?;
    publish_inputs(&mut store)?;
    let (identity, expected_receipt) = ready_variant(&mut store, &selected, "ready", 1)?;
    let input = GenerationAcceptance {
        expected_revision: store.head_revision()?,
        new_revision: RevisionId::new("accepted")?,
        identity,
        expected_receipt,
        native_asset: AssetId::new("native")?,
        sampled_asset: AssetId::new("sampled")?,
    };
    let before = store.snapshot()?;
    let (preview, mapped) = store.preview_generation_acceptance_contexts(&input, media_limits())?;
    assert_eq!(store.snapshot()?, before);
    let wrong_address = unchanged_relevance(&store, &input.new_revision)?;
    assert!(
        store
            .accept_generation_bundle(&input, &wrong_address, media_limits())
            .is_err()
    );
    assert_eq!(store.snapshot()?, before);
    let relevance = RelevancePlan {
        from_revision: input.expected_revision.clone(),
        to_revision: input.new_revision.clone(),
        observations: mapped
            .iter()
            .map(|request| RelevanceObservation {
                request_id: request.request_id.clone(),
                binding: request.binding.clone(),
                target: request.target.clone(),
                after_context: ContextObservation::Resolved(request.binding.context_sha256.clone()),
            })
            .collect(),
    };
    store.accept_generation_bundle(&input, &relevance, media_limits())?;
    assert_eq!(store.snapshot()?, preview.forward.apply(&before)?);
    assert_eq!(
        store.generation_request(&default.request_id)?.unwrap(),
        default
    );
    let accepted = store.generation_request(&selected.request_id)?.unwrap();
    assert_ne!(accepted.target.node, selected.target.node);
    assert_eq!(accepted.binding, selected.binding);
    assert_eq!(accepted.origin_target, selected.origin_target);
    assert!(
        matches!(&store.snapshot()?.nodes()[&accepted.target.node].kind,
        NodeKind::Hold { recipe } if matches!(recipe.video, HoldVideo::Generated { .. }))
    );
    assert!(
        matches!(&store.snapshot()?.nodes()[&NodeId::new("hold")?].kind,
        NodeKind::Hold { recipe } if matches!(recipe.video, HoldVideo::Background))
    );
    let next = RevisionId::new("undo")?;
    let (_, mapped) = store.preview_undo_generation_contexts(&input.new_revision, next.clone())?;
    store.undo_reconciled(
        &input.new_revision,
        next.clone(),
        &RelevancePlan {
            from_revision: input.new_revision.clone(),
            to_revision: next,
            observations: mapped
                .iter()
                .map(|request| RelevanceObservation {
                    request_id: request.request_id.clone(),
                    binding: request.binding.clone(),
                    target: request.target.clone(),
                    after_context: ContextObservation::Resolved(
                        request.binding.context_sha256.clone(),
                    ),
                })
                .collect(),
        },
    )?;
    assert_eq!(
        store
            .generation_request(&selected.request_id)?
            .unwrap()
            .target,
        selected.target
    );
    store.validate_full()?;
    drop(store);
    ProjectStore::open(&package, AccessMode::ReadOnly)?.validate_full()?;
    Ok(())
}

/// Publish the three retained inputs acceptance requires.
fn publish_inputs(store: &mut ProjectStore) -> Result {
    for bytes in INPUT_BYTES {
        store.promote_generated_object(&mut Cursor::new(bytes), &object(bytes), media_limits())?;
    }
    Ok(())
}

/// Make attempt `ordinal` of `request` a Ready variant with admission evidence.
fn ready_variant(
    store: &mut ProjectStore,
    request: &StoredGenerationRequest,
    attempt: &str,
    ordinal: u64,
) -> Result<(MessageIdentity, BundleValidationReceipt)> {
    let identity = begin(store, request, attempt)?;
    let candidate = native_candidate_for(request, ordinal);
    complete_bridge(store, &identity, &candidate)?;
    let receipt =
        receipt(&candidate).with_admission(admission(request.binding.context_sha256.clone()))?;
    store.record_generation_bundle_ready(&identity, &candidate, receipt.clone(), media_limits())?;
    Ok((identity, receipt))
}

#[test]
fn seeded_variants_share_one_request_and_discard_is_durable() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("variants.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let request = allocate_bridge(&mut store, "request", 7)?;
    publish_bundle(&mut store)?;
    publish_inputs(&mut store)?;

    let (first, first_receipt) = ready_variant(&mut store, &request, "attempt-1", 1)?;
    let (second, second_receipt) = ready_variant(&mut store, &request, "attempt-2", 2)?;
    // Every attempt is a distinct seeded variant of the one request.
    assert_eq!(first_receipt.provider().seed, 7);
    assert_eq!(second_receipt.provider().seed, 8);
    assert_eq!(request.provider.for_attempt(1), request.provider);

    // A declaration that repeats the request's own seed is not attempt 3's.
    let third = begin(&mut store, &request, "attempt-3")?;
    let repeated = native_candidate_for(&request, 1);
    complete_bridge(&mut store, &third, &repeated)?;
    let error = store
        .record_generation_bundle_ready(
            &third,
            &repeated,
            receipt(&repeated).with_admission(admission(request.binding.context_sha256.clone()))?,
            media_limits(),
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("does not exactly match"),
        "{error}"
    );
    assert_eq!(
        store.generation_attempt(&third)?.unwrap().checkpoint.state,
        JobState::Validating
    );
    store.fail_generation_attempt(
        &third,
        deadpan_jobs::HostFailure {
            code: deadpan_jobs::HostFailureCode::OutputValidationFailed,
            detail: deadpan_jobs::Diagnostic::new("wrong seed")?,
        },
    )?;
    let (fourth, fourth_receipt) = ready_variant(&mut store, &request, "attempt-4", 4)?;
    assert_eq!(fourth_receipt.provider().seed, 10);

    // The newest Ready variant is selected; any present one may be chosen.
    assert_eq!(
        store
            .selected_generation_bundle(&request.request_id)?
            .unwrap()
            .identity,
        fourth
    );
    store.select_generation_bundle_variant(&first)?;
    // Acceptance admits only the selected variant.
    let accept =
        |identity: &MessageIdentity, receipt: &BundleValidationReceipt| GenerationAcceptance {
            expected_revision: RevisionId::new("initial").unwrap(),
            new_revision: RevisionId::new("accepted").unwrap(),
            identity: identity.clone(),
            expected_receipt: receipt.clone(),
            native_asset: AssetId::new("native").unwrap(),
            sampled_asset: AssetId::new("sampled").unwrap(),
        };
    let mut unselected = accept(&second, &second_receipt);
    unselected.expected_revision = store.snapshot()?.revision_id().clone();
    assert!(matches!(
        store.preview_generation_acceptance(&unselected, media_limits()),
        Err(StoreError::GenerationAcceptance(message)) if message.contains("selected bundle changed")
    ));

    // Discard is the durable availability change: it clears a matching
    // selection and survives reopening, while the other variants remain.
    store.mark_generation_bundle_evicted(&first)?;
    assert!(
        store
            .selected_generation_bundle(&request.request_id)?
            .is_none()
    );
    drop(store);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let attempts = store.generation_attempts(&request.request_id, 0, 16)?;
    let available: Vec<_> = attempts
        .iter()
        .filter(|attempt| {
            attempt
                .bundle_receipt
                .as_ref()
                .is_some_and(|receipt| receipt.availability() == CandidateAvailability::Present)
        })
        .map(|attempt| attempt.ordinal)
        .collect();
    assert_eq!(available, vec![2, 4]);
    assert!(store.select_generation_bundle_variant(&first).is_err());
    store.select_generation_bundle_variant(&second)?;
    let mut input = accept(&second, &second_receipt);
    input.expected_revision = store.snapshot()?.revision_id().clone();
    let outcome = store.accept_generation_bundle(
        &input,
        &unchanged_relevance(&store, &input.new_revision)?,
        media_limits(),
    )?;
    assert_eq!(outcome.revision_id, input.new_revision);
    store.validate()?;

    // A later request for the Hold makes every earlier variant stale.
    let replacement = allocate_bridge(&mut store, "replacement", 20)?;
    assert!(
        store
            .selected_generation_bundle(&request.request_id)?
            .is_none()
    );
    assert!(store.select_generation_bundle_variant(&fourth).is_err());
    assert_eq!(replacement.provider.for_attempt(2).seed, 21);
    Ok(())
}

#[test]
fn discarding_a_variant_is_one_transaction_that_keeps_another_selection() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("discard.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let request = allocate_bridge(&mut store, "request", 3)?;
    publish_bundle(&mut store)?;
    publish_inputs(&mut store)?;
    let (first, _) = ready_variant(&mut store, &request, "attempt-1", 1)?;
    let (second, _) = ready_variant(&mut store, &request, "attempt-2", 2)?;
    let (third, _) = ready_variant(&mut store, &request, "attempt-3", 3)?;
    store.select_generation_bundle_variant(&first)?;

    // Discarding a variant that is not selected keeps the user's choice.
    assert_eq!(
        store.discard_generation_bundle_variant(&second)?,
        Some(first.attempt_id.clone())
    );
    // Discarding the selected one selects the newest remaining present one.
    assert_eq!(
        store.discard_generation_bundle_variant(&first)?,
        Some(third.attempt_id.clone())
    );
    assert_eq!(
        store
            .selected_generation_bundle(&request.request_id)?
            .unwrap()
            .identity,
        third
    );
    // The last one leaves no selection; repeating a discard changes nothing.
    assert_eq!(store.discard_generation_bundle_variant(&third)?, None);
    assert_eq!(store.discard_generation_bundle_variant(&third)?, None);
    for identity in [&first, &second, &third] {
        assert_eq!(
            store
                .generation_attempt(identity)?
                .unwrap()
                .bundle_receipt
                .unwrap()
                .availability(),
            CandidateAvailability::Evicted
        );
    }
    store.validate()?;
    Ok(())
}
