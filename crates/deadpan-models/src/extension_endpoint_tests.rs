use deadpan_core::{ExactRatio, FrameDuration, FrameRate, GeneratedContentId, NodeId};
use deadpan_jobs::{
    AxisLimits, DimensionLimits, ExtensionCapability, ExtensionCapturePolicy, FrameCountFormula,
    GenerationCaptureSpec, GenerationInputBinding, GenerationInputs, GenerationPictureIdentity,
    NativeDimensions, RelativeGenerationPicture, Sha256, WorkspaceArtifact, WorkspaceRef,
};
use serde_json::json;

use super::*;
use crate::{
    BoundaryClock, ExtensionContextPicture, ExtensionContinuityEvidence, ExtensionRegionCapture,
    MotionObservation,
};

fn object(tag: u8, length: u64) -> GeneratedObjectRef {
    GeneratedObjectRef::new(
        GeneratedContentId::new(format!("{tag:02x}").repeat(32)).unwrap(),
        length,
    )
    .unwrap()
}

fn declaration(path: &str, length: u64) -> WorkspaceArtifact {
    WorkspaceArtifact::new(
        WorkspaceRef::new(path).unwrap(),
        Sha256::new("a".repeat(64)).unwrap(),
        length,
    )
    .unwrap()
}

fn clock(position: i64) -> BoundaryClock {
    BoundaryClock::Definition {
        project_id: deadpan_core::ProjectId::new("project").unwrap(),
        revision_id: deadpan_core::RevisionId::new("revision").unwrap(),
        definition: NodeId::new("definition").unwrap(),
        position: ExactRatio::integer(position),
    }
}

fn fixture(
    direction: ExtensionDirection,
    frames: i64,
    opposite: bool,
) -> (
    ExtensionGenerationPlan,
    GeneratedObjectRef,
    ExtensionContext,
    ExtensionConditioningReceipt,
    ExtensionEndpointReport,
) {
    let rate = FrameRate::new(30000, 1001).unwrap();
    let plan = ExtensionGenerationPlan::new(
        direction,
        FrameDuration::new(frames).unwrap(),
        rate,
        &ExtensionCapability::new(
            FrameRate::new(24, 1).unwrap(),
            1,
            FrameCountFormula::new(8, 0, 8, 64).unwrap(),
            DimensionLimits::new(
                AxisLimits::new(64, 64, 1).unwrap(),
                AxisLimits::new(36, 36, 1).unwrap(),
            ),
            FrameDuration::new(60).unwrap(),
        )
        .unwrap(),
        NativeDimensions::new(64, 36).unwrap(),
    )
    .unwrap();
    let anchor = ExtensionContextPicture {
        picture: BoundaryPicture::AuthoredBlack { clock: clock(100) },
        frame: declaration("inputs/anchor.png", 10),
        content: None,
    };
    let offset = match direction {
        ExtensionDirection::FromLeft => frames + 1,
        ExtensionDirection::FromRight => -frames - 1,
    };
    let seam = if opposite {
        ExtensionOppositeSeam::PresentUnconditioned {
            picture: Box::new(BoundaryPicture::AuthoredBlack {
                clock: clock(100 + offset),
            }),
            frame: declaration("inputs/opposite.png", 10),
            content: None,
        }
    } else {
        ExtensionOppositeSeam::Absent
    };
    let terminal = RelativeGenerationPicture {
        position: ExactRatio::ZERO,
        picture: GenerationPictureIdentity::AuthoredBlack,
    };
    let continuity = ExtensionContinuityEvidence::new(
        GenerationInputBinding {
            duration: plan.project_frames(),
            frame_rate: rate,
            canvas: [64, 36],
            region: None,
            inputs: GenerationInputs::Extension {
                capture: GenerationCaptureSpec::Extension {
                    direction,
                    native_rate: plan.native_frame_rate(),
                    context_frames: 1,
                    policy: ExtensionCapturePolicy::TemporalContextV1,
                },
                samples: vec![terminal.clone()],
                support: vec![],
                terminal,
                opposite: opposite.then_some(RelativeGenerationPicture {
                    position: ExactRatio::integer(offset),
                    picture: GenerationPictureIdentity::AuthoredBlack,
                }),
            },
        },
        vec![],
        vec![None],
        declaration("inputs/continuity.bin", 12),
    )
    .unwrap();
    let context = ExtensionContext::new(
        plan.clone(),
        vec![anchor],
        RasterRect::new(0, 0, 64, 36).unwrap(),
        seam,
        "typed report fixture",
        ExtensionRegionCapture::None,
        continuity,
    )
    .unwrap();
    let receipt: ExtensionConditioningReceipt = serde_json::from_value(json!({
        "schema_version": 2, "operation": "extension",
        "manifest": {"declaration": declaration("inputs/context.json", 10), "object": object(1, 10)},
        "context": [{"declaration": context.anchor().frame, "object": object(2, 10)}],
        "opposite": if opposite { json!({"declaration": declaration("inputs/opposite.png", 10), "object": object(3, 10)}) } else { json!(null) },
        "signatures": {"declaration": context.continuity().signatures(), "object": object(4, 12)},
    })).unwrap();
    let sampled = object(5, 10);
    let contract = sampled_contract(&plan).unwrap();
    let observation = |entry, ordinal| match expected_join(&context, &receipt, entry).unwrap() {
        None => ExtensionEndpointJoin::Absent {},
        Some(expected) => ExtensionEndpointJoin::Measured {
            role: expected.role,
            picture: Box::new(expected.picture.clone()),
            object: expected.object.clone(),
            content: expected.content,
            endpoint: EndpointObservation {
                sampled_frame: ordinal,
                sampled_pts: contract.matroska_pts(ordinal).unwrap(),
                mean_absolute_rgb_difference: 0.0,
                gross_cell_fraction: 0.0,
            },
            quality: FrameObservation {
                after_frame: ordinal,
                mean_luma_shift: 0.0,
                mean_absolute_luma_change: 0.0,
                lighting_agreement_fraction: 1.0,
                textured_blocks: 0,
                matched_blocks: 0,
                motion: MotionObservation::Unavailable,
            },
        },
    };
    let report = ExtensionEndpointReport {
        schema_version: 1,
        profile: PROFILE.into(),
        plan: plan.clone(),
        sampled: contract,
        sampled_object: sampled.clone(),
        conditioning: receipt.clone(),
        presentation: context.presentation(),
        motion: MotionAmount::Still,
        thresholds: EndpointThresholds::policy(),
        quality_thresholds: QualityThresholds::for_motion(MotionAmount::Still),
        entry: observation(true, 0),
        exit: observation(false, contract.frames - 1),
    };
    (plan, sampled, context, receipt, report)
}

