//! Metadata-only revalidation fixtures. These identities are deliberately not
//! presented as admission of PNG/movie bytes or actual detector measurements.

use deadpan_analysis::generated_extension::ExtensionCoverage;
use deadpan_analysis::generated_geometry::extension::{self as face, RawExtensionLandmarkBatch};
use deadpan_analysis::generated_geometry::{FaceObservationSet, FrameObservation as FaceFrame};
use deadpan_jobs::landmarks::{self, InspectionExtensionObservations};
use serde_json::json;

use super::*;
use crate::{ExtensionGenerationBinding, ExtensionGeometryChecks, ExtensionPixelReport};

fn object(tag: u8, length: u64) -> GeneratedObjectRef {
    GeneratedObjectRef::new(
        GeneratedContentId::new(format!("{tag:02x}").repeat(32)).unwrap(),
        length,
    )
    .unwrap()
}

fn input_receipt(declaration: &WorkspaceArtifact, tag: u8) -> ConditioningArtifactReceipt {
    ConditioningArtifactReceipt::new(declaration.clone(), object(tag, declaration.byte_length()))
        .unwrap()
}

fn fixture(
    direction: ExtensionDirection,
    opposite: bool,
) -> (
    ExtensionContext,
    ExtensionConditioningReceipt,
    ExtensionGenerationBinding,
) {
    let context = context(direction, FrameRate::new(30, 1).unwrap(), opposite);
    let manifest = declaration(
        "inputs/context.json",
        &serde_json::to_vec(&context).unwrap(),
    );
    let receipt = ExtensionConditioningReceipt::new(
        input_receipt(&manifest, 1),
        context
            .context()
            .iter()
            .enumerate()
            .map(|(index, frame)| input_receipt(&frame.frame, index as u8 + 2))
            .collect(),
        match context.opposite() {
            ExtensionOppositeSeam::Absent => None,
            ExtensionOppositeSeam::PresentUnconditioned { frame, .. } => {
                Some(input_receipt(frame, 20))
            }
        },
        input_receipt(context.continuity().signatures(), 21),
    )
    .unwrap();
    let binding = ExtensionGenerationBinding::from_request(&request(&context, &manifest)).unwrap();
    (context, receipt, binding)
}

#[test]
fn pure_conditioning_binding_rejects_changed_intent_and_input_order() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        for opposite in [false, true] {
            let (context, receipt, binding) = fixture(direction, opposite);
            receipt.validate_binding(&context, &binding).unwrap();
            for field in [
                "project",
                "revision",
                "manifest",
                "sha256",
                "region",
                "raster",
                "direction",
            ] {
                let mut changed = binding.clone();
                match field {
                    "project" => changed.project_id = ProjectId::new("other").unwrap(),
                    "revision" => changed.revision_id = RevisionId::new("other").unwrap(),
                    "manifest" => {
                        changed.input.manifest = WorkspaceRef::new("inputs/other.json").unwrap()
                    }
                    "sha256" => changed.input.sha256 = Sha256::new("f".repeat(64)).unwrap(),
                    "region" => {
                        changed.constraints.region_target = Some(TargetId::new("other").unwrap())
                    }
                    "raster" => {
                        changed.constraints.video = VideoSpec::new(
                            changed.plan.project_frames(),
                            changed.plan.project_frame_rate(),
                            8,
                            4,
                        )
                        .unwrap()
                    }
                    "direction" => {
                        changed.plan = plan(
                            match direction {
                                ExtensionDirection::FromLeft => ExtensionDirection::FromRight,
                                ExtensionDirection::FromRight => ExtensionDirection::FromLeft,
                            },
                            FrameRate::new(30, 1).unwrap(),
                        );
                        changed.constraints.conditioning = match direction {
                            ExtensionDirection::FromLeft => ConditioningMode::ExtendFromRight,
                            ExtensionDirection::FromRight => ConditioningMode::ExtendFromLeft,
                        };
                    }
                    _ => unreachable!(),
                }
                assert!(
                    receipt.validate_binding(&context, &changed).is_err(),
                    "{field}"
                );
            }
            for field in ["frame order", "frame hash", "signatures", "opposite"] {
                let mut changed = receipt.clone();
                match field {
                    "frame order" => changed.context.swap(0, 1),
                    "frame hash" => {
                        changed.context[0] = input_receipt(
                            &WorkspaceArtifact::new(
                                context.context()[0].frame.reference().clone(),
                                Sha256::new("f".repeat(64)).unwrap(),
                                context.context()[0].frame.byte_length(),
                            )
                            .unwrap(),
                            22,
                        )
                    }
                    "signatures" => {
                        changed.signatures =
                            input_receipt(&declaration("inputs/other.bin", b"other-signatures"), 23)
                    }
                    "opposite" => {
                        changed.opposite = if opposite {
                            None
                        } else {
                            Some(input_receipt(
                                &declaration("inputs/opposite.png", b"opposite"),
                                24,
                            ))
                        }
                    }
                    _ => unreachable!(),
                }
                // Each mutation is still a structurally legal receipt. It must
                // fail its relationship to the independently retained context.
                changed.validate_shape().unwrap();
                assert!(
                    changed.validate_binding(&context, &binding).is_err(),
                    "{field}"
                );
            }
        }
    }
}

