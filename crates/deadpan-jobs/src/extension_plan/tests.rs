use super::*;
use crate::AxisLimits;

fn frames(count: i64) -> FrameDuration {
    FrameDuration::new(count).unwrap()
}
fn rate() -> FrameRate {
    FrameRate::new(24, 1).unwrap()
}
fn dimensions() -> NativeDimensions {
    NativeDimensions::new(768, 320).unwrap()
}
fn dimensions_limit() -> DimensionLimits {
    DimensionLimits::new(
        AxisLimits::new(768, 768, 32).unwrap(),
        AxisLimits::new(320, 320, 32).unwrap(),
    )
}
fn capability(maximum: u32) -> ExtensionCapability {
    ExtensionCapability::new(
        rate(),
        9,
        FrameCountFormula::new(8, 0, 8, maximum).unwrap(),
        dimensions_limit(),
        frames(1000),
    )
    .unwrap()
}
fn plan(direction: ExtensionDirection, output: i64) -> ExtensionGenerationPlan {
    ExtensionGenerationPlan::new(
        direction,
        frames(output),
        rate(),
        &capability(32),
        dimensions(),
    )
    .unwrap()
}

#[test]
fn extension_plan_rounds_generated_count_with_shorter_ties_and_keeps_requested_time() {
    for (output, expected) in [
        (1, 8),
        (8, 8),
        (9, 8),
        (12, 8),
        (13, 16),
        (20, 16),
        (21, 24),
        (28, 24),
        (29, 32),
        (32, 32),
    ] {
        for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
            let plan = plan(direction, output);
            assert_eq!(plan.project_frames(), frames(output));
            assert_eq!(plan.generated_frame_count(), expected);
            assert_eq!(plan.context_frame_count(), 9);
            assert_eq!(plan.native_frame_count(), 9 + expected);
            assert_eq!(plan.generated_latent_frame_count(), expected / 8);
            assert_eq!(
                plan.requested_duration(),
                ExactRatio::new(i128::from(output), 24).unwrap()
            );
            assert_eq!(
                plan.generated_duration(),
                ExactRatio::new(i128::from(expected), 24).unwrap()
            );
            assert_eq!(
                plan.speed(),
                ExactRatio::new(i128::from(expected), i128::from(output)).unwrap()
            );
            plan.validate_for(&capability(32)).unwrap();
        }
    }
}

#[test]
fn extension_plan_reports_inserted_generated_context_and_movie_intervals_separately() {
    let plan = plan(ExtensionDirection::FromLeft, 12);
    assert_eq!(plan.requested_duration(), ExactRatio::new(1, 2).unwrap());
    assert_eq!(plan.generated_duration(), ExactRatio::new(1, 3).unwrap());
    assert_eq!(
        plan.native_movie_duration(),
        ExactRatio::new(17, 24).unwrap()
    );
    assert_eq!(plan.context_duration(), ExactRatio::new(3, 8).unwrap());
    assert_eq!(plan.context_anchor_span(), ExactRatio::new(1, 3).unwrap());
    assert_eq!(plan.speed(), ExactRatio::new(2, 3).unwrap());
    assert_eq!(plan.retime_deviation(), ExactRatio::new(-1, 6).unwrap());
    assert_eq!(plan.sampling_map().generated_interval(), 9..17);
    assert_eq!(plan.sampling_map().context_interval(), 0..9);
    assert_eq!(plan.sampling_map().output_interval(), 0..12);
    let single = self::plan(ExtensionDirection::FromRight, 1);
    assert_eq!(single.speed(), ExactRatio::integer(8));
    assert_eq!(
        single.sample(0).unwrap().native_position(),
        ExactRatio::new(7, 2).unwrap()
    );
    assert_eq!(single.sample(0).unwrap().lower_index(), 3);
    assert_eq!(single.sample(0).unwrap().upper_index(), 4);
    assert_eq!(
        single.sample(0).unwrap().upper_weight(),
        ExactRatio::new(1, 2).unwrap()
    );
}

