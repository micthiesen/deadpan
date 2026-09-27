use deadpan_core::{
    AudioSample, ExactFrameRange, ExactRatio, SoundRippleMap, SoundRippleNode, SoundRoute,
};
use deadpan_plan::{
    AudioBoundaryRule, AudioQueryLimits, AudioSampleGrid, AudioSoundRoute, AudioSoundRouteQuery,
    PlanError, SignalSample,
};

fn q(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}

fn frames(start: i64, end: i64) -> ExactFrameRange {
    ExactFrameRange {
        start: ExactRatio::integer(start),
        end: ExactRatio::integer(end),
    }
}

fn insert(extent: i64, at: i64, duration: i64) -> SoundRippleMap {
    SoundRippleMap::new(
        ExactRatio::integer(extent),
        3,
        vec![
            SoundRippleNode::Keep {
                range: frames(0, at),
            },
            SoundRippleNode::Gap {
                duration: ExactRatio::integer(duration),
            },
            SoundRippleNode::Keep {
                range: frames(at, extent),
            },
            SoundRippleNode::Sequence {
                parts: vec![0, 1, 2],
            },
        ],
    )
    .unwrap()
}

fn root_grid(origin: ExactRatio, step: ExactRatio) -> AudioSampleGrid<AudioSample> {
    AudioSampleGrid::new(origin, step, AudioBoundaryRule::RoundEven).unwrap()
}

fn ntsc() -> AudioSampleGrid<AudioSample> {
    root_grid(ExactRatio::ZERO, q(5, 8008))
}

fn values(query: AudioSoundRouteQuery<AudioSample>) -> Vec<Option<ExactRatio>> {
    let mut next = query.samples.start.0;
    let mut result = vec![];
    for span in query.spans {
        assert_eq!(span.samples.start.0, next);
        assert!(span.samples.end.0 > next);
        for sample in span.samples.start.0..span.samples.end.0 {
            result.push(
                span.sampling
                    .map(|map| map.local_at(AudioSample(sample)).unwrap()),
            );
        }
        next = span.samples.end.0;
    }
    assert_eq!(next, query.samples.end.0);
    result
}

#[test]
fn two_ntsc_insertions_resume_the_current_sample_phase() {
    let route = SoundRoute::identity(ExactRatio::integer(6))
        .unwrap()
        .ripple(insert(6, 1, 1))
        .unwrap()
        .ripple(insert(7, 3, 1))
        .unwrap();
    let sampled = AudioSoundRoute::<AudioSample>::new(route, vec![ntsc(); 3]).unwrap();
    let all = values(
        sampled
            .query(sampled.samples(), AudioQueryLimits::default())
            .unwrap(),
    );
    let b = |frame| ntsc().boundary(ExactRatio::integer(frame)).unwrap().0;
    let original = |sample| ntsc().at(AudioSample(sample)).unwrap();
    // Independent dense PCM oracle: copy the previous sampled output from each
    // old cut for the newly allocated suffix, padding only at its real endpoint.
    let raw: Vec<_> = (0..b(6)).map(|sample| Some(original(sample))).collect();
    let mut first = raw[..b(1) as usize].to_vec();
    first.resize(b(2) as usize, None);
    for sample in b(2)..b(7) {
        first.push(raw.get((b(1) + sample - b(2)) as usize).copied().flatten());
    }
    let mut expected = first[..b(3) as usize].to_vec();
    expected.resize(b(4) as usize, None);
    for sample in b(4)..b(8) {
        expected.push(
            first
                .get((b(3) + sample - b(4)) as usize)
                .copied()
                .flatten(),
        );
    }
    assert_eq!(all, expected);
    assert_eq!(all[b(4) as usize], Some(original(3204)));
    assert_ne!(all[b(4) as usize], Some(original(b(2))));
}

