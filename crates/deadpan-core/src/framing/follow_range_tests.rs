use super::*;
use crate::framing_zoom::ZoomCurve;

fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}

fn recipe(curve: ZoomCurve) -> Framing {
    Framing {
        clock: FramingClock::OwnerOutput,
        value: FramingValue::Follow {
            target: crate::TargetId::new("moving-subject").unwrap(),
            scale: ratio(2, 1),
            fallback: FramingPose::new(ratio(1, 3), ratio(2, 7), ratio(1, 2)).unwrap(),
            centering: Default::default(),
            scale_transition: Some(FollowScaleTransition {
                factor: ratio(3, 1),
                curve,
                range: Some(FollowScaleRange {
                    start: ratio(1, 4),
                    end: ratio(3, 4),
                    from_factor: ratio(2, 1),
                }),
            }),
        },
    }
}

#[test]
fn ranged_follow_keeps_prefix_and_target_then_jumps_ramps_and_holds_both_bases() {
    for (curve, quarter) in [
        (ZoomCurve::Linear, ratio(9, 2)),
        (ZoomCurve::Smoothstep, ratio(69, 16)),
        (ZoomCurve::Step, ratio(4, 1)),
    ] {
        let value = recipe(curve);
        let at = |n, d| {
            let state = value
                .evaluate_with_follow_exact(ratio(n, d), ratio(8, 1))
                .unwrap();
            assert_eq!(state.follow.unwrap().target.as_str(), "moving-subject");
            assert_eq!(state.pose.center_x, ratio(1, 3));
            assert_eq!(state.pose.center_y, ratio(2, 7));
            assert_eq!(
                state.pose.scale,
                state
                    .follow
                    .unwrap()
                    .scale
                    .checked_div(ratio(4, 1))
                    .unwrap()
            );
            state.follow.unwrap().scale
        };
        assert_eq!(at(0, 1), ratio(2, 1));
        assert_eq!(at(3, 2), ratio(2, 1));
        assert_eq!(at(2, 1), ratio(4, 1));
        assert_eq!(at(3, 1), quarter);
        assert_eq!(at(6, 1), ratio(6, 1));
        assert_eq!(at(8, 1), ratio(6, 1));
        let wire = serde_json::to_string(&value).unwrap();
        assert_eq!(serde_json::from_str::<Framing>(&wire).unwrap(), value);
    }
}

#[test]
fn ranged_follow_retained_fractional_clock_and_owner_growth_preserve_every_sample() {
    let value = recipe(ZoomCurve::Smoothstep);
    let grown = value
        .prepend_owner_frames(
            FrameDuration::new(3).unwrap(),
            FrameDuration::new(8).unwrap(),
        )
        .unwrap();
    for n in 0..=32 {
        let local = ratio(n, 4);
        assert_eq!(
            value
                .evaluate_with_follow_exact(local, ratio(8, 1))
                .unwrap(),
            grown
                .evaluate_with_follow_exact(local.checked_add(ratio(3, 1)).unwrap(), ratio(14, 1))
                .unwrap()
        );
    }
    assert_eq!(
        grown.evaluate_exact(ratio(14, 1), ratio(14, 1)).unwrap(),
        value.evaluate_exact(ratio(8, 1), ratio(8, 1)).unwrap()
    );
    let mut fractional = value.clone();
    fractional.clock = FramingClock::RetainedOutput {
        offset: ratio(1, 3),
        duration: ratio(8, 3),
    };
    for n in 0..=6 {
        let current = ratio(n, 3);
        assert_eq!(
            fractional.evaluate_exact(current, ratio(3, 1)).unwrap(),
            value
                .evaluate_exact(
                    current
                        .checked_add(ratio(1, 3))
                        .unwrap()
                        .checked_mul(ratio(3, 1))
                        .unwrap(),
                    ratio(8, 1)
                )
                .unwrap()
        );
    }
}

