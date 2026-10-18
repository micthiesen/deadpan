use super::*;

fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}

#[test]
fn zoom_intent_validates_scale_bounds_and_eased_curves() {
    assert!(ZoomKind::Off.validate().is_ok());
    for scale in [ratio(1, 64), default_scale(), ratio(64, 1)] {
        assert!(ZoomKind::Smash { scale }.validate().is_ok());
        for curve in [ZoomCurve::Linear, ZoomCurve::Smoothstep] {
            assert!(
                ZoomKind::Creep {
                    from: Some(scale),
                    to: scale,
                    curve
                }
                .validate()
                .is_ok()
            );
            assert!(
                ZoomKind::Creep {
                    from: None,
                    to: scale,
                    curve
                }
                .validate()
                .is_ok()
            );
        }
    }
    for scale in [ratio(-1, 1), ratio(0, 1), ratio(1, 65), ratio(65, 1)] {
        assert!(ZoomKind::Smash { scale }.validate().is_err());
        assert!(
            ZoomKind::Creep {
                from: Some(scale),
                to: ExactRatio::ONE,
                curve: ZoomCurve::Linear
            }
            .validate()
            .is_err()
        );
        assert!(
            ZoomKind::Creep {
                from: None,
                to: scale,
                curve: ZoomCurve::Smoothstep
            }
            .validate()
            .is_err()
        );
    }
    assert_eq!(
        ZoomKind::Creep {
            from: None,
            to: ExactRatio::ONE,
            curve: ZoomCurve::Step
        }
        .validate()
        .unwrap_err(),
        "A creep eases; use :zoom for a step change."
    );
}

#[test]
fn zoom_intent_wire_roundtrips_and_refuses_unknown_fields() {
    for kind in [
        ZoomKind::Off,
        ZoomKind::Smash {
            scale: default_scale(),
        },
        ZoomKind::Creep {
            from: Some(ExactRatio::ONE),
            to: default_scale(),
            curve: ZoomCurve::Smoothstep,
        },
        ZoomKind::Creep {
            from: None,
            to: default_scale(),
            curve: ZoomCurve::Linear,
        },
    ] {
        let mut value = serde_json::to_value(kind).unwrap();
        assert_eq!(
            serde_json::from_value::<ZoomKind>(value.clone()).unwrap(),
            kind
        );
        value
            .as_object_mut()
            .unwrap()
            .insert("unrecognized".into(), serde_json::Value::Bool(true));
        assert!(serde_json::from_value::<ZoomKind>(value).is_err());
    }
    for curve in [ZoomCurve::Step, ZoomCurve::Linear, ZoomCurve::Smoothstep] {
        let value = serde_json::to_value(curve).unwrap();
        assert!(value.is_string());
        assert_eq!(serde_json::from_value::<ZoomCurve>(value).unwrap(), curve);
    }
    assert!(serde_json::from_str::<ZoomCurve>("\"bezier\"").is_err());
}

fn shot(range: Option<(u64, u64)>) -> Shot<'static> {
    Shot {
        entry: None,
        current: FramingPose::identity(),
        frames: 10,
        range,
        explicit: false,
    }
}

fn approx(value: ExactRatio) -> f64 {
    value.numerator() as f64 / value.denominator() as f64
}

fn close(value: ExactRatio, expected: f64) -> bool {
    (approx(value) - expected).abs() < 1e-8
}

fn evaluate(framing: &Option<Framing>, frame: u64) -> FramingPose {
    framing.as_ref().map_or(FramingPose::identity(), |framing| {
        framing
            .evaluate(
                ratio(2 * i128::from(frame) + 1, 2),
                crate::FrameDuration::new(10).unwrap(),
            )
            .unwrap()
    })
}

