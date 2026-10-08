use super::*;
use deadpan_core::{ExactRatio, FrameDuration, NodeId};
use deadpan_jobs::{
    ExtensionCapturePolicy, GenerationCaptureSpec, GenerationInputBinding, GenerationInputSupport,
    GenerationInputs, GenerationPictureIdentity, RelativeGenerationPicture, VideoSpec,
    WorkspaceArtifact, WorkspaceRef,
};
use image::ImageEncoder;
use serde_json::json;
use sha2::Digest;

use crate::{
    BoundaryClock, BoundaryPicture, ExtensionContextPicture, ExtensionContinuityEvidence,
    ExtensionOppositeSeam, ExtensionRegionCapture, RasterRect,
};

const LEFT_REPORT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/model-qualification/evidence/2026-10-07-extension-worker/from_left/workspace/outputs/provenance.json"
));
const RIGHT_REPORT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/model-qualification/evidence/2026-10-07-extension-worker/from_right/workspace/outputs/provenance.json"
));

struct Fixture {
    value: Value,
    binding: ExtensionGenerationBinding,
    declaration: NativeCandidateManifest,
    context: ExtensionContext,
}

impl Fixture {
    /// Use the real adapter's schema-3 field shape and loading claims, replacing
    /// its obsolete context explicitly with a synthetic current contract. This
    /// is a parser/policy fixture, not evidence of a new model execution.
    fn new(direction: ExtensionDirection) -> Self {
        let mut value: Value = serde_json::from_str(match direction {
            ExtensionDirection::FromLeft => LEFT_REPORT,
            ExtensionDirection::FromRight => RIGHT_REPORT,
        })
        .unwrap();
        let mut binding: ExtensionGenerationBinding =
            serde_json::from_value(value["request_binding"].clone()).unwrap();
        let context = synthetic_current_context(&binding);
        let context_bytes = serde_json::to_vec(&context).unwrap();
        binding.input.sha256 = sha(&context_bytes);
        value["request_binding"] = serde_json::to_value(&binding).unwrap();
        value["context"] = serde_json::to_value(&context).unwrap();
        value["test_fixture_notice"] =
            json!("synthetic current context; not a genuine inference report");
        let plan = &binding.plan;
        let dimensions = plan.native_dimensions();
        let declaration = NativeCandidateManifest {
            native: WorkspaceArtifact::new(
                WorkspaceRef::new("outputs/native.mp4").unwrap(),
                serde_json::from_value(value["native_sha256"].clone()).unwrap(),
                value["native_bytes"].as_u64().unwrap(),
            )
            .unwrap(),
            provenance: artifact(
                "outputs/provenance.json",
                &serde_json::to_vec(&value).unwrap(),
            ),
            video: VideoSpec::new(
                FrameDuration::new(i64::from(plan.native_frame_count())).unwrap(),
                plan.native_frame_rate(),
                dimensions.width(),
                dimensions.height(),
            )
            .unwrap(),
            provider: binding.provider.clone(),
        };
        Self {
            value,
            binding,
            declaration,
            context,
        }
    }

    fn validate(&self, value: Value) -> Result<(), QualificationError> {
        validate(value, &self.binding, &self.declaration, &self.context)
    }
}

fn sha(bytes: &[u8]) -> Sha256 {
    Sha256::new(
        sha2::Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
    )
    .unwrap()
}

fn artifact(path: &str, bytes: &[u8]) -> WorkspaceArtifact {
    WorkspaceArtifact::new(
        WorkspaceRef::new(path).unwrap(),
        sha(bytes),
        bytes.len() as u64,
    )
    .unwrap()
}