#[test]
fn extension_plan_fractional_rates_choose_and_report_exactly() {
    let project_rate = FrameRate::new(30000, 1001).unwrap();
    let native_rate = FrameRate::new(24000, 1001).unwrap();
    let cap = ExtensionCapability::new(
        native_rate,
        9,
        FrameCountFormula::new(8, 0, 8, 32).unwrap(),
        dimensions_limit(),
        frames(100),
    )
    .unwrap();
    let plan = ExtensionGenerationPlan::new(
        ExtensionDirection::FromRight,
        frames(15),
        project_rate,
        &cap,
        dimensions(),
    )
    .unwrap();
    assert_eq!(plan.generated_frame_count(), 8); // Ideal twelve, exactly tied.
    assert_eq!(
        plan.requested_duration(),
        ExactRatio::new(1001, 2000).unwrap()
    );
    assert_eq!(
        plan.generated_duration(),
        ExactRatio::new(1001, 3000).unwrap()
    );
    assert_eq!(
        plan.native_movie_duration(),
        ExactRatio::new(17017, 24000).unwrap()
    );
    assert_eq!(
        plan.context_duration(),
        ExactRatio::new(3003, 8000).unwrap()
    );
    assert_eq!(
        plan.context_anchor_span(),
        ExactRatio::new(1001, 3000).unwrap()
    );
    assert_eq!(plan.speed(), ExactRatio::new(2, 3).unwrap());
}

#[test]
fn extension_plan_sampling_never_fetches_context_in_both_directions() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        for output in 1..=32 {
            let plan = plan(direction, output);
            let interval = plan.sampling_map().generated_interval();
            for j in 0..u64::try_from(output).unwrap() {
                let sample = plan.sample(j).unwrap();
                assert_eq!(sample.output_index(), j);
                assert!(interval.contains(&i64::from(sample.lower_index())));
                assert!(interval.contains(&i64::from(sample.upper_index())));
                assert_eq!(
                    sample
                        .lower_weight()
                        .unwrap()
                        .checked_add(sample.upper_weight())
                        .unwrap(),
                    ExactRatio::ONE
                );
                assert!(sample.upper_weight().compare_integer(0).is_ge());
                assert!(sample.upper_weight().compare_integer(1).is_lt());
            }
            assert_eq!(
                plan.sample(u64::try_from(output).unwrap()),
                Err(ExtensionPlanError::SampleOutOfRange)
            );
            assert_eq!(
                plan.sample(u64::MAX),
                Err(ExtensionPlanError::SampleOutOfRange)
            );
        }
    }
}

#[test]
fn extension_capability_enforces_duration_before_rounding_but_accepts_its_exact_limit() {
    let cap = capability(32);
    assert_eq!(
        cap.maximum_requested_duration().unwrap(),
        ExactRatio::new(4, 3).unwrap()
    );
    assert_eq!(cap.maximum_native_frame_count(), 41);
    for count in [39, 40] {
        let plan = ExtensionGenerationPlan::new(
            ExtensionDirection::FromLeft,
            frames(count),
            FrameRate::new(30, 1).unwrap(),
            &cap,
            dimensions(),
        )
        .unwrap();
        assert_eq!(plan.generated_frame_count(), 32);
    }
    assert_eq!(
        ExtensionGenerationPlan::new(
            ExtensionDirection::FromLeft,
            frames(41),
            FrameRate::new(30, 1).unwrap(),
            &cap,
            dimensions(),
        ),
        Err(ExtensionPlanError::DurationOutsideCapability)
    );
    let output_limited = ExtensionCapability::new(
        rate(),
        9,
        FrameCountFormula::new(8, 0, 8, 32).unwrap(),
        dimensions_limit(),
        frames(12),
    )
    .unwrap();
    assert_eq!(
        ExtensionGenerationPlan::new(
            ExtensionDirection::FromLeft,
            frames(13),
            rate(),
            &output_limited,
            dimensions(),
        ),
        Err(ExtensionPlanError::OutputCountOutsideCapability)
    );
    assert_eq!(
        ExtensionGenerationPlan::new(
            ExtensionDirection::FromLeft,
            frames(0),
            rate(),
            &cap,
            dimensions(),
        ),
        Err(ExtensionPlanError::ZeroProjectFrames)
    );
    assert_eq!(
        ExtensionGenerationPlan::new(
            ExtensionDirection::FromLeft,
            frames(8),
            rate(),
            &cap,
            NativeDimensions::new(800, 320).unwrap(),
        ),
        Err(ExtensionPlanError::UnsupportedDimensions)
    );
}

