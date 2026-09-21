use deadpan_core::{BridgeInterpolation, ExactRatio, FrameDuration, FrameRate};
use deadpan_jobs::{
    AxisLimits, BridgeCapability, BridgeGenerationPlan, ConditioningMode, DimensionLimits,
    FrameCountFormula, FrameInterpolation, GenerationPlanError, NativeDimensions,
};
use serde_json::json;

fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}

fn rate(numerator: u32, denominator: u32) -> FrameRate {
    FrameRate::new(numerator, denominator).unwrap()
}

fn capability(
    supported: bool,
    native_rate: FrameRate,
    formula: FrameCountFormula,
) -> BridgeCapability {
    BridgeCapability::new(
        supported,
        native_rate,
        formula,
        DimensionLimits::new(
            AxisLimits::new(256, 1_024, 64).unwrap(),
            AxisLimits::new(192, 1_024, 64).unwrap(),
        ),
    )
}

fn dimensions() -> NativeDimensions {
    NativeDimensions::new(512, 320).unwrap()
}

#[test]
fn fractional_rates_choose_nearest_legal_count_and_report_exact_timing() {
    let capability = capability(
        true,
        rate(30, 1),
        FrameCountFormula::new(8, 1, 25, 97).unwrap(),
    );
    let plan =
        BridgeGenerationPlan::new(duration(45), rate(30_000, 1_001), &capability, dimensions())
            .unwrap();

    assert_eq!(plan.native_frame_count(), 49);
    assert_eq!(
        plan.requested_boundary_duration(),
        ExactRatio::new(23_023, 15_000).unwrap()
    );
    assert_eq!(
        plan.actual_boundary_duration(),
        ExactRatio::new(8, 5).unwrap()
    );
    assert_eq!(
        plan.retime_deviation(),
        ExactRatio::new(977, 15_000).unwrap()
    );
    assert_eq!(plan.interpolation(), FrameInterpolation::Linear);
    plan.validate_for(&capability).unwrap();
}

#[test]
fn legal_rounding_uses_distance_and_breaks_exact_ties_upward() {
    let every_four = capability(
        true,
        rate(1, 1),
        FrameCountFormula::new(4, 1, 5, 21).unwrap(),
    );
    let tie =
        BridgeGenerationPlan::new(duration(5), rate(1, 1), &every_four, dimensions()).unwrap();
    assert_eq!(tie.native_frame_count(), 9); // ideal 7: tie between 5 and 9

    assert_eq!(
        BridgeGenerationPlan::new(duration(5), rate(2, 1), &every_four, dimensions()).unwrap_err(),
        GenerationPlanError::FrameCountOutsideCapability
    );

    let exact = capability(
        true,
        rate(1, 1),
        FrameCountFormula::new(4, 1, 1, 21).unwrap(),
    );
    let exact_plan =
        BridgeGenerationPlan::new(duration(4), rate(1, 1), &exact, dimensions()).unwrap();
    assert_eq!(exact_plan.native_frame_count(), 5);
}

#[test]
fn one_frame_maps_to_strict_native_midpoint() {
    let capability = capability(
        true,
        rate(4, 1),
        FrameCountFormula::new(1, 0, 2, 20).unwrap(),
    );
    let plan =
        BridgeGenerationPlan::new(duration(1), rate(1, 1), &capability, dimensions()).unwrap();
    assert_eq!(plan.native_frame_count(), 9);
    let samples: Vec<_> = plan.samples().collect();
    assert_eq!(samples.len(), 1);
    let sample = samples[0];
    assert_eq!(sample.native_position(), ExactRatio::integer(4));
    assert_eq!(sample.lower_index(), 4);
    assert_eq!(sample.upper_index(), 4);
    assert_eq!(sample.upper_weight(), ExactRatio::ZERO);
}

