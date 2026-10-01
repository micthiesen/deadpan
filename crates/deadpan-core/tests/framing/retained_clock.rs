use super::*;

#[test]
fn retained_paths_hold_endpoints_without_retiming_any_curve() {
    for (curve, quarter) in [
        (FramingCurve::Step, ratio(1, 1)),
        (FramingCurve::Linear, ratio(5, 4)),
        (FramingCurve::Smoothstep, ratio(37, 32)),
        (
            FramingCurve::Cubic {
                control1: pose(1),
                control2: pose(1),
            },
            ratio(65, 64),
        ),
    ] {
        let original = creep(curve);
        let retained = original
            .prepend_owner_frames(duration(3), duration(10))
            .unwrap();
        assert_eq!(retained.value, original.value);
        assert_eq!(
            retained.clock,
            FramingClock::RetainedOutput {
                offset: ExactRatio::integer(-3),
                duration: ExactRatio::integer(10),
            }
        );
        // These expected values come from the curve polynomials at 1/4,
        // independently of either framing evaluator.
        assert_eq!(
            retained.evaluate(ratio(11, 2), duration(15)).unwrap().scale,
            quarter
        );
        for local in [ExactRatio::ZERO, ratio(5, 2), ExactRatio::integer(3)] {
            assert_eq!(retained.evaluate(local, duration(15)).unwrap(), pose(1));
        }
        for local in [
            ExactRatio::integer(13),
            ratio(27, 2),
            ExactRatio::integer(15),
        ] {
            assert_eq!(retained.evaluate(local, duration(15)).unwrap(), pose(2));
        }
        for numerator in 0..=80 {
            let old_local = ratio(numerator, 8);
            assert_eq!(
                retained
                    .evaluate(
                        old_local.checked_add(ExactRatio::integer(3)).unwrap(),
                        duration(15)
                    )
                    .unwrap(),
                original.evaluate(old_local, duration(10)).unwrap()
            );
        }
        // Retained clamping never permits a position outside the current owner.
        assert_eq!(
            retained.evaluate(ratio(-1, 2), duration(15)),
            Err(FramingError::TimeRange)
        );
        assert_eq!(
            retained.evaluate(ratio(31, 2), duration(15)),
            Err(FramingError::TimeRange)
        );
    }
}

#[test]
fn retained_clock_composes_prefixes_and_freezes_tail_growth() {
    let original = creep(FramingCurve::Linear);
    assert_eq!(original.clock, FramingClock::OwnerOutput);
    let tail = original
        .prepend_owner_frames(duration(0), duration(10))
        .unwrap();
    assert_eq!(
        tail.evaluate(ExactRatio::integer(5), duration(15))
            .unwrap()
            .scale,
        ratio(3, 2)
    );
    assert_ne!(
        tail.evaluate(ExactRatio::integer(5), duration(15)).unwrap(),
        original
            .evaluate(ExactRatio::integer(5), duration(15))
            .unwrap()
    );
    let successive = tail
        .prepend_owner_frames(duration(3), duration(15))
        .unwrap()
        .prepend_owner_frames(duration(2), duration(18))
        .unwrap();
    let combined = original
        .prepend_owner_frames(duration(5), duration(10))
        .unwrap();
    assert_eq!(successive, combined);
    assert_eq!(
        successive
            .prepend_owner_frames(duration(0), duration(20))
            .unwrap(),
        successive
    );
    assert_eq!(
        successive
            .evaluate(ExactRatio::integer(10), duration(22))
            .unwrap()
            .scale,
        ratio(3, 2)
    );
}

#[test]
fn retained_clock_keeps_fractional_domain_and_step_boundaries_exact() {
    let mut framing = Framing {
        clock: FramingClock::RetainedOutput {
            offset: ratio(-3, 2),
            duration: ratio(9, 2),
        },
        value: FramingValue::Envelope {
            envelope: FramingEnvelope {
                initial: pose(1),
                segments: vec![
                    FramingSegment {
                        end: ratio(1, 3),
                        pose: pose(2),
                        curve: FramingCurve::Step,
                    },
                    FramingSegment {
                        end: ExactRatio::ONE,
                        pose: pose(3),
                        curve: FramingCurve::Step,
                    },
                ],
            },
        },
    };
    for (local, expected) in [
        (ratio(299, 100), 1),
        (ratio(3, 1), 2),
        (ratio(301, 100), 2),
        (ratio(6, 1), 3),
    ] {
        assert_eq!(
            framing.evaluate_exact(local, ratio(13, 2)).unwrap(),
            pose(expected)
        );
    }
    framing = framing
        .prepend_owner_frames(duration(2), duration(7))
        .unwrap();
    assert_eq!(
        framing.clock,
        FramingClock::RetainedOutput {
            offset: ratio(-7, 2),
            duration: ratio(9, 2)
        }
    );
    assert_eq!(
        framing
            .evaluate(ExactRatio::integer(5), duration(9))
            .unwrap(),
        pose(2)
    );
}

