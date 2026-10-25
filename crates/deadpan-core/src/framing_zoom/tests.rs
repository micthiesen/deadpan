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
        whole_owner: None,
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

#[test]
fn normalized_ranges_preserve_integral_builder_results_and_explicit_full_follow_windows() {
    let fallback = FramingPose::new(ratio(1, 4), ratio(3, 4), ratio(5, 4)).unwrap();
    let followed = Framing {
        clock: FramingClock::RetainedOutput {
            offset: ratio(-2, 1),
            duration: ratio(7, 1),
        },
        value: FramingValue::Follow {
            target: TargetId::new("moving").unwrap(),
            scale: ratio(3, 2),
            fallback,
            scale_transition: None,
        },
    };
    let static_pose = Framing::static_pose(fallback).unwrap();
    for entry in [None, Some(&static_pose), Some(&followed)] {
        for range in [(0, 10), (0, 8), (2, 10), (2, 8)] {
            for kind in [
                ZoomKind::Off,
                ZoomKind::Smash { scale: ratio(2, 1) },
                ZoomKind::Creep {
                    from: None,
                    to: ratio(12, 5),
                    curve: ZoomCurve::Linear,
                },
                ZoomKind::Creep {
                    from: Some(ratio(9, 5)),
                    to: ratio(12, 5),
                    curve: ZoomCurve::Smoothstep,
                },
            ] {
                let ordinary = Shot {
                    entry,
                    whole_owner: Some(WholeFollowOwner::Hold),
                    range: Some(range),
                    ..shot(None)
                };
                let exact = Shot {
                    entry,
                    whole_owner: Some(WholeFollowOwner::Hold),
                    ..shot(None)
                };
                assert_eq!(
                    build(kind, &Center::Keep, &ordinary),
                    build_normalized_range(
                        kind,
                        &Center::Keep,
                        &exact,
                        (
                            ratio(i128::from(range.0), 10),
                            ratio(i128::from(range.1), 10)
                        )
                    )
                );
            }
        }
    }
}

#[test]
fn normalized_follow_range_uses_fractional_owner_extent_without_recipe_frame_rounding() {
    let fallback = FramingPose::new(ratio(1, 4), ratio(3, 4), ratio(1, 2)).unwrap();
    let entry = Framing {
        clock: FramingClock::RetainedOutput {
            offset: ratio(-2, 1),
            duration: ratio(7, 1),
        },
        value: FramingValue::Follow {
            target: TargetId::new("moving").unwrap(),
            scale: ratio(2, 1),
            fallback,
            scale_transition: None,
        },
    };
    let owner = Shot {
        entry: Some(&entry),
        whole_owner: Some(WholeFollowOwner::Retime),
        frames: 4,
        ..shot(None)
    };
    let result = build_normalized_range(
        ZoomKind::Creep {
            from: Some(ratio(3, 1)),
            to: ratio(4, 1),
            curve: ZoomCurve::Linear,
        },
        &Center::Keep,
        &owner,
        (ratio(1, 3), ratio(2, 3)),
    )
    .unwrap()
    .unwrap();
    assert_eq!(result.clock, FramingClock::OwnerOutput);
    for (local, live, silent) in [
        (ratio(1, 1), ratio(2, 1), ratio(1, 2)),
        (ratio(7, 6), ratio(3, 1), ratio(3, 4)),
        (ratio(7, 4), ratio(7, 2), ratio(7, 8)),
        (ratio(7, 3), ratio(4, 1), ratio(1, 1)),
        (ratio(3, 1), ratio(4, 1), ratio(1, 1)),
    ] {
        let actual = result
            .evaluate_with_follow_exact(local, ratio(7, 2))
            .unwrap();
        assert_eq!(actual.follow.unwrap().0.as_str(), "moving");
        assert_eq!(actual.follow.unwrap().1, live);
        assert_eq!(actual.pose.scale, silent);
        assert_eq!(
            [actual.pose.center_x, actual.pose.center_y],
            [ratio(1, 4), ratio(3, 4)]
        );
    }
    let FramingValue::Follow {
        scale_transition: Some(transition),
        ..
    } = &result.value
    else {
        panic!("expected live Follow")
    };
    assert_eq!(transition.range.as_ref().unwrap().start, ratio(1, 3));
    assert_eq!(transition.range.as_ref().unwrap().end, ratio(2, 3));
}