#[test]
fn extension_capability_uses_legal_counts_inside_unaligned_bounds() {
    let cap = ExtensionCapability::new(
        rate(),
        1,
        FrameCountFormula::new(8, 0, 9, 31).unwrap(),
        dimensions_limit(),
        frames(100),
    )
    .unwrap();
    assert_eq!(cap.maximum_requested_duration().unwrap(), ExactRatio::ONE);
    let minimum = ExtensionGenerationPlan::new(
        ExtensionDirection::FromLeft,
        frames(1),
        rate(),
        &cap,
        dimensions(),
    )
    .unwrap();
    assert_eq!(minimum.generated_frame_count(), 16);
    assert_eq!(minimum.context_anchor_span(), ExactRatio::ZERO);
    let maximum = ExtensionGenerationPlan::new(
        ExtensionDirection::FromLeft,
        frames(24),
        rate(),
        &cap,
        dimensions(),
    )
    .unwrap();
    assert_eq!(maximum.generated_frame_count(), 24);
    assert_eq!(
        ExtensionGenerationPlan::new(
            ExtensionDirection::FromLeft,
            frames(25),
            rate(),
            &cap,
            dimensions()
        ),
        Err(ExtensionPlanError::DurationOutsideCapability)
    );
}

#[test]
fn extension_provider_rules_are_stricter_than_core_operation_facts() {
    for context in [0, 2, 8, 10] {
        assert_eq!(
            ExtensionCapability::new(
                rate(),
                context,
                FrameCountFormula::new(8, 0, 8, 32).unwrap(),
                dimensions_limit(),
                frames(10)
            ),
            Err(ExtensionPlanError::InvalidCapability)
        );
    }
    for formula in [
        FrameCountFormula::new(8, 1, 9, 33).unwrap(),
        FrameCountFormula::new(16, 0, 16, 32).unwrap(),
    ] {
        assert_eq!(
            ExtensionCapability::new(rate(), 9, formula, dimensions_limit(), frames(10)),
            Err(ExtensionPlanError::InvalidCapability)
        );
    }
    let mut wire = serde_json::to_value(plan(ExtensionDirection::FromLeft, 12)).unwrap();
    wire["sampling"]["context_frame_count"] = serde_json::json!(2);
    wire["sampling"]["generated_frame_count"] = serde_json::json!(3);
    assert!(serde_json::from_value::<ExtensionSamplingMap>(wire["sampling"].clone()).is_ok());
    assert!(serde_json::from_value::<ExtensionGenerationPlan>(wire).is_err());
}

#[test]
fn extension_plan_revalidates_capability_and_cannot_silently_change_operation() {
    let plan = plan(ExtensionDirection::FromLeft, 12);
    let different_context = ExtensionCapability::new(
        rate(),
        1,
        FrameCountFormula::new(8, 0, 8, 32).unwrap(),
        dimensions_limit(),
        frames(100),
    )
    .unwrap();
    assert_eq!(
        plan.validate_for(&different_context),
        Err(ExtensionPlanError::CapabilityMismatch)
    );
    let mut wire = serde_json::to_value(&plan).unwrap();
    // A legal native count is not sufficient: this request's nearest count is8.
    wire["sampling"]["generated_frame_count"] = serde_json::json!(16);
    let altered: ExtensionGenerationPlan = serde_json::from_value(wire).unwrap();
    assert_eq!(
        altered.validate_for(&capability(32)),
        Err(ExtensionPlanError::CapabilityMismatch)
    );
    assert_eq!(
        plan.sampling_map().policy(),
        deadpan_core::ExtensionSamplingPolicy::FrameCentersClamped
    );
}