#[test]
fn whole_beat_smash_follows_a_target_and_otherwise_keeps_a_static_pose() {
    let id = TargetId::new("target-1").unwrap();
    let current = FramingPose::new(ratio(2, 5), ratio(1, 2), ExactRatio::ONE).unwrap();
    let shot = Shot {
        current,
        ..shot(None)
    };
    let followed = build(
        ZoomKind::Smash {
            scale: ratio(27, 20),
        },
        &Center::Target {
            id: id.clone(),
            fixed: None,
        },
        &shot,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        followed.value,
        FramingValue::Follow {
            target: id,
            scale: ratio(27, 20),
            fallback: FramingPose {
                scale: ratio(27, 20),
                ..current
            },
        }
    );
    let fixed = build(
        ZoomKind::Smash { scale: ratio(2, 1) },
        &Center::Point([ratio(1, 4), ratio(3, 4)]),
        &shot,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        fixed.value,
        FramingValue::Static {
            pose: FramingPose::new(ratio(1, 4), ratio(3, 4), ratio(2, 1)).unwrap()
        }
    );
    assert_eq!(build(ZoomKind::Off, &Center::Keep, &shot).unwrap(), None);
}

#[test]
fn creeps_ease_from_the_current_pose_toward_a_target_center() {
    let framing = build(
        ZoomKind::Creep {
            from: Some(ExactRatio::ONE),
            to: ratio(7, 5),
            curve: ZoomCurve::Linear,
        },
        &Center::Target {
            id: TargetId::new("target-1").unwrap(),
            fixed: Some([ratio(3, 4), ratio(1, 2)]),
        },
        &shot(None),
    )
    .unwrap();
    assert!(close(evaluate(&framing, 0).scale, 1.02));
    assert!(close(evaluate(&framing, 9).scale, 1.38));
    assert!(close(evaluate(&framing, 9).center_x, 0.7375));
    // A target that is not visible here has no fixed center to creep to.
    assert!(
        build(
            ZoomKind::Creep {
                from: None,
                to: ratio(7, 5),
                curve: ZoomCurve::Linear
            },
            &Center::Target {
                id: TargetId::new("target-1").unwrap(),
                fixed: None
            },
            &shot(None),
        )
        .is_err()
    );
}

#[test]
fn ranged_framing_keeps_the_beat_outside_the_range() {
    let scale = ratio(27, 20);
    let smash = build(
        ZoomKind::Smash { scale },
        &Center::Keep,
        &shot(Some((3, 6))),
    )
    .unwrap();
    let scales: Vec<_> = (0..10).map(|frame| evaluate(&smash, frame).scale).collect();
    assert_eq!(
        scales,
        [1, 1, 1, 0, 0, 0, 1, 1, 1, 1].map(|unit| if unit == 1 { ExactRatio::ONE } else { scale })
    );
    // A creep over 4..8 holds before, eases, then holds its target.
    let creep = build(
        ZoomKind::Creep {
            from: None,
            to: ratio(2, 1),
            curve: ZoomCurve::Linear,
        },
        &Center::Keep,
        &shot(Some((4, 8))),
    )
    .unwrap();
    assert_eq!(evaluate(&creep, 3).scale, ExactRatio::ONE);
    assert!(close(evaluate(&creep, 4).scale, 1.125));
    assert!(close(evaluate(&creep, 7).scale, 1.875));
    assert_eq!(evaluate(&creep, 9).scale, ratio(2, 1));
    // A range that reaches the beat end needs no trailing segment.
    let tail = build(
        ZoomKind::Smash { scale },
        &Center::Keep,
        &shot(Some((5, 10))),
    )
    .unwrap();
    assert_eq!(evaluate(&tail, 4).scale, ExactRatio::ONE);
    assert_eq!(evaluate(&tail, 9).scale, scale);
    // A whole-beat range is the whole beat.
    assert_eq!(
        build(
            ZoomKind::Smash { scale },
            &Center::Keep,
            &shot(Some((0, 10)))
        )
        .unwrap(),
        build(ZoomKind::Smash { scale }, &Center::Keep, &shot(None)).unwrap(),
    );
}