#[test]
fn samples_are_lazy_exact_and_exclude_both_conditioning_endpoints() {
    let capability = capability(
        true,
        rate(1, 1),
        FrameCountFormula::new(1, 0, 2, 20).unwrap(),
    );
    let plan =
        BridgeGenerationPlan::new(duration(3), rate(1, 1), &capability, dimensions()).unwrap();
    assert_eq!(plan.native_frame_count(), 5);
    let samples: Vec<_> = plan.samples().collect();
    assert_eq!(samples.len(), 3);
    for (sample, expected) in samples.iter().zip([1, 2, 3]) {
        assert_eq!(sample.native_position(), ExactRatio::integer(expected));
        assert!(sample.native_position().compare_integer(0).is_gt());
        assert!(
            sample
                .native_position()
                .compare_integer(i64::from(plan.native_frame_count() - 1))
                .is_lt()
        );
        assert_eq!(
            sample
                .lower_weight()
                .unwrap()
                .checked_add(sample.upper_weight())
                .unwrap(),
            ExactRatio::ONE
        );
    }
    assert_eq!(
        plan.sample(3).unwrap_err(),
        GenerationPlanError::SampleOutOfRange { index: 3, count: 3 }
    );
}

#[test]
fn rejects_unsupported_bridge_dimensions_and_out_of_capability_duration() {
    let formula = FrameCountFormula::new(8, 1, 25, 97).unwrap();
    let unsupported = capability(false, rate(30, 1), formula);
    assert_eq!(
        BridgeGenerationPlan::new(duration(45), rate(30, 1), &unsupported, dimensions())
            .unwrap_err(),
        GenerationPlanError::UnsupportedBridge
    );

    let supported = capability(true, rate(30, 1), formula);
    assert_eq!(
        BridgeGenerationPlan::for_conditioning(
            ConditioningMode::ExtendFromLeft,
            duration(45),
            rate(30, 1),
            &supported,
            dimensions(),
        )
        .unwrap_err(),
        GenerationPlanError::UnsupportedConditioning(ConditioningMode::ExtendFromLeft)
    );
    assert_eq!(
        BridgeGenerationPlan::new(
            duration(45),
            rate(30, 1),
            &supported,
            NativeDimensions::new(500, 320).unwrap(),
        )
        .unwrap_err(),
        GenerationPlanError::UnsupportedDimensions {
            width: 500,
            height: 320
        }
    );
    assert_eq!(
        BridgeGenerationPlan::new(duration(200), rate(30, 1), &supported, dimensions())
            .unwrap_err(),
        GenerationPlanError::FrameCountOutsideCapability
    );
}

#[test]
fn formula_and_dimension_limits_reject_invalid_bounds() {
    for result in [
        FrameCountFormula::new(0, 0, 1, 9),
        FrameCountFormula::new(4, 4, 1, 9),
        FrameCountFormula::new(4, 1, 10, 9),
        FrameCountFormula::new(8, 1, 2, 4),
    ] {
        assert!(result.is_err());
    }
    assert!(AxisLimits::new(0, 100, 64).is_err());
    assert!(AxisLimits::new(100, 99, 64).is_err());
    assert!(AxisLimits::new(1, 100, 0).is_err());
    assert_eq!(
        AxisLimits::new(65, 127, 64).unwrap_err(),
        GenerationPlanError::InvalidDimensionLimits
    );
    assert_eq!(
        AxisLimits::new(u32::MAX, u32::MAX, 2).unwrap_err(),
        GenerationPlanError::InvalidDimensionLimits
    );
}

#[test]
fn serialization_is_strict_and_revalidates_derived_fields() {
    let capability = capability(
        true,
        rate(30, 1),
        FrameCountFormula::new(8, 1, 25, 97).unwrap(),
    );
    let plan =
        BridgeGenerationPlan::new(duration(45), rate(30, 1), &capability, dimensions()).unwrap();
    let value = serde_json::to_value(&plan).unwrap();
    assert_eq!(
        value,
        json!({
            "schema_version": 1,
            "operation": "bridge",
            "interpolation": "linear",
            "project": {
                "interior_frames": 45,
                "frame_rate": {"numerator": 30, "denominator": 1}
            },
            "native": {
                "frame_count": 49,
                "frame_rate": {"numerator": 30, "denominator": 1},
                "width": 512,
                "height": 320
            },
            "timing": {
                "requested_boundary_duration": {"numerator": "23", "denominator": "15"},
                "actual_boundary_duration": {"numerator": "8", "denominator": "5"},
                "retime_deviation": {"numerator": "1", "denominator": "15"}
            },
            "sampling": {"endpoint_policy": "interior_only"}
        })
    );
    let decoded: BridgeGenerationPlan = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(decoded, plan);

    let mut corrupt = value.clone();
    corrupt["timing"]["actual_boundary_duration"] = json!({"numerator":"999", "denominator":"1"});
    assert!(serde_json::from_value::<BridgeGenerationPlan>(corrupt).is_err());

    let mut unknown = value.clone();
    unknown["native"]["surprise"] = json!(true);
    assert!(serde_json::from_value::<BridgeGenerationPlan>(unknown).is_err());

    let mut version = value;
    version["schema_version"] = json!(2);
    assert!(serde_json::from_value::<BridgeGenerationPlan>(version).is_err());

    let mut overflow = serde_json::to_value(&plan).unwrap();
    overflow["timing"]["requested_boundary_duration"]["numerator"] =
        json!("170141183460469231731687303715884105728");
    assert!(serde_json::from_value::<BridgeGenerationPlan>(overflow).is_err());
}

