use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use deadpan_core::{
    BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate, HoldAudio,
    HoldRecipe, HoldVideo, NodeId, PresentationBasis, ProjectDocument, ProjectId, RevisionId,
    Subtree,
};
use deadpan_jobs::artifact::ArtifactWorkspace;
use deadpan_jobs::{
    AttemptId, AxisLimits, BridgeCapability, BridgeGenerationPlan, CancellationToken,
    ConditioningMode, ContextArtifact, DimensionLimits, FrameCountFormula, HoldConstraints,
    HoldTarget, HostMessage, JobState, MessageIdentity, MotionAmount, NativeCandidateManifest,
    NativeDimensions, ProtocolVersion, ProviderPackId, ProviderPackVersion, ProviderSelection,
    RequestId, RequestVersion, RuntimeId, RuntimeVersion, Sha256, VideoSpec, WorkerMessage,
    WorkerStage, WorkspaceArtifact, WorkspaceRef,
};
use deadpan_media::protocol::ConversionLimits;
use deadpan_models::{
    GenerationBinding, QualificationError, QualificationLimits, SelectedBridgeProvider,
    qualify_bridge,
};
use deadpan_store::generated_media::GeneratedMediaLimits;
use deadpan_store::generation::GenerationRequestInput;
use deadpan_store::generation_attempts::{
    BeginGenerationAttempt, BundleValidationReceipt, ValidatorIdentity,
};
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::json;
use sha2::{Digest, Sha256 as Hasher};

fn capability() -> BridgeCapability {
    BridgeCapability::new(
        true,
        FrameRate::new(24, 1).unwrap(),
        FrameCountFormula::new(8, 1, 25, 97).unwrap(),
        DimensionLimits::new(
            AxisLimits::new(4, 4, 1).unwrap(),
            AxisLimits::new(2, 2, 1).unwrap(),
        ),
    )
}

fn plan() -> BridgeGenerationPlan {
    BridgeGenerationPlan::new(
        FrameDuration::new(30).unwrap(),
        FrameRate::new(30000, 1001).unwrap(),
        &capability(),
        NativeDimensions::new(4, 2).unwrap(),
    )
    .unwrap()
}

fn request() -> HostMessage {
    HostMessage::GenerateBridge {
        protocol: ProtocolVersion::V2,
        identity: MessageIdentity::new(
            RequestId::new("request").unwrap(),
            AttemptId::new("attempt").unwrap(),
        ),
        cancellation_token: CancellationToken::new("cancel").unwrap(),
        project_id: ProjectId::new("project").unwrap(),
        revision_id: RevisionId::new("revision").unwrap(),
        target: HoldTarget {
            hold_id: NodeId::new("hold").unwrap(),
            request_version: RequestVersion::new(1).unwrap(),
        },
        input: ContextArtifact {
            manifest: WorkspaceRef::new("inputs/context.json").unwrap(),
            sha256: Sha256::new("a".repeat(64)).unwrap(),
        },
        output_workspace: WorkspaceRef::new("outputs").unwrap(),
        constraints: HoldConstraints {
            video: VideoSpec::new(
                FrameDuration::new(30).unwrap(),
                FrameRate::new(30000, 1001).unwrap(),
                4,
                2,
            )
            .unwrap(),
            conditioning: ConditioningMode::Bridge,
            motion: MotionAmount::Still,
        },
        provider: Box::new(ProviderSelection {
            pack_id: ProviderPackId::new("fixture").unwrap(),
            pack_version: ProviderPackVersion::new("1").unwrap(),
            runtime_id: RuntimeId::new("fixture").unwrap(),
            runtime_version: RuntimeVersion::new("1").unwrap(),
            seed: 1,
        }),
        plan: Box::new(plan()),
    }
}

fn limits() -> QualificationLimits {
    QualificationLimits {
        media: ConversionLimits {
            max_input_bytes: 1024 * 1024,
            max_output_bytes: 1024 * 1024,
            max_scratch_bytes: 600,
            timeout_ms: 30000,
        },
        maximum_worker_provenance_bytes: 64 * 1024,
        maximum_host_provenance_bytes: 128 * 1024,
    }
}

