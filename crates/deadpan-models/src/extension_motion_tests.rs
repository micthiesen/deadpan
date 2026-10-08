use deadpan_core::{ExtensionDirection, FrameDuration, GeneratedContentId};
use deadpan_jobs::{
    AxisLimits, DimensionLimits, ExtensionCapability, FrameCountFormula, NativeDimensions,
};
use serde_json::json;

use super::*;

fn plan(direction: ExtensionDirection, output: i64, rate: FrameRate) -> ExtensionGenerationPlan {
    let capability = ExtensionCapability::new(
        FrameRate::new(24, 1).unwrap(),
        9,
        FrameCountFormula::new(8, 0, 8, 64).unwrap(),
        DimensionLimits::new(
            AxisLimits::new(64, 64, 1).unwrap(),
            AxisLimits::new(36, 36, 1).unwrap(),
        ),
        FrameDuration::new(1000).unwrap(),
    )
    .unwrap();
    ExtensionGenerationPlan::new(
        direction,
        FrameDuration::new(output).unwrap(),
        rate,
        &capability,
        NativeDimensions::new(64, 36).unwrap(),
    )
    .unwrap()
}

fn object() -> GeneratedObjectRef {
    GeneratedObjectRef::new(GeneratedContentId::new("a".repeat(64)).unwrap(), 256).unwrap()
}

fn grid(sample: impl Fn(usize, usize) -> u8) -> LumaGrid {
    let mut rgba = Vec::with_capacity(64 * 36 * 4);
    for y in 0..36 {
        for x in 0..64 {
            let value = sample(x, y);
            rgba.extend_from_slice(&[value, value, value, 255]);
        }
    }
    LumaGrid::from_rgba(&rgba, 64, 36, 64 * 4).unwrap()
}

fn texture(x: usize, y: usize) -> u8 {
    let mut value = (x as u32 + 1)
        .wrapping_mul(0x9e37_79b9)
        .wrapping_add((y as u32 + 1).wrapping_mul(0x85eb_ca6b));
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    32 + (value % 160) as u8
}

fn observations(plan: &ExtensionGenerationPlan) -> Vec<FrameObservation> {
    let interval = inspection_interval(plan).unwrap();
    let mut accumulator = Accumulator::new(interval.clone()).unwrap();
    for ordinal in interval {
        accumulator.push(ordinal, grid(|_, _| 100)).unwrap();
    }
    accumulator.finish().unwrap()
}

fn report(plan: &ExtensionGenerationPlan) -> ExtensionMotionReport {
    ExtensionMotionReport::new(plan, &object(), MotionAmount::Still, observations(plan)).unwrap()
}

#[test]
fn both_directions_retain_only_chronological_generated_pairs() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let plan = plan(direction, 8, FrameRate::new(24, 1).unwrap());
        let report = report(&plan);
        let interval = inspection_interval(&plan).unwrap();
        assert_eq!(report.transitions().len(), 7);
        assert_eq!(report.unavailable_motion_pairs(), 7);
        assert_eq!(report.measured_motion_pairs(), 0);
        assert_eq!(
            report.transitions().first().unwrap().after_frame,
            interval.start + 1
        );
        assert_eq!(
            report.transitions().last().unwrap().after_frame,
            interval.end - 1
        );
        let retained: ExtensionMotionReport =
            serde_json::from_slice(&serde_json::to_vec(&report).unwrap()).unwrap();
        retained
            .validate(&plan, &object(), MotionAmount::Still)
            .unwrap();
        assert_eq!(report, retained);
    }
}

#[test]
fn pair_clock_is_generated_centers_and_includes_a_single_output_frame() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        for (output, num, den, generated) in [
            (9, 30, 1, 8),
            (1, 30000, 1001, 8),
            (48, 60, 1, 16),
            (27, 24, 1, 24),
        ] {
            let plan = plan(direction, output, FrameRate::new(num, den).unwrap());
            assert_eq!(plan.generated_frame_count(), generated);
            let report = report(&plan);
            let expected = output as f64 * f64::from(den) / (f64::from(num) * f64::from(generated));
            assert!((report.pair_seconds() - expected).abs() < 1e-12);
            let bridge_spacing = (output as f64 + 1.0) * f64::from(den)
                / (f64::from(num) * f64::from(plan.native_frame_count() - 1));
            // With this fixture's K=9,E=8,N=1 the two formulas happen to
            // coincide. Other durations distinguish extension from Bridge.
            if output != 1 {
                assert!((report.pair_seconds() - bridge_spacing).abs() > 1e-6);
            }
        }
    }
}