#[test]
fn deserialized_plan_can_be_checked_against_selected_capability() {
    let selected = capability(
        true,
        rate(30, 1),
        FrameCountFormula::new(8, 1, 25, 97).unwrap(),
    );
    let plan =
        BridgeGenerationPlan::new(duration(45), rate(30, 1), &selected, dimensions()).unwrap();
    let wrong_formula = capability(
        true,
        rate(30, 1),
        FrameCountFormula::new(8, 3, 25, 99).unwrap(),
    );
    assert_eq!(
        plan.validate_for(&wrong_formula).unwrap_err(),
        GenerationPlanError::UnsupportedNativeFrameCount(49)
    );
    let wrong_rate = capability(
        true,
        rate(24, 1),
        FrameCountFormula::new(8, 1, 25, 97).unwrap(),
    );
    assert_eq!(
        plan.validate_for(&wrong_rate).unwrap_err(),
        GenerationPlanError::NativeFrameRateMismatch
    );

    let mut non_nearest = serde_json::to_value(&plan).unwrap();
    non_nearest["native"]["frame_count"] = json!(41);
    non_nearest["timing"]["actual_boundary_duration"] =
        json!({"numerator": "4", "denominator": "3"});
    non_nearest["timing"]["retime_deviation"] = json!({"numerator": "-1", "denominator": "5"});
    let non_nearest: BridgeGenerationPlan = serde_json::from_value(non_nearest).unwrap();
    assert_eq!(
        non_nearest.validate_for(&selected).unwrap_err(),
        GenerationPlanError::NativeFrameCountNotNearest {
            actual: 41,
            expected: 49,
        }
    );
}

#[test]
fn huge_requests_fail_boundedly_without_sample_allocation() {
    let bounded = capability(
        true,
        rate(u32::MAX, 1),
        FrameCountFormula::new(1, 0, 2, u32::MAX).unwrap(),
    );
    assert_eq!(
        BridgeGenerationPlan::new(
            duration(i64::MAX),
            rate(1, u32::MAX),
            &bounded,
            dimensions()
        )
        .unwrap_err(),
        GenerationPlanError::FrameCountOutsideCapability
    );

    let enormous_output = capability(
        true,
        rate(4, u32::MAX),
        FrameCountFormula::new(1, 0, 2, u32::MAX).unwrap(),
    );
    let plan = BridgeGenerationPlan::new(
        duration(i64::MAX),
        rate(u32::MAX, 1),
        &enormous_output,
        dimensions(),
    )
    .unwrap();
    let first = plan.samples().next().unwrap();
    assert_eq!(first.output_index(), 0);
}

#[test]
fn sampling_map_preserves_exact_plan_contract() {
    let plan = BridgeGenerationPlan::new(
        duration(45),
        rate(30_000, 1_001),
        &capability(
            true,
            rate(24, 1),
            FrameCountFormula::new(1, 0, 2, 97).unwrap(),
        ),
        dimensions(),
    )
    .unwrap();
    let map = plan.sampling_map().unwrap();
    assert_eq!(map.project_rate(), plan.project_frame_rate());
    assert_eq!(map.native_rate(), plan.native_frame_rate());
    assert_eq!(
        map.native_frame_count().frames(),
        i64::from(plan.native_frame_count())
    );
    assert_eq!(map.output_frame_count(), plan.project_frames());
    assert_eq!(
        map.interpolation(),
        BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp
    );
}
