//! These fixtures use the production envelope serializer, the qualified 25@24
//! and 30@30000/1001 clock contracts, and opaque byte objects. They verify durable
//! evidence admission, not media decoding or the worker's model-loading claims.

use deadpan_core::{AssetId, FrameDuration, FrameRate, GeneratedArtifact, NodeId};
use deadpan_jobs::{
    AttemptId, AxisLimits, BridgeCapability, ConditioningMode, DimensionLimits, FrameCountFormula,
    MotionAmount, NativeDimensions, ProviderPackId, ProviderPackVersion, RequestId, RequestVersion,
    RuntimeId, RuntimeVersion, VideoSpec, WorkspaceArtifact, WorkspaceRef,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256 as Sha256Hasher};

use super::*;
use crate::{
    AcceptedBridgeEvidence, BridgeContext, ConditioningReceipt, SelectedBridgeProvider,
    StoredBridgeProvenance,
};

fn object(bytes: &[u8]) -> GeneratedObjectRef {
    GeneratedObjectRef::new(
        GeneratedContentId::new(blake3::hash(bytes).to_hex().to_string()).unwrap(),
        u64::try_from(bytes.len()).unwrap(),
    )
    .unwrap()
}

fn declaration(reference: &str, bytes: &[u8]) -> WorkspaceArtifact {
    WorkspaceArtifact::new(
        WorkspaceRef::new(reference).unwrap(),
        Sha256::new(
            Sha256Hasher::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        )
        .unwrap(),
        u64::try_from(bytes.len()).unwrap(),
    )
    .unwrap()
}

fn report(video: VideoContract, bytes: &[u8], output_hash: &str) -> ConversionReport {
    ConversionReport {
        protocol: deadpan_media::protocol::REPORT_PROTOCOL_VERSION,
        video,
        output_bytes: u64::try_from(bytes.len()).unwrap(),
        input_rgb_sha256: "a".repeat(64),
        output_rgb_sha256: output_hash.into(),
        input_time_base_num: 1,
        input_time_base_den: 12288,
        output_time_base_num: 1,
        output_time_base_den: 1000,
        first_output_pts: 0,
        last_output_pts: video.matroska_pts(video.frames - 1).unwrap(),
        last_output_duration: i64::from(video.rate_den * 1000 / video.rate_num),
        ffv1_version: 3,
        slice_crc: true,
        discarded_audio_streams: 0,
    }
}

struct Fixture {
    envelope: Value,
    artifact: GeneratedArtifact,
    project: ProjectId,
    context: Vec<u8>,
}

