use super::*;
use deadpan_plan::{PictureClockSlope, ProviderClockCoverage, ProviderLeafKind};

fn q(n: i64, d: i64) -> ExactRatio {
    ExactRatio::new(i128::from(n), i128::from(d)).unwrap()
}

fn ample() -> BoundaryQueryLimits {
    BoundaryQueryLimits {
        max_scopes: 1_000_000,
        max_comparisons: 1_000_000,
    }
}

fn coverage(plan: &RenderPlan, start: ExactRatio, end: ExactRatio) -> ProviderClockCoverage {
    plan.provider_clock_coverage(ExactFrameRange { start, end }, ample())
        .unwrap()
}

fn complete(plan: &RenderPlan) -> ProviderClockCoverage {
    coverage(
        plan,
        ExactRatio::ZERO,
        plan.node_exact_duration(&plan.metadata().root).unwrap(),
    )
}

/// Independent rational interior-point oracle against ordinary point sampling.
/// Explicit seam tests below additionally establish the excluded right limits.
fn assert_oracle(plan: &RenderPlan, actual: &ProviderClockCoverage) {
    assert_eq!(actual.project_id, plan.metadata().project_id);
    assert_eq!(actual.revision_id, plan.metadata().revision_id);
    let mut next = actual.range.start;
    for span in &actual.spans {
        assert_eq!(span.range.start, next);
        assert!(span.range.start.compare(span.range.end).is_lt());
        next = span.range.end;
        for fraction in [q(0, 1), q(1, 7), q(1, 2), q(6, 7)] {
            let offset = span
                .range
                .end
                .checked_sub(span.range.start)
                .unwrap()
                .checked_mul(fraction)
                .unwrap();
            let position = span.range.start.checked_add(offset).unwrap();
            let sampled = plan
                .definition_picture(&plan.metadata().root, position, ample())
                .unwrap();
            assert_eq!(sampled.instance, span.instance);
            assert_eq!(sampled.gap_after, span.gap_after);
            let mut expected = span.picture.clone();
            match (&mut expected, span.clock) {
                (Picture::Source { point, .. }, PictureClockSlope::SourceTicks(rate)) => {
                    point.ticks = point
                        .ticks
                        .checked_add(offset.checked_mul(rate).unwrap())
                        .unwrap();
                }
                (
                    Picture::Accepted {
                        position, frame, ..
                    },
                    PictureClockSlope::AcceptedFrames(rate),
                ) => {
                    *position = position
                        .checked_add(offset.checked_mul(rate).unwrap())
                        .unwrap();
                    *frame = SourceFrameId(u64::try_from(position.floor()).unwrap());
                }
                (_, PictureClockSlope::Constant) => {}
                other => panic!("inconsistent clock {other:?}"),
            }
            assert_eq!(sampled.picture, expected, "at {position:?}");
        }
    }
    assert_eq!(next, actual.range.end);
}

#[test]
fn split_source_ids_and_positive_slopes_keep_exact_contiguous_source_limits() {
    let plan = RenderPlan::compile(&document(
        &["left", "speed"],
        vec![
            ("left", source(2, -1001, 1001)),
            ("right", source(4, 1001, 5005)),
            ("speed", retime("right", 3, 0, 4)),
        ],
    ))
    .unwrap();
    let actual = complete(&plan);
    assert_eq!(actual.spans.len(), 2);
    assert_eq!(
        actual.spans[0].range,
        ExactFrameRange {
            start: q(0, 1),
            end: q(2, 1)
        }
    );
    assert_eq!(
        actual.spans[1].range,
        ExactFrameRange {
            start: q(2, 1),
            end: q(5, 1)
        }
    );
    assert_eq!(actual.spans[0].instance.node, id("left"));
    assert_eq!(actual.spans[1].instance.node, id("right"));
    assert_eq!(
        actual.spans[0].clock,
        PictureClockSlope::SourceTicks(q(1001, 1))
    );
    assert_eq!(
        actual.spans[1].clock,
        PictureClockSlope::SourceTicks(q(4004, 3))
    );
    assert_eq!(ticks(&actual.spans[1].picture), q(1001, 1));
    assert!(
        actual
            .spans
            .iter()
            .all(|s| s.leaf_kind == ProviderLeafKind::Source && !s.in_repeat_gap)
    );
    assert_oracle(&plan, &actual);
    // The continuous clock has no dependency on which VFR ordinals are displayed.
    let measured = index("video", clock(), &[-1001, 17, 3100], 5005);
    assert_eq!(
        actual.spans[1]
            .picture
            .select_source_frame(&measured)
            .unwrap()
            .identity,
        SourceFrameId(1)
    );
}