fn quality(ordinal: u32) -> crate::FrameObservation {
    crate::FrameObservation {
        after_frame: ordinal,
        mean_luma_shift: 0.0,
        mean_absolute_luma_change: 0.0,
        lighting_agreement_fraction: 1.0,
        textured_blocks: 0,
        matched_blocks: 0,
        motion: crate::MotionObservation::Unavailable,
    }
}

fn pixel_report(
    context: &ExtensionContext,
    receipt: &ExtensionConditioningReceipt,
    native: &GeneratedObjectRef,
    sampled: &GeneratedObjectRef,
) -> ExtensionPixelReport {
    let plan = context.plan();
    let contract = crate::extension_endpoints::sampled_contract(plan).unwrap();
    let interval = plan.sampling_map().generated_interval();
    let join = |entry: bool| {
        let ordinal = if entry { 0 } else { contract.frames - 1 };
        let (role, picture, retained, content) =
            if entry == (plan.direction() == ExtensionDirection::FromLeft) {
                let anchor = match plan.direction() {
                    ExtensionDirection::FromLeft => receipt.context().last().unwrap(),
                    ExtensionDirection::FromRight => receipt.context().first().unwrap(),
                };
                (
                    crate::ExtensionJoinRole::Conditioned,
                    &context.anchor().picture,
                    anchor,
                    context.anchor().content,
                )
            } else {
                match (context.opposite(), receipt.opposite()) {
                    (ExtensionOppositeSeam::Absent, None) => {
                        return crate::ExtensionEndpointJoin::Absent {};
                    }
                    (
                        ExtensionOppositeSeam::PresentUnconditioned {
                            picture, content, ..
                        },
                        Some(retained),
                    ) => (
                        crate::ExtensionJoinRole::Unconditioned,
                        picture.as_ref(),
                        retained,
                        *content,
                    ),
                    _ => unreachable!(),
                }
            };
        crate::ExtensionEndpointJoin::Measured {
            role,
            picture: Box::new(picture.clone()),
            object: retained.object().clone(),
            content,
            endpoint: crate::EndpointObservation {
                sampled_frame: ordinal,
                sampled_pts: contract.matroska_pts(ordinal).unwrap(),
                mean_absolute_rgb_difference: 0.0,
                gross_cell_fraction: 0.0,
            },
            quality: quality(ordinal),
        }
    };
    let thresholds = crate::QualityThresholds::for_motion(MotionAmount::Still);
    serde_json::from_value(json!({
        "schema_version": 1, "profile": "deadpan-extension-pixels-1",
        "motion": {
            "schema_version": 1, "profile": "deadpan-extension-motion-lighting-1",
            "plan": plan, "native_object": native, "motion": MotionAmount::Still,
            "thresholds": thresholds,
            "transitions": ((interval.start + 1)..interval.end).map(|ordinal| quality(ordinal as u32)).collect::<Vec<_>>(),
        },
        "endpoints": {
            "schema_version": 1, "profile": "deadpan-extension-endpoints-1",
            "plan": plan, "sampled": contract, "sampled_object": sampled,
            "conditioning": receipt, "presentation": context.presentation(), "motion": MotionAmount::Still,
            "thresholds": crate::EndpointThresholds::policy(), "quality_thresholds": thresholds,
            "entry": join(true), "exit": join(false),
        },
    })).unwrap()
}