impl Fixture {
    fn new() -> Self {
        let capability = BridgeCapability::new(
            true,
            FrameRate::new(24, 1).unwrap(),
            FrameCountFormula::new(8, 1, 25, 97).unwrap(),
            DimensionLimits::new(
                AxisLimits::new(4, 4, 1).unwrap(),
                AxisLimits::new(2, 2, 1).unwrap(),
            ),
        );
        let plan = BridgeGenerationPlan::new(
            FrameDuration::new(30).unwrap(),
            FrameRate::new(30000, 1001).unwrap(),
            &capability,
            NativeDimensions::new(4, 2).unwrap(),
        )
        .unwrap();
        let left_bytes = b"prepared left image";
        let right_bytes = b"prepared right image";
        let left = declaration("inputs/left.png", left_bytes);
        let right = declaration("inputs/right.png", right_bytes);
        let context =
            BridgeContext::new(plan.clone(), left.clone(), right.clone(), "fixture RGB").unwrap();
        let context_bytes = serde_json::to_vec_pretty(&context).unwrap();
        let manifest = declaration("inputs/context.json", &context_bytes);
        let conditioning: ConditioningReceipt = serde_json::from_value(json!({
            "schema_version":1,
            "manifest":{"declaration":manifest,"object":object(&context_bytes)},
            "left":{"declaration":left,"object":object(left_bytes)},
            "right":{"declaration":right,"object":object(right_bytes)},
        }))
        .unwrap();
        let binding = GenerationBinding {
            identity: MessageIdentity::new(
                RequestId::new("request").unwrap(),
                AttemptId::new("attempt").unwrap(),
            ),
            project_id: ProjectId::new("project").unwrap(),
            revision_id: RevisionId::new("historical-revision").unwrap(),
            target: HoldTarget {
                hold_id: NodeId::new("original-hold").unwrap(),
                request_version: RequestVersion::new(1).unwrap(),
            },
            input: ContextArtifact {
                manifest: manifest.reference().clone(),
                sha256: manifest.sha256().clone(),
            },
            constraints: HoldConstraints {
                video: VideoSpec::new(plan.project_frames(), plan.project_frame_rate(), 4, 2)
                    .unwrap(),
                conditioning: ConditioningMode::Bridge,
                motion: MotionAmount::Still,
            },
            provider: ProviderSelection {
                pack_id: ProviderPackId::new("fixture").unwrap(),
                pack_version: ProviderPackVersion::new("1").unwrap(),
                runtime_id: RuntimeId::new("fixture").unwrap(),
                runtime_version: RuntimeVersion::new("1").unwrap(),
                seed: 1,
            },
            plan,
        };
        let worker_native = b"opaque worker native fixture";
        let worker = serde_json::to_string_pretty(&json!({
            "schema_version":2,"request_binding":binding,
            "runtime_commit":"a".repeat(40),"pack_revision":"b".repeat(40),"gemma_revision":"c".repeat(40),
            "adapter_sources_sha256":{"adapter.py":"a".repeat(64)},
            "loaded_ltx_sources_sha256":{"runtime.py":"b".repeat(64)},
            "verified_assets":[{"repository":"fixture/model","path":"weights.bin","size":1,"sha256":"c".repeat(64)}],
            "prompt_version":"fixture-1","prompt":"Keep the scene still.","seed":1,
            "context":context,"configuration":{"fixture":true},
            "model_color_interpretation":"fixture SDR sRGB","temporal_interpolation":"fixture linear encoded RGB",
            "conditioning_preprocessing":"fixture no transform",
            "native_sha256":declaration("outputs/native.mp4", worker_native).sha256(),
            "native_bytes":worker_native.len(),
        })).unwrap();
        let declaration = NativeCandidateManifest {
            native: declaration("outputs/native.mp4", worker_native),
            provenance: declaration("outputs/provenance.json", worker.as_bytes()),
            video: VideoSpec::new(
                FrameDuration::new(25).unwrap(),
                FrameRate::new(24, 1).unwrap(),
                4,
                2,
            )
            .unwrap(),
            provider: binding.provider.clone(),
        };
        let native_bytes = b"opaque canonical native fixture";
        let sampled_bytes = b"opaque canonical sampled fixture";
        let native = object(native_bytes);
        let sampled = object(sampled_bytes);
        let native_report = report(
            VideoContract {
                width: 4,
                height: 2,
                frames: 25,
                rate_num: 24,
                rate_den: 1,
            },
            native_bytes,
            &"a".repeat(64),
        );
        let sampled_report = report(
            VideoContract {
                width: 4,
                height: 2,
                frames: 30,
                rate_num: 30000,
                rate_den: 1001,
            },
            sampled_bytes,
            &"b".repeat(64),
        );
        let selected = SelectedBridgeProvider::new(binding.provider.clone(), capability);
        let bytes = crate::bounded_json::encode(
            &HostProvenance {
                schema_version: 3,
                validation_profile: "deadpan-ffv1-bridge-3",
                binding: &binding,
                selected_provider: &selected,
                declaration: &declaration,
                native: &native,
                sampled: &sampled,
                native_validation: &native_report,
                sampled_validation: &sampled_report,
                conditioning: &conditioning,
                native_span: native_report.output_span().unwrap(),
                sampled_span: sampled_report.output_span().unwrap(),
                worker_provenance_utf8: &worker,
            },
            128 * 1024,
        )
        .unwrap();
        Self {
            envelope: serde_json::from_slice(&bytes).unwrap(),
            artifact: GeneratedArtifact {
                sampled_asset: AssetId::new("sampled").unwrap(),
                sampled_object: sampled,
                native_asset: AssetId::new("native").unwrap(),
                native_object: native,
                provenance: object(&bytes),
                sampling: binding.plan.sampling_map().unwrap(),
            },
            project: binding.project_id,
            context: context_bytes,
        }
    }