#[test]
fn directions_bind_conditioned_and_optional_unconditioned_joins() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        for frames in [1, 30] {
            for opposite in [false, true] {
                let (plan, sampled, context, receipt, report) =
                    fixture(direction, frames, opposite);
                report
                    .validate(&plan, &sampled, &context, &receipt, MotionAmount::Still)
                    .unwrap();
                for (entry, join, ordinal) in [
                    (true, report.entry(), 0),
                    (false, report.exit(), frames as u32 - 1),
                ] {
                    if let ExtensionEndpointJoin::Measured {
                        role,
                        endpoint,
                        quality,
                        ..
                    } = join
                    {
                        assert_eq!(
                            *role,
                            if is_conditioned(&plan, entry) {
                                ExtensionJoinRole::Conditioned
                            } else {
                                ExtensionJoinRole::Unconditioned
                            }
                        );
                        assert_eq!(endpoint.sampled_frame, ordinal);
                        assert_eq!(
                            endpoint.sampled_pts,
                            report.sampled.matroska_pts(ordinal).unwrap()
                        );
                        assert!(matches!(quality.motion, MotionObservation::Unavailable));
                    } else {
                        assert!(!opposite && !is_conditioned(&plan, entry));
                    }
                }
                let restored: ExtensionEndpointReport =
                    serde_json::from_slice(&serde_json::to_vec(&report).unwrap()).unwrap();
                assert_eq!(restored, report);
            }
        }
    }
}

#[test]
fn extension_joins_reject_gross_and_lighting_changes_at_the_shared_thresholds() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let (plan, sampled, context, receipt, report) = fixture(direction, 30, true);
        for entry in [true, false] {
            for lighting in [false, true] {
                let mut changed = report.clone();
                let join = if entry {
                    &mut changed.entry
                } else {
                    &mut changed.exit
                };
                let ExtensionEndpointJoin::Measured {
                    role,
                    endpoint,
                    quality,
                    ..
                } = join
                else {
                    unreachable!()
                };
                let role = *role;
                if lighting {
                    quality.mean_luma_shift = 32.0;
                    quality.mean_absolute_luma_change = 32.0;
                    quality.lighting_agreement_fraction = 0.5;
                } else {
                    endpoint.mean_absolute_rgb_difference = 64.0;
                    endpoint.gross_cell_fraction = 0.75;
                }
                let error = changed
                    .validate(&plan, &sampled, &context, &receipt, MotionAmount::Still)
                    .unwrap_err()
                    .to_string();
                assert!(
                    error.contains(PROFILE) && error.contains(&format!("{role:?}")),
                    "{error}"
                );
                assert!(
                    error.contains(if lighting { "lighting" } else { "gross" }),
                    "{error}"
                );
                assert!(error.contains("sampled frame"), "{error}");
                assert!(!error.contains("native frame"), "{error}");
            }
        }
    }
}

