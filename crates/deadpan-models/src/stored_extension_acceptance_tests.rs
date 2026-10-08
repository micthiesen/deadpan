//! Retained evidence admission, using opaque media objects and synthetic policy
//! observations. These tests do not assert media decoding or model execution.

use deadpan_core::{
    AcceptedGeneration, AssetId, BridgeSamplingMap, ExtensionSamplingMap, GeneratedArtifact,
    HoldAudio, HoldFallback, HoldRecipe, HoldVideo,
};
use deadpan_media::protocol::{ConversionReport, VideoContract};
use serde_json::Value;

use super::*;
use crate::stored_extension::{ExtensionEnvelope, PROFILE, SCHEMA};
use crate::{
    AcceptedGenerationEvidence, SelectedExtensionProvider, StoredExtensionProvenance,
    StoredGeneratedProvenance,
};

fn byte_object(bytes: &[u8]) -> GeneratedObjectRef {
    GeneratedObjectRef::new(
        GeneratedContentId::new(blake3::hash(bytes).to_hex().to_string()).unwrap(),
        bytes.len() as u64,
    )
    .unwrap()
}

fn conversion(video: VideoContract, bytes: &[u8], output: char) -> ConversionReport {
    ConversionReport {
        protocol: deadpan_media::protocol::REPORT_PROTOCOL_VERSION,
        video,
        output_bytes: bytes.len() as u64,
        input_rgb_sha256: "a".repeat(64),
        output_rgb_sha256: output.to_string().repeat(64),
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

struct SavedFixture {
    envelope: Value,
    artifact: GeneratedArtifact,
    binding: ExtensionGenerationBinding,
    context: Vec<u8>,
}

impl SavedFixture {
    fn new(direction: ExtensionDirection, opposite: bool) -> Self {
        let (context, mut conditioning, mut binding) = fixture(direction, opposite);
        binding.constraints.instructions =
            Some(deadpan_jobs::HoldInstructions::new("Keep the hands still.").unwrap());
        let context_bytes = serde_json::to_vec(&context).unwrap();
        conditioning.manifest = ConditioningArtifactReceipt::new(
            conditioning.manifest.declaration().clone(),
            byte_object(&context_bytes),
        )
        .unwrap();
        let plan = &binding.plan;
        let native_bytes = b"opaque native movie";
        let sampled_bytes = b"opaque sampled movie";
        let native = byte_object(native_bytes);
        let sampled = byte_object(sampled_bytes);
        let native_validation = conversion(
            crate::extension_motion::native_contract(plan),
            native_bytes,
            'a',
        );
        let sampled_validation = conversion(
            crate::extension_endpoints::sampled_contract(plan).unwrap(),
            sampled_bytes,
            'b',
        );
        let worker_native = declaration("outputs/native.mp4", b"opaque worker movie");
        let interval = plan.sampling_map().generated_interval();
        let worker = serde_json::to_string(&json!({
            "schema_version":3, "operation":"extension", "direction":direction,
            "request_binding":binding, "context":context,
            "generated_interval":[interval.start,interval.end],
            "timing": {
                "requested_duration":plan.requested_duration(), "generated_duration":plan.generated_duration(),
                "native_movie_duration":plan.native_movie_duration(), "context_duration":plan.context_duration(),
                "context_anchor_span":plan.context_anchor_span(), "speed_conversion":plan.speed(), "retime_deviation":plan.retime_deviation(),
            },
            "pack_id":binding.provider.pack_id, "pack_version":binding.provider.pack_version,
            "runtime_id":binding.provider.runtime_id, "runtime_version":binding.provider.runtime_version,
            "model_manifest_sha256":"a".repeat(64), "seed":binding.provider.seed,
            "native_sha256":worker_native.sha256(), "native_bytes":worker_native.byte_length(),
            "runtime_commit":"a".repeat(40), "pack_revision":"b".repeat(40), "gemma_revision":"c".repeat(40),
            "adapter_sources_sha256":{"adapter.py":"a".repeat(64)}, "loaded_ltx_sources_sha256":{"runtime.py":"b".repeat(64)},
            "verified_assets":[{"repository":"fixture/model","path":"weights.bin","size":1,"sha256":"c".repeat(64)}],
            "prompt_version":"fixture-1", "prompt":"Keep the hands still.", "configuration":{"fixture":true},
            "model_color_interpretation":"fixture SDR sRGB", "temporal_interpolation":"fixture exact frame centers",
            "conditioning_preprocessing":"synthetic observations; no model executed",
        })).unwrap();
        let dimensions = plan.native_dimensions();
        let declaration = deadpan_jobs::NativeCandidateManifest {
            native: worker_native,
            provenance: declaration("outputs/provenance.json", worker.as_bytes()),
            video: VideoSpec::new(
                FrameDuration::new(i64::from(plan.native_frame_count())).unwrap(),
                plan.native_frame_rate(),
                dimensions.width(),
                dimensions.height(),
            )
            .unwrap(),
            provider: binding.provider.clone(),
        };
        let selected_provider = SelectedExtensionProvider::new(
            binding.provider.clone(),
            ExtensionCapability::new(
                FrameRate::new(24, 1).unwrap(),
                9,
                FrameCountFormula::new(8, 0, 8, 8).unwrap(),
                DimensionLimits::new(
                    AxisLimits::new(4, 4, 1).unwrap(),
                    AxisLimits::new(2, 2, 1).unwrap(),
                ),
                FrameDuration::new(10).unwrap(),
            )
            .unwrap(),
        );
        let envelope = ExtensionEnvelope {
            schema_version: SCHEMA,
            validation_profile: PROFILE.into(),
            binding: binding.clone(),
            selected_provider,
            declaration,
            pixels: pixel_report(&context, &conditioning, &native, &sampled),
            geometry: geometry_report(&context, &conditioning, &native),
            native_span: native_validation.output_span().unwrap(),
            sampled_span: sampled_validation.output_span().unwrap(),
            native: native.clone(),
            sampled: sampled.clone(),
            native_validation,
            sampled_validation,
            conditioning,
            worker_provenance_utf8: worker,
        };
        let bytes = serde_json::to_vec(&envelope).unwrap();
        let artifact = GeneratedArtifact {
            native_asset: AssetId::new("native").unwrap(),
            sampled_asset: AssetId::new("sampled").unwrap(),
            native_object: native,
            sampled_object: sampled,
            provenance: byte_object(&bytes),
            sampling: plan.sampling_map().clone().into(),
            content_aspect: None,
        };
        Self {
            envelope: serde_json::to_value(envelope).unwrap(),
            artifact,
            binding,
            context: context_bytes,
        }
    }

    fn wire(&self) -> (Vec<u8>, GeneratedArtifact) {
        let bytes = serde_json::to_vec(&self.envelope).unwrap();
        let mut artifact = self.artifact.clone();
        artifact.provenance = byte_object(&bytes);
        (bytes, artifact)
    }

    fn validate(&self) -> Result<AcceptedGenerationEvidence, QualificationError> {
        let (bytes, artifact) = self.wire();
        StoredGeneratedProvenance::from_bytes(&bytes, &artifact)?.validate_for(
            &artifact,
            &self.binding.project_id,
            &self.context,
        )
    }
}

#[test]
fn accepted_extensions_preserve_original_controls_and_all_input_identities() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        for opposite in [false, true] {
            let fixture = SavedFixture::new(direction, opposite);
            let evidence = fixture.validate().unwrap();
            assert_eq!(evidence.native_contract().frames, 17);
            assert_eq!(evidence.sampled_contract().frames, 3);
            assert_eq!(evidence.native_span().end().ticks, 708);
            assert_eq!(evidence.sampled_span().end().ticks, 100);
            assert_eq!(evidence.native_object(), &fixture.artifact.native_object);
            assert_eq!(evidence.sampled_object(), &fixture.artifact.sampled_object);
            assert_eq!(evidence.context_object(), &byte_object(&fixture.context));
            assert_eq!(
                evidence.generation_options().instructions.unwrap().as_str(),
                "Keep the hands still."
            );
            let inputs: Vec<_> = evidence.conditioning_inputs().collect();
            assert_eq!(inputs.len(), 11 + usize::from(opposite));
            let AcceptedGenerationEvidence::Extension(extension) = &evidence else {
                panic!("wrong operation");
            };
            let receipt = extension.conditioning();
            let expected: Vec<_> = std::iter::once(receipt.manifest())
                .chain(receipt.context())
                .chain(receipt.opposite())
                .chain(std::iter::once(receipt.signatures()))
                .map(|input| (input.object(), input.declaration().sha256().as_str()))
                .collect();
            assert_eq!(inputs, expected);
            assert_eq!(inputs.last().unwrap().0, receipt.signatures().object());
            assert_eq!(extension.binding(), &fixture.binding);
        }
    }
}