#[test]
fn ranged_follow_uses_wide_progress_and_independent_round_even_scale_oracle() {
    let mut value = recipe(ZoomCurve::Linear);
    let FramingValue::Follow {
        scale,
        fallback,
        scale_transition,
        ..
    } = &mut value.value
    else {
        unreachable!()
    };
    *scale = ratio(3, 5);
    fallback.scale = ratio(5, 7);
    let transition = scale_transition.as_mut().unwrap();
    transition.factor = ratio(2, 1);
    transition.range = Some(FollowScaleRange {
        start: ratio(1, 3),
        end: ratio(2, 3),
        from_factor: ExactRatio::ONE,
    });
    // These products exceed i128. The exact progress is less than 1/2 by
    // 3/(2*MAX), far below half a Q32 unit, so it rounds to exactly 1/2.
    let state = value
        .evaluate_with_follow_exact(ratio(i128::MAX / 2, 1), ratio(i128::MAX, 1))
        .unwrap();
    let round = |n: i128, d: i128| {
        let whole = n / d;
        let rem = n % d;
        whole + i128::from(rem * 2 > d || (rem * 2 == d && whole % 2 != 0))
    };
    let q = i128::from(FRAMING_NUMERIC_SCALE);
    let oracle = |n, d| ratio(round(round(n * q, d) + round(2 * n * q, d), 2), q);
    assert_eq!(state.follow.unwrap().scale, oracle(3, 5));
    assert_eq!(state.pose.scale, oracle(5, 7));
}

#[test]
fn ranged_follow_validates_geometry_both_scale_branches_and_closed_wire() {
    let valid = recipe(ZoomCurve::Linear);
    for range in [
        FollowScaleRange {
            start: ratio(-1, 4),
            end: ratio(3, 4),
            from_factor: ExactRatio::ONE,
        },
        FollowScaleRange {
            start: ratio(3, 4),
            end: ratio(3, 4),
            from_factor: ExactRatio::ONE,
        },
        FollowScaleRange {
            start: ratio(3, 4),
            end: ratio(1, 4),
            from_factor: ExactRatio::ONE,
        },
        FollowScaleRange {
            start: ExactRatio::ZERO,
            end: ratio(5, 4),
            from_factor: ExactRatio::ONE,
        },
        FollowScaleRange {
            start: ratio(1, 1_000_003),
            end: ratio(3, 4),
            from_factor: ExactRatio::ONE,
        },
        FollowScaleRange {
            start: ExactRatio::ZERO,
            end: ratio(1_000_002, 1_000_003),
            from_factor: ExactRatio::ONE,
        },
        FollowScaleRange {
            start: ratio(1, 4),
            end: ratio(3, 4),
            from_factor: ExactRatio::ZERO,
        },
        FollowScaleRange {
            start: ratio(1, 4),
            end: ratio(3, 4),
            from_factor: ratio(40, 1),
        },
        FollowScaleRange {
            start: ratio(1, 4),
            end: ratio(3, 4),
            from_factor: ratio(1, 64),
        },
    ] {
        let mut invalid = valid.clone();
        let FramingValue::Follow {
            scale_transition, ..
        } = &mut invalid.value
        else {
            unreachable!()
        };
        scale_transition.as_mut().unwrap().range = Some(range);
        assert!(invalid.validate().is_err(), "{range:?}");
    }
    // Live 2*2 is valid while fallback 40*2 is not: from_factor must be
    // checked independently even when the terminal factor remains valid.
    let mut fallback = valid.clone();
    let FramingValue::Follow {
        fallback: base,
        scale_transition,
        ..
    } = &mut fallback.value
    else {
        unreachable!()
    };
    base.scale = ratio(40, 1);
    scale_transition.as_mut().unwrap().factor = ExactRatio::ONE;
    assert!(fallback.validate().is_err());
    let wire = serde_json::to_value(&valid).unwrap();
    let mut unknown = wire.clone();
    unknown["value"]["scale_transition"]["range"]["other"] = serde_json::json!(0);
    assert!(serde_json::from_value::<Framing>(unknown).is_err());
    let encoded = serde_json::to_string(&wire).unwrap();
    let start = serde_json::to_string(&ExactRatio::ZERO).unwrap();
    let duplicate = encoded.replace(
        "\"from_factor\":",
        &format!("\"start\":{start},\"from_factor\":"),
    );
    assert!(
        serde_json::from_str::<Framing>(&duplicate)
            .unwrap_err()
            .to_string()
            .contains("duplicate field")
    );
    assert_eq!(valid.record_count(), 3);
    let mut node = crate::BeatNode::sequence("ranged", Vec::new());
    node.framing = Some(valid);
    assert_eq!(
        validate_nodes(std::iter::repeat_n(&node, MAX_FRAMING_RECORDS / 3)).unwrap(),
        MAX_FRAMING_RECORDS - 1
    );
    assert!(validate_nodes(std::iter::repeat_n(&node, MAX_FRAMING_RECORDS / 3 + 1)).is_err());
}