#[test]
fn abrupt_return_steps_out_over_a_range_and_never_flattens_a_path() {
    let zoomed =
        Framing::static_pose(FramingPose::new(ratio(2, 5), ratio(1, 2), ratio(3, 2)).unwrap())
            .unwrap();
    let shot = Shot {
        entry: Some(&zoomed),
        current: FramingPose::new(ratio(2, 5), ratio(1, 2), ratio(3, 2)).unwrap(),
        frames: 10,
        range: Some((6, 10)),
        explicit: false,
    };
    let returned = build(ZoomKind::Off, &Center::Keep, &shot).unwrap();
    assert_eq!(evaluate(&returned, 5).scale, ratio(3, 2));
    assert_eq!(evaluate(&returned, 6), FramingPose::identity());
    assert_eq!(evaluate(&returned, 9), FramingPose::identity());
    let path = Framing::creep(
        FramingPose::identity(),
        FramingPose {
            scale: ratio(2, 1),
            ..FramingPose::identity()
        },
        FramingCurve::Smoothstep,
    )
    .unwrap();
    let shot = Shot {
        entry: Some(&path),
        ..shot
    };
    assert!(build(ZoomKind::Off, &Center::Keep, &shot).is_err());
    assert!(build(ZoomKind::Smash { scale: ratio(2, 1) }, &Center::Keep, &shot).is_err());
    // An unframed beat stepping out over a range is unchanged.
    let shot = Shot {
        entry: None,
        current: FramingPose::identity(),
        ..shot
    };
    assert_eq!(build(ZoomKind::Off, &Center::Keep, &shot).unwrap(), None);
}

#[test]
fn whole_beat_keep_retains_a_follow_and_refuses_to_flatten_a_path() {
    let id = TargetId::new("target-1").unwrap();
    let fallback = FramingPose::new(ratio(2, 5), ratio(1, 2), ratio(11, 10)).unwrap();
    let follow = Framing {
        clock: FramingClock::RetainedOutput {
            offset: ExactRatio::ONE,
            duration: ExactRatio::integer(12),
        },
        value: FramingValue::Follow {
            target: id.clone(),
            scale: ratio(11, 10),
            fallback,
        },
    };
    // The displayed pose is the target's center, not the fallback.
    let current = FramingPose::new(ratio(3, 5), ratio(1, 2), ratio(11, 10)).unwrap();
    let shot_on = |entry| Shot {
        entry: Some(entry),
        current,
        ..shot(None)
    };
    let smash = build(
        ZoomKind::Smash { scale: ratio(2, 1) },
        &Center::Keep,
        &shot_on(&follow),
    )
    .unwrap()
    .unwrap();
    assert_eq!(smash.clock, follow.clock);
    assert_eq!(
        smash.value,
        FramingValue::Follow {
            target: id,
            scale: ratio(2, 1),
            fallback: FramingPose {
                scale: ratio(2, 1),
                ..fallback
            },
        }
    );
    let creep = ZoomKind::Creep {
        from: None,
        to: ratio(2, 1),
        curve: ZoomCurve::Smoothstep,
    };
    assert_eq!(
        build(creep, &Center::Keep, &shot_on(&follow)).unwrap_err(),
        FOLLOW_CREEP_REFUSAL
    );
    // An explicit fixed center replaces the follow on request.
    assert!(matches!(
        build(
            creep,
            &Center::Point([ratio(1, 2), ratio(1, 2)]),
            &Shot {
                explicit: true,
                ..shot_on(&follow)
            }
        ),
        Ok(Some(Framing {
            value: FramingValue::Envelope { .. },
            ..
        }))
    ));
    let path = Framing::creep(
        FramingPose::identity(),
        FramingPose {
            scale: ratio(2, 1),
            ..FramingPose::identity()
        },
        FramingCurve::Smoothstep,
    )
    .unwrap();
    for kind in [ZoomKind::Smash { scale: ratio(3, 2) }, creep] {
        assert_eq!(
            build(kind, &Center::Keep, &shot_on(&path)).unwrap_err(),
            PATH_REFUSAL
        );
        // ,z resolving the selected target is not an explicit replacement.
        let target = Center::Target {
            id: TargetId::new("target-1").unwrap(),
            fixed: Some([ratio(1, 2), ratio(1, 2)]),
        };
        assert_eq!(
            build(kind, &target, &shot_on(&path)).unwrap_err(),
            PATH_REFUSAL
        );
        assert!(
            build(
                kind,
                &target,
                &Shot {
                    explicit: true,
                    ..shot_on(&path)
                }
            )
            .is_ok()
        );
    }
    assert_eq!(
        build(ZoomKind::Off, &Center::Keep, &shot_on(&path)).unwrap(),
        None
    );
}