    fn wire(&self) -> (Vec<u8>, GeneratedArtifact) {
        let bytes = serde_json::to_vec(&self.envelope).unwrap();
        let mut artifact = self.artifact.clone();
        artifact.provenance = object(&bytes);
        (bytes, artifact)
    }

    fn validate(&self) -> Result<AcceptedBridgeEvidence, QualificationError> {
        let (bytes, artifact) = self.wire();
        StoredBridgeProvenance::from_bytes(&bytes, &artifact.provenance)?.validate_for(
            &artifact,
            &self.project,
            &self.context,
        )
    }

    fn replace_worker(&mut self, worker: String) {
        self.envelope["declaration"]["provenance"] =
            serde_json::to_value(declaration("outputs/provenance.json", worker.as_bytes()))
                .unwrap();
        self.envelope["worker_provenance_utf8"] = json!(worker);
    }

    fn replace_context(&mut self, context: Value) {
        self.context = serde_json::to_vec(&context).unwrap();
        let declared = declaration("inputs/context.json", &self.context);
        self.envelope["conditioning"]["manifest"] =
            json!({"declaration":declared,"object":object(&self.context)});
        self.envelope["binding"]["input"]["sha256"] = json!(declared.sha256());
    }
}

#[test]
fn stored_production_envelope_preserves_measured_clocks_and_historical_identity() {
    let mut fixture = Fixture::new();
    // The durable reader never binds mutable asset aliases or a current Hold/revision.
    fixture.artifact.sampled_asset = AssetId::new("copied-sampled").unwrap();
    fixture.artifact.native_asset = AssetId::new("copied-native").unwrap();
    let evidence = fixture.validate().unwrap();
    assert_eq!(evidence.native_contract().frames, 25);
    assert_eq!(evidence.sampled_contract().frames, 30);
    assert_eq!(evidence.native_span().end().ticks, 1041);
    assert_eq!(evidence.sampled_span().end().ticks, 1001);
    assert_eq!(evidence.native_object(), &fixture.artifact.native_object);
    assert_eq!(evidence.sampled_object(), &fixture.artifact.sampled_object);
    assert_eq!(evidence.context_object(), &object(&fixture.context));
    assert_eq!(evidence.left_object(), &object(b"prepared left image"));
    assert_eq!(evidence.right_object(), &object(b"prepared right image"));
    assert_eq!(
        evidence.left_sha256(),
        declaration("left", b"prepared left image")
            .sha256()
            .as_str()
    );
    assert_eq!(
        evidence.right_sha256(),
        declaration("right", b"prepared right image")
            .sha256()
            .as_str()
    );
    assert_eq!(evidence.provenance_object(), &fixture.wire().1.provenance);
}

#[test]
fn stored_wire_rejects_hash_length_duplicate_unknown_legacy_and_oversize() {
    let fixture = Fixture::new();
    let (bytes, artifact) = fixture.wire();
    assert!(StoredBridgeProvenance::from_bytes(&bytes, &object(b"wrong")).is_err());
    let bad_length = GeneratedObjectRef::new(
        artifact.provenance.content().clone(),
        artifact.provenance.byte_length() + 1,
    )
    .unwrap();
    assert!(StoredBridgeProvenance::from_bytes(&bytes, &bad_length).is_err());
    for field in ["\"schema_version\":3,", "\"unknown\":true,"] {
        let altered =
            String::from_utf8(bytes.clone())
                .unwrap()
                .replacen('{', &format!("{{{field}"), 1);
        assert!(
            StoredBridgeProvenance::from_bytes(altered.as_bytes(), &object(altered.as_bytes()))
                .is_err()
        );
    }
    for version in [1, 2, 4] {
        let mut changed = Fixture::new();
        changed.envelope["schema_version"] = json!(version);
        assert!(changed.validate().is_err());
    }
    let oversized = vec![b' '; 32 * 1024 * 1024 + 1];
    assert!(StoredBridgeProvenance::from_bytes(&oversized, &object(&oversized)).is_err());
}

