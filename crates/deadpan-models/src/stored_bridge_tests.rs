//! These fixtures use the production envelope serializer, the qualified 25@24
//! and 30@30000/1001 clock contracts, and opaque byte objects. They verify durable
//! evidence admission, not media decoding or the worker's model-loading claims.

use deadpan_core::{AssetId, FrameDuration, FrameRate, GeneratedArtifact, NodeId};
use deadpan_jobs::{
    AttemptId, AxisLimits, BridgeCapability, BridgeGenerationPlan, ConditioningMode,
    DimensionLimits, FrameCountFormula, MotionAmount, NativeDimensions, ProviderPackId,
    ProviderPackVersion, RequestId, RequestVersion, RuntimeId, RuntimeVersion, VideoSpec,
    WorkspaceArtifact, WorkspaceRef,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256 as Sha256Hasher};

use super::*;
use crate::{
    AcceptedBridgeEvidence, BridgeContext, ConditioningReceipt, SelectedBridgeProvider,
    StoredBridgeProvenance,
};

#[test]
fn accepted_controls_require_matching_host_and_worker_bindings() {
    let mut fixture = Fixture::new();
    fixture.envelope["binding"]["constraints"]["motion"] = json!("subtle");
    fixture.envelope["binding"]["constraints"]["instructions"] = json!("Keep the hands still.");
    assert!(
        fixture.validate().is_err(),
        "a host-only controls change must fail"
    );
    let mut worker: Value =
        serde_json::from_str(fixture.envelope["worker_provenance_utf8"].as_str().unwrap()).unwrap();
    worker["request_binding"] = fixture.envelope["binding"].clone();
    fixture.replace_worker(serde_json::to_string(&worker).unwrap());
    let options = fixture.validate().unwrap().generation_options();
    assert_eq!(options.motion, MotionAmount::Subtle);
    assert_eq!(
        options.instructions.unwrap().as_str(),
        "Keep the hands still."
    );
    assert_eq!(options.region_target, deadpan_jobs::GenerationTarget::None);
}