#[test]
fn retained_clock_static_poses_are_ranged_like_any_static_pose() {
    let zoomed = FramingPose::new(ratio(2, 5), ratio(1, 2), ratio(3, 2)).unwrap();
    let retained = Framing {
        clock: FramingClock::RetainedOutput {
            offset: ratio(-3, 1),
            duration: ExactRatio::integer(7),
        },
        value: FramingValue::Static { pose: zoomed },
    };
    let shot = Shot {
        entry: Some(&retained),
        current: zoomed,
        frames: 10,
        range: Some((2, 5)),
        explicit: false,
    };
    let returned = build(ZoomKind::Off, &Center::Keep, &shot).unwrap();
    let scales: Vec<_> = (0..10)
        .map(|frame| evaluate(&returned, frame).scale)
        .collect();
    assert_eq!(scales[1], ratio(3, 2));
    assert_eq!(scales[2], ExactRatio::ONE);
    assert_eq!(scales[4], ExactRatio::ONE);
    assert_eq!(scales[5], ratio(3, 2));
    // Ranged edits on follows and paths name what they would replace.
    let follow = Framing {
        clock: FramingClock::OwnerOutput,
        value: FramingValue::Follow {
            target: TargetId::new("target-1").unwrap(),
            scale: ratio(3, 2),
            fallback: zoomed,
        },
    };
    assert_eq!(
        build(
            ZoomKind::Off,
            &Center::Keep,
            &Shot {
                entry: Some(&follow),
                ..shot
            }
        )
        .unwrap_err(),
        RANGED_FOLLOW_REFUSAL
    );
}

#[test]
fn ranges_beyond_the_progress_grid_refuse_with_the_frame_limit() {
    let shot = Shot {
        entry: None,
        current: FramingPose::identity(),
        frames: 1_000_003,
        range: Some((1, 2)),
        explicit: false,
    };
    let error = build(ZoomKind::Smash { scale: ratio(2, 1) }, &Center::Keep, &shot).unwrap_err();
    assert!(error.contains("one million frames"), "{error}");
    // Reducible progress within the grid is admitted.
    let shot = Shot {
        frames: 2_000_000,
        range: Some((500_000, 1_000_000)),
        ..shot
    };
    assert!(build(ZoomKind::Smash { scale: ratio(2, 1) }, &Center::Keep, &shot).is_ok());
}

#[test]
fn targets_are_sampled_where_a_step_lands_or_a_creep_arrives() {
    let smash = ZoomKind::Smash { scale: ratio(2, 1) };
    let creep = ZoomKind::Creep {
        from: None,
        to: ratio(2, 1),
        curve: ZoomCurve::Linear,
    };
    assert_eq!(sample_frame(smash, None, 10), None);
    assert_eq!(sample_frame(smash, Some((3, 6)), 10), Some(3));
    assert_eq!(sample_frame(creep, Some((3, 6)), 10), Some(5));
    assert_eq!(sample_frame(creep, None, 10), Some(9));
    assert_eq!(sample_frame(ZoomKind::Off, Some((3, 6)), 10), None);
}
