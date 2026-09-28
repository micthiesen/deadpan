use deadpan_core::*;
use serde_json::json;

fn number(value: i64) -> ExactRatio {
    ExactRatio::integer(value)
}
fn fraction(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn range(start: i64, end: i64) -> ExactFrameRange {
    ExactFrameRange {
        start: number(start),
        end: number(end),
    }
}
fn slice(destination: ExactFrameRange, recipe: Option<ExactFrameRange>) -> SoundRouteSlice {
    SoundRouteSlice {
        destination,
        recipe,
    }
}
fn query(route: &SoundRoute, range: ExactFrameRange) -> SoundRouteQuery {
    route
        .query(range, SoundRouteQueryLimits::default())
        .unwrap()
}
fn map(extent: i64, nodes: Vec<SoundRippleNode>) -> SoundRippleMap {
    SoundRippleMap::new(
        number(extent),
        u32::try_from(nodes.len() - 1).unwrap(),
        nodes,
    )
    .unwrap()
}

fn locate(map: &SoundRippleMap, at: ExactRatio, bias: InsertionBias) -> SoundRoutePoint {
    map.locate(at, bias, SoundRouteQueryLimits::default())
        .unwrap()
}

#[test]
fn point_lookup_returns_complete_leaves_with_explicit_boundary_bias() {
    let map = map(
        10,
        vec![
            SoundRippleNode::Keep { range: range(2, 4) },
            SoundRippleNode::Gap {
                duration: fraction(1, 2),
            },
            SoundRippleNode::Keep { range: range(6, 9) },
            SoundRippleNode::Sequence {
                parts: vec![0, 1, 2],
            },
        ],
    );
    let first = slice(range(0, 2), Some(range(2, 4)));
    let gap = slice(
        ExactFrameRange {
            start: number(2),
            end: fraction(5, 2),
        },
        None,
    );
    let last = slice(
        ExactFrameRange {
            start: fraction(5, 2),
            end: fraction(11, 2),
        },
        Some(range(6, 9)),
    );
    for bias in [InsertionBias::Left, InsertionBias::Right] {
        assert_eq!(locate(&map, fraction(1, 3), bias).slice, Some(first));
        assert_eq!(locate(&map, fraction(9, 4), bias).slice, Some(gap));
        assert_eq!(locate(&map, number(4), bias).slice, Some(last));
    }
    assert_eq!(locate(&map, number(0), InsertionBias::Left).slice, None);
    assert_eq!(
        locate(&map, number(0), InsertionBias::Right).slice,
        Some(first)
    );
    assert_eq!(
        locate(&map, number(2), InsertionBias::Left).slice,
        Some(first)
    );
    assert_eq!(
        locate(&map, number(2), InsertionBias::Right).slice,
        Some(gap)
    );
    assert_eq!(
        locate(&map, fraction(5, 2), InsertionBias::Left).slice,
        Some(gap)
    );
    assert_eq!(
        locate(&map, fraction(5, 2), InsertionBias::Right).slice,
        Some(last)
    );
    assert_eq!(
        locate(&map, fraction(11, 2), InsertionBias::Left).slice,
        Some(last)
    );
    assert_eq!(
        locate(&map, fraction(11, 2), InsertionBias::Right).slice,
        None
    );
    for at in [fraction(-1, 2), number(6)] {
        assert!(
            map.locate(at, InsertionBias::Right, Default::default())
                .is_err()
        );
    }
}

#[test]
fn adjacent_keeps_remain_separate_point_anchors_even_when_range_queries_merge_them() {
    let map = map(
        5,
        vec![
            SoundRippleNode::Keep { range: range(0, 2) },
            SoundRippleNode::Keep { range: range(2, 5) },
            SoundRippleNode::Sequence { parts: vec![0, 1] },
        ],
    );
    assert_eq!(
        locate(&map, number(2), InsertionBias::Left).slice,
        Some(slice(range(0, 2), Some(range(0, 2))))
    );
    assert_eq!(
        locate(&map, number(2), InsertionBias::Right).slice,
        Some(slice(range(2, 5), Some(range(2, 5))))
    );
    let route = SoundRoute::identity(number(5))
        .unwrap()
        .ripple(map)
        .unwrap();
    assert_eq!(
        query(&route, range(0, 5)).slices,
        [slice(range(0, 5), Some(range(0, 5)))]
    );
}

#[test]
fn chronological_point_anchors_retain_the_ntsc_sample_phase_that_flattening_loses() {
    let first = map(
        4,
        vec![
            SoundRippleNode::Gap {
                duration: number(1),
            },
            SoundRippleNode::Keep { range: range(0, 4) },
            SoundRippleNode::Sequence { parts: vec![0, 1] },
        ],
    );
    let second = map(
        5,
        vec![
            SoundRippleNode::Keep { range: range(0, 2) },
            SoundRippleNode::Gap {
                duration: number(1),
            },
            SoundRippleNode::Keep { range: range(2, 5) },
            SoundRippleNode::Sequence {
                parts: vec![0, 1, 2],
            },
        ],
    );
    let second_anchor = locate(&second, fraction(7, 2), InsertionBias::Right)
        .slice
        .unwrap();
    assert_eq!(second_anchor, slice(range(3, 6), Some(range(2, 5))));
    let first_anchor = locate(
        &first,
        second_anchor.recipe.unwrap().start,
        InsertionBias::Right,
    )
    .slice
    .unwrap();
    assert_eq!(first_anchor, slice(range(1, 5), Some(range(0, 4))));
    let samples_per_frame = fraction(8008, 5); // 48 kHz at 30000/1001 fps.
    let boundary = |frame: ExactRatio| {
        frame
            .checked_mul(samples_per_frame)
            .unwrap()
            .round_even()
            .unwrap()
    };
    let current = boundary(second_anchor.destination.start);
    let previous = boundary(second_anchor.recipe.unwrap().start) + current
        - boundary(second_anchor.destination.start);
    let original = boundary(first_anchor.recipe.unwrap().start) + previous
        - boundary(first_anchor.destination.start);
    assert_eq!((current, previous, original), (4805, 3203, 1601));
    assert_ne!(original, boundary(number(1)));

    let route = SoundRoute::identity(number(4))
        .unwrap()
        .ripple(first)
        .unwrap()
        .ripple(second)
        .unwrap()
        .window(ExactFrameRange {
            start: fraction(7, 2),
            end: number(6),
        })
        .unwrap();
    assert_eq!(
        (0..4).map(|id| route.node_extent(id)).collect::<Vec<_>>(),
        [
            Some(number(4)),
            Some(number(5)),
            Some(number(6)),
            Some(fraction(5, 2))
        ]
    );
    assert_eq!(route.node_extent(4), None);
    assert_eq!(route.node_extent(u32::MAX), None);
    assert_eq!(
        SoundRoute::from_json(&route.to_json().unwrap()).unwrap(),
        route
    );
}

#[test]
fn point_lookup_seeks_a_billion_repeat_and_selects_both_sides_of_its_seams() {
    let route = billion_route();
    let SoundRouteNode::Ripple { map, .. } = &route.nodes()[1] else {
        unreachable!()
    };
    let last = locate(map, number(1_999_999_998), InsertionBias::Right);
    assert_eq!(
        last.slice,
        Some(slice(
            range(1_999_999_998, 1_999_999_999),
            Some(range(2_999_999_997, 2_999_999_998))
        ))
    );
    assert_eq!(
        locate(map, number(1_999_999_998), InsertionBias::Left).slice,
        Some(slice(range(1_999_999_997, 1_999_999_998), None))
    );
    assert_eq!(
        locate(map, number(1_999_999_999), InsertionBias::Left).slice,
        last.slice
    );
    assert_eq!(
        locate(map, number(1_999_999_999), InsertionBias::Right).slice,
        Some(slice(range(1_999_999_999, 2_000_000_000), None))
    );
    assert_eq!(
        locate(map, number(2_000_000_000), InsertionBias::Left).slice,
        Some(slice(range(1_999_999_999, 2_000_000_000), None))
    );
    assert_eq!(
        locate(map, number(2_000_000_000), InsertionBias::Right).slice,
        None
    );
    assert_eq!(
        last.stats,
        locate(map, number(0), InsertionBias::Right).stats
    );
    assert!(last.stats.work < 20, "{:?}", last.stats);
}

#[test]
fn nested_gapless_repeat_point_lookup_accumulates_strides_at_exact_seams() {
    let map = map(
        100,
        vec![
            SoundRippleNode::Keep { range: range(1, 2) },
            SoundRippleNode::Repeat {
                body: 0,
                count: 3,
                input_stride: number(4),
            },
            SoundRippleNode::Repeat {
                body: 1,
                count: 5,
                input_stride: number(20),
            },
        ],
    );
    for (at, left, right) in [
        (
            3,
            slice(range(2, 3), Some(range(9, 10))),
            slice(range(3, 4), Some(range(21, 22))),
        ),
        (
            14,
            slice(range(13, 14), Some(range(85, 86))),
            slice(range(14, 15), Some(range(89, 90))),
        ),
    ] {
        assert_eq!(
            locate(&map, number(at), InsertionBias::Left).slice,
            Some(left)
        );
        assert_eq!(
            locate(&map, number(at), InsertionBias::Right).slice,
            Some(right)
        );
    }
}

#[test]
fn point_lookup_uses_sequence_indexes_and_compresses_zero_stride_gap_subtrees() {
    let mut nodes: Vec<_> = (0..1000)
        .map(|i| SoundRippleNode::Keep {
            range: range(i * 2, i * 2 + 1),
        })
        .collect();
    nodes.push(SoundRippleNode::Sequence {
        parts: (0..1000).collect(),
    });
    let indexed = map(2000, nodes);
    let last = locate(&indexed, number(999), InsertionBias::Right);
    assert_eq!(
        last.slice,
        Some(slice(range(999, 1000), Some(range(1998, 1999))))
    );
    assert!(last.stats.work < 20, "{:?}", last.stats);

    let gaps = map(
        1,
        vec![
            SoundRippleNode::Gap {
                duration: fraction(1, 10_000),
            },
            SoundRippleNode::Repeat {
                body: 0,
                count: 1_000_000_000,
                input_stride: number(0),
            },
        ],
    );
    let compressed = gaps
        .locate(
            fraction(1, 10_000),
            InsertionBias::Left,
            SoundRouteQueryLimits {
                maximum_work: 2,
                maximum_spans: 1,
            },
        )
        .unwrap();
    assert_eq!(compressed.slice, Some(slice(range(0, 100_000), None)));
    assert_eq!(
        compressed.stats,
        SoundRouteQueryStats {
            work: 2,
            node_visits: 1
        }
    );
    assert_eq!(
        compressed,
        locate(&gaps, number(99_999), InsertionBias::Right)
    );
}

#[test]
fn a_gap_with_zero_rounded_samples_still_has_its_exact_point_interval() {
    let seam = number(1).checked_add(fraction(1, 10_000)).unwrap();
    let map = map(
        2,
        vec![
            SoundRippleNode::Keep { range: range(0, 1) },
            SoundRippleNode::Gap {
                duration: fraction(1, 10_000),
            },
            SoundRippleNode::Keep { range: range(1, 2) },
            SoundRippleNode::Sequence {
                parts: vec![0, 1, 2],
            },
        ],
    );
    let gap = Some(slice(
        ExactFrameRange {
            start: number(1),
            end: seam,
        },
        None,
    ));
    assert_eq!(locate(&map, number(1), InsertionBias::Right).slice, gap);
    assert_eq!(locate(&map, seam, InsertionBias::Left).slice, gap);
    assert_eq!(
        locate(&map, seam, InsertionBias::Right).slice,
        Some(slice(
            ExactFrameRange {
                start: seam,
                end: seam.checked_add(number(1)).unwrap()
            },
            Some(range(1, 2))
        ))
    );
    assert_eq!(
        number(1)
            .checked_mul(fraction(8008, 5))
            .unwrap()
            .round_even()
            .unwrap(),
        seam.checked_mul(fraction(8008, 5))
            .unwrap()
            .round_even()
            .unwrap()
    );
}

#[test]
fn point_lookup_limits_are_shared_inclusive_and_leave_the_map_unchanged() {
    let route = billion_route();
    let SoundRouteNode::Ripple { map, .. } = &route.nodes()[1] else {
        unreachable!()
    };
    let before = map.to_json().unwrap();
    let expected = locate(map, number(2), InsertionBias::Right);
    assert_eq!(
        map.locate(
            number(2),
            InsertionBias::Right,
            SoundRouteQueryLimits {
                maximum_work: expected.stats.work,
                maximum_spans: 1,
            }
        )
        .unwrap(),
        expected
    );
    for limits in [
        SoundRouteQueryLimits {
            maximum_work: expected.stats.work - 1,
            maximum_spans: 1,
        },
        SoundRouteQueryLimits {
            maximum_work: 0,
            maximum_spans: 1,
        },
        SoundRouteQueryLimits {
            maximum_work: MAX_SOUND_ROUTE_QUERY_WORK + 1,
            maximum_spans: 1,
        },
        SoundRouteQueryLimits {
            maximum_work: 100,
            maximum_spans: 0,
        },
        SoundRouteQueryLimits {
            maximum_work: 100,
            maximum_spans: MAX_SOUND_ROUTE_QUERY_SPANS + 1,
        },
    ] {
        assert_eq!(
            map.locate(number(2), InsertionBias::Right, limits)
                .unwrap_err()
                .code,
            DocumentErrorCode::LimitExceeded
        );
    }
    assert_eq!(map.to_json().unwrap(), before);
    assert_eq!(locate(map, number(2), InsertionBias::Right), expected);
}

#[test]
fn deleting_the_start_keeps_the_source_suffix_and_complete_recipe() {
    let original = SoundRoute::identity(number(12)).unwrap();
    let edited = original
        .ripple(map(
            12,
            vec![SoundRippleNode::Keep {
                range: range(7, 12),
            }],
        ))
        .unwrap();
    assert_eq!(edited.recipe_extent(), number(12));
    assert_eq!(edited.output_extent(), number(5));
    assert_eq!(
        query(&edited, range(0, 5)).slices,
        [slice(range(0, 5), Some(range(7, 12)))]
    );
    assert_eq!(
        query(&original, range(0, 12)).slices,
        [slice(range(0, 12), Some(range(0, 12)))]
    );
}

#[test]
fn composing_delete_insert_and_fractional_window_preserves_exact_phase_coordinates() {
    let original = SoundRoute::identity(number(12)).unwrap();
    let deleted = original
        .ripple(map(
            12,
            vec![
                SoundRippleNode::Keep { range: range(0, 4) },
                SoundRippleNode::Keep {
                    range: range(7, 12),
                },
                SoundRippleNode::Sequence { parts: vec![0, 1] },
            ],
        ))
        .unwrap();
    let inserted = deleted
        .ripple(map(
            9,
            vec![
                SoundRippleNode::Keep { range: range(0, 5) },
                SoundRippleNode::Gap {
                    duration: number(3),
                },
                SoundRippleNode::Keep { range: range(5, 9) },
                SoundRippleNode::Sequence {
                    parts: vec![0, 1, 2],
                },
            ],
        ))
        .unwrap();
    assert_eq!(
        query(&inserted, range(0, 12)).slices,
        [
            slice(range(0, 4), Some(range(0, 4))),
            slice(range(4, 5), Some(range(7, 8))),
            slice(range(5, 8), None),
            slice(range(8, 12), Some(range(8, 12))),
        ]
    );
    let cropped = inserted
        .window(ExactFrameRange {
            start: fraction(9, 2),
            end: fraction(17, 2),
        })
        .unwrap();
    assert_eq!(cropped.recipe_extent(), number(12));
    assert_eq!(
        query(&cropped, range(0, 4)).slices,
        [
            slice(
                ExactFrameRange {
                    start: number(0),
                    end: fraction(1, 2)
                },
                Some(ExactFrameRange {
                    start: fraction(15, 2),
                    end: number(8)
                })
            ),
            slice(
                ExactFrameRange {
                    start: fraction(1, 2),
                    end: fraction(7, 2)
                },
                None
            ),
            slice(
                ExactFrameRange {
                    start: fraction(7, 2),
                    end: number(4)
                },
                Some(ExactFrameRange {
                    start: number(8),
                    end: fraction(17, 2)
                })
            ),
        ]
    );
    let json = cropped.to_json().unwrap();
    assert_eq!(SoundRoute::from_json(&json).unwrap(), cropped);
    assert_eq!(serde_json::from_str::<SoundRoute>(&json).unwrap(), cropped);
    assert_eq!(
        serde_json::from_value::<SoundRoute>(serde_json::to_value(&cropped).unwrap()).unwrap(),
        cropped
    );
    #[derive(serde::Serialize, serde::Deserialize)]
    #[serde(tag = "kind")]
    enum Envelope {
        Sound { route: SoundRoute },
    }
    let nested = serde_json::to_string(&Envelope::Sound {
        route: cropped.clone(),
    })
    .unwrap();
    let Envelope::Sound { route: decoded } = serde_json::from_str(&nested).unwrap();
    assert_eq!(decoded, cropped);
}

fn billion_route() -> SoundRoute {
    SoundRoute::identity(number(3_000_000_000))
        .unwrap()
        .ripple(map(
            3_000_000_000,
            vec![
                SoundRippleNode::Keep { range: range(0, 1) },
                SoundRippleNode::Gap {
                    duration: number(1),
                },
                SoundRippleNode::Sequence { parts: vec![0, 1] },
                SoundRippleNode::Repeat {
                    body: 2,
                    count: 1_000_000_000,
                    input_stride: number(3),
                },
            ],
        ))
        .unwrap()
}

#[test]
fn billion_repeat_short_seek_does_not_visit_preceding_plays() {
    let route = billion_route();
    let last = query(&route, range(1_999_999_998, 2_000_000_000));
    assert_eq!(
        last.slices,
        [
            slice(
                range(1_999_999_998, 1_999_999_999),
                Some(range(2_999_999_997, 2_999_999_998))
            ),
            slice(range(1_999_999_999, 2_000_000_000), None),
        ]
    );
    assert!(last.stats.work < 30, "{:?}", last.stats);
    assert_eq!(last.stats.work, query(&route, range(0, 2)).stats.work);
    assert!(route.to_json().unwrap().len() < 2048);
}

#[test]
fn nested_repeat_strides_accumulate_and_fractional_half_open_queries_do_not_leak() {
    let route = SoundRoute::identity(number(100))
        .unwrap()
        .ripple(map(
            100,
            vec![
                SoundRippleNode::Keep { range: range(1, 2) },
                SoundRippleNode::Repeat {
                    body: 0,
                    count: 3,
                    input_stride: number(4),
                },
                SoundRippleNode::Repeat {
                    body: 1,
                    count: 5,
                    input_stride: number(20),
                },
            ],
        ))
        .unwrap();
    let result = query(
        &route,
        ExactFrameRange {
            start: fraction(27, 2),
            end: fraction(29, 2),
        },
    );
    assert_eq!(
        result.slices,
        [
            slice(
                ExactFrameRange {
                    start: fraction(27, 2),
                    end: number(14)
                },
                Some(ExactFrameRange {
                    start: fraction(171, 2),
                    end: number(86)
                })
            ),
            slice(
                ExactFrameRange {
                    start: number(14),
                    end: fraction(29, 2)
                },
                Some(ExactFrameRange {
                    start: number(89),
                    end: fraction(179, 2)
                })
            ),
        ]
    );
    assert_eq!(query(&route, range(14, 14)).slices, []);
}

#[test]
fn sequence_prefix_index_seeks_without_scanning_the_prefix() {
    let mut nodes: Vec<_> = (0..1000)
        .map(|i| SoundRippleNode::Keep {
            range: range(i * 2, i * 2 + 1),
        })
        .collect();
    nodes.push(SoundRippleNode::Sequence {
        parts: (0..1000).collect(),
    });
    let route = SoundRoute::identity(number(2000))
        .unwrap()
        .ripple(map(2000, nodes))
        .unwrap();
    let last = query(&route, range(999, 1000));
    assert_eq!(
        last.slices,
        [slice(range(999, 1000), Some(range(1998, 1999)))]
    );
    assert!(last.stats.work < 25, "{:?}", last.stats);
}

#[test]
fn shared_dag_summaries_are_admitted_without_expanding_output() {
    let mut nodes = vec![SoundRippleNode::Gap {
        duration: number(1),
    }];
    for previous in 0..30 {
        nodes.push(SoundRippleNode::Sequence {
            parts: vec![previous, previous],
        });
    }
    let route = SoundRoute::identity(number(1))
        .unwrap()
        .ripple(map(1, nodes))
        .unwrap();
    assert_eq!(route.output_extent(), number(1 << 30));
    let result = query(&route, range((1 << 30) - 1, 1 << 30));
    assert_eq!(result.slices, [slice(range((1 << 30) - 1, 1 << 30), None)]);
    assert!(result.stats.work < 150);
}

#[test]
fn query_limits_fail_atomically_and_do_not_mutate_the_route() {
    let route = billion_route();
    let before = route.to_json().unwrap();
    for limits in [
        SoundRouteQueryLimits {
            maximum_work: 1,
            maximum_spans: 100,
        },
        SoundRouteQueryLimits {
            maximum_work: 100,
            maximum_spans: 1,
        },
        SoundRouteQueryLimits {
            maximum_work: 0,
            maximum_spans: 1,
        },
        SoundRouteQueryLimits {
            maximum_work: 100,
            maximum_spans: MAX_SOUND_ROUTE_QUERY_SPANS + 1,
        },
    ] {
        assert_eq!(
            route.query(range(0, 2), limits).unwrap_err().code,
            DocumentErrorCode::LimitExceeded
        );
    }
    assert!(
        route
            .query(
                range(0, 2_000_000_000),
                SoundRouteQueryLimits {
                    maximum_work: 100,
                    maximum_spans: 100
                }
            )
            .is_err()
    );
    assert_eq!(route.to_json().unwrap(), before);
    assert_eq!(
        query(&route, range(0, 1)).slices,
        [slice(range(0, 1), Some(range(0, 1)))]
    );
}

#[test]
fn typed_ingress_rejects_bad_indices_cycles_ranges_order_and_repeat_overlap() {
    let invalid_maps = [
        vec![SoundRippleNode::Sequence { parts: vec![0] }],
        vec![SoundRippleNode::Repeat {
            body: 9,
            count: 2,
            input_stride: number(1),
        }],
        vec![SoundRippleNode::Keep { range: range(2, 1) }],
        vec![SoundRippleNode::Keep {
            range: range(-1, 1),
        }],
        vec![SoundRippleNode::Keep {
            range: range(0, 11),
        }],
        vec![SoundRippleNode::Gap {
            duration: number(0),
        }],
        vec![SoundRippleNode::Sequence { parts: vec![] }],
        vec![
            SoundRippleNode::Keep { range: range(2, 3) },
            SoundRippleNode::Keep { range: range(0, 1) },
            SoundRippleNode::Sequence { parts: vec![0, 1] },
        ],
        vec![
            SoundRippleNode::Keep { range: range(0, 2) },
            SoundRippleNode::Repeat {
                body: 0,
                count: 2,
                input_stride: number(1),
            },
        ],
        vec![
            SoundRippleNode::Keep { range: range(0, 1) },
            SoundRippleNode::Repeat {
                body: 0,
                count: 0,
                input_stride: number(1),
            },
        ],
        vec![
            SoundRippleNode::Gap {
                duration: number(1),
            },
            SoundRippleNode::Gap {
                duration: number(1),
            },
        ],
    ];
    for nodes in invalid_maps {
        assert!(SoundRippleMap::new(number(10), (nodes.len() - 1) as u32, nodes).is_err());
    }
    assert!(
        SoundRoute::new(
            number(10),
            0,
            vec![SoundRouteNode::Window {
                input: 0,
                selection: range(0, 1)
            }]
        )
        .is_err()
    );
    assert!(
        SoundRoute::new(
            number(10),
            0,
            vec![SoundRouteNode::Recipe {}, SoundRouteNode::Recipe {}]
        )
        .is_err()
    );
    let route = SoundRoute::identity(number(10)).unwrap();
    assert!(route.window(range(1, 11)).is_err());
    assert!(
        route
            .ripple(map(9, vec![SoundRippleNode::Keep { range: range(0, 9) }]))
            .is_err()
    );
    assert!(
        route
            .query(range(9, 11), SoundRouteQueryLimits::default())
            .is_err()
    );
}

#[test]
fn wire_is_closed_and_checks_computed_extents_and_bytes() {
    let route = SoundRoute::identity(number(10)).unwrap();
    let canonical = serde_json::to_value(&route).unwrap();
    for forged in [
        {
            let mut wire = canonical.clone();
            wire["unexpected"] = json!(null);
            wire
        },
        {
            let mut wire = canonical.clone();
            wire["output_extent"] = json!(number(9));
            wire
        },
        {
            let mut wire = canonical.clone();
            wire["nodes"][0]["extra"] = json!(1);
            wire
        },
        {
            let mut wire = canonical.clone();
            wire["root"] = json!(4);
            wire
        },
    ] {
        assert!(SoundRoute::from_json(&forged.to_string()).is_err());
    }
    let too_large = " ".repeat(MAX_SOUND_ROUTE_JSON_BYTES + 1);
    assert_eq!(
        SoundRoute::from_json(&too_large).unwrap_err().code,
        DocumentErrorCode::LimitExceeded
    );
    assert_eq!(
        SoundRippleMap::from_json(&too_large).unwrap_err().code,
        DocumentErrorCode::LimitExceeded
    );
    let ripple = map(10, vec![SoundRippleNode::Keep { range: range(2, 7) }]);
    assert_eq!(
        SoundRippleMap::from_json(&ripple.to_json().unwrap()).unwrap(),
        ripple
    );
    let mut wrong = serde_json::to_value(ripple).unwrap();
    wrong["input_extent"] = json!(number(6));
    assert!(SoundRippleMap::from_json(&wrong.to_string()).is_err());
}

#[test]
fn construction_bounds_depth_nodes_edges_and_checked_arithmetic() {
    let mut nodes = vec![SoundRippleNode::Keep { range: range(0, 1) }];
    for previous in 0..MAX_SOUND_ROUTE_DEPTH {
        nodes.push(SoundRippleNode::Repeat {
            body: previous as u32,
            count: 1,
            input_stride: number(1),
        });
    }
    assert_eq!(
        SoundRippleMap::new(number(1), (nodes.len() - 1) as u32, nodes)
            .unwrap_err()
            .code,
        DocumentErrorCode::LimitExceeded
    );
    assert!(
        SoundRippleMap::new(
            number(1),
            0,
            vec![
                SoundRippleNode::Gap {
                    duration: number(1)
                };
                MAX_SOUND_ROUTE_NODES + 1
            ]
        )
        .is_err()
    );
    assert!(
        SoundRippleMap::new(
            number(1),
            1,
            vec![
                SoundRippleNode::Gap {
                    duration: number(1)
                },
                SoundRippleNode::Sequence {
                    parts: vec![0; MAX_SOUND_ROUTE_EDGES + 1]
                },
            ]
        )
        .is_err()
    );
    assert_eq!(
        SoundRippleMap::new(
            number(i64::MAX),
            1,
            vec![
                SoundRippleNode::Keep {
                    range: range(0, i64::MAX)
                },
                SoundRippleNode::Repeat {
                    body: 0,
                    count: 2,
                    input_stride: number(i64::MAX)
                },
            ]
        )
        .unwrap_err()
        .code,
        DocumentErrorCode::TimingOverflow
    );
    // Chronological history is separately bounded by arena capacity, not the
    // structural nesting limit of one ripple pattern.
    let mut history = vec![SoundRouteNode::Recipe {}];
    for input in 0..MAX_SOUND_ROUTE_NODES - 1 {
        history.push(SoundRouteNode::Window {
            input: input as u32,
            selection: range(0, 1),
        });
    }
    let route = SoundRoute::new(number(1), (history.len() - 1) as u32, history).unwrap();
    let prior = route.to_json().unwrap();
    assert_eq!(
        route.window(range(0, 1)).unwrap_err().code,
        DocumentErrorCode::LimitExceeded
    );
    assert_eq!(route.to_json().unwrap(), prior);
    assert_eq!(
        query(&route, range(0, 1)).slices,
        [slice(range(0, 1), Some(range(0, 1)))]
    );
}

#[test]
fn repeat_input_stride_is_not_repeated_playback_and_gaps_do_not_inherit_source() {
    let route = SoundRoute::identity(number(10)).unwrap();
    assert!(
        SoundRippleMap::new(
            number(10),
            1,
            vec![
                SoundRippleNode::Keep { range: range(2, 4) },
                SoundRippleNode::Repeat {
                    body: 0,
                    count: 2,
                    input_stride: number(0)
                },
            ]
        )
        .is_err()
    );
    let gap = route
        .ripple(map(
            10,
            vec![SoundRippleNode::Gap {
                duration: number(3),
            }],
        ))
        .unwrap();
    assert_eq!(query(&gap, range(0, 3)).slices, [slice(range(0, 3), None)]);
    assert_eq!(gap.recipe_extent(), number(10));

    let repeated_gap = route
        .ripple(map(
            10,
            vec![
                SoundRippleNode::Gap {
                    duration: number(1),
                },
                SoundRippleNode::Repeat {
                    body: 0,
                    count: 1_000_000_000,
                    input_stride: number(0),
                },
            ],
        ))
        .unwrap();
    let result = query(&repeated_gap, range(0, 1_000_000_000));
    assert_eq!(result.slices, [slice(range(0, 1_000_000_000), None)]);
    assert!(result.stats.work < 10);
}