#[test]
fn accepted_extension_copy_and_shortening_keep_the_full_historical_sampling_map() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let fixture = SavedFixture::new(direction, true);
        let (bytes, mut artifact) = fixture.wire();
        artifact.native_asset = AssetId::new("copied-native").unwrap();
        artifact.sampled_asset = AssetId::new("copied-sampled").unwrap();
        let recipe = HoldRecipe {
            picture_context: None,
            duration: FrameDuration::new(1).unwrap(),
            audio: HoldAudio::Silence,
            video: HoldVideo::Generated {
                accepted: Box::new(AcceptedGeneration {
                    artifact: artifact.clone(),
                    fallback: HoldFallback::Background,
                }),
            },
        };
        let HoldVideo::Generated { accepted } = &recipe.video else {
            unreachable!()
        };
        assert!(recipe.duration < accepted.artifact.sampling.output_frame_count());
        let evidence = StoredGeneratedProvenance::from_bytes(&bytes, &accepted.artifact)
            .unwrap()
            .validate_for(
                &accepted.artifact,
                &fixture.binding.project_id,
                &fixture.context,
            )
            .unwrap();
        assert_eq!(evidence.sampled_contract().frames, 3);
        let AcceptedGenerationEvidence::Extension(extension) = evidence else {
            panic!("wrong operation");
        };
        assert_eq!(extension.binding().revision_id, fixture.binding.revision_id);
        assert_eq!(extension.binding().target, fixture.binding.target);

        // Shortening belongs to the Hold recipe. Changing immutable sampling
        // would describe another generated artifact and must be rejected.
        let map = fixture.binding.plan.sampling_map();
        artifact.sampling = ExtensionSamplingMap::new(
            direction,
            map.project_rate(),
            map.native_rate(),
            map.context_frame_count(),
            map.generated_frame_count(),
            recipe.duration,
            map.interpolation(),
        )
        .unwrap()
        .into();
        assert!(
            StoredGeneratedProvenance::from_bytes(&bytes, &artifact)
                .unwrap()
                .validate_for(&artifact, &fixture.binding.project_id, &fixture.context)
                .is_err()
        );
    }
}

