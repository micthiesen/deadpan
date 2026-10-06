use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use deadpan_cli::picture::{
    PreparedPicture, PreparedProjectPicture, ProjectPictureError, ProjectPictureSession,
};
use deadpan_core::{
    AssetId, BeatNode, CapturedCanvas, CapturedFit, CapturedFraming, ColorPolicy, Command,
    CommandRequest, ExactRatio, FrameDuration, FrameRate, FramingPose, GeneratedArtifact,
    GeneratedObjectRef, HoldAudio, HoldRecipe, HoldVideo, NodeId, NodeKind, PresentationBasis,
    ProjectDocument, ProjectFrame, ProjectId, RevisionId, SourceFrameId, SourceTimeBase, Subtree,
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
    BridgeQualification, ConditioningLimits, GenerationBinding, QualificationError,
    QualificationLimits, RetainedConditioning, SelectedBridgeProvider, capture_bridge_conditioning,
    qualify_bridge,
};
use deadpan_store::generated_media::{GeneratedMediaError, GeneratedMediaLimits};
use deadpan_store::generation::{
    ContextObservation, GenerationRequestInput, RelevanceObservation, RelevancePlan,
};
use deadpan_store::generation_acceptance::GenerationAcceptance;
use deadpan_store::generation_attempts::{
    BeginGenerationAttempt, BundleAdmissionEvidence, BundleInputObjects, BundleValidationReceipt,
    ValidatorIdentity,
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
    manifest: WorkspaceArtifact,
}

impl Fixture {
    fn conditioning(&self) -> RetainedConditioning {
        capture_bridge_conditioning(
            &self.workspace,
            &self.request,
            &self.manifest,
            &WorkspaceRef::new("inputs").unwrap(),
            ConditioningLimits {
                maximum_manifest_bytes: 64 * 1024,
                maximum_frame_bytes: 1024,
                timeout_ms: 30000,
            },
            &AtomicBool::new(false),
        )
        .unwrap()
    }