#[test]
fn stored_schema5_requires_endpoint_evidence_bound_to_retained_geometry() {
    let evidence = Fixture::endpoint_checked().validate().unwrap();
    assert!(evidence.quality().is_some());
    assert_eq!(evidence.endpoints().unwrap().entry().sampled_frame, 0);
    assert_eq!(evidence.endpoints().unwrap().exit().sampled_frame, 29);
    let mut missing = Fixture::endpoint_checked();
    missing
        .envelope
        .as_object_mut()
        .unwrap()
        .remove("endpoints");
    assert!(missing.validate().is_err());
    let mut null = Fixture::endpoint_checked();
    null.envelope["endpoints"] = Value::Null;
    assert!(null.validate().is_err());
    for field in [
        "sampled_object",
        "context_object",
        "left_object",
        "right_object",
    ] {
        let mut changed = Fixture::endpoint_checked();
        changed.envelope["endpoints"][field] = json!(object(b"different retained object"));
        assert!(changed.validate().is_err(), "{field}");
    }
    let mut wrong_geometry = Fixture::endpoint_checked();
    wrong_geometry.envelope["endpoints"]["geometry"]["presentation"] =
        json!({"x":1,"y":0,"width":2,"height":2});
    let (bytes, artifact) = wrong_geometry.wire();
    // The centered rectangle is structurally valid but disagrees with the
    // separately retained context. Full media admission must bind both.
    let parsed = StoredBridgeProvenance::from_bytes(&bytes, &artifact.provenance).unwrap();
    assert!(
        parsed
            .validate_for(&artifact, &wrong_geometry.project, &wrong_geometry.context)
            .is_err()
    );
    for endpoint in ["entry", "exit"] {
        let mut rejected = Fixture::endpoint_checked();
        rejected.envelope["endpoints"][endpoint]["mean_absolute_rgb_difference"] = json!(100.0);
        rejected.envelope["endpoints"][endpoint]["gross_cell_fraction"] = json!(1.0);
        assert!(rejected.validate().is_err(), "{endpoint}");
    }
    for profile in [3, 4] {
        let mut wrong_version = Fixture::endpoint_checked();
        wrong_version.envelope["schema_version"] = json!(profile);
        wrong_version.envelope["validation_profile"] =
            json!(format!("deadpan-ffv1-bridge-{profile}"));
        assert!(wrong_version.validate().is_err());
    }
    let mut unknown = Fixture::endpoint_checked();
    unknown.envelope["endpoints"]["unknown"] = json!(true);
    assert!(unknown.validate().is_err());
}

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
    /// An accepted bundle whose retained context uses the version-1 grammar
    /// written before measured boundary evidence existed.
    fn new() -> Self {
        Self::with_context(|plan, left, right| {
            BridgeContext::legacy_v1(plan, left, right, "fixture RGB").unwrap()
        })
    }

    /// The same bundle with a version-2 context declaring `model_color_space`.
    fn measured(model_color_space: crate::BridgeColor) -> Self {
        Self::with_context(|plan, left, right| {
            BridgeContext::legacy_v2(
                plan,
                left,
                right,
                "fixture RGB",
                model_color_space,
                crate::BridgeBoundaries {
                    left: crate::BoundaryPicture::AuthoredBlack {
                        clock: crate::BoundaryClock::Project { frame: 9 },
                    },
                    right: crate::BoundaryPicture::AuthoredBlack {
                        clock: crate::BoundaryClock::Project { frame: 40 },
                    },
                },
            )
            .unwrap()
        })
    }

    fn with_context(
        context: impl FnOnce(
            BridgeGenerationPlan,
            WorkspaceArtifact,
            WorkspaceArtifact,
        ) -> BridgeContext,
    ) -> Self {
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
        let context = context(plan.clone(), left.clone(), right.clone());
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
                instructions: None,
                region_target: None,
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
                quality: None,
                endpoints: None,
                geometry: None,
                region: None,
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
                sampling: binding.plan.sampling_map().unwrap().into(),
                content_aspect: None,
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

    fn schema4(mut self, report_motion: Option<MotionAmount>) -> Self {
        let plan: BridgeGenerationPlan =
            serde_json::from_value(self.envelope["binding"]["plan"].clone()).unwrap();
        let motion: MotionAmount =
            serde_json::from_value(self.envelope["binding"]["constraints"]["motion"].clone())
                .unwrap();
        self.envelope["schema_version"] = json!(4);
        self.envelope["validation_profile"] = json!("deadpan-ffv1-bridge-4");
        self.envelope["quality"] = serde_json::to_value(crate::quality::test_report(
            &plan,
            report_motion.unwrap_or(motion),
        ))
        .unwrap();
        self
    }

    fn endpoint_checked() -> Self {
        let mut fixture = Self::with_context(|plan, left, right| {
            BridgeContext::new(
                plan,
                left,
                right,
                "fixture RGB",
                crate::CANONICAL_BRIDGE_COLOR,
                crate::BridgeBoundaries {
                    left: crate::BoundaryPicture::AuthoredBlack {
                        clock: crate::BoundaryClock::Project { frame: 9 },
                    },
                    right: crate::BoundaryPicture::AuthoredBlack {
                        clock: crate::BoundaryClock::Project { frame: 40 },
                    },
                },
                crate::ConditioningGeometry {
                    presentation: crate::RasterRect::new(0, 0, 4, 2).unwrap(),
                    left_content: None,
                    right_content: None,
                },
            )
            .unwrap()
        })
        .schema4(None);
        let plan = serde_json::from_value(fixture.envelope["binding"]["plan"].clone()).unwrap();
        let conditioning =
            serde_json::from_value(fixture.envelope["conditioning"].clone()).unwrap();
        let context: BridgeContext = serde_json::from_slice(&fixture.context).unwrap();
        let endpoints = crate::endpoints::test_report(
            &plan,
            &fixture.artifact.sampled_object,
            &conditioning,
            *context.geometry().unwrap(),
        );
        fixture.envelope["schema_version"] = json!(5);
        fixture.envelope["validation_profile"] = json!("deadpan-ffv1-bridge-5");
        fixture.envelope["endpoints"] = serde_json::to_value(endpoints).unwrap();
        fixture
    }

    fn geometry_checked() -> Self {
        let mut fixture = Self::endpoint_checked();
        let plan = serde_json::from_value(fixture.envelope["binding"]["plan"].clone()).unwrap();
        let conditioning =
            serde_json::from_value(fixture.envelope["conditioning"].clone()).unwrap();
        let context: BridgeContext = serde_json::from_slice(&fixture.context).unwrap();
        let geometry = crate::geometry::test_report(
            &plan,
            &fixture.artifact.native_object,
            &conditioning,
            *context.geometry().unwrap(),
        );
        fixture.envelope["schema_version"] = json!(6);
        fixture.envelope["validation_profile"] = json!("deadpan-ffv1-bridge-6");
        fixture.envelope["geometry"] = serde_json::to_value(geometry).unwrap();
        fixture
    }

    fn region_checked() -> Self {
        let mut fixture = Self::geometry_checked();
        let plan = serde_json::from_value(fixture.envelope["binding"]["plan"].clone()).unwrap();
        let conditioning =
            serde_json::from_value(fixture.envelope["conditioning"].clone()).unwrap();
        let context: BridgeContext = serde_json::from_slice(&fixture.context).unwrap();
        let region = crate::region::test_report(
            &plan,
            &fixture.artifact.native_object,
            &conditioning,
            &context,
        );
        fixture.envelope["schema_version"] = json!(7);
        fixture.envelope["validation_profile"] = json!("deadpan-ffv1-bridge-7");
        fixture.envelope["region"] = serde_json::to_value(region).unwrap();
        fixture
    }

    fn definition_checked() -> Self {
        let mut fixture = Self::region_checked();
        let mut context: Value = serde_json::from_slice(&fixture.context).unwrap();
        context["schema_version"] = json!(5);
        for (side, numerator) in [("left", "19"), ("right", "81")] {
            context["boundaries"][side] = json!({"authored_black":{"clock":{
                "kind":"definition",
                "project_id":fixture.envelope["binding"]["project_id"],
                "revision_id":fixture.envelope["binding"]["revision_id"],
                "definition":"hold-definition",
                "position":{"numerator":numerator,"denominator":"2"},
            }}});
        }
        fixture.replace_bound_context(context);
        fixture.envelope["schema_version"] = json!(8);
        fixture.envelope["validation_profile"] = json!("deadpan-ffv1-bridge-8");
        fixture
    }

    /// Recompute every content binding so admission failures isolate the
    /// context's semantic relationship to the immutable generation origin.
    fn replace_bound_context(&mut self, context: Value) {
        self.replace_context(context.clone());
        let object = self.envelope["conditioning"]["manifest"]["object"].clone();
        for report in ["endpoints", "geometry"] {
            self.envelope[report]["context_object"] = object.clone();
        }
        // Region evidence retains the exact boundary clocks as well as the
        // context object. Rebuild it when the fixture changes those clocks.
        let plan = serde_json::from_value(self.envelope["binding"]["plan"].clone()).unwrap();
        let conditioning = serde_json::from_value(self.envelope["conditioning"].clone()).unwrap();
        let retained: BridgeContext = serde_json::from_value(context.clone()).unwrap();
        self.envelope["region"] = serde_json::to_value(crate::region::test_report(
            &plan,
            &self.artifact.native_object,
            &conditioning,
            &retained,
        ))
        .unwrap();
        let mut worker: Value =
            serde_json::from_str(self.envelope["worker_provenance_utf8"].as_str().unwrap())
                .unwrap();
        worker["request_binding"] = self.envelope["binding"].clone();
        worker["context"] = context;
        self.replace_worker(serde_json::to_string(&worker).unwrap());
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
fn schema8_admits_exact_definition_clocks_and_retained_project_clocks() {
    let mut fixture = Fixture::definition_checked();
    let context: BridgeContext = serde_json::from_slice(&fixture.context).unwrap();
    assert_eq!(context.schema_version(), 5);
    // An accepted provider remains usable after copy/alias changes. Its
    // boundary clock still names the immutable historical generation origin.
    fixture.artifact.sampled_asset = AssetId::new("copied-sampled").unwrap();
    fixture.artifact.native_asset = AssetId::new("copied-native").unwrap();
    let evidence = fixture.validate().unwrap();
    assert!(evidence.geometry().is_some());
    assert!(evidence.region().is_some());

    // Profile 8 strengthens definition provenance, while the explicitly tagged
    // Project clock of context4 keeps its established meaning.
    let mut project_clock = Fixture::region_checked();
    assert_eq!(
        serde_json::from_slice::<BridgeContext>(&project_clock.context)
            .unwrap()
            .schema_version(),
        4
    );
    assert!(project_clock.validate().is_ok()); // Retained profile 7.
    project_clock.envelope["schema_version"] = json!(8);
    project_clock.envelope["validation_profile"] = json!("deadpan-ffv1-bridge-8");
    assert!(project_clock.validate().is_ok());
}

#[test]
fn schema8_rejects_definition_clocks_bound_to_another_immutable_origin() {
    for field in ["project_id", "revision_id"] {
        let mut fixture = Fixture::definition_checked();
        let mut context: Value = serde_json::from_slice(&fixture.context).unwrap();
        for side in ["left", "right"] {
            context["boundaries"][side]["authored_black"]["clock"][field] = json!("other-origin");
        }
        fixture.replace_bound_context(context);
        let error = match fixture.validate() {
            Ok(_) => panic!("accepted another {field}"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("worker origin"),
            "{field}: {error}"
        );
    }
}

#[test]
fn definition_clocks_cannot_be_downgraded_to_older_host_profiles() {
    let mut fixture = Fixture::definition_checked();
    fixture.envelope["schema_version"] = json!(7);
    fixture.envelope["validation_profile"] = json!("deadpan-ffv1-bridge-7");
    let error = match fixture.validate() {
        Ok(_) => panic!("definition clocks admitted under profile 7"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("definition clocks require stored bridge profile 8")
    );
    for missing in ["quality", "endpoints", "geometry", "region"] {
        let mut fixture = Fixture::definition_checked();
        fixture.envelope.as_object_mut().unwrap().remove(missing);
        assert!(fixture.validate().is_err(), "profile 8 missing {missing}");
    }
    let mut mismatched = Fixture::definition_checked();
    mismatched.envelope["validation_profile"] = json!("deadpan-ffv1-bridge-7");
    assert!(mismatched.validate().is_err());
}

#[test]
fn schema6_with_actual_context3_remains_readable_without_region_capture() {
    let mut fixture = Fixture::geometry_checked();
    let mut context: Value = serde_json::from_slice(&fixture.context).unwrap();
    context["schema_version"] = json!(3);
    context.as_object_mut().unwrap().remove("region");
    fixture.replace_context(context.clone());
    let context_object = fixture.envelope["conditioning"]["manifest"]["object"].clone();
    for report in ["geometry", "endpoints"] {
        fixture.envelope[report]["context_object"] = context_object.clone();
    }
    let mut worker: Value =
        serde_json::from_str(fixture.envelope["worker_provenance_utf8"].as_str().unwrap()).unwrap();
    worker["request_binding"] = fixture.envelope["binding"].clone();
    worker["context"] = context;
    fixture.replace_worker(serde_json::to_string(&worker).unwrap());
    let retained: BridgeContext = serde_json::from_slice(&fixture.context).unwrap();
    assert_eq!(retained.schema_version(), 3);
    assert!(retained.region().is_none());
    let evidence = fixture.validate().unwrap();
    assert!(evidence.geometry().is_some());
    assert!(evidence.region().is_none());
}

#[test]
fn schema7_requires_region_evidence_and_explicit_capture_binding() {
    let evidence = Fixture::region_checked().validate().unwrap();
    assert_eq!(
        evidence.region().unwrap().unavailable_reason(),
        Some("no selected region target")
    );
    assert!(evidence.geometry().is_some());
    assert!(
        Fixture::geometry_checked()
            .validate()
            .unwrap()
            .region()
            .is_none()
    );
    for mutation in [
        "missing", "null", "unknown", "reason", "native", "geometry", "target",
    ] {
        let mut fixture = Fixture::region_checked();
        match mutation {
            "missing" => {
                fixture.envelope.as_object_mut().unwrap().remove("region");
            }
            "null" => fixture.envelope["region"] = Value::Null,
            "unknown" => fixture.envelope["region"]["unknown"] = json!(true),
            "reason" => fixture.envelope["region"]["evidence"]["reason"] = json!("passes"),
            "native" => {
                fixture.envelope["region"]["native_object"] = json!(object(b"other native"))
            }
            "geometry" => {
                fixture.envelope["region"]["geometry"]["presentation"] =
                    json!({"x":1,"y":0,"width":2,"height":2})
            }
            "target" => {
                fixture.envelope["binding"]["constraints"]["region_target"] = json!("other-subject")
            }
            _ => unreachable!(),
        }
        assert!(fixture.validate().is_err(), "{mutation}");
    }
    for version in [3, 4, 5, 6] {
        let mut fixture = Fixture::region_checked();
        fixture.envelope["schema_version"] = json!(version);
        fixture.envelope["validation_profile"] = json!(format!("deadpan-ffv1-bridge-{version}"));
        assert!(
            fixture.validate().is_err(),
            "region injected in legacy schema {version}"
        );
    }
}

#[test]
fn legacy_profiles_cannot_claim_a_selected_region_without_evidence() {
    for mut fixture in [
        Fixture::new(),
        Fixture::new().schema4(None),
        Fixture::endpoint_checked(),
        Fixture::geometry_checked(),
    ] {
        fixture.envelope["binding"]["constraints"]["region_target"] = json!("subject");
        let mut worker: Value =
            serde_json::from_str(fixture.envelope["worker_provenance_utf8"].as_str().unwrap())
                .unwrap();
        worker["request_binding"] = fixture.envelope["binding"].clone();
        fixture.replace_worker(serde_json::to_string(&worker).unwrap());
        let (bytes, artifact) = fixture.wire();
        let result = StoredBridgeProvenance::from_bytes(&bytes, &artifact.provenance);
        let error = match result {
            Ok(_) => panic!("admitted selected target under legacy profile"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("selected region targets require stored region evidence"),
            "{error}"
        );
    }
}

#[test]
fn schema6_requires_bound_recomputed_landmark_evidence_and_keeps_old_profiles() {
    let evidence = Fixture::geometry_checked().validate().unwrap();
    assert_eq!(
        evidence
            .geometry()
            .unwrap()
            .assessment()
            .geometry
            .measured_tracks,
        0
    );
    assert!(Fixture::new().validate().unwrap().geometry().is_none());
    assert!(
        Fixture::new()
            .schema4(None)
            .validate()
            .unwrap()
            .geometry()
            .is_none()
    );
    assert!(
        Fixture::endpoint_checked()
            .validate()
            .unwrap()
            .geometry()
            .is_none()
    );
    for change in [
        "missing",
        "null",
        "unknown",
        "wrong-object",
        "wrong-pts",
        "missing-frame",
        "runtime",
        "policy",
        "invented-measurement",
        "no-boundaries",
        "wrong-crop",
        "legacy-injection",
    ] {
        let mut fixture = Fixture::geometry_checked();
        match change {
            "missing" => {
                fixture.envelope.as_object_mut().unwrap().remove("geometry");
            }
            "null" => fixture.envelope["geometry"] = Value::Null,
            "unknown" => fixture.envelope["geometry"]["unknown"] = json!(true),
            "wrong-object" => {
                fixture.envelope["geometry"]["native_object"]["content"]["digest"] =
                    json!("a".repeat(64))
            }
            "wrong-pts" => {
                fixture.envelope["geometry"]["observations"]["frames"][0]["pts"] = json!(1)
            }
            "missing-frame" => {
                fixture.envelope["geometry"]["observations"]["frames"]
                    .as_array_mut()
                    .unwrap()
                    .pop();
            }
            "runtime" => fixture.envelope["geometry"]["runtime"]["request_revision"] = json!(2),
            "policy" => {
                fixture.envelope["geometry"]["assessment"]["thresholds"]["center_residual"] =
                    json!(1.0)
            }
            "invented-measurement" => {
                fixture.envelope["geometry"]["assessment"]["geometry"]["measured_tracks"] = json!(1)
            }
            "no-boundaries" => {
                fixture.envelope["geometry"]["observations"]["boundaries"] = Value::Null
            }
            "wrong-crop" => {
                fixture.envelope["geometry"]["geometry"]["presentation"]["width"] = json!(2)
            }
            "legacy-injection" => {
                fixture.envelope["schema_version"] = json!(5);
                fixture.envelope["validation_profile"] = json!("deadpan-ffv1-bridge-5");
            }
            _ => unreachable!(),
        }
        assert!(fixture.validate().is_err(), "{change}");
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
    assert!(evidence.quality().is_none());
}

#[test]
fn schema_four_requires_a_strict_quality_report_matching_the_request() {
    let fixture = Fixture::new().schema4(None);
    let evidence = fixture.validate().unwrap();
    assert_eq!(
        serde_json::to_value(
            evidence
                .quality()
                .expect("schema four has quality evidence")
        )
        .unwrap(),
        fixture.envelope["quality"]
    );

    let mut missing = Fixture::new().schema4(None);
    missing.envelope.as_object_mut().unwrap().remove("quality");
    assert!(missing.validate().is_err(), "schema four needs a report");

    let mut null = Fixture::new().schema4(None);
    null.envelope["quality"] = Value::Null;
    assert!(null.validate().is_err(), "quality cannot be null");

    let mut unknown = Fixture::new().schema4(None);
    unknown.envelope["quality"]["unknown"] = json!(true);
    assert!(unknown.validate().is_err(), "quality fields are strict");

    let mut missing_field = Fixture::new().schema4(None);
    let quality = missing_field.envelope["quality"].as_object_mut().unwrap();
    let field = quality
        .keys()
        .next()
        .cloned()
        .expect("quality report has required fields");
    quality.remove(&field);
    assert!(
        missing_field.validate().is_err(),
        "quality report fields are required"
    );

    let mut wrong_native_contract = Fixture::new().schema4(None);
    wrong_native_contract.envelope["quality"]["native"]["frames"] = json!(24);
    assert!(
        wrong_native_contract.validate().is_err(),
        "quality evidence must describe the planned native contract"
    );

    let mut bad_thresholds = Fixture::new().schema4(None);
    bad_thresholds.envelope["quality"]["thresholds"]["maximum_motion_per_second"] = json!(99.0);
    assert!(
        bad_thresholds.validate().is_err(),
        "quality thresholds must match the motion policy"
    );

    let mut missing_transition = Fixture::new().schema4(None);
    missing_transition.envelope["quality"]["transitions"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(
        missing_transition.validate().is_err(),
        "every native adjacent-frame pair must be represented"
    );

    let mut out_of_order = Fixture::new().schema4(None);
    out_of_order.envelope["quality"]["transitions"][0]["after_frame"] = json!(2);
    assert!(
        out_of_order.validate().is_err(),
        "native transition identities must be ordered"
    );

    let inconsistent = Fixture::new().schema4(Some(MotionAmount::Moderate));
    assert!(
        inconsistent.validate().is_err(),
        "quality motion must match the bound request"
    );

    let mut rejected_lighting = Fixture::new().schema4(None);
    rejected_lighting.envelope["quality"]["transitions"][0]["mean_luma_shift"] = json!(33.0);
    rejected_lighting.envelope["quality"]["transitions"][0]["mean_absolute_luma_change"] =
        json!(40.0);
    assert!(
        rejected_lighting.validate().is_err(),
        "a report rejected for abrupt lighting cannot admit media"
    );

    let mut legacy_with_quality = Fixture::new().schema4(None);
    legacy_with_quality.envelope["schema_version"] = json!(3);
    legacy_with_quality.envelope["validation_profile"] = json!("deadpan-ffv1-bridge-3");
    assert!(
        legacy_with_quality.validate().is_err(),
        "schema three does not admit schema four quality data"
    );

    let mut legacy_with_null_quality = Fixture::new();
    legacy_with_null_quality.envelope["quality"] = Value::Null;
    assert!(
        legacy_with_null_quality.validate().is_err(),
        "schema three must omit the quality field entirely"
    );
}

#[test]
fn measured_contexts_admit_and_a_foreign_model_space_is_refused() {
    let evidence = Fixture::measured(crate::CANONICAL_BRIDGE_COLOR)
        .validate()
        .unwrap();
    assert_eq!(evidence.sampled_contract().frames, 30);
    let mut wide = crate::CANONICAL_BRIDGE_COLOR;
    wide.primaries = crate::BridgePrimaries::Bt2020;
    let Err(QualificationError::Request(reason)) = Fixture::measured(wide).validate() else {
        panic!("a BT.2020 model space must not admit sRGB masters")
    };
    assert!(
        reason.contains("colour interpretation mismatch"),
        "{reason}"
    );
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
                .unwrap()
                .into();
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
fn common_saved_reader_preserves_bridge_inputs_and_rejects_extension_dispatch() {
    let fixture = Fixture::definition_checked();
    let (bytes, artifact) = fixture.wire();
    let stored = crate::StoredGeneratedProvenance::from_bytes(&bytes, &artifact).unwrap();
    assert_eq!(stored.context_object(), &object(&fixture.context));
    let evidence = stored
        .validate_for(&artifact, &fixture.project, &fixture.context)
        .unwrap();
    let crate::AcceptedGenerationEvidence::Bridge(bridge) = &evidence else {
        panic!("bridge envelope dispatched as extension");
    };
    assert_eq!(evidence.native_contract(), bridge.native_contract());
    assert_eq!(evidence.sampled_contract(), bridge.sampled_contract());
    assert_eq!(evidence.generation_options(), bridge.generation_options());
    let inputs: Vec<_> = evidence.conditioning_inputs().collect();
    let receipt = bridge.conditioning();
    let expected: Vec<_> = [receipt.manifest(), receipt.left(), receipt.right()]
        .into_iter()
        .map(|input| (input.object(), input.declaration().sha256().as_str()))
        .collect();
    assert_eq!(inputs, expected);

    let mut extension = artifact.clone();
    extension.sampling = deadpan_core::ExtensionSamplingMap::new(
        deadpan_core::ExtensionDirection::FromLeft,
        artifact.sampling.project_rate(),
        artifact.sampling.native_rate(),
        FrameDuration::new(9).unwrap(),
        FrameDuration::new(16).unwrap(),
        artifact.sampling.output_frame_count(),
        artifact.sampling.interpolation(),
    )
    .unwrap()
    .into();
    assert_eq!(
        extension.sampling.native_frame_count(),
        artifact.sampling.native_frame_count()
    );
    assert!(crate::StoredGeneratedProvenance::from_bytes(&bytes, &extension).is_err());
    assert!(
        StoredBridgeProvenance::from_bytes(&bytes, &artifact.provenance)
            .unwrap()
            .validate_for(&extension, &fixture.project, &fixture.context)
            .is_err()
    );
    assert!(
        crate::StoredGeneratedProvenance::from_bytes(&bytes, &artifact)
            .unwrap()
            .validate_for(&extension, &fixture.project, &fixture.context)
            .is_err()
    );
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