#[test]
fn shuffled_partial_reads_keep_the_same_phase_and_gap_samples() {
    let route = SoundRoute::identity(ExactRatio::integer(6))
        .unwrap()
        .ripple(insert(6, 1, 1))
        .unwrap()
        .ripple(insert(7, 3, 1))
        .unwrap()
        .window(ExactFrameRange {
            start: q(7, 3),
            end: q(23, 3),
        })
        .unwrap();
    let shifted = root_grid(q(-1, 7), q(5, 8008));
    let sampled =
        AudioSoundRoute::<AudioSample>::new(route, vec![ntsc(), ntsc(), ntsc(), shifted]).unwrap();
    let whole = values(
        sampled
            .query(sampled.samples(), AudioQueryLimits::default())
            .unwrap(),
    );
    let origin = sampled.samples().start.0;
    let end = sampled.samples().end.0;
    let mut offsets = (origin..end).step_by(137).collect::<Vec<_>>();
    offsets.reverse();
    for start in offsets {
        let stop = (start + 137).min(end);
        let part = values(
            sampled
                .query(
                    AudioSample(start)..AudioSample(stop),
                    AudioQueryLimits::default(),
                )
                .unwrap(),
        );
        assert_eq!(
            part,
            whole[(start - origin) as usize..(stop - origin) as usize]
        );
    }
}

#[test]
fn deleting_an_event_start_keeps_the_complete_recipe_and_its_suffix_phase() {
    let grid = root_grid(ExactRatio::ZERO, q(1, 3));
    let route = SoundRoute::identity(ExactRatio::integer(8)).unwrap();
    let deletion = SoundRippleMap::new(
        ExactRatio::integer(8),
        0,
        vec![SoundRippleNode::Keep {
            range: frames(3, 8),
        }],
    )
    .unwrap();
    let sampled =
        AudioSoundRoute::<AudioSample>::new(route.ripple(deletion).unwrap(), vec![grid; 2])
            .unwrap();
    assert_eq!(sampled.route().recipe_extent(), ExactRatio::integer(8));
    let actual = values(
        sampled
            .query(sampled.samples(), AudioQueryLimits::default())
            .unwrap(),
    );
    assert_eq!(actual, (9..24).map(|n| Some(q(n, 3))).collect::<Vec<_>>());
}

#[test]
fn recipe_exhaustion_is_silent_when_new_allocation_has_an_extra_sample() {
    let route = SoundRoute::identity(q(3, 2))
        .unwrap()
        .window(ExactFrameRange {
            start: ExactRatio::ZERO,
            end: q(3, 2),
        })
        .unwrap();
    let old = root_grid(q(-1, 5), ExactRatio::ONE);
    let new = root_grid(q(-3, 5), ExactRatio::ONE);
    // Old [0, 2), new [1, 2): this direction shrinks allocation.
    let shrinking = AudioSoundRoute::<AudioSample>::new(route.clone(), vec![old, new]).unwrap();
    assert_eq!(
        values(
            shrinking
                .query(shrinking.samples(), AudioQueryLimits::default())
                .unwrap()
        ),
        vec![Some(q(-1, 5))]
    );
    let growing = AudioSoundRoute::<AudioSample>::new(route, vec![new, old]).unwrap();
    assert_eq!(
        values(
            growing
                .query(growing.samples(), AudioQueryLimits::default())
                .unwrap()
        ),
        vec![Some(q(2, 5)), None]
    );
}

#[test]
fn a_shifted_window_preserves_prior_gaps_in_the_retained_sample_clock() {
    let route = SoundRoute::identity(ExactRatio::integer(4))
        .unwrap()
        .ripple(insert(4, 1, 1))
        .unwrap()
        .window(frames(0, 5))
        .unwrap();
    let initial = root_grid(ExactRatio::ZERO, q(2, 3));
    let moved = root_grid(q(-1, 3), q(2, 3));
    let prior = AudioSoundRoute::<AudioSample>::new(
        SoundRoute::identity(ExactRatio::integer(4))
            .unwrap()
            .ripple(insert(4, 1, 1))
            .unwrap(),
        vec![initial; 2],
    )
    .unwrap();
    let old = values(
        prior
            .query(prior.samples(), AudioQueryLimits::default())
            .unwrap(),
    );
    let sampled =
        AudioSoundRoute::<AudioSample>::new(route, vec![initial, initial, moved]).unwrap();
    let mut expected = old;
    expected.resize(
        (sampled.samples().end.0 - sampled.samples().start.0) as usize,
        None,
    );
    assert_eq!(
        values(
            sampled
                .query(sampled.samples(), AudioQueryLimits::default())
                .unwrap()
        ),
        expected
    );
}