#[test]
fn extension_plan_wire_is_strict_versioned_and_revalidated_without_a_model() {
    let plan = plan(ExtensionDirection::FromRight, 12);
    let wire = serde_json::to_value(&plan).unwrap();
    assert_eq!(wire["schema_version"], 1);
    assert_eq!(wire["operation"], "extension");
    assert_eq!(wire["sampling"]["direction"], "from_right");
    assert_eq!(
        serde_json::from_value::<ExtensionGenerationPlan>(wire.clone()).unwrap(),
        plan
    );
    for (field, value) in [
        ("schema_version", serde_json::json!(2)),
        ("operation", serde_json::json!("bridge")),
        ("extra", serde_json::json!(false)),
    ] {
        let mut invalid = wire.clone();
        invalid[field] = value;
        assert!(
            serde_json::from_value::<ExtensionGenerationPlan>(invalid).is_err(),
            "{field}"
        );
    }
    for (field, value) in [
        ("width", serde_json::json!(0)),
        ("height", serde_json::json!(4294967296_u64)),
        ("extra", serde_json::json!(1)),
    ] {
        let mut invalid = wire.clone();
        invalid["native_dimensions"][field] = value;
        assert!(
            serde_json::from_value::<ExtensionGenerationPlan>(invalid).is_err(),
            "{field}"
        );
    }
    let mut missing = wire;
    missing.as_object_mut().unwrap().remove("operation");
    assert!(serde_json::from_value::<ExtensionGenerationPlan>(missing).is_err());
}

#[test]
fn extension_plan_maximum_counts_and_extreme_rates_are_bounded_without_allocation() {
    let maximum_generated = u32::MAX / 8 * 8;
    let formula = FrameCountFormula::new(8, 0, 8, u32::MAX).unwrap();
    assert_eq!(
        ExtensionCapability::new(rate(), 9, formula, dimensions_limit(), frames(i64::MAX)),
        Err(ExtensionPlanError::InvalidCapability)
    );
    let cap =
        ExtensionCapability::new(rate(), 1, formula, dimensions_limit(), frames(i64::MAX)).unwrap();
    let plan = ExtensionGenerationPlan::new(
        ExtensionDirection::FromLeft,
        frames(i64::from(maximum_generated)),
        rate(),
        &cap,
        dimensions(),
    )
    .unwrap();
    assert_eq!(plan.native_frame_count(), maximum_generated + 1);
    assert_eq!(
        plan.sample(u64::from(maximum_generated - 1))
            .unwrap()
            .native_position(),
        ExactRatio::integer(i64::from(maximum_generated))
    );
    let mut wire = serde_json::to_value(&plan).unwrap();
    wire["sampling"]["context_frame_count"] = serde_json::json!(9);
    assert!(serde_json::from_value::<ExtensionGenerationPlan>(wire).is_err());
    let fast = FrameRate::new(u32::MAX, 1).unwrap();
    let slow = FrameRate::new(1, u32::MAX).unwrap();
    let cap =
        ExtensionCapability::new(fast, 1, formula, dimensions_limit(), frames(i64::MAX)).unwrap();
    assert_eq!(
        ExtensionGenerationPlan::new(
            ExtensionDirection::FromRight,
            frames(i64::MAX),
            slow,
            &cap,
            dimensions()
        ),
        Err(ExtensionPlanError::DurationOutsideCapability)
    );
}
