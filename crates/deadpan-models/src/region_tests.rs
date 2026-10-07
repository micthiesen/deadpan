use super::*;
use crate::{
    BoundaryPicture, CapturedRegionBoundary, DecodedBoundary, MeasuredStream, ModelInputConversion,
    RasterRect, RegionCaptureUnavailable,
};
use deadpan_analysis::generated_region::{
    RawRegionFrame, RegionCheckStatus, RegionObservation, RegionObservationUnavailableReason,
};
use deadpan_core::{
    AssetId, ExactRatio, FrameDuration, FrameRate, GeneratedContentId, SourceFrameId, SourcePoint,
    SourceQualificationId, SourceTimeBase, SourceTimestamp, TargetId, TargetRegion, TargetSource,
};
use deadpan_jobs::{
    AxisLimits, BridgeCapability, DimensionLimits, FrameCountFormula, NativeDimensions, Sha256,
    WorkspaceArtifact, WorkspaceRef,
};
use serde_json::json;

fn object(tag: u8) -> GeneratedObjectRef {
    GeneratedObjectRef::new(
        GeneratedContentId::new(format!("{tag:02x}").repeat(32)).unwrap(),
        10,
    )
    .unwrap()
}

fn fixture(
    selected: bool,
) -> (
    BridgeGenerationPlan,
    GeneratedObjectRef,
    ConditioningReceipt,
    BridgeContext,
) {
    let plan = BridgeGenerationPlan::new(
        FrameDuration::new(30).unwrap(),
        FrameRate::new(30000, 1001).unwrap(),
        &BridgeCapability::new(
            true,
            FrameRate::new(24, 1).unwrap(),
            FrameCountFormula::new(8, 1, 25, 97).unwrap(),
            DimensionLimits::new(
                AxisLimits::new(64, 64, 1).unwrap(),
                AxisLimits::new(36, 36, 1).unwrap(),
            ),
        ),
        NativeDimensions::new(64, 36).unwrap(),
    )
    .unwrap();
    let declaration = |path: &str| {
        WorkspaceArtifact::new(
            WorkspaceRef::new(path).unwrap(),
            Sha256::new("a".repeat(64)).unwrap(),
            10,
        )
        .unwrap()
    };
    let receipt = serde_json::from_value(json!({
        "schema_version":1,
        "manifest":{"declaration":declaration("inputs/context.json"),"object":object(1)},
        "left":{"declaration":declaration("inputs/left.png"),"object":object(2)},
        "right":{"declaration":declaration("inputs/right.png"),"object":object(3)},
    }))
    .unwrap();
    let boundary = |right: bool| BoundaryPicture::Original {
        clock: crate::BoundaryClock::Project {
            frame: if right { 31 } else { 0 },
        },
        asset: AssetId::new("original").unwrap(),
        qualification: SourceQualificationId::new("a".repeat(64)).unwrap(),
        picture: DecodedBoundary {
            source_frame: SourceFrameId(u64::from(right)),
            pts: SourceTimestamp {
                ticks: i64::from(right),
                time_base: SourceTimeBase::new(1, 24).unwrap(),
            },
            stream: MeasuredStream {
                codec: "rawvideo".into(),
                pixel_format: "rgb24".into(),
                width: 64,
                height: 36,
                sample_aspect: [1, 1],
                rotation_quarter_turns: 0,
                decoded_sample_bits: 8,
                color: crate::CANONICAL_BRIDGE_COLOR,
            },
            model_input: ModelInputConversion::SrgbCodesUnchanged,
        },
    };
    let full = RasterRect::new(0, 0, 64, 36).unwrap();
    let mut context = BridgeContext::new(
        plan.clone(),
        declaration("inputs/left.png"),
        declaration("inputs/right.png"),
        "fixture",
        crate::CANONICAL_BRIDGE_COLOR,
        BridgeBoundaries {
            left: boundary(false),
            right: boundary(true),
        },
        ConditioningGeometry {
            presentation: full,
            left_content: Some(full),
            right_content: Some(full),
        },
    )
    .unwrap();
    if selected {
        let captured = |right: bool| CapturedRegionBoundary::Available {
            asset: AssetId::new("original").unwrap(),
            point: SourcePoint {
                ticks: ExactRatio::integer(i64::from(right)),
                time_base: SourceTimeBase::new(1, 24).unwrap(),
            },
            source: TargetSource::Manual,
            region: TargetRegion {
                center: [300_000, 400_000],
                size: [200_000, 200_000],
            },
            confidence: None,
        };
        context = context
            .with_region(RegionCapture::Selected {
                target: TargetId::new("subject").unwrap(),
                label: "Selected subject".into(),
                target_sha256: Sha256::new("b".repeat(64)).unwrap(),
                left: Box::new(captured(false)),
                right: Box::new(captured(true)),
            })
            .unwrap();
    }
    (plan, object(4), receipt, context)
}