fn declared(reference: &str, bytes: &[u8]) -> WorkspaceArtifact {
    WorkspaceArtifact::new(
        WorkspaceRef::new(reference).unwrap(),
        Sha256::new(sha256(bytes)).unwrap(),
        bytes.len() as u64,
    )
    .unwrap()
}

fn sha256(bytes: &[u8]) -> String {
    Hasher::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

struct Fixture {
    directory: tempfile::TempDir,
    workspace: ArtifactWorkspace,
    request: HostMessage,
    declaration: NativeCandidateManifest,
    provenance: Vec<u8>,
}

impl Fixture {
    fn selected_provider(&self) -> SelectedBridgeProvider {
        SelectedBridgeProvider::new(self.declaration.provider.clone(), capability())
    }

    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("outputs")).unwrap();
        let workspace = ArtifactWorkspace::open(directory.path()).unwrap();
        let request = request();
        let binding = GenerationBinding::from_request(&request).unwrap();
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rgb25_24.mp4");
        let native = fs::read(source).unwrap();
        fs::write(directory.path().join("outputs/native.mp4"), &native).unwrap();
        let provenance = serde_json::to_vec_pretty(&json!({
            "schema_version": 2, "request_binding": binding,
            "prompt_version": "fixture-1", "prompt": "Keep the scene still.", "seed": 1,
            "configuration": {"fixture":true},
            "runtime_commit":"a".repeat(40), "pack_revision":"b".repeat(40),
            "gemma_revision":"c".repeat(40),
            "adapter_sources_sha256":{"adapter.py":"a".repeat(64)},
            "loaded_ltx_sources_sha256":{"runtime.py":"b".repeat(64)},
            "verified_assets":[{"repository":"fixture/model", "path":"weights.bin", "size":1, "sha256":"c".repeat(64)}],
            "model_color_interpretation":"fixture SDR sRGB", "temporal_interpolation":"fixture linear encoded RGB",
            "conditioning_preprocessing":"fixture no transform",
            "native_sha256": sha256(&native), "native_bytes":native.len(),
            "context": {
                "schema_version":1, "model_color":"srgb", "plan":binding.plan,
                "left":{"reference":"inputs/left.png","sha256":"d".repeat(64),"byte_length":1},
                "right":{"reference":"inputs/right.png","sha256":"e".repeat(64),"byte_length":1},
                "input_color_interpretation":"fixture RGB"
            },
        }))
        .unwrap();
        fs::write(
            directory.path().join("outputs/provenance.json"),
            &provenance,
        )
        .unwrap();
        let declaration = NativeCandidateManifest {
            native: declared("outputs/native.mp4", &native),
            provenance: declared("outputs/provenance.json", &provenance),
            video: VideoSpec::new(
                FrameDuration::new(25).unwrap(),
                FrameRate::new(24, 1).unwrap(),
                4,
                2,
            )
            .unwrap(),
            provider: binding.provider,
        };
        Self {
            directory,
            workspace,
            request,
            declaration,
            provenance,
        }
    }

    fn replace_provenance(&mut self, bytes: Vec<u8>) {
        fs::write(
            self.directory.path().join("outputs/provenance.json"),
            &bytes,
        )
        .unwrap();
        self.declaration.provenance = declared("outputs/provenance.json", &bytes);
        self.provenance = bytes;
    }
}

