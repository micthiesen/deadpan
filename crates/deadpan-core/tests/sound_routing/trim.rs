use super::*;

fn allocation(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}
fn trim(start: i64, end: i64, i: i64, o: i64) -> RootSoundOperation {
    RootSoundOperation::Trim {
        range: allocation(start, end),
        in_frames: i,
        out_frames: o,
    }
}
fn keeps(operation: RootSoundOperation, extent: i64) -> Vec<(i64, i64, i64, i64, bool, bool)> {
    operation
        .projection(extent)
        .unwrap()
        .keeps()
        .map(|keep| {
            (
                keep.input.start().0,
                keep.input.end().0,
                keep.output.start().0,
                keep.output.end().0,
                keep.start_cut,
                keep.end_cut,
            )
        })
        .collect()
}

#[test]
fn trim_normal_form_has_literal_keeps_gaps_and_true_cut_flags() {
    assert_eq!(
        keeps(trim(10, 20, 0, 0), 40),
        vec![(0, 40, 0, 40, false, false)]
    );
    assert!(trim(10, 20, 0, 0).projection(40).unwrap().is_identity());
    assert_eq!(
        keeps(trim(10, 20, 2, 2), 40),
        vec![
            (0, 10, 0, 10, false, true),
            (12, 20, 10, 18, true, true),
            (20, 40, 20, 40, true, false)
        ]
    );
    assert!(!trim(10, 20, 2, 2).projection(40).unwrap().is_identity());
    assert_eq!(
        keeps(trim(10, 20, 0, -3), 40),
        vec![(0, 17, 0, 17, false, true), (20, 40, 17, 37, true, false)]
    );
    assert_eq!(
        keeps(trim(10, 20, 15, 0), 40),
        vec![(0, 10, 0, 10, false, true), (25, 40, 10, 25, true, false)]
    );
    assert_eq!(
        keeps(trim(10, 20, -2, 3), 40),
        vec![
            (0, 10, 0, 10, false, true),
            (10, 20, 12, 22, true, true),
            (20, 40, 25, 45, true, false)
        ]
    );
    assert_eq!(
        keeps(trim(10, 20, 2, -3), 40),
        vec![
            (0, 10, 0, 10, false, true),
            (12, 17, 10, 15, true, true),
            (20, 40, 15, 35, true, false)
        ]
    );
    let route = RootSoundRoute {
        recipe_extent: frames(40),
        recipe_grid: RootSoundGrid::root(FrameRate::new(30, 1).unwrap()),
        edits: vec![RootSoundEdit {
            grid: RootSoundGrid::root(FrameRate::new(30, 1).unwrap()),
            operation: trim(10, 20, -2, 3),
            cuts: Default::default(),
        }],
    }
    .compile()
    .unwrap();
    let SoundRouteNode::Ripple { map, .. } = &route.nodes()[1] else {
        panic!()
    };
    assert_eq!(
        map.nodes()
            .iter()
            .filter(|node| matches!(node, SoundRippleNode::Keep { .. }))
            .count(),
        3
    );
    assert_eq!(
        map.nodes()
            .iter()
            .filter(|node| matches!(node, SoundRippleNode::Gap { .. }))
            .count(),
        2
    );
    assert_eq!(map.output_extent(), ExactRatio::integer(45));
}

#[test]
fn every_scalar_edge_projection_matches_existing_operations_and_cut_flags() {
    for (t, u, d) in [(0, 10, 20), (5, 15, 20), (10, 20, 20)] {
        for amount in [1, 3, 9] {
            let pairs = [
                (
                    trim(t, u, amount, 0),
                    RootSoundOperation::Delete {
                        range: allocation(t, t + amount),
                    },
                ),
                (
                    trim(t, u, -amount, 0),
                    RootSoundOperation::Insert {
                        at: ProjectFrame(t),
                        duration: frames(amount),
                    },
                ),
                (
                    trim(t, u, 0, amount),
                    RootSoundOperation::Insert {
                        at: ProjectFrame(u),
                        duration: frames(amount),
                    },
                ),
                (
                    trim(t, u, 0, -amount),
                    RootSoundOperation::Delete {
                        range: allocation(u - amount, u),
                    },
                ),
            ];
            for (combined, scalar) in pairs {
                assert_eq!(
                    combined.projection(d).unwrap(),
                    scalar.projection(d).unwrap()
                );
            }
        }
    }
    // Legacy cuts at exterior boundaries retain their side, including a Gap-only replacement.
    assert_eq!(
        keeps(
            RootSoundOperation::Insert {
                at: ProjectFrame(0),
                duration: frames(2)
            },
            10
        ),
        vec![(0, 10, 2, 12, true, false)]
    );
    assert_eq!(
        keeps(
            RootSoundOperation::Insert {
                at: ProjectFrame(10),
                duration: frames(2)
            },
            10
        ),
        vec![(0, 10, 0, 10, false, true)]
    );
    assert_eq!(
        keeps(
            RootSoundOperation::Delete {
                range: allocation(0, 2)
            },
            10
        ),
        vec![(2, 10, 0, 8, true, false)]
    );
    assert_eq!(
        keeps(
            RootSoundOperation::Delete {
                range: allocation(8, 10)
            },
            10
        ),
        vec![(0, 8, 0, 8, false, true)]
    );
    assert_eq!(
        keeps(
            RootSoundOperation::Replace {
                range: allocation(2, 8),
                duration: frames(3)
            },
            10
        ),
        vec![(0, 2, 0, 2, false, true), (8, 10, 5, 7, true, false)]
    );
    assert!(
        keeps(
            RootSoundOperation::Replace {
                range: allocation(0, 10),
                duration: frames(3)
            },
            10
        )
        .is_empty()
    );
}

