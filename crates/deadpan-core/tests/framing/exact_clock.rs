use super::*;

#[test]
fn derived_owner_extents_keep_all_curve_shapes_and_exact_segment_boundaries() {
    for curve in [
        FramingCurve::Step,
        FramingCurve::Linear,
        FramingCurve::Smoothstep,
        FramingCurve::Cubic {
            control1: pose(3),
            control2: pose(1),
        },
    ] {
        let framing = creep(curve);
        for ticks in 0..=60 {
            let local = ratio(ticks, 10);
            let expected = framing.evaluate(local, duration(6)).unwrap();
            for factor in [ratio(1, 7), ratio(3, 2), ratio(7, 3)] {
                assert_eq!(
                    framing
                        .evaluate_exact(
                            local.checked_mul(factor).unwrap(),
                            ExactRatio::integer(6).checked_mul(factor).unwrap(),
                        )
                        .unwrap(),
                    expected
                );
            }
        }
    }

    let framing = Framing {
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
    for (local, expected) in [(ratio(149, 100), 1), (ratio(3, 2), 2), (ratio(151, 100), 2)] {
        assert_eq!(
            framing.evaluate_exact(local, ratio(9, 2)).unwrap(),
            pose(expected)
        );
    }
    assert_eq!(
        framing.evaluate_exact(ratio(9, 2), ratio(9, 2)).unwrap(),
        pose(3)
    );
}

#[test]
fn exact_clock_rejects_invalid_extents_and_positions_for_static_and_moving_framing() {
    for framing in [
        creep(FramingCurve::Linear),
        Framing::static_pose(pose(1)).unwrap(),
    ] {
        for (local, extent) in [
            (ExactRatio::ZERO, ExactRatio::ZERO),
            (ExactRatio::ZERO, ratio(-1, 3)),
            (ratio(-1, 7), ratio(3, 2)),
            (ratio(16, 10), ratio(3, 2)),
            (ratio(i128::MAX, i128::MAX - 1), ExactRatio::ONE),
        ] {
            assert_eq!(
                framing.evaluate_exact(local, extent),
                Err(FramingError::TimeRange)
            );
        }
    }
}

#[test]
fn exact_clock_matches_independent_unbounded_fraction_oracles() {
    #[derive(serde::Deserialize)]
    struct Case {
        local: ExactRatio,
        duration: ExactRatio,
        start: ExactRatio,
        end: ExactRatio,
        progress: u64,
    }
    #[derive(serde::Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
        quotient_overflow_cases: usize,
        endpoint_overflow_cases: usize,
    }
    let fixture: Fixture =
        serde_json::from_str(include_str!("../fixtures/framing-clock-oracles.json")).unwrap();
    assert!(fixture.cases.len() >= 300);
    let mut quotient_overflows = 0;
    let mut endpoint_overflows = 0;
    for (index, case) in fixture.cases.into_iter().enumerate() {
        quotient_overflows += usize::from(case.local.checked_div(case.duration).is_err());
        endpoint_overflows += usize::from(case.duration.checked_mul(case.start).is_err());
        let mut segments = Vec::new();
        if case.start != ExactRatio::ZERO {
            segments.push(FramingSegment {
                end: case.start,
                pose: pose(1),
                curve: FramingCurve::Step,
            });
        }
        segments.push(FramingSegment {
            end: case.end,
            pose: pose(2),
            curve: FramingCurve::Linear,
        });
        if case.end != ExactRatio::ONE {
            segments.push(FramingSegment {
                end: ExactRatio::ONE,
                pose: pose(2),
                curve: FramingCurve::Step,
            });
        }
        let framing = Framing {
            value: FramingValue::Envelope {
                envelope: FramingEnvelope {
                    initial: pose(1),
                    segments,
                },
            },
        };
        assert_eq!(
            framing
                .evaluate_exact(case.local, case.duration)
                .unwrap()
                .scale,
            ratio(
                i128::from(FRAMING_NUMERIC_SCALE) + i128::from(case.progress),
                i128::from(FRAMING_NUMERIC_SCALE)
            ),
            "independent clock fixture {index}"
        );
    }
    assert!(fixture.quotient_overflow_cases > 0 && fixture.endpoint_overflow_cases > 0);
    assert!(quotient_overflows >= fixture.quotient_overflow_cases);
    assert!(endpoint_overflows >= fixture.endpoint_overflow_cases);
}