fn synthetic_current_context(binding: &ExtensionGenerationBinding) -> ExtensionContext {
    let plan = &binding.plan;
    let dimensions = plan.native_dimensions();
    let raster = [dimensions.width(), dimensions.height()];
    let mut black_png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut black_png)
        .write_image(
            &vec![0; (raster[0] * raster[1] * 3) as usize],
            raster[0],
            raster[1],
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    let black = artifact("inputs/black.png", &black_png);
    let project_rate = plan.project_frame_rate();
    let native_rate = plan.native_frame_rate();
    let step = ExactRatio::new(
        i128::from(project_rate.numerator()) * i128::from(native_rate.denominator()),
        i128::from(project_rate.denominator()) * i128::from(native_rate.numerator()),
    )
    .unwrap();
    let anchor_ordinal = match plan.direction() {
        ExtensionDirection::FromLeft => plan.context_frame_count() - 1,
        ExtensionDirection::FromRight => 0,
    };
    let context: Vec<_> = (0..plan.context_frame_count())
        .map(|ordinal| ExtensionContextPicture {
            picture: BoundaryPicture::AuthoredBlack {
                clock: BoundaryClock::Definition {
                    project_id: binding.project_id.clone(),
                    revision_id: binding.revision_id.clone(),
                    definition: NodeId::new("synthetic-sequence").unwrap(),
                    position: ExactRatio::integer(20)
                        .checked_add(
                            step.checked_mul(ExactRatio::integer(i64::from(ordinal)))
                                .unwrap(),
                        )
                        .unwrap(),
                },
            },
            frame: black.clone(),
            content: None,
        })
        .collect();
    let samples: Vec<_> = (0..plan.context_frame_count())
        .map(|ordinal| RelativeGenerationPicture {
            position: step
                .checked_mul(ExactRatio::integer(
                    i64::from(ordinal) - i64::from(anchor_ordinal),
                ))
                .unwrap(),
            picture: GenerationPictureIdentity::AuthoredBlack,
        })
        .collect();
    let terminal = samples.last().unwrap().clone();
    let support = vec![GenerationInputSupport {
        start: samples[0].position,
        end_exclusive: terminal.position,
        first: GenerationPictureIdentity::AuthoredBlack,
        last: GenerationPictureIdentity::AuthoredBlack,
    }];
    let continuity = ExtensionContinuityEvidence::new(
        GenerationInputBinding {
            duration: plan.project_frames(),
            frame_rate: project_rate,
            canvas: raster,
            inputs: GenerationInputs::Extension {
                capture: GenerationCaptureSpec::Extension {
                    direction: plan.direction(),
                    native_rate,
                    context_frames: plan.context_frame_count(),
                    policy: ExtensionCapturePolicy::TemporalContextV1,
                },
                samples,
                opposite: None,
                support,
                terminal,
            },
            region: None,
        },
        vec![],
        vec![None, None],
        artifact("inputs/continuity.bin", b"DPSIG001\0\0\0\0"),
    )
    .unwrap();
    ExtensionContext::new(
        plan.clone(),
        context,
        RasterRect::new(0, 0, raster[0], raster[1]).unwrap(),
        ExtensionOppositeSeam::Absent,
        "synthetic authored black descriptor for provenance unit tests",
        ExtensionRegionCapture::None,
        continuity,
    )
    .unwrap()
}

#[test]
fn actual_adapter_shape_accepts_both_directions_with_an_explicit_current_test_context() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let fixture = Fixture::new(direction);
        fixture.validate(fixture.value.clone()).unwrap();
        assert_eq!(fixture.value["context"]["schema_version"], 2);
        assert!(
            fixture.value["test_fixture_notice"]
                .as_str()
                .unwrap()
                .contains("not a genuine")
        );
    }
}

#[test]
fn historical_genuine_report_cannot_be_admitted_as_current_context() {
    for (direction, original) in [
        (ExtensionDirection::FromLeft, LEFT_REPORT),
        (ExtensionDirection::FromRight, RIGHT_REPORT),
    ] {
        let fixture = Fixture::new(direction);
        let value: Value = serde_json::from_str(original).unwrap();
        assert_eq!(value["schema_version"], 3);
        assert_eq!(value["context"]["schema_version"], 1);
        assert!(fixture.validate(value).is_err());
    }
}

#[test]
fn every_operation_binding_and_native_claim_is_required_and_exact() {
    let fixture = Fixture::new(ExtensionDirection::FromLeft);
    for (pointer, replacement) in [
        ("/schema_version", json!(2)),
        ("/operation", json!("bridge")),
        ("/direction", json!("from_right")),
        (
            "/request_binding/identity/attempt_id",
            json!("other-attempt"),
        ),
        ("/request_binding/project_id", json!("other-project")),
        ("/request_binding/revision_id", json!("other-revision")),
        ("/request_binding/target/hold_id", json!("other-hold")),
        ("/request_binding/input/sha256", json!("f".repeat(64))),
        ("/request_binding/constraints/motion", json!("subtle")),
        ("/request_binding/provider/seed", json!(1)),
        ("/seed", json!(1)),
        ("/pack_id", json!("other-pack")),
        ("/pack_version", json!("other-version")),
        ("/runtime_id", json!("other-runtime")),
        ("/runtime_version", json!("other-runtime-version")),
        ("/native_sha256", json!("f".repeat(64))),
        ("/native_bytes", json!(1)),
        (
            "/context/input_color_interpretation",
            json!("changed host context"),
        ),
        ("/generated_interval", json!([0, 8])),
        ("/model_manifest_sha256", json!("invalid digest")),
    ] {
        let mut value = fixture.value.clone();
        *value.pointer_mut(pointer).expect(pointer) = replacement;
        assert!(fixture.validate(value).is_err(), "{pointer}");
    }
    for key in [
        "schema_version",
        "operation",
        "direction",
        "request_binding",
        "context",
        "generated_interval",
        "timing",
        "pack_id",
        "pack_version",
        "runtime_id",
        "runtime_version",
        "model_manifest_sha256",
        "seed",
        "native_sha256",
        "native_bytes",
        "runtime_commit",
        "pack_revision",
        "gemma_revision",
        "adapter_sources_sha256",
        "loaded_ltx_sources_sha256",
        "verified_assets",
        "prompt_version",
        "prompt",
        "configuration",
        "model_color_interpretation",
        "temporal_interpolation",
        "conditioning_preprocessing",
    ] {
        let mut value = fixture.value.clone();
        value.as_object_mut().unwrap().remove(key);
        assert!(fixture.validate(value).is_err(), "missing {key}");
    }
}