#[test]
fn single_generated_picture_has_no_internal_observation_or_measured_motion() {
    // ExtensionSamplingMap allows E=1; current provider plans require E%8=0.
    // Exercise the provider-independent inspection rather than relaxing that rule.
    for interval in [0..1, 9..10] {
        let mut accumulator = Accumulator::new(interval.clone()).unwrap();
        assert!(
            Accumulator::new(interval.clone())
                .unwrap()
                .finish()
                .is_err()
        );
        accumulator.push(interval.start, grid(texture)).unwrap();
        let pairs = accumulator.finish().unwrap();
        assert!(pairs.is_empty());
        validate_coverage(&pairs, interval).unwrap();
        assert_eq!(
            pairs
                .iter()
                .filter(|pair| matches!(pair.motion, MotionObservation::Measured { .. }))
                .count(),
            0
        );
    }
    assert_eq!(
        pair_seconds(1, FrameRate::new(30, 1).unwrap(), 1),
        1.0 / 30.0
    );
}

#[test]
fn matching_ends_do_not_hide_a_generated_interior_flash() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let plan = plan(direction, 8, FrameRate::new(24, 1).unwrap());
        let interval = inspection_interval(&plan).unwrap();
        let flash = interval.start + 3;
        let mut accumulator = Accumulator::new(interval.clone()).unwrap();
        for ordinal in interval {
            accumulator
                .push(
                    ordinal,
                    grid(|_, _| if ordinal == flash { 180 } else { 100 }),
                )
                .unwrap();
        }
        let error = ExtensionMotionReport::new(
            &plan,
            &object(),
            MotionAmount::Still,
            accumulator.finish().unwrap(),
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains(&format!("abrupt lighting at native frame {flash}")),
            "{error}"
        );
    }
}

#[test]
fn matching_ends_do_not_hide_generated_interior_motion_or_short_output_speed() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let plan = plan(direction, 8, FrameRate::new(24, 1).unwrap());
        let interval = inspection_interval(&plan).unwrap();
        let moved = interval.start + 3;
        let mut accumulator = Accumulator::new(interval.clone()).unwrap();
        for ordinal in interval {
            accumulator
                .push(
                    ordinal,
                    grid(|x, y| {
                        if ordinal == moved {
                            x.checked_sub(2)
                                .map_or(112, |source_x| texture(source_x, y))
                        } else {
                            texture(x, y)
                        }
                    }),
                )
                .unwrap();
        }
        let pairs = accumulator.finish().unwrap();
        let error =
            ExtensionMotionReport::new(&plan, &object(), MotionAmount::Still, pairs.clone())
                .unwrap_err()
                .to_string();
        assert!(
            error.contains(&format!("excessive motion at native frame {moved}")),
            "{error}"
        );
        // Two pixels / 64 / (1/24 second) = 0.75/s, between Still and Subtle.
        let report =
            ExtensionMotionReport::new(&plan, &object(), MotionAmount::Subtle, pairs.clone())
                .unwrap();
        assert_eq!(report.measured_motion_pairs(), 7);
        let short = self::plan(direction, 1, FrameRate::new(24, 1).unwrap());
        assert!(
            ExtensionMotionReport::new(&short, &object(), MotionAmount::Moderate, pairs).is_err()
        );
    }
}

#[test]
fn generated_coverage_cannot_be_missing_reordered_extra_or_context() {
    let plan = plan(
        ExtensionDirection::FromLeft,
        8,
        FrameRate::new(24, 1).unwrap(),
    );
    let valid = report(&plan);
    let mut variants = Vec::new();
    let mut missing = valid.clone();
    missing.transitions.remove(2);
    variants.push(missing);
    let mut swapped = valid.clone();
    swapped.transitions.swap(1, 2);
    variants.push(swapped);
    let mut extra = valid.clone();
    extra.transitions.push(extra.transitions[0].clone());
    variants.push(extra);
    let mut context = valid.clone();
    context.transitions[0].after_frame = 8;
    variants.push(context);
    let mut fake_measured = valid.clone();
    fake_measured.transitions[0].motion = MotionObservation::Measured {
        maximum: 0.0,
        p95: 0.0,
    };
    variants.push(fake_measured);
    for changed in variants {
        assert!(
            changed
                .validate(&plan, &object(), MotionAmount::Still)
                .is_err()
        );
    }
    let mut accumulator = Accumulator::new(9..17).unwrap();
    assert!(accumulator.push(8, grid(texture)).is_err());
    accumulator.push(9, grid(texture)).unwrap();
    assert!(accumulator.push(11, grid(texture)).is_err());
    assert!(accumulator.finish().is_err());
}