    fn qualification<'a>(
        &'a self,
        selected_provider: &'a SelectedBridgeProvider,
    ) -> BridgeQualification<'a> {
        BridgeQualification {
            request: &self.request,
            declaration: &self.declaration,
            selected_provider,
            conditioning: self.conditioning(),
        }
    }

    fn selected_provider(&self) -> SelectedBridgeProvider {
        SelectedBridgeProvider::new(self.declaration.provider.clone(), capability())
    }

    /// The retained version-1 grammar: bundles captured before measured
    /// boundary evidence must still qualify and admit.
    fn new() -> Self {
        Self::with_context(|left, right| {
            json!({
                "schema_version":1, "model_color":"srgb", "plan":plan(),
                "left":left, "right":right,
                "input_color_interpretation":"fixture RGB"
            })
        })
    }

    /// A version-2 context whose model declares `model_color_space`.
    fn measured(model_color_space: serde_json::Value) -> Self {
        Self::with_context(|left, right| {
            json!({
                "schema_version":2, "model_color_space":model_color_space, "plan":plan(),
                "left":left, "right":right,
                "input_color_interpretation":"fixture RGB",
                "boundaries":{
                    "left":{"original":{
                        "project_frame":9, "asset":"original", "qualification":"d".repeat(64),
                        "picture":{
                            "source_frame":9,
                            "pts":{"ticks":9009, "time_base":{"numerator":1, "denominator":30000}},
                            "stream":{
                                "codec":"h264", "pixel_format":"yuv420p", "width":320,
                                "height":180, "sample_aspect":[1,1], "rotation_quarter_turns":0,
                                "decoded_sample_bits":8,
                                "color":{"transfer":"bt709", "primaries":"bt709",
                                    "matrix":"bt709", "range":"limited"}
                            },
                            "model_input":"rec709_codes_as_srgb"
                        }
                    }},
                    "right":{"authored_black":{"project_frame":40}}
                }
            })
        })
    }

    fn with_context(
        context: impl FnOnce(WorkspaceArtifact, WorkspaceArtifact) -> serde_json::Value,
    ) -> Self {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("outputs")).unwrap();
        fs::create_dir(directory.path().join("inputs")).unwrap();
        let workspace = ArtifactWorkspace::open(directory.path()).unwrap();
        // Opaque prepared-byte fixtures exercise retention, not PNG validation.
        let left = b"prepared left image";
        let right = b"prepared right image";
        fs::write(directory.path().join("inputs/left.png"), left).unwrap();
        fs::write(directory.path().join("inputs/right.png"), right).unwrap();
        let context = context(
            declared("inputs/left.png", left),
            declared("inputs/right.png", right),
        );
        let context_bytes = serde_json::to_vec_pretty(&context).unwrap();
        fs::write(directory.path().join("inputs/context.json"), &context_bytes).unwrap();
        let manifest = declared("inputs/context.json", &context_bytes);
        let mut request = request();
        let HostMessage::GenerateBridge { input, .. } = &mut request else {
            unreachable!()
        };
        input.sha256 = manifest.sha256().clone();
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
            "context": context,
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
            manifest,
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
    let provider = fixture.selected_provider();
    let inputs = fixture.qualification(&provider);
    // Qualification must use the host snapshots even if the worker's input
    // directory has disappeared after launch.
    fs::remove_dir_all(fixture.directory.path().join("inputs")).unwrap();
    let bundle = qualify_bridge(
        Path::new(env!("CARGO_BIN_EXE_deadpan-media-worker")),
        &fixture.workspace,
        inputs,
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
    let retained_receipt = bundle.conditioning().receipt().clone();
    let (_, _, mut provenance, conditioning) = bundle.into_parts();
    let (_, mut left, mut right) = conditioning.into_parts();
    let mut left_bytes = Vec::new();
    let mut right_bytes = Vec::new();
    left.read_to_end(&mut left_bytes).unwrap();
    right.read_to_end(&mut right_bytes).unwrap();
    assert_eq!(left_bytes, b"prepared left image");
    assert_eq!(right_bytes, b"prepared right image");
    let mut bytes = Vec::new();
    provenance.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes.len() as u64, provenance_ref.byte_length());
    assert_eq!(
        blake3::hash(&bytes).to_hex().as_str(),
        provenance_ref.content().digest()
    );
    let envelope: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(envelope["schema_version"], 3);
    assert_eq!(
        envelope["conditioning"],
        serde_json::to_value(retained_receipt).unwrap()
    );
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
fn measured_context_qualifies_and_a_foreign_model_space_fails_before_any_codec() {
    let canonical = json!({"transfer":"srgb", "primaries":"bt709", "matrix":"rgb", "range":"full"});
    let fixture = Fixture::measured(canonical);
    let provider = fixture.selected_provider();
    let bundle = qualify_bridge(
        Path::new(env!("CARGO_BIN_EXE_deadpan-media-worker")),
        &fixture.workspace,
        fixture.qualification(&provider),
        limits(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let boundaries = bundle.conditioning().context().boundaries().unwrap();
    assert_eq!(boundaries.left.project_frame(), 9);
    assert_eq!(bundle.conditioning().context().schema_version(), 2);

    // A model declared to emit wide-gamut PQ would have its pictures silently
    // reinterpreted as the canonical sRGB masters; qualification refuses it
    // before starting a codec.
    let foreign = Fixture::measured(
        json!({"transfer":"pq", "primaries":"bt2020", "matrix":"rgb", "range":"full"}),
    );
    let provider = foreign.selected_provider();
    let Err(QualificationError::Request(reason)) = qualify_bridge(
        Path::new("/missing/codec"),
        &foreign.workspace,
        foreign.qualification(&provider),
        limits(),
        &AtomicBool::new(false),
    ) else {
        panic!("a foreign model colour space must fail qualification")
    };
    assert!(
        reason.contains("colour interpretation mismatch"),
        "{reason}"
    );
    assert!(reason.contains("pq transfer, bt2020 primaries"), "{reason}");
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
    let mut wrong_left = original.clone();
    wrong_left["context"]["left"] = original["context"]["right"].clone();
    let mut wrong_color = original.clone();
    wrong_color["context"]["input_color_interpretation"] = json!("Different source assumptions");
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
        serde_json::to_vec(&wrong_left).unwrap(),
        serde_json::to_vec(&wrong_color).unwrap(),
        duplicate,
    ] {
        fixture.replace_provenance(bytes);
        let error = qualify_bridge(
            Path::new("/missing/codec"),
            &fixture.workspace,
            fixture.qualification(&fixture.selected_provider()),
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
            fixture.qualification(&fixture.selected_provider()),
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
            fixture.qualification(&fixture.selected_provider()),
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
            fixture.qualification(&fixture.selected_provider()),
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
            fixture.qualification(&fixture.selected_provider()),
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
                fixture.qualification(&selected),
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
        BridgeQualification { request: &request, ..fixture.qualification(&fixture.selected_provider()) }, limits(), &AtomicBool::new(false)),
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

fn generated_picture_context() -> CapturedFraming {
    CapturedFraming::new(vec![CapturedCanvas {
        width: 960,
        height: 540,
        fit: CapturedFit::Fill,
        layers: vec![
            Some(FramingPose {
                scale: ExactRatio::integer(2),
                ..Default::default()
            }),
            None,
        ],
    }])
    .unwrap()
}

fn expected_generated_rgba(ordinal: u32) -> [u8; 32] {
    assert!(ordinal < 30);
    // The retained fixture has 25 native frames. Its 30 sampled interior
    // positions are (ordinal + 1) * 24 / 31. Compute the documented RGB
    // fixture pattern and encoded-RGB half-up interpolation independently of
    // the production sampling map, converter and picture decoder.
    let position = (ordinal + 1) * 24;
    let lower = position / 31;
    let remainder = position % 31;
    let mut expected = [0; 32];
    for y in 0..2_u32 {
        for x in 0..4_u32 {
            let native_pixel = |frame| {
                [
                    (17 * frame + 31 * x + 7 * y + 3) % 256,
                    (29 * frame + 5 * x + 47 * y + 11) % 256,
                    (43 * frame + 13 * x + 19 * y + 23) % 256,
                ]
            };
            let left = native_pixel(lower);
            let right = native_pixel(lower + 1);
            let offset = usize::try_from((y * 4 + x) * 4).unwrap();
            for channel in 0..3 {
                let weighted = left[channel] * (31 - remainder) + right[channel] * remainder;
                expected[offset + channel] = u8::try_from((weighted + 15) / 31).unwrap();
            }
            expected[offset + 3] = 255;
        }
    }
    expected
}

fn assert_generated_picture(
    picture: &PreparedProjectPicture,
    revision: &RevisionId,
    expected_artifact: &GeneratedArtifact,
    ordinal: u32,
) {
    assert_eq!(picture.project_id, ProjectId::new("project").unwrap());
    assert_eq!(&picture.revision_id, revision);
    assert_eq!(picture.project_frame, ProjectFrame(i64::from(ordinal)));
    assert_eq!(picture.canvas, [1920, 1080]);
    assert_eq!(picture.frame_rate, FrameRate::new(30_000, 1_001).unwrap());
    assert_eq!(
        picture.picture_context.as_deref(),
        Some(&generated_picture_context())
    );
    let PreparedPicture::Generated {
        artifact,
        id,
        frame,
    } = &picture.picture
    else {
        panic!("accepted Generated Hold must prepare its sampled master")
    };
    assert_eq!(artifact.as_ref(), expected_artifact);
    assert_eq!(*id, SourceFrameId(u64::from(ordinal)));
    assert_eq!((frame.metadata().width, frame.metadata().height), (4, 2));
    assert_eq!(frame.metadata().row_stride_bytes, 16);
    assert_eq!(
        frame.metadata().pts.time_base,
        SourceTimeBase::new(1, 1000).unwrap()
    );
    assert_eq!(
        frame.metadata().pts.ticks,
        (i64::from(ordinal) * 1_001_000 + 15_000) / 30_000
    );
    assert_eq!(frame.bytes(), expected_generated_rgba(ordinal));
}

fn prepare_generated_picture(
    session: &mut ProjectPictureSession,
    artifact: &GeneratedArtifact,
    ordinal: u32,
) -> PreparedProjectPicture {
    let picture = session
        .prepare(ProjectFrame(i64::from(ordinal)), &AtomicBool::new(false))
        .unwrap();
    assert_generated_picture(&picture, session.revision(), artifact, ordinal);
    picture
}

fn open_pictures(package: &Path, revision: &RevisionId) -> ProjectPictureSession {
    ProjectPictureSession::open_revision(package, revision, None, &AtomicBool::new(false)).unwrap()
}

fn retain_generated_picture_fixture(
    package: &Path,
    reverted_revision: &RevisionId,
    artifact: &GeneratedArtifact,
) {
    let Some(destination) = std::env::var_os("DEADPAN_GENERATED_PICTURE_FIXTURE_ROOT") else {
        return;
    };
    let destination = PathBuf::from(destination);
    assert!(
        destination.is_absolute(),
        "fixture scratch destination must be absolute"
    );
    let parent = destination.parent().unwrap().canonicalize().unwrap();
    let temporary = std::env::temp_dir().canonicalize().unwrap();
    let system_temporary = Path::new("/tmp").canonicalize().unwrap();
    assert!(
        parent.starts_with(&temporary) || parent.starts_with(&system_temporary),
        "fixture retention is restricted to temporary scratch directories"
    );
    let destination = parent.join(destination.file_name().unwrap());
    fs::create_dir(&destination).expect("fixture scratch destination must be new");

    // Restore through durable history. Never fabricate an accepted document or
    // copy a live SQLite main file without its WAL. All other sessions have
    // closed before this helper; close these final readers before relocation.
    let mut store = ProjectStore::open(package, AccessMode::ReadWrite).unwrap();
    let revision = RevisionId::new("ui-generated-ready").unwrap();
    let relevance = fixture_relevance(&store, reverted_revision, &revision);
    store
        .undo_reconciled(reverted_revision, revision.clone(), &relevance)
        .unwrap();
    let document = store.snapshot().unwrap();
    let mut pictures = open_pictures(package, &revision);
    assert_eq!(pictures.range().end(), ProjectFrame(30));
    for ordinal in 0..30 {
        prepare_generated_picture(&mut pictures, artifact, ordinal);
    }
    drop(pictures);
    drop(store);

    let retained_package = destination.join("accepted.deadpan");
    fs::rename(package, &retained_package).expect("move the closed qualified fixture package");
    let expectations = json!({
        "schema_version": 1,
        "fixture": "rgb25_24 sampled to 30 frames at 30000/1001",
        "project_id": document.project_id(),
        "revision_id": revision,
        "artifact": artifact,
        "picture_context": generated_picture_context(),
        "frames": (0..30_u32).map(|ordinal| json!({
            "ordinal": ordinal,
            "pts": (i64::from(ordinal) * 1_001_000 + 15_000) / 30_000,
            "rgba": expected_generated_rgba(ordinal),
        })).collect::<Vec<_>>(),
    });
    fs::write(
        destination.join("generated-picture-fixture.json"),
        serde_json::to_vec_pretty(&expectations).unwrap(),
    )
    .unwrap();
    println!(
        "Retained accepted Generated picture fixture: {}",
        retained_package.display()
    );
}

fn assert_cold_generated_dependencies(
    package: &Path,
    revision: &RevisionId,
    artifact: &GeneratedArtifact,
    retained: &mut ProjectPictureSession,
    objects: &[GeneratedObjectRef],
) {
    let scratch = tempfile::tempdir().unwrap();
    for (index, object) in objects.iter().enumerate() {
        let path = package
            .join("Media/Generated")
            .join(format!("blake3-{}", object.content().digest()));
        let original_bytes = fs::read(&path).unwrap();
        let saved = scratch.path().join(format!("dependency-{index}"));
        fs::rename(&path, &saved).unwrap();
        let mut cold = open_pictures(package, revision);
        let error = cold
            .prepare(ProjectFrame(0), &AtomicBool::new(false))
            .expect_err("cold admission must require every retained object");
        assert!(
            matches!(
                &error,
                ProjectPictureError::GeneratedObject(GeneratedMediaError::MissingObject(content))
                    if content == object.content()
            ),
            "missing dependency {index}: {error:?}"
        );
        prepare_generated_picture(retained, artifact, 29);
        drop(cold);
        fs::rename(&saved, &path).unwrap();

        let mut corrupted = original_bytes.clone();
        corrupted[0] ^= 1;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(&path, corrupted).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
        let mut cold = open_pictures(package, revision);
        let error = cold
            .prepare(ProjectFrame(0), &AtomicBool::new(false))
            .expect_err("cold admission must rehash every retained object");
        assert!(
            matches!(
                &error,
                ProjectPictureError::GeneratedObject(GeneratedMediaError::HashMismatch {
                    expected, ..
                }) if expected == object.content()
            ),
            "corrupt dependency {index}: {error:?}"
        );
        prepare_generated_picture(retained, artifact, 0);
        drop(cold);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(&path, original_bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
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
                                picture_context: Some(generated_picture_context()),
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
    )
    .unwrap();
    edit.forward.apply(&document).unwrap()
}

#[test]
fn real_bundle_acceptance_is_explicit_durable_and_reversible_after_relocation() {
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
        fixture.qualification(&fixture.selected_provider()),
        limits(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let native_ref = bundle.native().object().clone();
    let sampled_ref = bundle.sampled().object().clone();
    let provenance_ref = bundle.provenance().object().clone();
    let native_span = bundle.native_span();
    let sampled_span = bundle.sampled_span();
    let inputs = bundle.conditioning().receipt();
    let admission = BundleAdmissionEvidence::new(
        native_span,
        sampled_span,
        BundleInputObjects::new(
            binding.input.sha256.clone(),
            inputs.manifest().object().clone(),
            inputs.left().object().clone(),
            inputs.right().object().clone(),
        )
        .unwrap(),
    )
    .unwrap();
    let receipt = BundleValidationReceipt::new(
        &fixture.declaration,
        native_ref.clone(),
        sampled_ref.clone(),
        provenance_ref.clone(),
        binding.constraints.video.clone(),
        binding.plan.clone(),
        ValidatorIdentity::new("native-ffv1", "bridge-3").unwrap(),
    )
    .unwrap()
    .with_admission(admission)
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
    let (mut native, mut sampled, mut provenance, conditioning) = bundle.into_parts();
    let (manifest, left, right) = conditioning.into_parts();
    let mut retained_refs = Vec::new();
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
    for mut input in [manifest, left, right] {
        // Each dependency is required; no incomplete bundle may become Ready.
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
        let object = input.object().clone();
        store
            .promote_generated_object(&mut input, &object, budget)
            .unwrap();
        retained_refs.push(object);
    }
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
    let accept = GenerationAcceptance {
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new("accepted").unwrap(),
        identity: binding.identity.clone(),
        expected_receipt: receipt.clone(),
        sampled_asset: AssetId::new("sampled").unwrap(),
        native_asset: AssetId::new("native").unwrap(),
    };
    let preview = store
        .preview_generation_acceptance(&accept, budget)
        .unwrap();
    let expected_accepted = preview.forward.apply(&document).unwrap();
    assert_eq!(store.snapshot().unwrap(), document); // Preview never commits.
    let relevance = fixture_relevance(&store, document.revision_id(), &accept.new_revision);
    store
        .accept_generation_bundle(&accept, &relevance, budget)
        .unwrap();
    let accepted = store.snapshot().unwrap();
    assert_eq!(accepted, expected_accepted);
    let NodeKind::Hold { recipe } = &accepted.nodes()[&binding.target.hold_id].kind else {
        unreachable!()
    };
    let HoldVideo::Generated {
        accepted: generation,
    } = &recipe.video
    else {
        panic!("explicit accept did not change provider")
    };
    assert_eq!(generation.artifact.sampled_object, sampled_ref);
    assert_eq!(generation.artifact.native_object, native_ref);
    assert_eq!(generation.artifact.provenance, provenance_ref);
    let artifact = generation.artifact.clone();
    assert_eq!(
        accepted.assets()[&accept.native_asset].video,
        Some(native_span)
    );
    assert_eq!(
        accepted.assets()[&accept.sampled_asset].video,
        Some(sampled_span)
    );
    // Drop the converter's private masters before reading through the project.
    // Accepted playback must depend only on the retained package and recipe.
    drop((native, sampled, provenance));
    let mut pictures = open_pictures(&package, accepted.revision_id());
    let captured = prepare_generated_picture(&mut pictures, &artifact, 7);
    for ordinal in 0..30 {
        prepare_generated_picture(&mut pictures, &artifact, ordinal);
    }
    assert!(matches!(
        pictures.prepare(ProjectFrame(30), &AtomicBool::new(false)),
        Err(ProjectPictureError::FrameOutOfRange { .. })
    ));
    drop(pictures);
    drop(store);
    let relocated = tempfile::tempdir().unwrap();
    let relocated_package = relocated.path().join("retained.deadpan");
    fs::rename(&package, &relocated_package).unwrap();
    fs::remove_dir_all(fixture.directory.path().join("inputs")).unwrap();
    fs::remove_dir_all(fixture.directory.path().join("outputs")).unwrap();
    drop(fixture);
    let mut reopened = ProjectStore::open(&relocated_package, AccessMode::ReadWrite).unwrap();
    let selected = reopened
        .selected_generation_bundle(&binding.identity.request_id)
        .unwrap()
        .unwrap();
    assert_eq!(selected.identity, binding.identity);
    assert_eq!(selected.receipt, receipt);
    assert_eq!(reopened.snapshot().unwrap(), accepted);
    let mut historical = open_pictures(&relocated_package, accepted.revision_id());
    for ordinal in [0, 7, 29] {
        prepare_generated_picture(&mut historical, &artifact, ordinal);
    }
    let undo_revision = RevisionId::new("undo-accept").unwrap();
    let relevance = fixture_relevance(&reopened, accepted.revision_id(), &undo_revision);
    reopened
        .undo_reconciled(accepted.revision_id(), undo_revision.clone(), &relevance)
        .unwrap();
    assert!(reopened.snapshot().unwrap().assets().is_empty());
    let mut undone = open_pictures(&relocated_package, &undo_revision);
    assert!(matches!(
        undone
            .prepare(ProjectFrame(7), &AtomicBool::new(false))
            .unwrap()
            .picture,
        PreparedPicture::Background
    ));
    drop(undone);
    prepare_generated_picture(&mut historical, &artifact, 7);
    let redo_revision = RevisionId::new("redo-accept").unwrap();
    let relevance = fixture_relevance(&reopened, &undo_revision, &redo_revision);
    reopened
        .redo_reconciled(&undo_revision, redo_revision.clone(), &relevance)
        .unwrap();
    assert_eq!(reopened.snapshot().unwrap().assets(), accepted.assets());
    let mut redone = open_pictures(&relocated_package, &redo_revision);
    prepare_generated_picture(&mut redone, &artifact, 29);
    drop(redone);

    let shorter_revision = RevisionId::new("shorter-accepted-prefix").unwrap();
    let relevance = fixture_relevance(&reopened, &redo_revision, &shorter_revision);
    reopened
        .commit_reconciled(
            &CommandRequest {
                project_id: binding.project_id.clone(),
                expected_revision: redo_revision,
                new_revision: shorter_revision.clone(),
                command: Command::SetHoldDuration {
                    node: binding.target.hold_id.clone(),
                    duration: FrameDuration::new(12).unwrap(),
                },
            },
            &relevance,
        )
        .unwrap();
    assert!(reopened.current_generation_requests().unwrap().is_empty());
    let mut shortened = open_pictures(&relocated_package, &shorter_revision);
    assert_eq!(shortened.range().end(), ProjectFrame(12));
    for ordinal in 0..12 {
        prepare_generated_picture(&mut shortened, &artifact, ordinal);
    }
    assert!(matches!(
        shortened.prepare(ProjectFrame(12), &AtomicBool::new(false)),
        Err(ProjectPictureError::FrameOutOfRange { .. })
    ));
    drop(shortened);
    prepare_generated_picture(&mut historical, &artifact, 29);

    let extended_revision = RevisionId::new("restored-accepted-prefix").unwrap();
    let relevance = fixture_relevance(&reopened, &shorter_revision, &extended_revision);
    reopened
        .commit_reconciled(
            &CommandRequest {
                project_id: binding.project_id.clone(),
                expected_revision: shorter_revision,
                new_revision: extended_revision.clone(),
                command: Command::SetHoldDuration {
                    node: binding.target.hold_id.clone(),
                    duration: FrameDuration::new(30).unwrap(),
                },
            },
            &relevance,
        )
        .unwrap();
    let mut extended = open_pictures(&relocated_package, &extended_revision);
    for ordinal in [0, 11, 12, 29] {
        prepare_generated_picture(&mut extended, &artifact, ordinal);
    }
    drop(extended);
    let reverted_revision = RevisionId::new("reverted").unwrap();
    let relevance = fixture_relevance(&reopened, &extended_revision, &reverted_revision);
    reopened
        .commit_reconciled(
            &CommandRequest {
                project_id: binding.project_id.clone(),
                expected_revision: extended_revision,
                new_revision: reverted_revision.clone(),
                command: Command::RevertGeneratedHold {
                    node: binding.target.hold_id.clone(),
                },
            },
            &relevance,
        )
        .unwrap();
    let reverted = reopened.snapshot().unwrap();
    let NodeKind::Hold { recipe } = &reverted.nodes()[&binding.target.hold_id].kind else {
        unreachable!()
    };
    assert_eq!(recipe.video, HoldVideo::Background);
    let mut fallback = open_pictures(&relocated_package, &reverted_revision);
    let fallback_picture = fallback
        .prepare(ProjectFrame(7), &AtomicBool::new(false))
        .unwrap();
    assert!(matches!(
        &fallback_picture.picture,
        PreparedPicture::Background
    ));
    assert_eq!(
        fallback_picture.picture_context.as_deref(),
        Some(&generated_picture_context())
    );
    drop(fallback);
    prepare_generated_picture(&mut historical, &artifact, 7);
    assert_generated_picture(&captured, accepted.revision_id(), &artifact, 7);
    // Cold historical admission also succeeds after the request became stale
    // and the current Hold returned to its deterministic fallback.
    let mut cold_history = open_pictures(&relocated_package, accepted.revision_id());
    prepare_generated_picture(&mut cold_history, &artifact, 29);
    drop(cold_history);
    for object in [&native_ref, &sampled_ref, &provenance_ref]
        .into_iter()
        .chain(retained_refs.iter())
    {
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
    let objects = [native_ref, sampled_ref, provenance_ref]
        .into_iter()
        .chain(retained_refs)
        .collect::<Vec<_>>();
    assert_eq!(objects.len(), 6);
    assert_cold_generated_dependencies(
        &relocated_package,
        accepted.revision_id(),
        &artifact,
        &mut historical,
        &objects,
    );
    let mut restored_cold = open_pictures(&relocated_package, accepted.revision_id());
    prepare_generated_picture(&mut restored_cold, &artifact, 29);
    drop(restored_cold);
    drop(reopened);
    prepare_generated_picture(&mut historical, &artifact, 29);
    drop(historical);
    assert_generated_picture(&captured, accepted.revision_id(), &artifact, 7);
    retain_generated_picture_fixture(&relocated_package, &reverted_revision, &artifact);
}

fn fixture_relevance(store: &ProjectStore, from: &RevisionId, to: &RevisionId) -> RelevancePlan {
    RelevancePlan {
        from_revision: from.clone(),
        to_revision: to.clone(),
        observations: store
            .current_generation_requests()
            .unwrap()
            .into_iter()
            .map(|request| RelevanceObservation {
                request_id: request.request_id,
                after_context: ContextObservation::Resolved(request.binding.context_sha256.clone()),
                binding: request.binding,
            })
            .collect(),
    }
}