#[test]
fn trim_projection_checks_extents_but_not_cancelled_intermediate_arithmetic() {
    for extreme in [i64::MIN, i64::MAX] {
        assert_eq!(
            keeps(trim(10, 20, extreme, extreme), 100),
            vec![(0, 10, 0, 10, false, true), (20, 100, 20, 100, true, false)]
        );
    }
    assert!(trim(10, 20, 31, 0).projection(40).is_err()); // Required prefix cannot fit.
    assert!(trim(-1, 20, 0, 0).projection(40).is_err());
    assert!(trim(10, 41, 0, 0).projection(40).is_err());
    assert!(trim(10, 10, 0, 0).projection(40).is_err());
    assert_eq!(
        trim(0, 1, 0, 1).projection(i64::MAX).unwrap_err().code,
        DocumentErrorCode::TimingOverflow
    );
    let zero = trim(0, 10, 10, 0).projection(10).unwrap();
    assert_eq!(zero.output_duration(), FrameDuration::ZERO);
    assert!(zero.keeps().next().is_none());
    assert!(!zero.is_identity());
}

#[test]
fn repeated_three_keep_maps_obey_existing_total_arena_budget() {
    let rate = FrameRate::new(30, 1).unwrap();
    let edit = RootSoundEdit {
        grid: RootSoundGrid::root(rate),
        operation: trim(10, 20, -1, 1),
        cuts: Default::default(),
    };
    // Recipe + N * (one chronological Ripple + six map nodes).
    let count = (MAX_SOUND_ROUTE_NODES - 1) / 7;
    let mut route = RootSoundRoute {
        recipe_extent: frames(40),
        recipe_grid: RootSoundGrid::root(rate),
        edits: vec![edit; count],
    };
    assert!(count < MAX_ROOT_SOUND_EDITS);
    assert!(route.compile().is_ok());
    route.edits.push(edit);
    assert_eq!(
        route.compile().unwrap_err().code,
        DocumentErrorCode::LimitExceeded
    );
}

#[test]
fn trim_wire_is_closed_and_frozen_routes_refuse_even_identity_trim() {
    let original = fixture();
    let (_, mut transaction) = edit(&original, insert(&original, 35, "trim-equivalent"));
    for patch in [&mut transaction.forward, &mut transaction.inverse] {
        for change in patch.sound_routes.values_mut() {
            for route in change.before.iter_mut().chain(change.after.iter_mut()) {
                route.edits.last_mut().unwrap().operation = trim(10, 35, 0, 1);
            }
        }
    }
    let after = transaction.forward.apply(&original).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), original);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    for operation in [trim(10, 35, 0, 1), trim(10, 35, 0, 0)] {
        let mut wire = if matches!(operation, RootSoundOperation::Trim { out_frames: 1, .. }) {
            json!(&after)
        } else {
            json!(&original)
        };
        wire["sound_routes"] = json!({"sound":RootSoundRoute {recipe_extent:frames(100),recipe_grid:RootSoundGrid::root(original.presentation_basis().frame_rate),edits:vec![RootSoundEdit {grid:RootSoundGrid::root(original.presentation_basis().frame_rate),operation,cuts:Default::default()}]}});
        macro_rules! check {
            ($version:literal,$adapter:ident) => {{
                wire["schema_version"] = json!($version);
                let plain = wire.to_string();
                assert!($adapter::Document::from_json(&plain).is_err());
                let escaped = plain.replace("\"type\":\"trim\"", "\"ty\\u0070e\":\"trim\"");
                assert_ne!(plain, escaped);
                assert!($adapter::Document::from_json(&escaped).is_err());
                assert!(
                    $adapter::matches_edit(
                        &serde_json::to_string(&transaction).unwrap(),
                        &transaction
                    )
                    .is_err()
                );
            }};
        }
        check!(30, legacy_v30);
        check!(31, legacy_v31);
        check!(32, legacy_v32);
    }
    let value = json!(trim(10, 20, 2, 2));
    assert_eq!(
        serde_json::from_value::<RootSoundOperation>(value.clone()).unwrap(),
        trim(10, 20, 2, 2)
    );
    let mut extra = value.clone();
    extra["slip_frames"] = json!(0);
    assert!(serde_json::from_value::<RootSoundOperation>(extra).is_err());
    let mut null = value;
    null["in_frames"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<RootSoundOperation>(null).is_err());
}