#[test]
fn fractional_scope_terminal_is_covered_without_a_terminal_sample() {
    let mut speed = retime("source", 2, 0, 4);
    let NodeKind::Retime { pitch, .. } = &mut speed.kind else {
        unreachable!()
    };
    *pitch = PitchPolicy::FollowSpeed;
    let mut tail = retime("tail-source", 1, 0, 1);
    let NodeKind::Retime { pitch, .. } = &mut tail.kind else {
        unreachable!()
    };
    *pitch = PitchPolicy::FollowSpeed;
    let base = document(
        &["scope", "tail"],
        vec![
            ("scope", BeatNode::sequence("scope", vec![id("speed")])),
            ("speed", speed),
            ("source", source(4, 0, 4004)),
            ("tail", tail),
            ("tail-source", source(1, 4004, 5005)),
        ],
    );
    let mut wire = serde_json::to_value(base).unwrap();
    // Public documents require an integral root, while ordinary child scopes
    // may end between frames. The separate tail completes the root's frame.
    wire["derived_clocks"] = serde_json::to_value(BTreeMap::from([
        (
            id("speed"),
            DerivedRetimeClock {
                extent: q(3, 2),
                child_start: q(0, 1),
                child_end: q(4, 1),
                preserve_input: None,
            },
        ),
        (
            id("tail"),
            DerivedRetimeClock {
                extent: q(1, 2),
                child_start: q(0, 1),
                child_end: q(1, 1),
                preserve_input: None,
            },
        ),
    ]))
    .unwrap();
    let plan =
        RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap();
    assert_eq!(plan.node_exact_duration(&id("root")), Some(q(2, 1)));
    assert_eq!(plan.node_exact_duration(&id("scope")), Some(q(3, 2)));
    let actual = coverage(&plan, q(0, 1), q(3, 2));
    assert_eq!(actual.range.end, q(3, 2));
    assert_eq!(actual.spans.len(), 1);
    assert_eq!(
        actual.spans[0].clock,
        PictureClockSlope::SourceTicks(q(8008, 3))
    );
    assert_oracle(&plan, &actual);
    assert!(
        plan.definition_picture(&id("scope"), q(3, 2), ample())
            .is_err()
    );
    assert!(
        plan.definition_picture_coverage(&id("scope"), q(0, 1), q(3, 2), ample())
            .is_err()
    );
}

#[test]
fn visible_and_removed_cutaways_are_ignored_without_adding_seams() {
    for removed in [false, true] {
        let mut host = source(8, 0, 8008);
        host.cutaways.push(Cutaway {
            range: range(2, 4),
            asset: asset_id("video"),
            selection: span(30030, 31031).into(),
            fit: CutawayFit::Loop,
            removed,
        });
        let doc = document(&["host"], vec![("host", host)]);
        let plan = RenderPlan::compile(&doc).unwrap();
        let actual = complete(&plan);
        assert_eq!(actual.spans.len(), 1);
        assert_eq!(ticks(&actual.spans[0].picture), q(0, 1));
        assert_oracle(
            &RenderPlan::compile(&document_without_cutaways(&doc)).unwrap(),
            &actual,
        );
        assert_ne!(
            plan.picture(ProjectFrame(2)).unwrap().picture,
            plan.provider_picture(ProjectFrame(2)).unwrap().picture
        );
    }
}