#[test]
fn all_seven_clocks_require_exact_fractional_values_without_context_conflation() {
    let fixture = Fixture::new(ExtensionDirection::FromLeft);
    for key in [
        "requested_duration",
        "generated_duration",
        "native_movie_duration",
        "context_duration",
        "context_anchor_span",
        "speed_conversion",
        "retime_deviation",
    ] {
        let mut wrong = fixture.value.clone();
        wrong["timing"][key]["numerator"] = json!("1234567");
        assert!(fixture.validate(wrong).is_err(), "{key}");
        let mut missing = fixture.value.clone();
        missing["timing"].as_object_mut().unwrap().remove(key);
        assert!(fixture.validate(missing).is_err(), "missing {key}");
        for denominator in [json!(0), json!("0"), json!("-1"), json!(null)] {
            let mut wrong = fixture.value.clone();
            wrong["timing"][key]["denominator"] = denominator;
            assert!(fixture.validate(wrong).is_err(), "bad fraction {key}");
        }
    }
    let mut context_as_generated = fixture.value.clone();
    context_as_generated["timing"]["generated_duration"] =
        fixture.value["timing"]["native_movie_duration"].clone();
    assert!(fixture.validate(context_as_generated).is_err());
    let mut extra = fixture.value.clone();
    extra["timing"]["unknown"] = json!({"numerator":"0","denominator":"1"});
    assert!(fixture.validate(extra).is_err());
}

#[test]
fn signed_retime_deviation_is_preserved_when_the_requested_interval_is_longer() {
    let mut fixture = Fixture::new(ExtensionDirection::FromLeft);
    let mut plan = serde_json::to_value(&fixture.binding.plan).unwrap();
    plan["sampling"]["output_frame_count"] = json!(10);
    fixture.binding.plan = serde_json::from_value(plan).unwrap();
    let video = fixture.binding.constraints.video;
    fixture.binding.constraints.video = VideoSpec::new(
        FrameDuration::new(10).unwrap(),
        video.frame_rate(),
        video.width(),
        video.height(),
    )
    .unwrap();
    fixture.context = synthetic_current_context(&fixture.binding);
    fixture.binding.input.sha256 = sha(&serde_json::to_vec(&fixture.context).unwrap());
    fixture.value["request_binding"] = serde_json::to_value(&fixture.binding).unwrap();
    fixture.value["context"] = serde_json::to_value(&fixture.context).unwrap();
    // 10 / (30000/1001) = 1001/3000; 8/24 - 1001/3000 = -1/3000.
    fixture.value["timing"]["requested_duration"] =
        json!({"numerator":"1001","denominator":"3000"});
    fixture.value["timing"]["speed_conversion"] = json!({"numerator":"1000","denominator":"1001"});
    fixture.value["timing"]["retime_deviation"] = json!({"numerator":"-1","denominator":"3000"});
    fixture.validate(fixture.value.clone()).unwrap();
    fixture.value["timing"]["retime_deviation"]["numerator"] = json!("1");
    assert!(fixture.validate(fixture.value.clone()).is_err());
}

#[test]
fn native_declaration_and_expected_context_cannot_disagree_with_the_binding() {
    let fixture = Fixture::new(ExtensionDirection::FromLeft);
    let mut declaration = fixture.declaration.clone();
    declaration.provider.seed += 1;
    assert!(
        validate(
            fixture.value.clone(),
            &fixture.binding,
            &declaration,
            &fixture.context
        )
        .is_err()
    );
    declaration = fixture.declaration.clone();
    declaration.video = VideoSpec::new(
        FrameDuration::new(16).unwrap(),
        fixture.binding.plan.native_frame_rate(),
        768,
        320,
    )
    .unwrap();
    assert!(
        validate(
            fixture.value.clone(),
            &fixture.binding,
            &declaration,
            &fixture.context
        )
        .is_err()
    );
    let other = Fixture::new(ExtensionDirection::FromRight);
    assert!(
        validate(
            fixture.value.clone(),
            &fixture.binding,
            &fixture.declaration,
            &other.context
        )
        .is_err()
    );
}