fn geometry_report(
    context: &ExtensionContext,
    receipt: &ExtensionConditioningReceipt,
    native_object: &GeneratedObjectRef,
) -> ExtensionGeometryChecks {
    let native = crate::extension_motion::native_contract(context.plan());
    let expected = crate::landmark_inspection::extension_picture_pts(native).unwrap();
    let interval = context.plan().sampling_map().generated_interval();
    let coverage = ExtensionCoverage {
        direction: context.plan().direction(),
        start: interval.start as u32,
        end: interval.end as u32,
    };
    let raw = RawExtensionLandmarkBatch {
        schema_version: 1,
        coverage,
        anchor: FaceObservationSet::Detected { faces: vec![] },
        frames: coverage
            .ordinals()
            .map(|ordinal| FaceFrame {
                ordinal,
                pts: expected[ordinal as usize],
                observation: FaceObservationSet::Detected { faces: vec![] },
            })
            .collect(),
    };
    let crop = context.presentation();
    let geometry = face::analyze(
        &raw,
        &expected,
        [native.width, native.height],
        [crop.x, crop.y, crop.width, crop.height],
        false,
    )
    .unwrap();
    serde_json::from_value(json!({
        "schema_version": 1, "profile": "deadpan-extension-face-mouth-region-1",
        "native": native, "native_object": native_object, "context": context, "conditioning": receipt,
        "runtime": { "engine": landmarks::ENGINE, "request_revision": landmarks::REQUEST_REVISION, "constellation": landmarks::CONSTELLATION },
        "region_runtime": null, "timings": {"decode_millis": 0, "vision_millis": 0, "elapsed_millis": 0},
        "observations": InspectionExtensionObservations {schema_version: 1, landmarks: raw, region: None},
        "geometry": geometry, "region": null, "region_unavailable_reason": context.region().unavailable_reason(),
    })).unwrap()
}

#[test]
fn saved_pixel_and_geometry_reports_bind_full_contracts_objects_and_policy() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        for opposite in [false, true] {
            let (context, receipt, binding) = fixture(direction, opposite);
            let native = crate::extension_motion::native_contract(&binding.plan);
            let sampled = crate::extension_endpoints::sampled_contract(&binding.plan).unwrap();
            let native_object = object(30, 100);
            let sampled_object = object(31, 101);
            let pixels = pixel_report(&context, &receipt, &native_object, &sampled_object);
            let geometry = geometry_report(&context, &receipt, &native_object);
            let pixels_valid = |report: &ExtensionPixelReport,
                                native,
                                sampled,
                                native_object,
                                sampled_object,
                                binding| {
                report.validate_bound(
                    (native, native_object),
                    (sampled, sampled_object),
                    &context,
                    &receipt,
                    binding,
                )
            };
            let geometry_valid =
                |report: &ExtensionGeometryChecks, native, native_object, binding| {
                    report.validate_bound(native, native_object, &context, &receipt, binding)
                };
            pixels_valid(
                &pixels,
                &native,
                &sampled,
                &native_object,
                &sampled_object,
                &binding,
            )
            .unwrap();
            geometry_valid(&geometry, &native, &native_object, &binding).unwrap();
            let mut changed_native = native;
            changed_native.width *= 2;
            let mut changed_sampled = sampled;
            changed_sampled.width *= 2;
            assert!(
                pixels_valid(
                    &pixels,
                    &changed_native,
                    &sampled,
                    &native_object,
                    &sampled_object,
                    &binding
                )
                .is_err()
            );
            assert!(
                pixels_valid(
                    &pixels,
                    &native,
                    &changed_sampled,
                    &native_object,
                    &sampled_object,
                    &binding
                )
                .is_err()
            );
            assert!(geometry_valid(&geometry, &changed_native, &native_object, &binding).is_err());
            let other_object = object(32, 100);
            assert!(
                pixels_valid(
                    &pixels,
                    &native,
                    &sampled,
                    &other_object,
                    &sampled_object,
                    &binding
                )
                .is_err()
            );
            assert!(
                pixels_valid(
                    &pixels,
                    &native,
                    &sampled,
                    &native_object,
                    &other_object,
                    &binding
                )
                .is_err()
            );
            assert!(geometry_valid(&geometry, &native, &other_object, &binding).is_err());
            let mut changed_binding = binding.clone();
            changed_binding.constraints.motion = MotionAmount::Moderate;
            assert!(
                pixels_valid(
                    &pixels,
                    &native,
                    &sampled,
                    &native_object,
                    &sampled_object,
                    &changed_binding
                )
                .is_err()
            );

            let mut changed = serde_json::to_value(&pixels).unwrap();
            changed["motion"]["transitions"]
                .as_array_mut()
                .unwrap()
                .remove(0);
            let changed = serde_json::from_value(changed).unwrap();
            assert!(
                pixels_valid(
                    &changed,
                    &native,
                    &sampled,
                    &native_object,
                    &sampled_object,
                    &binding
                )
                .is_err()
            );
            let mut changed = serde_json::to_value(&geometry).unwrap();
            changed["geometry"]["measured_face_frames"] = json!(1);
            let changed = serde_json::from_value(changed).unwrap();
            assert!(geometry_valid(&changed, &native, &native_object, &binding).is_err());
        }
    }
}