#[test]
fn join_motion_uses_one_project_frame_spacing() {
    let (plan, sampled, context, receipt, mut report) =
        fixture(ExtensionDirection::FromLeft, 30, true);
    let ExtensionEndpointJoin::Measured { quality, .. } = &mut report.entry else {
        unreachable!()
    };
    quality.textured_blocks = 36;
    quality.matched_blocks = 36;
    quality.motion = MotionObservation::Measured {
        maximum: 0.02,
        p95: 0.02,
    };
    let error = report
        .validate(&plan, &sampled, &context, &receipt, MotionAmount::Still)
        .unwrap_err()
        .to_string();
    assert!(error.contains("excessive motion"), "{error}");
    assert!(error.contains("sampled frame 0"), "{error}");
    let ExtensionEndpointJoin::Measured { quality, .. } = &mut report.entry else {
        unreachable!()
    };
    quality.motion = MotionObservation::Measured {
        maximum: 0.01,
        p95: 0.01,
    };
    report
        .validate(&plan, &sampled, &context, &receipt, MotionAmount::Still)
        .unwrap();
}

#[test]
fn endpoint_report_rejects_mutated_objects_roles_clocks_geometry_and_policy() {
    let (plan, sampled, context, receipt, report) =
        fixture(ExtensionDirection::FromRight, 30, true);
    let original = serde_json::to_value(&report).unwrap();
    let mutations = [
        ("/schema_version", json!(2)),
        ("/profile", json!("wrong")),
        ("/sampled_object", json!(object(99, 10))),
        ("/conditioning/context/0/object", json!(object(99, 10))),
        ("/conditioning/signatures/object", json!(object(99, 12))),
        ("/conditioning/manifest/object", json!(object(99, 10))),
        ("/presentation/x", json!(1)),
        ("/entry/role", json!("conditioned")),
        ("/entry/object", json!(object(99, 10))),
        (
            "/entry/picture/authored_black/clock/position",
            json!({"numerator": 2, "denominator": 1}),
        ),
        (
            "/entry/content",
            json!({"x":0,"y":0,"width":64,"height":36}),
        ),
        ("/entry/endpoint/sampled_frame", json!(1)),
        ("/exit/endpoint/sampled_pts", json!(0)),
        ("/exit/quality/after_frame", json!(0)),
        ("/thresholds/gross_mean_difference", json!(65.0)),
        ("/quality_thresholds/maximum_motion_per_second", json!(20.0)),
        ("/entry", json!({"status":"absent"})),
    ];
    for (pointer, value) in mutations {
        let mut changed = original.clone();
        *changed
            .pointer_mut(pointer)
            .unwrap_or_else(|| panic!("missing {pointer}")) = value;
        if let Ok(changed) = serde_json::from_value::<ExtensionEndpointReport>(changed) {
            assert!(
                changed
                    .validate(&plan, &sampled, &context, &receipt, MotionAmount::Still)
                    .is_err(),
                "accepted {pointer}"
            );
        }
    }
    let mut unknown = original.clone();
    unknown["extra"] = json!(true);
    assert!(serde_json::from_value::<ExtensionEndpointReport>(unknown).is_err());
    let mut missing = original;
    missing["entry"].as_object_mut().unwrap().remove("content");
    assert!(serde_json::from_value::<ExtensionEndpointReport>(missing).is_err());
    let mut hidden_motion = serde_json::to_value(&report).unwrap();
    hidden_motion["entry"]["quality"]["motion"]["p95"] = json!(0.0);
    assert!(serde_json::from_value::<ExtensionEndpointReport>(hidden_motion).is_err());
}

#[test]
fn absent_seam_has_no_forged_or_hidden_measurements() {
    let (plan, sampled, context, receipt, report) = fixture(ExtensionDirection::FromLeft, 1, false);
    let mut hidden = serde_json::to_value(&report).unwrap();
    hidden["exit"]["quality"] = json!({"mean_luma_shift":0});
    assert!(serde_json::from_value::<ExtensionEndpointReport>(hidden).is_err());
    let mut forged = report.clone();
    forged.exit = report.entry.clone();
    assert!(
        forged
            .validate(&plan, &sampled, &context, &receipt, MotionAmount::Still)
            .is_err()
    );
}