fn runtime() -> RegionRuntimeReport {
    RegionRuntimeReport {
        engine: deadpan_jobs::landmarks::REGION_ENGINE.into(),
        request_revision: deadpan_jobs::landmarks::REGION_REQUEST_REVISION,
        tracking_level: deadpan_jobs::landmarks::REGION_TRACKING_LEVEL.into(),
    }
}

fn timings() -> InspectionTimings {
    InspectionTimings {
        decode_millis: 1,
        vision_millis: 2,
        elapsed_millis: 4,
    }
}

fn raw(context: &BridgeContext) -> RawRegionBatch {
    let contract = crate::geometry::contract(context.plan());
    let seeds = captured_seeds(context, contract).unwrap().unwrap();
    let observation = RegionObservation::Tracked {
        region: seeds.left,
        confidence: 0.95,
    };
    RawRegionBatch {
        schema_version: 1,
        seeds,
        left: observation,
        right: observation,
        frames: picture_pts(contract)
            .unwrap()
            .into_iter()
            .enumerate()
            .map(|(ordinal, pts)| RawRegionFrame {
                ordinal: u32::try_from(ordinal).unwrap(),
                pts,
                observation,
            })
            .collect(),
    }
}

#[test]
fn no_selected_subject_is_explicit_unavailable_evidence() {
    let (plan, native, conditioning, context) = fixture(false);
    let report = assemble(
        &plan,
        &native,
        &conditioning,
        &context,
        None,
        None,
        timings(),
    )
    .unwrap();
    assert_eq!(
        report.unavailable_reason(),
        Some("no selected region target")
    );
    assert_eq!(report.assessment(), None);
    assert_eq!(report.capture().target_id(), None);
    assert_eq!(report.inspection_timings(), None);
    report.validate(&plan, &native, &conditioning).unwrap();
    report.validate_context(&context).unwrap();
}

#[test]
fn selected_but_lost_authored_subject_keeps_its_identity_and_reason() {
    let (plan, native, conditioning, context) = fixture(true);
    let mut capture = context.region().unwrap().clone();
    let RegionCapture::Selected { left, .. } = &mut capture else {
        panic!("selected fixture")
    };
    **left = CapturedRegionBoundary::Unavailable {
        reason: RegionCaptureUnavailable::LostTrack,
    };
    let context = context.with_region(capture).unwrap();
    let report = assemble(
        &plan,
        &native,
        &conditioning,
        &context,
        None,
        None,
        timings(),
    )
    .unwrap();
    assert_eq!(report.capture().target_id().unwrap().as_str(), "subject");
    assert_eq!(
        report.unavailable_reason(),
        Some("left: authored target tracking was lost")
    );
    assert!(report.assessment().is_none());
}

#[test]
fn fresh_measurement_requires_explicit_capture_even_when_old_context_is_readable() {
    let (plan, native, conditioning, context) = fixture(false);
    let mut legacy = serde_json::to_value(&context).unwrap();
    legacy["schema_version"] = json!(3);
    legacy.as_object_mut().unwrap().remove("region");
    let legacy: BridgeContext = serde_json::from_value(legacy).unwrap();
    assert!(legacy.region().is_none());
    assert!(captured_seeds(&legacy, crate::geometry::contract(&plan)).is_err());
    assert!(
        assemble(
            &plan,
            &native,
            &conditioning,
            &legacy,
            None,
            None,
            timings()
        )
        .is_err()
    );
}

#[test]
fn measured_subject_retains_exact_native_coverage_and_pinned_runtime() {
    let (plan, native, conditioning, context) = fixture(true);
    let report = assemble(
        &plan,
        &native,
        &conditioning,
        &context,
        Some(raw(&context)),
        Some(runtime()),
        timings(),
    )
    .unwrap();
    assert_eq!(report.capture(), context.region().unwrap());
    assert_eq!(
        report.assessment().unwrap().measured_frames,
        plan.native_frame_count()
    );
    assert_eq!(
        report.assessment().unwrap().status,
        RegionCheckStatus::Measured
    );
    assert!(report.unavailable_reason().is_none());
    assert_eq!(report.inspection_timings(), Some(timings()));
    let bytes = serde_json::to_vec(&report).unwrap();
    let restored: BridgeRegionReport = serde_json::from_slice(&bytes).unwrap();
    restored.validate(&plan, &native, &conditioning).unwrap();
    restored.validate_context(&context).unwrap();
}

#[test]
fn observation_and_runtime_presence_must_match_the_explicit_capture() {
    let (plan, native, conditioning, context) = fixture(true);
    assert!(
        assemble(
            &plan,
            &native,
            &conditioning,
            &context,
            None,
            Some(runtime()),
            timings()
        )
        .is_err()
    );
    assert!(
        assemble(
            &plan,
            &native,
            &conditioning,
            &context,
            Some(raw(&context)),
            None,
            timings()
        )
        .is_err()
    );
    assert!(
        assemble(
            &plan,
            &native,
            &conditioning,
            &context,
            None,
            None,
            timings()
        )
        .is_err()
    );
    let observations = raw(&context);
    let (_, _, _, unselected) = fixture(false);
    assert!(
        assemble(
            &plan,
            &native,
            &conditioning,
            &unselected,
            Some(observations),
            Some(runtime()),
            timings()
        )
        .is_err()
    );
}