#[test]
fn intrinsic_point_ceil_and_root_round_even_have_distinct_allocations() {
    let route = SoundRoute::identity(q(2, 3)).unwrap();
    let root = AudioSoundRoute::<AudioSample>::new(
        route.clone(),
        vec![root_grid(ExactRatio::ZERO, q(1, 2000))],
    )
    .unwrap();
    let signal = AudioSoundRoute::<SignalSample>::new(
        route,
        vec![
            AudioSampleGrid::new(ExactRatio::ZERO, q(1, 2000), AudioBoundaryRule::PointCeil)
                .unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(root.samples(), AudioSample(0)..AudioSample(1333));
    assert_eq!(signal.samples(), SignalSample(0)..SignalSample(1334));
    let query = signal
        .query(
            SignalSample(1333)..SignalSample(1334),
            AudioQueryLimits::default(),
        )
        .unwrap();
    assert_eq!(
        query.spans[0]
            .sampling
            .unwrap()
            .local_at(SignalSample(1333))
            .unwrap(),
        q(1333, 2000)
    );
}

#[test]
fn tiny_zero_sample_gap_is_skipped_without_losing_the_following_keep() {
    let map = SoundRippleMap::new(
        ExactRatio::integer(4),
        3,
        vec![
            SoundRippleNode::Keep {
                range: frames(0, 1),
            },
            SoundRippleNode::Gap {
                duration: q(1, 100),
            },
            SoundRippleNode::Keep {
                range: frames(1, 4),
            },
            SoundRippleNode::Sequence {
                parts: vec![0, 1, 2],
            },
        ],
    )
    .unwrap();
    let route = SoundRoute::identity(ExactRatio::integer(4))
        .unwrap()
        .ripple(map)
        .unwrap();
    let grid = root_grid(ExactRatio::ZERO, ExactRatio::ONE);
    let sampled = AudioSoundRoute::<AudioSample>::new(route, vec![grid; 2]).unwrap();
    assert_eq!(
        values(
            sampled
                .query(sampled.samples(), AudioQueryLimits::default())
                .unwrap()
        ),
        (0..4)
            .map(|n| Some(ExactRatio::integer(n)))
            .collect::<Vec<_>>()
    );
}

#[test]
fn billion_period_route_seeks_directly_and_gap_only_route_stays_compact() {
    let extent = ExactRatio::integer(1_000_000_000);
    let map = SoundRippleMap::new(
        extent,
        3,
        vec![
            SoundRippleNode::Keep {
                range: frames(0, 1),
            },
            SoundRippleNode::Gap {
                duration: ExactRatio::ONE,
            },
            SoundRippleNode::Sequence { parts: vec![0, 1] },
            SoundRippleNode::Repeat {
                body: 2,
                count: 1_000_000_000,
                input_stride: ExactRatio::ONE,
            },
        ],
    )
    .unwrap();
    let grid = root_grid(ExactRatio::ZERO, ExactRatio::ONE);
    let sampled = AudioSoundRoute::<AudioSample>::new(
        SoundRoute::identity(extent).unwrap().ripple(map).unwrap(),
        vec![grid; 2],
    )
    .unwrap();
    let query = sampled
        .query(
            AudioSample(1_999_999_996)..AudioSample(2_000_000_000),
            AudioQueryLimits::default(),
        )
        .unwrap();
    assert!(query.stats.work < 64);
    assert_eq!(
        values(query),
        vec![
            Some(ExactRatio::integer(999_999_998)),
            None,
            Some(ExactRatio::integer(999_999_999)),
            None
        ]
    );
    assert!(
        sampled
            .query(
                sampled.samples(),
                AudioQueryLimits {
                    maximum_work: 100,
                    maximum_spans: 4
                }
            )
            .is_err()
    );

    let gaps = SoundRippleMap::new(
        ExactRatio::ONE,
        1,
        vec![
            SoundRippleNode::Gap {
                duration: ExactRatio::ONE,
            },
            SoundRippleNode::Repeat {
                body: 0,
                count: 1_000_000_000,
                input_stride: ExactRatio::ZERO,
            },
        ],
    )
    .unwrap();
    let sampled = AudioSoundRoute::<AudioSample>::new(
        SoundRoute::identity(ExactRatio::ONE)
            .unwrap()
            .ripple(gaps)
            .unwrap(),
        vec![grid; 2],
    )
    .unwrap();
    let query = sampled
        .query(sampled.samples(), AudioQueryLimits::default())
        .unwrap();
    assert!(query.stats.work < 10);
    assert_eq!(query.spans.len(), 1);
    assert!(query.spans[0].sampling.is_none());
}

#[test]
fn admission_rejects_missing_clocks_changed_rates_rules_and_overflow() {
    let route = SoundRoute::identity(ExactRatio::integer(6))
        .unwrap()
        .ripple(insert(6, 1, 1))
        .unwrap();
    assert!(AudioSoundRoute::<AudioSample>::new(route.clone(), vec![ntsc()]).is_err());
    assert!(
        AudioSoundRoute::<AudioSample>::new(
            route.clone(),
            vec![ntsc(), root_grid(ExactRatio::ZERO, q(1, 1600))]
        )
        .is_err()
    );
    assert!(
        AudioSoundRoute::<AudioSample>::new(
            route.clone(),
            vec![
                ntsc(),
                AudioSampleGrid::new(ExactRatio::ZERO, q(5, 8008), AudioBoundaryRule::PointCeil)
                    .unwrap()
            ]
        )
        .is_err()
    );
    assert!(
        AudioSoundRoute::<AudioSample>::new(
            route.clone(),
            vec![root_grid(ExactRatio::ZERO, q(1, i128::from(i64::MAX))); 2]
        )
        .is_err()
    );
    let sampled = AudioSoundRoute::<AudioSample>::new(route, vec![ntsc(); 2]).unwrap();
    assert!(matches!(
        sampled.query(AudioSample(-1)..AudioSample(0), AudioQueryLimits::default()),
        Err(PlanError::AudioRangeOutOfRange)
    ));
    assert!(matches!(
        sampled.query(AudioSample(1)..AudioSample(0), AudioQueryLimits::default()),
        Err(PlanError::AudioRangeOutOfRange)
    ));
    assert!(matches!(
        sampled.query(
            sampled.samples(),
            AudioQueryLimits {
                maximum_spans: 1,
                maximum_work: 0
            }
        ),
        Err(PlanError::InvalidAudioLimits)
    ));
}

#[test]
fn shared_work_and_span_budgets_are_exact_and_empty_reads_do_no_work() {
    let route = SoundRoute::identity(ExactRatio::integer(6))
        .unwrap()
        .ripple(insert(6, 1, 1))
        .unwrap();
    let sampled = AudioSoundRoute::<AudioSample>::new(route, vec![ntsc(); 2]).unwrap();
    let full = sampled
        .query(sampled.samples(), AudioQueryLimits::default())
        .unwrap();
    let limits = AudioQueryLimits {
        maximum_work: full.stats.work,
        maximum_spans: full.spans.len(),
    };
    assert_eq!(sampled.query(sampled.samples(), limits).unwrap(), full);
    assert!(
        sampled
            .query(
                sampled.samples(),
                AudioQueryLimits {
                    maximum_work: limits.maximum_work - 1,
                    ..limits
                }
            )
            .is_err()
    );
    assert!(
        sampled
            .query(
                sampled.samples(),
                AudioQueryLimits {
                    maximum_spans: limits.maximum_spans - 1,
                    ..limits
                }
            )
            .is_err()
    );
    let empty = sampled
        .query(
            AudioSample(37)..AudioSample(37),
            AudioQueryLimits {
                maximum_work: 1,
                maximum_spans: 1,
            },
        )
        .unwrap();
    assert!(empty.spans.is_empty());
    assert_eq!(empty.stats.work, 0);
}

#[test]
fn fractional_windows_and_signed_grid_origins_match_dense_sample_copy() {
    let route = SoundRoute::identity(ExactRatio::integer(4))
        .unwrap()
        .ripple(insert(4, 1, 1))
        .unwrap()
        .window(ExactFrameRange {
            start: q(1, 2),
            end: q(9, 2),
        })
        .unwrap();
    let origins = [q(-3, 2), q(-1, 2), ExactRatio::ZERO, q(1, 2), q(3, 2)];
    for step in [ExactRatio::ONE, q(2, 3), q(3, 2)] {
        for old_origin in origins {
            for inserted_origin in origins {
                for window_origin in origins {
                    let grids = [old_origin, inserted_origin, window_origin]
                        .map(|origin| root_grid(origin, step));
                    let b = |grid: usize, frame: ExactRatio| grids[grid].boundary(frame).unwrap().0;
                    let old = b(0, ExactRatio::ZERO)..b(0, ExactRatio::integer(4));
                    let first = b(1, ExactRatio::ZERO)..b(1, ExactRatio::integer(5));
                    let first_gap = b(1, ExactRatio::ONE)..b(1, ExactRatio::integer(2));
                    let dense_first: std::collections::BTreeMap<_, _> = first
                        .clone()
                        .map(|n| {
                            let (old_sample, selected) = if n < first_gap.start {
                                (
                                    old.start + n - first.start,
                                    old.start..b(0, ExactRatio::ONE),
                                )
                            } else if n >= first_gap.end {
                                (
                                    b(0, ExactRatio::ONE) + n - first_gap.end,
                                    b(0, ExactRatio::ONE)..old.end,
                                )
                            } else {
                                return (n, None);
                            };
                            (
                                n,
                                selected
                                    .contains(&old_sample)
                                    .then(|| grids[0].at(AudioSample(old_sample)).unwrap()),
                            )
                        })
                        .collect();
                    let sampled =
                        AudioSoundRoute::<AudioSample>::new(route.clone(), grids.to_vec()).unwrap();
                    let samples = sampled.samples();
                    let expected = (samples.start.0..samples.end.0)
                        .map(|n| {
                            let previous = b(1, q(1, 2)) + n - samples.start.0;
                            if previous >= b(1, q(9, 2)) {
                                return None;
                            }
                            dense_first.get(&previous).copied().flatten()
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(
                        values(sampled.query(samples, AudioQueryLimits::default()).unwrap()),
                        expected,
                        "{grids:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_moved_selection_never_reintroduces_samples_after_its_old_cut() {
    let recipe = SoundRoute::identity(ExactRatio::integer(4)).unwrap();
    let selection = ExactFrameRange {
        start: ExactRatio::ZERO,
        end: q(3, 2),
    };
    let old = root_grid(q(1, 2), ExactRatio::ONE);
    let current = root_grid(ExactRatio::ZERO, ExactRatio::ONE);
    let window = recipe.window(selection).unwrap();
    let keep = recipe
        .ripple(
            SoundRippleMap::new(
                ExactRatio::integer(4),
                0,
                vec![SoundRippleNode::Keep { range: selection }],
            )
            .unwrap(),
        )
        .unwrap();
    for route in [window, keep] {
        let sampled = AudioSoundRoute::<AudioSample>::new(route, vec![old, current]).unwrap();
        assert_eq!(sampled.route().recipe_extent(), ExactRatio::integer(4));
        assert_eq!(sampled.samples(), AudioSample(0)..AudioSample(2));
        assert_eq!(
            values(
                sampled
                    .query(sampled.samples(), AudioQueryLimits::default())
                    .unwrap()
            ),
            vec![Some(q(1, 2)), None]
        );
    }
}