#[test]
fn accepted_extension_rejects_project_media_operation_and_complete_map_substitutions() {
    let fixture = SavedFixture::new(ExtensionDirection::FromLeft, true);
    let (bytes, artifact) = fixture.wire();
    let read = || StoredGeneratedProvenance::from_bytes(&bytes, &artifact).unwrap();
    assert!(
        read()
            .validate_for(
                &artifact,
                &ProjectId::new("other-project").unwrap(),
                &fixture.context
            )
            .is_err()
    );
    for field in [
        "native",
        "sampled",
        "provenance",
        "direction",
        "interval",
        "operation",
    ] {
        let mut changed = artifact.clone();
        let map = fixture.binding.plan.sampling_map();
        match field {
            "native" => changed.native_object = byte_object(b"other native"),
            "sampled" => changed.sampled_object = byte_object(b"other sampled"),
            "provenance" => changed.provenance = byte_object(b"other provenance"),
            "direction" => {
                changed.sampling = ExtensionSamplingMap::new(
                    ExtensionDirection::FromRight,
                    map.project_rate(),
                    map.native_rate(),
                    map.context_frame_count(),
                    map.generated_frame_count(),
                    map.output_frame_count(),
                    map.interpolation(),
                )
                .unwrap()
                .into()
            }
            "interval" => {
                changed.sampling = ExtensionSamplingMap::new(
                    map.direction(),
                    map.project_rate(),
                    map.native_rate(),
                    FrameDuration::new(8).unwrap(),
                    FrameDuration::new(9).unwrap(),
                    map.output_frame_count(),
                    map.interpolation(),
                )
                .unwrap()
                .into()
            }
            "operation" => {
                changed.sampling = BridgeSamplingMap::new(
                    map.project_rate(),
                    map.native_rate(),
                    map.native_frame_count(),
                    map.output_frame_count(),
                    map.interpolation(),
                )
                .unwrap()
                .into()
            }
            _ => unreachable!(),
        }
        assert!(
            read()
                .validate_for(&changed, &fixture.binding.project_id, &fixture.context)
                .is_err(),
            "{field}"
        );
        if field == "operation" {
            assert!(StoredGeneratedProvenance::from_bytes(&bytes, &changed).is_err());
            assert!(
                StoredExtensionProvenance::from_bytes(&bytes, &artifact.provenance)
                    .unwrap()
                    .validate_artifact(&changed, &fixture.binding.project_id, &fixture.context)
                    .is_err()
            );
        }
    }
}

#[test]
fn accepted_extension_rechecks_retained_manifest_original_revision_and_report_policies() {
    let fixture = SavedFixture::new(ExtensionDirection::FromLeft, true);
    let (bytes, artifact) = fixture.wire();
    let mut context = fixture.context.clone();
    context.push(b' ');
    assert!(
        StoredGeneratedProvenance::from_bytes(&bytes, &artifact)
            .unwrap()
            .validate_for(&artifact, &fixture.binding.project_id, &context)
            .is_err()
    );
    for (pointer, replacement) in [
        ("/binding/revision_id", json!("current-revision")),
        ("/binding/target/hold_id", json!("copied-hold")),
        (
            "/binding/constraints/instructions",
            json!("Different current controls"),
        ),
        ("/pixels/motion/transitions/0/mean_luma_shift", json!(100.0)),
        (
            "/pixels/endpoints/entry/endpoint/gross_cell_fraction",
            json!(1.0),
        ),
        ("/geometry/geometry/measured_face_frames", json!(1)),
        (
            "/conditioning/signatures/declaration/sha256",
            json!("f".repeat(64)),
        ),
    ] {
        let mut changed = SavedFixture::new(ExtensionDirection::FromLeft, true);
        *changed.envelope.pointer_mut(pointer).expect(pointer) = replacement;
        assert!(changed.validate().is_err(), "{pointer}");
    }
}