#[test]
fn retained_report_rejects_altered_identity_clock_policy_and_thresholds() {
    let plan = plan(
        ExtensionDirection::FromRight,
        8,
        FrameRate::new(24, 1).unwrap(),
    );
    let valid = report(&plan);
    let mut variants = Vec::new();
    let mut schema = valid.clone();
    schema.schema_version = 2;
    variants.push(schema);
    let mut profile = valid.clone();
    profile.profile = "deadpan-motion-lighting-1".into();
    variants.push(profile);
    let mut object = valid.clone();
    object.native_object =
        GeneratedObjectRef::new(object.native_object.content().clone(), 257).unwrap();
    variants.push(object);
    let mut clock = valid.clone();
    clock.plan = self::plan(
        ExtensionDirection::FromRight,
        9,
        FrameRate::new(24, 1).unwrap(),
    );
    variants.push(clock);
    let mut direction = valid.clone();
    direction.plan = self::plan(
        ExtensionDirection::FromLeft,
        8,
        FrameRate::new(24, 1).unwrap(),
    );
    variants.push(direction);
    let mut motion = valid.clone();
    motion.motion = MotionAmount::Moderate;
    variants.push(motion);
    let mut thresholds = valid.clone();
    thresholds.thresholds.maximum_motion_per_second = 100.0;
    variants.push(thresholds);
    for changed in variants {
        assert!(
            changed
                .validate(&plan, &self::object(), MotionAmount::Still)
                .is_err()
        );
    }
    for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut report = valid.clone();
        report.transitions[0].mean_luma_shift = number;
        assert!(
            report
                .validate(&plan, &self::object(), MotionAmount::Still)
                .is_err()
        );
    }
}

#[test]
fn retained_json_rejects_unknown_fields_and_excessive_observation_count() {
    let plan = plan(
        ExtensionDirection::FromLeft,
        8,
        FrameRate::new(24, 1).unwrap(),
    );
    let valid = serde_json::to_value(report(&plan)).unwrap();
    let mut variants = Vec::new();
    let mut outer = valid.clone();
    outer["extra"] = json!(true);
    variants.push(outer);
    let mut pair = valid.clone();
    pair["transitions"][0]["extra"] = json!(true);
    variants.push(pair);
    let mut unavailable = valid.clone();
    unavailable["transitions"][0]["motion"]["maximum"] = json!(0);
    variants.push(unavailable);
    let mut threshold = valid.clone();
    threshold["thresholds"]["extra"] = json!(true);
    variants.push(threshold);
    let mut oversized = valid.clone();
    oversized["transitions"] = json!(vec![valid["transitions"][0].clone(); MAX_PAIRS + 1]);
    variants.push(oversized);
    let mut missing = valid.clone();
    missing.as_object_mut().unwrap().remove("native_object");
    variants.push(missing);
    for changed in variants {
        assert!(serde_json::from_value::<ExtensionMotionReport>(changed).is_err());
    }
    let mut at_limit = valid;
    at_limit["transitions"] = json!(vec![at_limit["transitions"][0].clone(); MAX_PAIRS]);
    let retained: ExtensionMotionReport = serde_json::from_value(at_limit).unwrap();
    assert!(
        retained
            .validate(&plan, &object(), MotionAmount::Still)
            .is_err()
    );
    assert!(Accumulator::new(0..1026).is_err());
    assert!(Accumulator::new(4095..4097).is_err());
    assert!(Accumulator::new(1..1).is_err());
}

#[test]
fn cancellation_and_deadline_are_distinct_terminal_failures() {
    let cancelled = AtomicBool::new(true);
    assert!(matches!(
        remaining(Instant::now() + Duration::from_secs(10), &cancelled),
        Err(QualificationError::Cancelled)
    ));
    cancelled.store(false, Ordering::Release);
    assert!(matches!(
        remaining(Instant::now(), &cancelled),
        Err(QualificationError::Deadline)
    ));
    assert!(remaining(Instant::now() + Duration::from_secs(10), &cancelled).is_ok());
}