#[test]
fn stored_semantic_substitutions_fail_after_rehashing_the_envelope() {
    for (pointer, replacement) in [
        ("/validation_profile", json!("deadpan-ffv1-bridge-2")),
        ("/selected_provider/supported", json!(false)),
        ("/selected_provider/selection/seed", json!(2)),
        ("/selected_provider/frame_counts/minimum", json!(33)),
        (
            "/binding/constraints/conditioning",
            json!("extend_from_left"),
        ),
        ("/binding/constraints/video/width", json!(5)),
        ("/binding/constraints/video/frames", json!(29)),
        ("/declaration/provider/seed", json!(2)),
        ("/declaration/video/frames", json!(24)),
        ("/native/content/digest", json!("c".repeat(64))),
        ("/sampled/content/digest", json!("d".repeat(64))),
        ("/native_validation/output_bytes", json!(1)),
        ("/sampled_validation/protocol", json!(1)),
        ("/sampled_validation/video/width", json!(5)),
        ("/sampled_validation/video/frames", json!(29)),
        ("/sampled_validation/video/rate_num", json!(30001)),
        ("/native_validation/input_rgb_sha256", json!("c".repeat(64))),
        (
            "/sampled_validation/input_rgb_sha256",
            json!("d".repeat(64)),
        ),
        ("/sampled_validation/output_rgb_sha256", json!("invalid")),
        ("/sampled_validation/input_time_base_den", json!(0)),
        ("/sampled_validation/first_output_pts", json!(1)),
        ("/native_validation/last_output_pts", json!(1001)),
        ("/native_validation/last_output_duration", json!(42)),
        ("/sampled_validation/ffv1_version", json!(1)),
        ("/sampled_validation/slice_crc", json!(false)),
        ("/sampled_validation/discarded_audio_streams", json!(8)),
        ("/native_span/end/ticks", json!(1042)),
        ("/sampled_span/end/ticks", json!(1002)),
        (
            "/conditioning/manifest/declaration/sha256",
            json!("e".repeat(64)),
        ),
        ("/conditioning/left/declaration/byte_length", json!(1)),
        (
            "/conditioning/right/declaration/sha256",
            json!("f".repeat(64)),
        ),
        ("/worker_provenance_utf8", json!("{}")),
    ] {
        let mut changed = Fixture::new();
        *changed
            .envelope
            .pointer_mut(pointer)
            .unwrap_or_else(|| panic!("missing {pointer}")) = replacement;
        assert!(changed.validate().is_err(), "admitted {pointer}");
    }
}