#[test]
fn complete_bundle_derives_media_and_retains_exact_worker_provenance() {
    let fixture = Fixture::new();
    let bundle = qualify_bridge(
        Path::new(env!("CARGO_BIN_EXE_deadpan-media-worker")),
        &fixture.workspace,
        &fixture.request,
        &fixture.declaration,
        &fixture.selected_provider(),
        limits(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(bundle.declaration(), &fixture.declaration);
    assert_eq!(
        bundle.binding(),
        &GenerationBinding::from_request(&fixture.request).unwrap()
    );
    assert_eq!(
        bundle.native().report().output_rgb_sha256,
        "324eb76430a9f2cf8303120656a8c167edd986418b763926d9250b1a3dd2adc9"
    );
    assert_eq!(
        bundle.sampled().report().output_rgb_sha256,
        "3f44a221f04474dd1804beb59556259e03f4fd2b518b3b4c3a9efa431efa4ee5"
    );
    let provenance_ref = bundle.provenance().object().clone();
    let native_ref = bundle.native().object().clone();
    let sampled_ref = bundle.sampled().object().clone();
    let (_, _, mut provenance) = bundle.into_parts();
    let mut bytes = Vec::new();
    provenance.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes.len() as u64, provenance_ref.byte_length());
    assert_eq!(
        blake3::hash(&bytes).to_hex().as_str(),
        provenance_ref.content().digest()
    );
    let envelope: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        envelope["worker_provenance_utf8"]
            .as_str()
            .unwrap()
            .as_bytes(),
        fixture.provenance
    );
    assert_eq!(
        envelope["native"],
        serde_json::to_value(native_ref).unwrap()
    );
    assert_eq!(
        envelope["sampled"],
        serde_json::to_value(sampled_ref).unwrap()
    );
}

#[test]
fn provenance_binding_and_containment_fail_before_any_codec_is_started() {
    let mut fixture = Fixture::new();
    let original: serde_json::Value = serde_json::from_slice(&fixture.provenance).unwrap();
    let mut mismatched = original.clone();
    mismatched["request_binding"]["identity"]["attempt_id"] = json!("different");
    let mut absent = original.clone();
    absent.as_object_mut().unwrap().remove("request_binding");
    let mut missing_assets = original.clone();
    missing_assets
        .as_object_mut()
        .unwrap()
        .remove("verified_assets");
    let mut bad_asset = original.clone();
    bad_asset["verified_assets"][0]["sha256"] = json!("bad");
    let mut duplicate_asset = original.clone();
    duplicate_asset["verified_assets"]
        .as_array_mut()
        .unwrap()
        .push(original["verified_assets"][0].clone());
    let mut wrong_seed = original.clone();
    wrong_seed["seed"] = json!(2);
    let mut wrong_native = original.clone();
    wrong_native["native_bytes"] = json!(1);
    let mut wrong_context = original.clone();
    wrong_context["context"]["schema_version"] = json!(2);
    let duplicate = String::from_utf8(fixture.provenance.clone())
        .unwrap()
        .replacen('{', "{\"schema_version\":2,", 1)
        .into_bytes();
    for bytes in [
        serde_json::to_vec(&mismatched).unwrap(),
        serde_json::to_vec(&absent).unwrap(),
        serde_json::to_vec(&missing_assets).unwrap(),
        serde_json::to_vec(&bad_asset).unwrap(),
        serde_json::to_vec(&duplicate_asset).unwrap(),
        serde_json::to_vec(&wrong_seed).unwrap(),
        serde_json::to_vec(&wrong_native).unwrap(),
        serde_json::to_vec(&wrong_context).unwrap(),
        duplicate,
    ] {
        fixture.replace_provenance(bytes);
        let error = qualify_bridge(
            Path::new("/missing/codec"),
            &fixture.workspace,
            &fixture.request,
            &fixture.declaration,
            &fixture.selected_provider(),
            limits(),
            &AtomicBool::new(false),
        );
        assert!(matches!(
            error,
            Err(QualificationError::Provenance(_) | QualificationError::Json(_))
        ));
    }
    fixture.replace_provenance(serde_json::to_vec(&original).unwrap());
    fs::write(
        fixture.directory.path().join("outputs/provenance.json"),
        b"changed",
    )
    .unwrap();
    assert!(matches!(
        qualify_bridge(
            Path::new("/missing/codec"),
            &fixture.workspace,
            &fixture.request,
            &fixture.declaration,
            &fixture.selected_provider(),
            limits(),
            &AtomicBool::new(false)
        ),
        Err(QualificationError::Artifact(_))
    ));
}