#[test]
fn actual_hold_leaf_is_retained_for_forward_reverse_freeze_and_accepted_pictures() {
    for video in [
        HoldVideo::Play {
            asset: asset_id("video"),
            span: span(1001, 3003),
            origin: None,
        },
        HoldVideo::Reverse {
            asset: asset_id("video"),
            span: span(1001, 3003),
            origin: None,
        },
        HoldVideo::Freeze {
            asset: asset_id("video"),
            timestamp: span(1001, 2002).start(),
        },
        HoldVideo::Accepted {
            asset: asset_id("video"),
            frames: range(10, 14),
        },
    ] {
        let plan = RenderPlan::compile(&document(
            &["hold"],
            vec![(
                "hold",
                node(NodeKind::Hold {
                    recipe: HoldRecipe {
                        duration: duration(4),
                        video,
                        picture_context: None,
                        audio: HoldAudio::Silence,
                    },
                }),
            )],
        ))
        .unwrap();
        let actual = complete(&plan);
        assert!(
            actual
                .spans
                .iter()
                .all(|s| s.leaf_kind == ProviderLeafKind::Hold && !s.in_repeat_gap)
        );
        assert_oracle(&plan, &actual);
        match &actual.spans[0].picture {
            Picture::Source { .. } => assert!(matches!(
                actual.spans[0].clock,
                PictureClockSlope::SourceTicks(_)
            )),
            Picture::Freeze { .. } => {
                assert_eq!(actual.spans[0].clock, PictureClockSlope::Constant)
            }
            Picture::Accepted { .. } => assert_eq!(
                actual.spans[0].clock,
                PictureClockSlope::AcceptedFrames(q(1, 1))
            ),
            other => panic!("unexpected {other:?}"),
        }
    }
    let base = document(&["pause"], vec![("pause", hold(30))]);
    let (artifact, assets) = generated_fixture("provider-", ['a', 'b', 'c']);
    let accepted = deadpan_core::apply(
        &base,
        &CommandRequest {
            project_id: base.project_id().clone(),
            expected_revision: base.revision_id().clone(),
            new_revision: revision("accepted"),
            command: Command::AcceptGeneratedHold {
                node: id("pause"),
                artifact,
                assets,
            },
        },
    )
    .unwrap()
    .forward
    .apply(&base)
    .unwrap();
    let plan = RenderPlan::compile(&accepted).unwrap();
    let actual = complete(&plan);
    assert_eq!(actual.spans[0].leaf_kind, ProviderLeafKind::Hold);
    assert!(matches!(
        actual.spans[0].picture,
        Picture::Accepted {
            generated: Some(_),
            ..
        }
    ));
    assert_oracle(&plan, &actual);
}

#[test]
fn repeat_play_identity_and_default_gap_seams_remain_distinct() {
    let plan = RenderPlan::compile(&document(
        &["repeat"],
        vec![
            ("repeat", repeat("source", 3, 1, "plays")),
            ("source", source(2, 0, 2002)),
        ],
    ))
    .unwrap();
    let actual = complete(&plan);
    assert_eq!(
        actual.spans.iter().map(|s| s.range.end).collect::<Vec<_>>(),
        [q(2, 1), q(3, 1), q(5, 1), q(6, 1), q(8, 1)]
    );
    for ordinal in 0..3 {
        let play = &actual.spans[ordinal * 2];
        assert_eq!(play.leaf_kind, ProviderLeafKind::Source);
        assert_eq!(
            play.instance.repeats[0].iteration.ordinal,
            u32::try_from(ordinal).unwrap()
        );
        assert!(!play.in_repeat_gap);
        if ordinal < 2 {
            let gap = &actual.spans[ordinal * 2 + 1];
            assert_eq!(gap.leaf_kind, ProviderLeafKind::DefaultRepeatGap);
            assert!(gap.in_repeat_gap);
            assert_eq!(
                gap.gap_after,
                Some(play.instance.repeats[0].iteration.clone())
            );
        }
    }
    assert_oracle(&plan, &actual);
}

#[test]
fn explicit_source_gap_and_nested_play_keep_ancestor_gap_provenance() {
    let base = document(
        &["outer"],
        vec![
            ("outer", repeat("source", 2, 1, "outer-plays")),
            ("source", source(2, 0, 2002)),
        ],
    );
    let mut wire = serde_json::to_value(base).unwrap();
    wire["nodes"]["gap"] = serde_json::to_value(repeat("gap-source", 2, 1, "inner-plays")).unwrap();
    wire["nodes"]["gap-source"] = serde_json::to_value(source(1, 10010, 11011)).unwrap();
    wire["gap_overrides"]["outer"] = serde_json::json!([
        {"iteration":{"allocation":"outer-plays","ordinal":0},"root":"gap"}
    ]);
    let plan =
        RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap();
    let actual = complete(&plan);
    assert_eq!(actual.spans.len(), 5);
    for at in [1, 3] {
        let gap = &actual.spans[at];
        assert_eq!(gap.leaf_kind, ProviderLeafKind::Source);
        assert!(gap.in_repeat_gap);
        assert!(gap.gap_after.is_none());
        assert_eq!(gap.instance.repeats.len(), 2);
        assert_eq!(gap.instance.repeats[0].node, id("outer"));
        assert_eq!(gap.instance.repeats[1].node, id("gap"));
    }
    assert_eq!(
        actual.spans[2].leaf_kind,
        ProviderLeafKind::DefaultRepeatGap
    );
    assert!(actual.spans[2].in_repeat_gap);
    assert!(!actual.spans[4].in_repeat_gap);
    assert_oracle(&plan, &actual);
}