#[test]
fn sustained_region_drift_is_rejected_before_a_report_can_be_admitted() {
    let (plan, native, conditioning, context) = fixture(true);
    let mut observations = raw(&context);
    for frame in &mut observations.frames[1..3] {
        frame.observation = RegionObservation::Tracked {
            region: deadpan_analysis::NormalizedRect::new(0.65, 0.3, 0.2, 0.2).unwrap(),
            confidence: 0.95,
        };
    }
    let error = assemble(
        &plan,
        &native,
        &conditioning,
        &context,
        Some(observations),
        Some(runtime()),
        timings(),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("gross selected-region drift beginning at native frame 1")
    );
}

#[test]
fn lost_worker_track_retains_partial_coverage_without_restart() {
    let (plan, native, conditioning, context) = fixture(true);
    let mut observations = raw(&context);
    observations.frames[2].observation = RegionObservation::Unavailable {
        reason: RegionObservationUnavailableReason::LostTrack,
    };
    let report = assemble(
        &plan,
        &native,
        &conditioning,
        &context,
        Some(observations),
        Some(runtime()),
        timings(),
    )
    .unwrap();
    let assessment = report.assessment().unwrap();
    assert_eq!(assessment.measured_frames, 2);
    assert_eq!(assessment.unavailable_frames, plan.native_frame_count() - 2);
    assert_eq!(assessment.first_unavailable_ordinal, Some(2));
}

#[test]
fn altered_raw_policy_runtime_and_object_bindings_are_rejected() {
    let (plan, native, conditioning, context) = fixture(true);
    let report = assemble(
        &plan,
        &native,
        &conditioning,
        &context,
        Some(raw(&context)),
        Some(runtime()),
        timings(),
    )
    .unwrap();
    for field in [
        "native_object",
        "context_object",
        "left_object",
        "right_object",
    ] {
        let mut value = serde_json::to_value(&report).unwrap();
        value[field] = json!(object(9));
        let changed: BridgeRegionReport = serde_json::from_value(value).unwrap();
        assert!(
            changed.validate(&plan, &native, &conditioning).is_err(),
            "{field}"
        );
    }
    for mutation in [
        "runtime", "pts", "frames", "seed", "policy", "coverage", "timings",
    ] {
        let mut changed = report.clone();
        let RegionEvidence::Measured {
            runtime,
            timings,
            observations,
            assessment,
        } = &mut changed.evidence
        else {
            panic!("measured fixture")
        };
        match mutation {
            "runtime" => runtime.request_revision += 1,
            "pts" => observations.frames[0].pts += 1,
            "frames" => {
                observations.frames.pop();
            }
            "seed" => {
                observations.seeds.left =
                    deadpan_analysis::NormalizedRect::new(0.1, 0.1, 0.2, 0.2).unwrap()
            }
            "policy" => assessment.thresholds.center_residual = 1.0,
            "coverage" => assessment.measured_frames = 0,
            "timings" => timings.vision_millis = timings.elapsed_millis + 1,
            _ => unreachable!(),
        }
        assert!(
            changed.validate(&plan, &native, &conditioning).is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn context_comparison_binds_the_selected_target_even_for_identical_boxes() {
    let (plan, native, conditioning, context) = fixture(true);
    let report = assemble(
        &plan,
        &native,
        &conditioning,
        &context,
        Some(raw(&context)),
        Some(runtime()),
        timings(),
    )
    .unwrap();
    let mut capture = context.region().unwrap().clone();
    let RegionCapture::Selected { target, .. } = &mut capture else {
        panic!("selected fixture")
    };
    *target = TargetId::new("other-subject").unwrap();
    assert!(
        report
            .validate_context(&context.with_region(capture).unwrap())
            .is_err()
    );
}

#[test]
fn unavailable_reason_and_wire_shape_cannot_be_relabelled() {
    let (plan, native, conditioning, context) = fixture(false);
    let report = assemble(
        &plan,
        &native,
        &conditioning,
        &context,
        None,
        None,
        timings(),
    )
    .unwrap();
    let mut changed = report.clone();
    changed.evidence = RegionEvidence::Unavailable {
        reason: "all checks passed".into(),
    };
    assert!(changed.validate(&plan, &native, &conditioning).is_err());
    let mut wire = serde_json::to_value(&report).unwrap();
    wire["evidence"]["unexpected"] = json!(true);
    assert!(serde_json::from_value::<BridgeRegionReport>(wire).is_err());
}