#[test]
fn common_source_and_model_claim_limits_reject_malformed_and_oversized_receipts() {
    let fixture = Fixture::new(ExtensionDirection::FromLeft);
    for key in ["runtime_commit", "pack_revision", "gemma_revision"] {
        for revision in ["a".repeat(39), "A".repeat(40), "g".repeat(40)] {
            let mut value = fixture.value.clone();
            value[key] = json!(revision);
            assert!(fixture.validate(value).is_err(), "{key}");
        }
    }
    for key in ["adapter_sources_sha256", "loaded_ltx_sources_sha256"] {
        for receipts in [
            json!({}),
            json!({"../escape.py": "a".repeat(64)}),
            json!({"/absolute.py": "a".repeat(64)}),
            json!({"source.py": "bad digest"}),
            Value::Object(
                (0..4097)
                    .map(|index| (format!("source-{index}.py"), json!("a".repeat(64))))
                    .collect(),
            ),
        ] {
            let mut value = fixture.value.clone();
            value[key] = receipts;
            assert!(fixture.validate(value).is_err(), "{key}");
        }
    }
    let claim = fixture.value["verified_assets"][0].clone();
    for assets in [
        json!([]),
        json!([claim.clone(), claim.clone()]),
        json!(vec![claim; 2049]),
    ] {
        let mut value = fixture.value.clone();
        value["verified_assets"] = assets;
        assert!(fixture.validate(value).is_err());
    }
    for (pointer, replacement) in [
        ("/verified_assets/0/size", json!(0)),
        ("/verified_assets/0/repository", json!("../escape")),
        ("/verified_assets/0/path", json!("/absolute")),
        ("/verified_assets/0/sha256", json!("not a digest")),
    ] {
        let mut value = fixture.value.clone();
        *value.pointer_mut(pointer).unwrap() = replacement;
        assert!(fixture.validate(value).is_err(), "{pointer}");
    }
}

#[test]
fn shared_receipt_limits_admit_their_boundaries_and_reject_long_paths() {
    let fixture = Fixture::new(ExtensionDirection::FromLeft);
    let mut value = fixture.value.clone();
    value["adapter_sources_sha256"] = Value::Object(
        (0..4096)
            .map(|index| (format!("source-{index}.py"), json!("a".repeat(64))))
            .collect(),
    );
    value["loaded_ltx_sources_sha256"] = json!({"x".repeat(1024): "b".repeat(64)});
    value["verified_assets"] = Value::Array(
        (0..2048)
            .map(|index| {
                json!({
                    "repository": "model/repository", "path": format!("weight-{index}.safetensors"),
                    "size": 1, "sha256": "c".repeat(64),
                })
            })
            .collect(),
    );
    fixture.validate(value).unwrap();
    for key in ["adapter_sources_sha256", "loaded_ltx_sources_sha256"] {
        let mut value = fixture.value.clone();
        value[key] = json!({"x".repeat(1025): "b".repeat(64)});
        assert!(fixture.validate(value).is_err(), "{key}");
    }
}

#[test]
fn common_text_and_configuration_bounds_are_preserved_and_diagnostics_are_allowed() {
    let fixture = Fixture::new(ExtensionDirection::FromLeft);
    for (key, bound) in [
        ("prompt_version", 256),
        ("prompt", 64 * 1024),
        ("model_color_interpretation", 4096),
        ("temporal_interpolation", 4096),
        ("conditioning_preprocessing", 4096),
    ] {
        for text in [" ".into(), "nul\0value".into(), "x".repeat(bound + 1)] {
            let mut value = fixture.value.clone();
            value[key] = json!(text);
            assert!(fixture.validate(value).is_err(), "{key}");
        }
        let mut at_bound = fixture.value.clone();
        at_bound[key] = json!("x".repeat(bound));
        fixture.validate(at_bound).unwrap();
    }
    for configuration in [
        json!({}),
        Value::Object(
            (0..257)
                .map(|index| (format!("field-{index}"), json!(true)))
                .collect(),
        ),
    ] {
        let mut value = fixture.value.clone();
        value["configuration"] = configuration;
        assert!(fixture.validate(value).is_err());
    }
    let mut diagnostic = fixture.value.clone();
    diagnostic["future_diagnostic"] = json!({"worker_claim":"retained only","samples":[1,2,3]});
    fixture.validate(diagnostic).unwrap();
}