#[test]
fn held_source_edge_slack_stays_raw_with_its_selection() {
    let plan = RenderPlan::compile(&document(
        &["source"],
        vec![(
            "source",
            source_with_mapping(
                4,
                0,
                1500,
                SourceVideoMapping::Duration {
                    frames: q(3, 2),
                    endpoints: EndpointPolicy::HoldAdjacent,
                },
            ),
        )],
    ))
    .unwrap();
    let actual = complete(&plan);
    assert_eq!(actual.spans.len(), 1);
    let span = &actual.spans[0];
    assert_eq!(span.clock, PictureClockSlope::SourceTicks(q(1000, 1)));
    let Picture::Source {
        point,
        selection,
        endpoints,
        ..
    } = span.picture
    else {
        panic!("source")
    };
    assert_eq!(point.ticks, q(0, 1));
    assert_eq!(selection.end().ticks, q(1500, 1));
    assert_eq!(endpoints, EndpointPolicy::HoldAdjacent);
    // The affine excluded root end is 4000, beyond the selected 1500 ticks.
    assert_eq!(
        ticks(&plan.provider_picture(ProjectFrame(3)).unwrap().picture),
        q(3500, 1)
    );
    assert_oracle(&plan, &actual);
}

#[test]
fn work_limits_cover_the_whole_query_and_span_exhaustion_returns_no_result() {
    let plan = RenderPlan::compile(&document(
        &["repeat"],
        vec![
            ("repeat", repeat("source", 1_000_000_000, 0, "plays")),
            ("source", source(1, 0, 1001)),
        ],
    ))
    .unwrap();
    let actual = coverage(&plan, q(999_999_990, 1), q(1_000_000_000, 1));
    assert_eq!(actual.spans.len(), 10);
    assert!(actual.lookup.visited_nodes < 40);
    let exact = BoundaryQueryLimits {
        max_scopes: actual.lookup.visited_nodes,
        max_comparisons: actual.lookup.sequence_comparisons
            + actual.lookup.iteration_run_comparisons,
    };
    assert_eq!(
        plan.provider_clock_coverage(actual.range, exact).unwrap(),
        actual
    );
    for limits in [
        BoundaryQueryLimits {
            max_scopes: exact.max_scopes - 1,
            ..exact
        },
        BoundaryQueryLimits {
            max_comparisons: exact.max_comparisons - 1,
            ..exact
        },
    ] {
        assert!(matches!(
            plan.provider_clock_coverage(actual.range, limits),
            Err(PlanError::PictureQueryLimit(_))
        ));
    }
    assert!(matches!(
        plan.provider_clock_coverage(
            ExactFrameRange {
                start: q(0, 1),
                end: q(513, 1)
            },
            ample()
        ),
        Err(PlanError::PictureQueryLimit("picture spans"))
    ));
}

#[test]
fn aggregate_metadata_budget_is_enforced_before_span_cap() {
    let names: Vec<_> = (0..96)
        .map(|i| format!("repeat-{i:03}-{}", "x".repeat(117)))
        .collect();
    let mut nodes: Vec<_> = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let child = names
                .get(i + 1)
                .map(String::as_str)
                .unwrap_or("leaf-repeat");
            (name.as_str(), repeat(child, 1, 0, name))
        })
        .collect();
    nodes.extend([
        ("leaf-repeat", repeat("source", 128, 0, "leaf-plays")),
        ("source", source(1, 0, 1001)),
    ]);
    let plan = RenderPlan::compile(&document(&[names[0].as_str()], nodes)).unwrap();
    assert!(matches!(
        plan.provider_clock_coverage(
            ExactFrameRange {
                start: q(0, 1),
                end: q(128, 1)
            },
            ample()
        ),
        Err(PlanError::PictureQueryLimit("context metadata"))
    ));
}

#[test]
fn empty_reversed_negative_outside_and_arithmetic_overflow_fail_atomically() {
    let plan =
        RenderPlan::compile(&document(&["source"], vec![("source", source(4, 0, 4004))])).unwrap();
    for (start, end) in [
        (q(0, 1), q(0, 1)),
        (q(3, 1), q(2, 1)),
        (q(-1, 1), q(1, 1)),
        (q(0, 1), q(4001, 1000)),
        (q(4, 1), q(5, 1)),
    ] {
        assert!(
            plan.provider_clock_coverage(ExactFrameRange { start, end }, ample())
                .is_err()
        );
    }
    let tiny = ExactRatio::new(1, i128::MAX).unwrap();
    assert!(matches!(
        plan.provider_clock_coverage(
            ExactFrameRange {
                start: tiny,
                end: q(1, 1)
            },
            ample()
        ),
        Err(PlanError::Time(TimeError::Overflow))
    ));
    let empty = RenderPlan::compile(&document(&[], vec![])).unwrap();
    assert!(
        empty
            .provider_clock_coverage(
                ExactFrameRange {
                    start: q(0, 1),
                    end: q(0, 1)
                },
                ample()
            )
            .is_err()
    );
}
