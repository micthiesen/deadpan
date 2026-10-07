use deadpan_core::{FrameDuration, FrameRate, GeneratedContentId};
use deadpan_jobs::{
    AxisLimits, BridgeCapability, DimensionLimits, FrameCountFormula, NativeDimensions, Sha256,
    WorkspaceArtifact, WorkspaceRef,
};
use serde_json::json;

use super::*;

fn object(tag: u8) -> GeneratedObjectRef {
    GeneratedObjectRef::new(
        GeneratedContentId::new(format!("{tag:02x}").repeat(32)).unwrap(),
        10,
    )
    .unwrap()
}

fn fixture(
    frames: i64,
) -> (
    BridgeGenerationPlan,
    GeneratedObjectRef,
    ConditioningReceipt,
    BridgeContext,
    BridgeEndpointReport,
) {
    let plan = BridgeGenerationPlan::new(
        FrameDuration::new(frames).unwrap(),
        FrameRate::new(30000, 1001).unwrap(),
        &BridgeCapability::new(
            true,
            FrameRate::new(24, 1).unwrap(),
            FrameCountFormula::new(1, 0, 2, 97).unwrap(),
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
    let receipt: ConditioningReceipt = serde_json::from_value(json!({
        "schema_version":1,
        "manifest":{"declaration":declaration("inputs/context.json"),"object":object(1)},
        "left":{"declaration":declaration("inputs/left.png"),"object":object(2)},
        "right":{"declaration":declaration("inputs/right.png"),"object":object(3)},
    }))
    .unwrap();
    let geometry = ConditioningGeometry {
        presentation: RasterRect::new(0, 0, 64, 36).unwrap(),
        left_content: None,
        right_content: None,
    };
    let context = BridgeContext::new(
        plan.clone(),
        declaration("inputs/left.png"),
        declaration("inputs/right.png"),
        "fixture",
        crate::CANONICAL_BRIDGE_COLOR,
        crate::BridgeBoundaries {
            left: crate::BoundaryPicture::AuthoredBlack {
                clock: crate::BoundaryClock::Project { frame: 0 },
            },
            right: crate::BoundaryPicture::AuthoredBlack {
                clock: crate::BoundaryClock::Project { frame: frames + 1 },
            },
        },
        geometry,
    )
    .unwrap();
    let sampled = object(4);
    let report = test_report(&plan, &sampled, &receipt, *context.geometry().unwrap());
    (plan, sampled, receipt, context, report)
}

#[test]
fn exact_sampled_endpoints_include_single_frame_and_fractional_clocks() {
    for frames in [1, 30] {
        let (plan, sampled, receipt, context, report) = fixture(frames);
        report.validate(&plan, &sampled, &receipt).unwrap();
        report.validate_context(&context).unwrap();
        assert_eq!(report.entry.sampled_frame, 0);
        assert_eq!(report.exit.sampled_frame, frames as u32 - 1);
        assert_eq!(report.exit.sampled_pts, if frames == 1 { 0 } else { 968 });
        let restored: BridgeEndpointReport =
            serde_json::from_slice(&serde_json::to_vec(&report).unwrap()).unwrap();
        assert_eq!(report, restored);
    }
}

#[test]
fn either_join_rejects_at_exact_gross_threshold_but_local_changes_do_not() {
    let (plan, sampled, receipt, _, mut report) = fixture(30);
    for entry in [true, false] {
        let mut bad = report.clone();
        let pair = if entry { &mut bad.entry } else { &mut bad.exit };
        pair.mean_absolute_rgb_difference = 64.0;
        pair.gross_cell_fraction = 0.75;
        let error = bad
            .validate(&plan, &sampled, &receipt)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(if entry {
                "gross entry discontinuity"
            } else {
                "gross exit discontinuity"
            }),
            "{error}"
        );
        pair_reset(&mut bad, entry, 63.999, 0.75);
        bad.validate(&plan, &sampled, &receipt).unwrap();
        pair_reset(&mut bad, entry, 64.0, 0.74999);
        bad.validate(&plan, &sampled, &receipt).unwrap();
    }
    report.entry.mean_absolute_rgb_difference = 127.5;
    report.entry.gross_cell_fraction = 0.5;
    report.validate(&plan, &sampled, &receipt).unwrap();
}

fn pair_reset(report: &mut BridgeEndpointReport, entry: bool, mean: f64, fraction: f64) {
    let pair = if entry {
        &mut report.entry
    } else {
        &mut report.exit
    };
    pair.mean_absolute_rgb_difference = mean;
    pair.gross_cell_fraction = fraction;
}

#[test]
fn endpoint_reports_bind_objects_geometry_clock_and_policy() {
    let (plan, sampled, receipt, context, report) = fixture(30);
    let mut mutations = Vec::new();
    let mut wrong = report.clone();
    wrong.sampled_object = object(9);
    mutations.push(wrong);
    let mut wrong = report.clone();
    wrong.context_object = object(9);
    mutations.push(wrong);
    let mut wrong = report.clone();
    wrong.left_object = object(9);
    mutations.push(wrong);
    let mut wrong = report.clone();
    wrong.right_object = object(9);
    mutations.push(wrong);
    let mut wrong = report.clone();
    wrong.exit.sampled_frame -= 1;
    mutations.push(wrong);
    let mut wrong = report.clone();
    wrong.exit.sampled_pts += 1;
    mutations.push(wrong);
    let mut wrong = report.clone();
    wrong.thresholds.gross_mean_difference += 1.0;
    mutations.push(wrong);
    let mut wrong = report.clone();
    wrong.geometry.presentation.x = u32::MAX;
    mutations.push(wrong);
    let mut wrong = report.clone();
    wrong.entry.mean_absolute_rgb_difference = f64::NAN;
    mutations.push(wrong);
    let mut wrong = report.clone();
    wrong.entry.gross_cell_fraction = f64::INFINITY;
    mutations.push(wrong);
    let mut wrong = report.clone();
    wrong.entry.gross_cell_fraction = 1.0;
    mutations.push(wrong);
    for wrong in mutations {
        assert!(
            wrong.validate(&plan, &sampled, &receipt).is_err(),
            "{wrong:?}"
        );
    }
    let mut other_crop = report.clone();
    other_crop.geometry.presentation = RasterRect::centered(32, 36, [64, 36]).unwrap();
    other_crop.validate(&plan, &sampled, &receipt).unwrap();
    assert!(other_crop.validate_context(&context).is_err());
    let mut unknown = serde_json::to_value(&report).unwrap();
    unknown["unrecognized"] = json!(true);
    assert!(serde_json::from_value::<BridgeEndpointReport>(unknown).is_err());
}