#[test]
fn normalized_range_rejects_conflicting_clocks_invalid_bounds_and_unrepresentable_windows() {
    let kind = ZoomKind::Smash { scale: ratio(2, 1) };
    for window in [
        (ratio(-1, 4), ratio(1, 2)),
        (ratio(1, 2), ratio(1, 2)),
        (ratio(3, 4), ratio(1, 4)),
        (ratio(1, 4), ratio(5, 4)),
    ] {
        assert!(build_normalized_range(kind, &Center::Keep, &shot(None), window).is_err());
    }
    assert!(
        build_normalized_range(
            kind,
            &Center::Keep,
            &shot(Some((1, 2))),
            (ratio(1, 4), ratio(3, 4))
        )
        .is_err()
    );
    assert!(
        build_normalized_range(
            kind,
            &Center::Keep,
            &shot(None),
            (ratio(1, 1_000_003), ratio(1, 2))
        )
        .is_err()
    );
    let path = Framing::creep(
        FramingPose::identity(),
        FramingPose::new(ratio(1, 2), ratio(1, 2), ratio(2, 1)).unwrap(),
        FramingCurve::Linear,
    )
    .unwrap();
    assert!(
        build_normalized_range(
            kind,
            &Center::Keep,
            &Shot {
                entry: Some(&path),
                ..shot(None)
            },
            (ratio(1, 4), ratio(3, 4))
        )
        .is_err()
    );
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
            scale_transition: None,
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
        whole_owner: None,
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
            scale_transition: None,
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
            scale_transition: None,
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
        whole_owner: None,
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
            scale_transition: None,
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
        whole_owner: None,
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

#[test]
fn follow_creep_keeps_both_bases_and_requires_a_whole_admitted_owner() {
    let fallback = FramingPose::new(ratio(1, 3), ratio(2, 7), ratio(1, 2)).unwrap();
    let entry = Framing {
        clock: FramingClock::RetainedOutput {
            offset: ratio(-2, 1),
            duration: ratio(7, 1),
        },
        value: FramingValue::Follow {
            target: TargetId::new("speaker").unwrap(),
            scale: ratio(2, 1),
            fallback,
            scale_transition: None,
        },
    };
    let kind = ZoomKind::Creep {
        from: None,
        to: ratio(4, 1),
        curve: ZoomCurve::Linear,
    };
    for owner in [
        WholeFollowOwner::Source,
        WholeFollowOwner::Hold,
        WholeFollowOwner::Sequence,
        WholeFollowOwner::Repeat,
        WholeFollowOwner::Retime,
    ] {
        let admitted = Shot {
            entry: Some(&entry),
            whole_owner: Some(owner),
            ..shot(None)
        };
        let result = build(kind, &Center::Keep, &admitted).unwrap().unwrap();
        assert_eq!(result.clock, FramingClock::OwnerOutput);
        let value = result
            .evaluate_with_follow_exact(ratio(1, 1), ratio(4, 1))
            .unwrap();
        assert_eq!(value.follow.unwrap().1, ratio(5, 2));
        assert_eq!(value.pose.scale, ratio(5, 8));
        assert_eq!(
            [value.pose.center_x, value.pose.center_y],
            [ratio(1, 3), ratio(2, 7)]
        );
        let explicit = build(
            ZoomKind::Creep {
                from: Some(ratio(3, 1)),
                to: ratio(4, 1),
                curve: ZoomCurve::Linear,
            },
            &Center::Keep,
            &admitted,
        )
        .unwrap()
        .unwrap();
        let initial = explicit
            .evaluate_with_follow_exact(ExactRatio::ZERO, ratio(4, 1))
            .unwrap();
        assert_eq!(initial.follow.unwrap().1, ratio(3, 1));
        assert_eq!(initial.pose.scale, ratio(3, 4));
        for range in [Some((0, 10)), Some((1, 9))] {
            let ranged = build(kind, &Center::Keep, &Shot { range, ..admitted })
                .unwrap()
                .unwrap();
            assert_eq!(ranged.clock, FramingClock::OwnerOutput);
            assert!(matches!(
                ranged.value,
                FramingValue::Follow {
                    scale_transition: Some(crate::FollowScaleTransition { range: Some(_), .. }),
                    ..
                }
            ));
        }
        assert!(
            build(
                kind,
                &Center::Keep,
                &Shot {
                    whole_owner: None,
                    ..admitted
                }
            )
            .is_err()
        );
        assert!(
            build(
                kind,
                &Center::Keep,
                &Shot {
                    entry: Some(&result),
                    ..admitted
                }
            )
            .is_err()
        );
        assert!(
            build(
                ZoomKind::Smash { scale: ratio(3, 1) },
                &Center::Keep,
                &Shot {
                    entry: Some(&result),
                    ..admitted
                }
            )
            .is_err()
        );
        // Explicit target selection continues to author a sampled ordinary path.
        assert!(matches!(
            build(
                kind,
                &Center::Point([ratio(1, 2), ratio(1, 2)]),
                &Shot {
                    explicit: true,
                    entry: Some(&result),
                    ..admitted
                }
            )
            .unwrap()
            .unwrap()
            .value,
            FramingValue::Envelope { .. }
        ));
    }
}

#[test]
fn ranged_follow_creep_preserves_prefix_and_separate_bases_with_an_explicit_from_jump() {
    let entry = Framing {
        clock: FramingClock::RetainedOutput {
            offset: ratio(-7, 1),
            duration: ratio(50, 1),
        },
        value: FramingValue::Follow {
            target: TargetId::new("moving").unwrap(),
            scale: ratio(2, 1),
            fallback: FramingPose::new(ratio(1, 3), ratio(2, 7), ratio(1, 2)).unwrap(),
            scale_transition: None,
        },
    };
    let base = Shot {
        whole_owner: Some(WholeFollowOwner::Source),
        entry: Some(&entry),
        current: FramingPose::identity(),
        range: Some((2, 6)),
        frames: 8,
        explicit: false,
    };
    let kind = ZoomKind::Creep {
        from: Some(ratio(3, 1)),
        to: ratio(4, 1),
        curve: ZoomCurve::Linear,
    };
    let result = build(kind, &Center::Keep, &base).unwrap().unwrap();
    let at = |frame| {
        result
            .evaluate_with_follow_exact(ratio(frame, 1), ratio(8, 1))
            .unwrap()
    };
    for (frame, live) in [
        (0, ratio(2, 1)),
        (1, ratio(2, 1)),
        (2, ratio(3, 1)),
        (4, ratio(7, 2)),
        (6, ratio(4, 1)),
        (8, ratio(4, 1)),
    ] {
        let state = at(frame);
        assert_eq!(state.follow.unwrap().0.as_str(), "moving");
        assert_eq!(state.follow.unwrap().1, live);
        assert_eq!(state.pose.scale, live.checked_div(ratio(4, 1)).unwrap());
        assert_eq!(state.pose.center_x, ratio(1, 3));
        assert_eq!(state.pose.center_y, ratio(2, 7));
    }
    assert_eq!(result.clock, FramingClock::OwnerOutput);
    assert!(
        build(
            kind,
            &Center::Keep,
            &Shot {
                whole_owner: None,
                ..base
            }
        )
        .is_err()
    );
    // A path or a different center cannot replace the live context outside
    // this range. The ranged feature does not weaken those refusals.
    assert!(
        build(
            kind,
            &Center::Keep,
            &Shot {
                entry: Some(&result),
                ..base
            }
        )
        .is_err()
    );
    assert!(build(kind, &Center::Point([ratio(1, 2), ratio(1, 2)]), &base).is_err());
    assert!(build(ZoomKind::Off, &Center::Keep, &base).is_err());
    assert!(build(ZoomKind::Smash { scale: ratio(3, 1) }, &Center::Keep, &base).is_err());
    for range in [Some((0, 0)), Some((4, 4)), Some((6, 2)), Some((1, 9))] {
        assert!(build(kind, &Center::Keep, &Shot { range, ..base }).is_err());
    }
    assert!(
        build(
            kind,
            &Center::Keep,
            &Shot {
                frames: 1_000_003,
                range: Some((1, 2)),
                ..base
            }
        )
        .is_err()
    );
}