#[test]
fn retained_clock_admission_and_arithmetic_fail_without_mutation() {
    let original = creep(FramingCurve::Linear);
    assert_eq!(
        original.prepend_owner_frames(duration(1), duration(0)),
        Err(FramingError::TimeRange)
    );
    assert_eq!(original.clock, FramingClock::OwnerOutput);
    let mut invalid = original.clone();
    invalid.clock = FramingClock::RetainedOutput {
        offset: ExactRatio::ZERO,
        duration: ExactRatio::ZERO,
    };
    assert_eq!(invalid.validate(), Err(FramingError::TimeRange));
    assert!(serde_json::from_value::<Framing>(serde_json::to_value(&invalid).unwrap()).is_err());
    invalid.clock = FramingClock::RetainedOutput {
        offset: ExactRatio::new(i128::MIN, 1).unwrap(),
        duration: ExactRatio::ONE,
    };
    let saved = invalid.clone();
    assert_eq!(
        invalid.prepend_owner_frames(duration(1), duration(10)),
        Err(FramingError::Overflow)
    );
    assert_eq!(invalid, saved);
    invalid.clock = FramingClock::RetainedOutput {
        offset: ExactRatio::new(i128::MAX, 1).unwrap(),
        duration: ExactRatio::ONE,
    };
    assert_eq!(
        invalid.evaluate(ExactRatio::ONE, duration(10)),
        Err(FramingError::Overflow)
    );

    let retained = original
        .prepend_owner_frames(duration(3), duration(10))
        .unwrap();
    let wire = serde_json::to_value(&retained).unwrap();
    assert_eq!(
        serde_json::from_value::<Framing>(wire.clone()).unwrap(),
        retained
    );
    let mut unknown = wire.clone();
    unknown["clock"]["extra"] = json!(true);
    assert!(serde_json::from_value::<Framing>(unknown).is_err());
    let mut negative = wire.clone();
    negative["clock"]["duration"] = serde_json::to_value(ratio(-1, 2)).unwrap();
    assert!(serde_json::from_value::<Framing>(negative).is_err());
    let mut unknown_type = wire;
    unknown_type["clock"]["type"] = json!("source_pts");
    assert!(serde_json::from_value::<Framing>(unknown_type).is_err());
    let ordinary = serde_json::to_value(&original).unwrap();
    assert!(ordinary.get("clock").is_none());
    assert_eq!(
        serde_json::from_value::<Framing>(ordinary.clone()).unwrap(),
        original
    );
    let mut explicit = ordinary;
    explicit["clock"] = json!({"type":"owner_output"});
    assert_eq!(
        serde_json::from_value::<Framing>(explicit.clone()).unwrap(),
        original
    );
    explicit["clock"]["offset"] = serde_json::to_value(ExactRatio::ZERO).unwrap();
    assert!(serde_json::from_value::<Framing>(explicit).is_err());
}

#[test]
fn retained_framing_round_trips_atomic_commands_and_exact_inverse_patches() {
    let before = edit(
        &document(),
        Command::SetFraming {
            node: id("hold"),
            framing: Some(creep(FramingCurve::Smoothstep)),
        },
    );
    let retained = before.nodes()[&id("hold")]
        .framing
        .as_ref()
        .unwrap()
        .prepend_owner_frames(duration(3), duration(8))
        .unwrap();
    let after = edit(
        &before,
        Command::SetFraming {
            node: id("hold"),
            framing: Some(retained.clone()),
        },
    );
    assert_eq!(
        after.nodes()[&id("hold")].framing.as_ref().unwrap(),
        &retained
    );
    let divided = split(&after, &id("hold"), 4, "retained-cut");
    let recipes: Vec<_> = divided
        .nodes()
        .values()
        .filter_map(|node| node.framing.as_ref())
        .collect();
    assert_eq!(recipes, vec![&retained, &retained]);
}