#[test]
fn cancellation_and_provenance_budgets_cannot_return_a_partial_bundle() {
    let fixture = Fixture::new();
    assert!(matches!(
        qualify_bridge(
            Path::new("/missing/codec"),
            &fixture.workspace,
            &fixture.request,
            &fixture.declaration,
            &fixture.selected_provider(),
            limits(),
            &AtomicBool::new(true)
        ),
        Err(QualificationError::Cancelled)
    ));
    let small = QualificationLimits {
        maximum_worker_provenance_bytes: 1,
        ..limits()
    };
    assert!(matches!(
        qualify_bridge(
            Path::new("/missing/codec"),
            &fixture.workspace,
            &fixture.request,
            &fixture.declaration,
            &fixture.selected_provider(),
            small,
            &AtomicBool::new(false)
        ),
        Err(QualificationError::Artifact(_))
    ));
    let small = QualificationLimits {
        maximum_host_provenance_bytes: 1,
        ..limits()
    };
    assert!(matches!(
        qualify_bridge(
            Path::new(env!("CARGO_BIN_EXE_deadpan-media-worker")),
            &fixture.workspace,
            &fixture.request,
            &fixture.declaration,
            &fixture.selected_provider(),
            small,
            &AtomicBool::new(false)
        ),
        Err(QualificationError::Provenance(_))
    ));
}

#[test]
fn host_selected_capability_rejects_mismatched_or_non_nearest_plans_before_io() {
    let fixture = Fixture::new();
    fs::remove_file(fixture.directory.path().join("outputs/provenance.json")).unwrap();
    let mut other_selection = fixture.declaration.provider.clone();
    other_selection.seed += 1;
    let unsupported = BridgeCapability::new(
        false,
        capability().native_frame_rate(),
        capability().frame_counts(),
        capability().dimensions(),
    );
    for selected in [
        SelectedBridgeProvider::new(other_selection, capability()),
        SelectedBridgeProvider::new(fixture.declaration.provider.clone(), unsupported),
    ] {
        assert!(matches!(
            qualify_bridge(
                Path::new("/missing/codec"),
                &fixture.workspace,
                &fixture.request,
                &fixture.declaration,
                &selected,
                limits(),
                &AtomicBool::new(false)
            ),
            Err(QualificationError::Request(_))
        ));
    }
    // This is intrinsically consistent and 33 is legal in the selected 8k+1
    // family, but it is not the nearest legal count for the requested duration.
    let mut wire = serde_json::to_value(plan()).unwrap();
    wire["native"]["frame_count"] = json!(33);
    wire["timing"]["actual_boundary_duration"] = json!({"numerator":"4","denominator":"3"});
    wire["timing"]["retime_deviation"] = json!({"numerator":"8969","denominator":"30000"});
    let non_nearest: BridgeGenerationPlan = serde_json::from_value(wire).unwrap();
    let mut request = fixture.request.clone();
    let HostMessage::GenerateBridge { plan, .. } = &mut request else {
        unreachable!()
    };
    **plan = non_nearest;
    assert!(
        matches!(qualify_bridge(Path::new("/missing/codec"), &fixture.workspace,
        &request, &fixture.declaration, &fixture.selected_provider(), limits(), &AtomicBool::new(false)),
        Err(QualificationError::Request(reason)) if reason.contains("nearest"))
    );

    let selected = fixture.selected_provider();
    let serialized = serde_json::to_value(&selected).unwrap();
    assert_eq!(
        serde_json::from_value::<SelectedBridgeProvider>(serialized.clone()).unwrap(),
        selected
    );
    for bad in [json!(0), json!(true), json!(1.5)] {
        let mut corrupted = serialized.clone();
        corrupted["frame_counts"]["step"] = bad;
        assert!(serde_json::from_value::<SelectedBridgeProvider>(corrupted).is_err());
    }
}

fn hold_document() -> ProjectDocument {
    let document = ProjectDocument::new(
        ProjectId::new("project").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(30000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root").unwrap(),
    )
    .unwrap();
    let hold = NodeId::new("hold").unwrap();
    let edit = deadpan_core::apply(
        &document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new("revision").unwrap(),
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
                                duration: FrameDuration::new(30).unwrap(),
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                            },
                        ),
                    )]),
                    overrides: BTreeMap::new(),
                },
            },
        },
    )
    .unwrap();
    edit.forward.apply(&document).unwrap()
}

