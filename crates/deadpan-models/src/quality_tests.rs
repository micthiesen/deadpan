use deadpan_core::{FrameDuration, FrameRate};
use deadpan_jobs::{
    AxisLimits, BridgeCapability, DimensionLimits, FrameCountFormula, NativeDimensions,
};

use super::*;

fn plan(frames: i64) -> BridgeGenerationPlan {
    let capability = BridgeCapability::new(
        true,
        FrameRate::new(24, 1).unwrap(),
        FrameCountFormula::new(8, 1, 25, 97).unwrap(),
        DimensionLimits::new(
            AxisLimits::new(64, 64, 1).unwrap(),
            AxisLimits::new(36, 36, 1).unwrap(),
        ),
    );
    BridgeGenerationPlan::new(
        FrameDuration::new(frames).unwrap(),
        FrameRate::new(30, 1).unwrap(),
        &capability,
        NativeDimensions::new(64, 36).unwrap(),
    )
    .unwrap()
}

fn grid(flash: u8) -> LumaGrid {
    LumaGrid::from_rgba(&[flash, flash, flash, 255].repeat(64 * 36), 64, 36, 64 * 4).unwrap()
}

#[test]
fn lighting_flash_return_is_rejected_even_when_endpoints_match() {
    let plan = plan(30);
    let mut accumulator = Accumulator::new(&plan, MotionAmount::Still).unwrap();
    for frame in 0..plan.native_frame_count() {
        accumulator
            .push(grid(if frame == 12 { 180 } else { 100 }))
            .unwrap();
    }
    let error = accumulator
        .finish(&plan, MotionAmount::Still)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("abrupt lighting at native frame 12"),
        "{error}"
    );
    assert!(error.contains("80.000"), "{error}");
}

#[test]
fn textureless_pairs_remain_explicitly_unavailable() {
    let plan = plan(30);
    let mut accumulator = Accumulator::new(&plan, MotionAmount::Still).unwrap();
    for _ in 0..plan.native_frame_count() {
        accumulator.push(grid(100)).unwrap();
    }
    let report = accumulator.finish(&plan, MotionAmount::Still).unwrap();
    assert_eq!(
        report.unavailable_motion_pairs(),
        (plan.native_frame_count() - 1) as usize
    );
    let bytes = serde_json::to_vec(&report).unwrap();
    let retained: BridgeQualityReport = serde_json::from_slice(&bytes).unwrap();
    retained.validate(&plan, MotionAmount::Still).unwrap();
    assert_eq!(retained, report);
}

#[test]
fn motion_uses_authored_time_and_selected_motion_limit() {
    let plan = plan(30);
    let mut report = test_report(&plan, MotionAmount::Still);
    let spacing = 31.0 / (30.0 * f64::from(plan.native_frame_count() - 1));
    assert!((report.pair_seconds() - spacing).abs() < 1e-12);
    assert!((spacing - 1.0 / 24.0).abs() > 1e-3);
    report.transitions[8].textured_blocks = 36;
    report.transitions[8].matched_blocks = 36;
    // 0.75 normalized units/second, above Still and below Subtle.
    report.transitions[8].motion = MotionObservation::Measured {
        maximum: spacing * 0.75,
        p95: spacing * 0.75,
    };
    let error = report
        .validate(&plan, MotionAmount::Still)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("excessive motion at native frame 9"),
        "{error}"
    );
    report.motion = MotionAmount::Subtle;
    report.thresholds = QualityThresholds::for_motion(MotionAmount::Subtle);
    report.validate(&plan, MotionAmount::Subtle).unwrap();
}

#[test]
fn lighting_requires_coherent_shift_and_includes_the_limit() {
    let plan = plan(30);
    let mut report = test_report(&plan, MotionAmount::Moderate);
    report.transitions[0].mean_luma_shift = 32.0;
    report.transitions[0].mean_absolute_luma_change = 40.0;
    report.transitions[0].lighting_agreement_fraction = 0.49;
    report.validate(&plan, MotionAmount::Moderate).unwrap();
    report.transitions[0].lighting_agreement_fraction = 0.5;
    assert!(report.validate(&plan, MotionAmount::Moderate).is_err());
}

#[test]
fn incomplete_extra_and_nonfinite_measurements_fail_closed() {
    let plan = plan(30);
    let mut accumulator = Accumulator::new(&plan, MotionAmount::Still).unwrap();
    accumulator.push(grid(100)).unwrap();
    assert!(accumulator.finish(&plan, MotionAmount::Still).is_err());
    let mut accumulator = Accumulator::new(&plan, MotionAmount::Still).unwrap();
    for _ in 0..plan.native_frame_count() {
        accumulator.push(grid(100)).unwrap();
    }
    assert!(accumulator.push(grid(100)).is_err());
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut report = test_report(&plan, MotionAmount::Still);
        report.transitions[0].mean_luma_shift = value;
        assert!(report.validate(&plan, MotionAmount::Still).is_err());
    }
}

#[test]
fn edited_thresholds_or_inconsistent_coverage_cannot_admit_retained_report() {
    let plan = plan(30);
    let mut report = test_report(&plan, MotionAmount::Still);
    report.thresholds.maximum_motion_per_second = 100.0;
    assert!(report.validate(&plan, MotionAmount::Still).is_err());
    let mut report = test_report(&plan, MotionAmount::Still);
    report.transitions[0].motion = MotionObservation::Measured {
        maximum: 0.0,
        p95: 0.0,
    };
    assert!(report.validate(&plan, MotionAmount::Still).is_err());
}