#[test]
fn stored_artifact_project_and_sampling_substitutions_fail() {
    let fixture = Fixture::new();
    let (bytes, artifact) = fixture.wire();
    let read = || StoredBridgeProvenance::from_bytes(&bytes, &artifact.provenance).unwrap();
    assert!(
        read()
            .validate_for(
                &artifact,
                &ProjectId::new("other").unwrap(),
                &fixture.context
            )
            .is_err()
    );
    for slot in ["native", "sampled", "provenance", "sampling"] {
        let mut changed = artifact.clone();
        match slot {
            "native" => changed.native_object = object(b"other native"),
            "sampled" => changed.sampled_object = object(b"other sampled"),
            "provenance" => changed.provenance = object(b"other provenance"),
            "sampling" => {
                changed.sampling = deadpan_core::BridgeSamplingMap::new(
                    artifact.sampling.project_rate(),
                    artifact.sampling.native_rate(),
                    artifact.sampling.native_frame_count(),
                    FrameDuration::new(29).unwrap(),
                    artifact.sampling.interpolation(),
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            read()
                .validate_for(&changed, &fixture.project, &fixture.context)
                .is_err(),
            "{slot}"
        );
    }
}

#[test]
fn retained_context_and_worker_bytes_require_both_hashes_and_strict_bindings() {
    let mut changed = Fixture::new();
    changed.context.push(b' ');
    assert!(changed.validate().is_err());
    let mut changed = Fixture::new();
    changed.envelope["conditioning"]["manifest"]["object"]["content"]["digest"] =
        json!("e".repeat(64));
    assert!(changed.validate().is_err());
    let mut changed = Fixture::new();
    changed.envelope["conditioning"]["manifest"]["declaration"]["sha256"] = json!("e".repeat(64));
    changed.envelope["binding"]["input"]["sha256"] = json!("e".repeat(64));
    assert!(changed.validate().is_err());
    let mut changed = Fixture::new();
    let original = changed.envelope["worker_provenance_utf8"].as_str().unwrap();
    // Same length and valid JSON, but the declared SHA-256 remains the old one.
    changed.envelope["worker_provenance_utf8"] = json!(original.replacen("Keep", "Hold", 1));
    assert!(changed.validate().is_err());

    for replacement in [
        "duplicate",
        "wrong-seed",
        "wrong-native",
        "empty-assets",
        "wrong-binding",
    ] {
        let mut changed = Fixture::new();
        let original = changed.envelope["worker_provenance_utf8"].as_str().unwrap();
        let mut worker: Value = serde_json::from_str(original).unwrap();
        match replacement {
            "wrong-seed" => worker["seed"] = json!(2),
            "wrong-native" => worker["native_sha256"] = json!("d".repeat(64)),
            "empty-assets" => worker["verified_assets"] = json!([]),
            "wrong-binding" => worker["request_binding"]["revision_id"] = json!("later"),
            _ => {}
        }
        let text = if replacement == "duplicate" {
            original.replacen('{', "{\"seed\":1,", 1)
        } else {
            serde_json::to_string(&worker).unwrap()
        };
        changed.replace_worker(text);
        assert!(changed.validate().is_err(), "{replacement}");
    }

    for replacement in ["unknown", "wrong-input", "wrong-plan"] {
        let mut changed = Fixture::new();
        let mut context: Value = serde_json::from_slice(&changed.context).unwrap();
        match replacement {
            "unknown" => context["unknown"] = json!(true),
            "wrong-input" => context["left"]["sha256"] = json!("a".repeat(64)),
            "wrong-plan" => context["plan"]["schema_version"] = json!(99),
            _ => unreachable!(),
        }
        changed.replace_context(context);
        assert!(changed.validate().is_err(), "{replacement}");
    }
}

#[test]
fn retained_nested_resource_bounds_are_enforced_before_admission() {
    let mut changed = Fixture::new();
    changed.replace_worker(" ".repeat(4 * 1024 * 1024 + 1));
    assert!(changed.validate().is_err());
    for field in ["manifest", "left", "right"] {
        let mut changed = Fixture::new();
        let limit = if field == "manifest" {
            1024 * 1024
        } else {
            64 * 1024 * 1024
        };
        changed.envelope["conditioning"][field]["object"]["byte_length"] = json!(limit + 1);
        changed.envelope["conditioning"][field]["declaration"]["byte_length"] = json!(limit + 1);
        assert!(changed.validate().is_err(), "{field}");
    }
    let mut changed = Fixture::new();
    let duplicate = String::from_utf8(changed.context.clone())
        .unwrap()
        .replacen('{', "{\"schema_version\":1,", 1);
    changed.context = duplicate.into_bytes();
    let declared = declaration("inputs/context.json", &changed.context);
    changed.envelope["conditioning"]["manifest"] =
        json!({"declaration":declared,"object":object(&changed.context)});
    changed.envelope["binding"]["input"]["sha256"] = json!(declared.sha256());
    assert!(changed.validate().is_err());
}