#[test]
fn actual_bundle_becomes_ready_only_after_all_objects_exist_and_survives_reopen() {
    let fixture = Fixture::new();
    let binding = GenerationBinding::from_request(&fixture.request).unwrap();
    let package = fixture.directory.path().join("project.deadpan");
    let document = hold_document();
    let mut store = ProjectStore::create(&package, &document).unwrap();
    let stored_request = store
        .record_bridge_generation_request(
            GenerationRequestInput {
                request_id: binding.identity.request_id.clone(),
                expected_revision: binding.revision_id.clone(),
                hold_id: binding.target.hold_id.clone(),
                context_sha256: binding.input.sha256.clone(),
                constraints: binding.constraints.clone(),
                provider: binding.provider.clone(),
            },
            binding.plan.clone(),
        )
        .unwrap();
    assert_eq!(stored_request.bridge_plan.as_ref(), Some(&binding.plan));
    store
        .begin_generation_attempt(BeginGenerationAttempt {
            identity: binding.identity.clone(),
            cancellation_token: CancellationToken::new("cancel").unwrap(),
        })
        .unwrap();
    for stage in [WorkerStage::Preflight, WorkerStage::Inference] {
        store
            .record_generation_worker_message(&WorkerMessage::Stage {
                protocol: ProtocolVersion::V2,
                identity: binding.identity.clone(),
                stage,
            })
            .unwrap();
    }
    store
        .record_generation_worker_message(&WorkerMessage::CompletedBridge {
            protocol: ProtocolVersion::V2,
            identity: binding.identity.clone(),
            candidate: fixture.declaration.clone(),
        })
        .unwrap();
    let bundle = qualify_bridge(
        Path::new(env!("CARGO_BIN_EXE_deadpan-media-worker")),
        &fixture.workspace,
        &fixture.request,
        &fixture.declaration,
        &fixture.selected_provider(),
        limits(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let native_ref = bundle.native().object().clone();
    let sampled_ref = bundle.sampled().object().clone();
    let provenance_ref = bundle.provenance().object().clone();
    let receipt = BundleValidationReceipt::new(
        &fixture.declaration,
        native_ref.clone(),
        sampled_ref.clone(),
        provenance_ref.clone(),
        binding.constraints.video.clone(),
        binding.plan.clone(),
        ValidatorIdentity::new("native-ffv1", "bridge-1").unwrap(),
    )
    .unwrap();
    let budget = GeneratedMediaLimits::new(1024 * 1024).unwrap();
    assert!(
        store
            .record_generation_bundle_ready(
                &binding.identity,
                &fixture.declaration,
                receipt.clone(),
                budget
            )
            .is_err()
    );
    assert_eq!(
        store
            .generation_attempt(&binding.identity)
            .unwrap()
            .unwrap()
            .checkpoint
            .state,
        JobState::Validating
    );
    let (mut native, mut sampled, mut provenance) = bundle.into_parts();
    store
        .promote_generated_object(&mut native, &native_ref, budget)
        .unwrap();
    store
        .promote_generated_object(&mut sampled, &sampled_ref, budget)
        .unwrap();
    assert!(
        store
            .record_generation_bundle_ready(
                &binding.identity,
                &fixture.declaration,
                receipt.clone(),
                budget
            )
            .is_err()
    );
    store
        .promote_generated_object(&mut provenance, &provenance_ref, budget)
        .unwrap();
    store
        .record_generation_bundle_ready(
            &binding.identity,
            &fixture.declaration,
            receipt.clone(),
            budget,
        )
        .unwrap();
    assert_eq!(store.snapshot().unwrap(), document);
    assert!(
        store
            .selected_generation_candidate(&binding.identity.request_id)
            .unwrap()
            .is_none()
    );
    drop(store);
    let reopened = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
    let selected = reopened
        .selected_generation_bundle(&binding.identity.request_id)
        .unwrap()
        .unwrap();
    assert_eq!(selected.identity, binding.identity);
    assert_eq!(selected.receipt, receipt);
    assert_eq!(reopened.snapshot().unwrap(), document);
    for object in [&native_ref, &sampled_ref, &provenance_ref] {
        let mut bytes = Vec::new();
        reopened
            .snapshot_generated_object(object, budget)
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(
            blake3::hash(&bytes).to_hex().as_str(),
            object.content().digest()
        );
    }
}
