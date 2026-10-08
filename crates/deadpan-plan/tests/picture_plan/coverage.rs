use super::*;
use deadpan_plan::{DefinitionPictureCoverage, PictureClockSlope, ScopedHoldContextRequest};

fn ample() -> BoundaryQueryLimits {
    BoundaryQueryLimits {
        max_scopes: 1_000_000,
        max_comparisons: 1_000_000,
    }
}

fn q(n: i64, d: i64) -> ExactRatio {
    ExactRatio::new(i128::from(n), i128::from(d)).unwrap()
}

fn coverage(
    plan: &RenderPlan,
    definition: &str,
    start: ExactRatio,
    end: ExactRatio,
) -> DefinitionPictureCoverage {
    plan.definition_picture_coverage(&id(definition), start, end, ample())
        .unwrap()
}

/// Independent point oracle: apply the advertised affine clock, then compare
/// against ordinary canonical lookups at rational interior points. No coverage
/// helper or continuity formula supplies the expected provider value.
fn assert_oracle(plan: &RenderPlan, actual: &DefinitionPictureCoverage) {
    for (i, span) in actual.spans.iter().enumerate() {
        assert!(span.start.position.compare(span.end_exclusive).is_lt());
        assert_eq!(
            span.end_exclusive,
            actual
                .spans
                .get(i + 1)
                .map_or(actual.terminal.position, |next| next.start.position)
        );
        for fraction in [q(0, 1), q(1, 5), q(1, 2), q(4, 5)] {
            let offset = span
                .end_exclusive
                .checked_sub(span.start.position)
                .unwrap()
                .checked_mul(fraction)
                .unwrap();
            let position = span.start.position.checked_add(offset).unwrap();
            let sample = plan
                .definition_picture(&span.start.definition, position, ample())
                .unwrap();
            assert_eq!(sample.instance, span.start.instance);
            assert_eq!(sample.gap_after, span.start.gap_after);
            let mut expected = span.start.picture.clone();
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
                unexpected => panic!("inconsistent provider clock {unexpected:?}"),
            }
            assert_eq!(sample.picture, expected, "at {position:?}");
        }
    }
    let end = &actual.terminal;
    assert_eq!(
        end,
        &plan
            .definition_picture(&end.definition, end.position, ample())
            .unwrap()
    );
}

#[test]
fn coverage_is_closed_with_an_exact_terminal_and_no_fake_empty_span() {
    let plan = RenderPlan::compile(&document(
        &["a", "b"],
        vec![("a", source(2, 0, 2002)), ("b", source(2, 5005, 7007))],
    ))
    .unwrap();
    let actual = coverage(&plan, "root", q(1, 2), q(2, 1));
    assert_eq!(actual.spans.len(), 1);
    assert_eq!(actual.spans[0].start.instance.node, id("a"));
    assert_eq!(actual.spans[0].end_exclusive, q(2, 1));
    assert_eq!(actual.terminal.instance.node, id("b"));
    assert_eq!(ticks(&actual.terminal.picture), q(5005, 1));
    assert_oracle(&plan, &actual);
    let empty = coverage(&plan, "root", q(2, 1), q(2, 1));
    assert!(empty.spans.is_empty());
    assert_eq!(empty.terminal, actual.terminal);
    assert!(
        plan.definition_picture_coverage(&id("root"), q(2, 1), q(1, 1), ample())
            .is_err()
    );
    assert!(matches!(
        plan.definition_picture_coverage(&id("root"), q(0, 1), q(4, 1), ample()),
        Err(PlanError::DefinitionPictureOutOfRange { .. })
    ));
    assert!(matches!(
        plan.definition_picture_coverage(&id("root"), q(-1, 1), q(0, 1), ample()),
        Err(PlanError::DefinitionPictureOutOfRange { .. })
    ));
}

#[test]
fn coverage_keeps_definition_scope_without_expanding_outer_repeats() {
    let doc = super::definition::nested(1_000_000_000);
    let plan = RenderPlan::compile(&doc).unwrap();
    let actual = coverage(&plan, "local", q(1, 2), q(21, 2));
    assert_eq!(actual.spans.len(), 3);
    assert!(
        actual
            .spans
            .iter()
            .all(|s| s.start.instance.repeats.is_empty())
    );
    assert!(actual.lookup.visited_nodes <= 8);
    assert_oracle(&plan, &actual);
}

#[test]
fn coverage_preserves_sparse_repeat_play_and_implicit_gap_seams() {
    let plan = RenderPlan::compile(&document(
        &["repeat"],
        vec![
            ("repeat", repeat("source", 1_000_000_000, 1, "plays")),
            ("source", source(2, 0, 2002)),
        ],
    ))
    .unwrap();
    let actual = coverage(&plan, "repeat", q(2_999_999_980, 1), q(2_999_999_989, 1));
    assert!(actual.spans.len() <= 8);
    assert!(actual.spans.iter().any(|s| s.start.gap_after.is_some()));
    assert!(
        actual
            .spans
            .iter()
            .any(|s| !s.start.instance.repeats.is_empty())
    );
    assert!(actual.lookup.visited_nodes < 20);
    assert!(actual.lookup.iteration_run_comparisons < 20);
    assert_oracle(&plan, &actual);
}

#[test]
fn coverage_proves_reverse_and_forward_hold_clamp_tails() {
    for reverse in [false, true] {
        let video = if reverse {
            HoldVideo::Reverse {
                asset: asset_id("video"),
                span: span(1001, 3003),
            }
        } else {
            HoldVideo::Play {
                asset: asset_id("video"),
                span: span(1001, 3003),
            }
        };
        let plan = RenderPlan::compile(&document(
            &["speed"],
            vec![
                ("speed", retime("hold", 4, 0, 8)),
                (
                    "hold",
                    node(NodeKind::Hold {
                        recipe: HoldRecipe {
                            duration: duration(8),
                            video,
                            picture_context: None,
                            audio: HoldAudio::Silence,
                        },
                    }),
                ),
            ],
        ))
        .unwrap();
        let actual = coverage(&plan, "root", q(1, 4), q(7, 2));
        assert_eq!(actual.spans.len(), 2);
        assert_eq!(actual.spans[0].end_exclusive, q(1, 1));
        assert_eq!(
            actual.spans[0].clock,
            PictureClockSlope::SourceTicks(q(if reverse { -2002 } else { 2002 }, 1))
        );
        assert_eq!(
            actual.spans[1].clock,
            PictureClockSlope::SourceTicks(ExactRatio::ZERO)
        );
        assert_oracle(&plan, &actual);
    }
}

fn cutaway(fit: CutawayFit, removed: bool) -> BeatNode {
    let mut host = source(10, 0, 10010);
    host.cutaways.push(Cutaway {
        range: range(1, 9),
        asset: asset_id("video"),
        selection: ExactSourceSpan::from(span(30030, 32032)),
        fit,
        removed,
    });
    host
}

#[test]
fn every_cutaway_fit_has_exact_validity_neighborhoods_under_retime() {
    for fit in [
        CutawayFit::Hold,
        CutawayFit::Loop,
        CutawayFit::Gap,
        CutawayFit::Bounce,
    ] {
        let plan = RenderPlan::compile(&document(
            &["speed"],
            vec![
                ("speed", retime("host", 5, 0, 10)),
                ("host", cutaway(fit, false)),
            ],
        ))
        .unwrap();
        let actual = coverage(&plan, "root", q(1, 4), q(19, 4));
        let ends: Vec<_> = actual.spans.iter().map(|s| s.end_exclusive).collect();
        let expected = match fit {
            CutawayFit::Hold | CutawayFit::Gap => vec![q(1, 2), q(3, 2), q(9, 2), q(19, 4)],
            CutawayFit::Loop | CutawayFit::Bounce => {
                vec![q(1, 2), q(3, 2), q(5, 2), q(7, 2), q(9, 2), q(19, 4)]
            }
        };
        assert_eq!(ends, expected, "{fit:?}");
        if fit == CutawayFit::Gap {
            assert_eq!(ticks(&actual.spans[2].start.picture), q(3003, 1));
            assert_eq!(
                actual.spans[2].clock,
                PictureClockSlope::SourceTicks(q(2002, 1))
            );
        }
        if fit == CutawayFit::Bounce {
            assert_eq!(
                actual.spans[2].clock,
                PictureClockSlope::SourceTicks(q(-2002, 1))
            );
        }
        assert_oracle(&plan, &actual);
    }
    let plan = RenderPlan::compile(&document(
        &["host"],
        vec![("host", cutaway(CutawayFit::Loop, true))],
    ))
    .unwrap();
    let actual = coverage(&plan, "root", q(1, 2), q(19, 2));
    assert_eq!(actual.spans.len(), 3);
    assert_eq!(actual.spans[1].start.picture, Picture::Background);
    assert_eq!(actual.spans[1].clock, PictureClockSlope::Constant);
    assert_oracle(&plan, &actual);
}

#[test]
fn coverage_finds_a_tiny_cutaway_between_all_native_context_samples() {
    let mut host = source(200, 0, 100000);
    host.cutaways.push(Cutaway {
        range: range(181, 182),
        asset: asset_id("video"),
        selection: ExactSourceSpan::from(span(60000, 61001)),
        fit: CutawayFit::Hold,
        removed: false,
    });
    let doc = document(
        &["speed", "pause"],
        vec![
            ("speed", retime("host", 20, 0, 200)),
            ("host", host),
            ("pause", hold(1)),
        ],
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let result = plan
        .scoped_hold_context(
            &ScopedHoldContextRequest {
                target: super::definition::target("pause", &[]),
                direction: ExtensionDirection::FromLeft,
                native_rate: FrameRate::new(24, 1).unwrap(),
                frame_count: 9,
            },
            ample(),
        )
        .unwrap();
    // The compressed cutaway occupies [18.1,18.2), while native context points
    // near it are ~17.0,18.25,19.5. Point sampling alone misses it entirely.
    assert!(result.pictures.iter().all(|p| match &p.picture {
        Picture::Source { selection, .. } => selection.start().ticks != q(60000, 1),
        _ => false,
    }));
    let cut = result
        .coverage
        .spans
        .iter()
        .find(|s| s.start.position == q(181, 10))
        .unwrap();
    assert_eq!(cut.end_exclusive, q(91, 5));
    assert_eq!(ticks(&cut.start.picture), q(60000, 1));
    assert_eq!(cut.clock, PictureClockSlope::SourceTicks(q(10010, 1)));
    assert_oracle(&plan, &result.coverage);
}

#[test]
fn accepted_clock_is_affine_while_frame_ordinals_remain_half_open() {
    let plan = RenderPlan::compile(&document(
        &["speed"],
        vec![
            ("speed", retime("hold", 4, 0, 8)),
            (
                "hold",
                node(NodeKind::Hold {
                    recipe: HoldRecipe {
                        duration: duration(8),
                        video: HoldVideo::Accepted {
                            asset: asset_id("video"),
                            frames: range(10, 18),
                        },
                        picture_context: None,
                        audio: HoldAudio::Silence,
                    },
                }),
            ),
        ],
    ))
    .unwrap();
    let actual = coverage(&plan, "root", q(1, 4), q(15, 4));
    assert_eq!(actual.spans.len(), 1);
    assert_eq!(
        actual.spans[0].clock,
        PictureClockSlope::AcceptedFrames(q(2, 1))
    );
    assert_oracle(&plan, &actual);
}

#[test]
fn spans_share_lookup_and_span_limits_with_every_context_in_a_batch() {
    let plan = RenderPlan::compile(&document(
        &["repeat", "pause"],
        vec![
            ("repeat", repeat("source", 80, 0, "plays")),
            ("source", source(1, 0, 1001)),
            ("pause", hold(1)),
        ],
    ))
    .unwrap();
    let request = ScopedHoldContextRequest {
        target: super::definition::target("pause", &[]),
        direction: ExtensionDirection::FromLeft,
        native_rate: FrameRate::new(30000, 1001).unwrap(),
        frame_count: 64,
    };
    let single = plan.scoped_hold_context(&request, ample()).unwrap();
    assert_eq!(single.coverage.spans.len(), 64);
    let exact = BoundaryQueryLimits {
        max_scopes: single.lookup.visited_nodes,
        max_comparisons: single.lookup.sequence_comparisons
            + single.lookup.iteration_run_comparisons,
    };
    assert_eq!(plan.scoped_hold_context(&request, exact).unwrap(), single);
    for reduced in [
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
            plan.scoped_hold_context(&request, reduced),
            Err(PlanError::PictureQueryLimit(_))
        ));
    }
    assert_eq!(
        plan.scoped_hold_context_batch(&vec![request.clone(); 8], ample())
            .unwrap()
            .len(),
        8
    );
    assert!(matches!(
        plan.scoped_hold_context_batch(&vec![request; 9], ample()),
        Err(PlanError::PictureQueryLimit("picture spans"))
    ));
}

#[test]
fn huge_repeat_queries_stop_at_the_span_cap_without_expansion() {
    let plan = RenderPlan::compile(&document(
        &["repeat"],
        vec![
            ("repeat", repeat("source", 1_000_000_000, 0, "plays")),
            ("source", source(1, 0, 1001)),
        ],
    ))
    .unwrap();
    let exact = coverage(&plan, "repeat", q(1, 2), q(1023, 2));
    assert_eq!(
        exact.spans.len(),
        deadpan_plan::MAX_DEFINITION_PICTURE_SPANS
    );
    assert!(exact.lookup.visited_nodes <= 1026);
    assert!(matches!(
        plan.definition_picture_coverage(&id("repeat"), q(1, 2), q(1025, 2), ample()),
        Err(PlanError::PictureQueryLimit("picture spans"))
    ));
}

#[test]
fn explicit_gap_branches_and_dormant_gap_definitions_keep_their_own_clocks() {
    let base = document(
        &["repeat"],
        vec![
            ("repeat", repeat("source", 2, 1, "plays")),
            ("source", source(2, 0, 2002)),
        ],
    );
    let mut wire = serde_json::to_value(base).unwrap();
    for (name, value) in [
        (
            "gap",
            BeatNode::sequence("Gap", vec![id("gap-fast"), id("gap-freeze")]),
        ),
        ("gap-fast", retime("gap-source", 1, 1, 3)),
        ("gap-source", source(4, 10010, 14014)),
        (
            "gap-freeze",
            node(NodeKind::Hold {
                recipe: HoldRecipe {
                    duration: duration(1),
                    video: HoldVideo::Freeze {
                        asset: asset_id("video"),
                        timestamp: span(14014, 15015).start(),
                    },
                    picture_context: None,
                    audio: HoldAudio::Silence,
                },
            }),
        ),
        ("dormant", source(4, 30030, 34034)),
    ] {
        wire["nodes"][name] = serde_json::to_value(value).unwrap();
    }
    wire["gap_overrides"]["repeat"] = serde_json::json!([
        {"iteration":{"allocation":"plays","ordinal":0},"root":"gap"},
        {"iteration":{"allocation":"plays","ordinal":1},"root":"dormant"}
    ]);
    let plan =
        RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap();
    let actual = coverage(&plan, "repeat", q(3, 2), q(9, 2));
    assert_eq!(actual.spans.len(), 4);
    assert_eq!(actual.spans[1].start.instance.node, id("gap-source"));
    assert_eq!(
        actual.spans[1].clock,
        PictureClockSlope::SourceTicks(q(2002, 1))
    );
    assert_eq!(actual.spans[2].start.instance.node, id("gap-freeze"));
    assert_eq!(actual.spans[2].clock, PictureClockSlope::Constant);
    assert!(actual.spans.iter().all(|s| s.start.gap_after.is_none()));
    assert_oracle(&plan, &actual);
    let dormant = coverage(&plan, "dormant", q(1, 2), q(7, 2));
    assert_eq!(dormant.spans.len(), 1);
    assert!(dormant.spans[0].start.instance.repeats.is_empty());
    assert_oracle(&plan, &dormant);
}

#[test]
fn affine_source_span_keeps_exact_selection_and_endpoint_policy() {
    let measured = index("video", clock(), &[0, 500, 1000, 1500, 2000], 2500);
    for endpoints in [EndpointPolicy::Reject, EndpointPolicy::HoldAdjacent] {
        let plan = RenderPlan::compile(&document(
            &["outer"],
            vec![
                (
                    "source",
                    source_with_mapping(
                        2,
                        0,
                        1500,
                        SourceVideoMapping::Duration {
                            frames: q(3, 2),
                            endpoints,
                        },
                    ),
                ),
                ("inner", retime("source", 4, 0, 2)),
                ("outer", retime("inner", 8, 0, 4)),
            ],
        ))
        .unwrap();
        let actual = coverage(&plan, "root", q(1, 2), q(15, 2));
        assert_eq!(actual.spans.len(), 1);
        assert_eq!(
            actual.spans[0].clock,
            PictureClockSlope::SourceTicks(q(250, 1))
        );
        assert_oracle(&plan, &actual);
        let selected = actual.terminal.picture.select_source_frame(&measured);
        match endpoints {
            EndpointPolicy::Reject => assert!(selected.is_err()),
            EndpointPolicy::HoldAdjacent => {
                assert_eq!(selected.unwrap().identity, SourceFrameId(2))
            }
        }
    }
}

#[test]
fn coverage_seeks_a_wide_sequence_with_logarithmic_comparisons() {
    let names: Vec<_> = (0..9000).map(|i| format!("source-{i}")).collect();
    let roots: Vec<_> = names.iter().map(String::as_str).collect();
    let nodes = names
        .iter()
        .map(|name| (name.as_str(), source(1, 0, 1001)))
        .collect();
    let plan = RenderPlan::compile(&document(&roots, nodes)).unwrap();
    let actual = coverage(&plan, "root", q(17_995, 2), q(17_999, 2));
    assert_eq!(actual.spans.len(), 3);
    assert_eq!(actual.lookup.visited_nodes, 8);
    assert!(actual.lookup.sequence_comparisons <= 56);
    assert_oracle(&plan, &actual);
}

fn assert_exact_work_limits(
    plan: &RenderPlan,
    start: ExactRatio,
    end: ExactRatio,
    actual: &DefinitionPictureCoverage,
) {
    let exact = BoundaryQueryLimits {
        max_scopes: actual.lookup.visited_nodes,
        max_comparisons: actual.lookup.sequence_comparisons
            + actual.lookup.iteration_run_comparisons,
    };
    assert_eq!(
        &plan
            .definition_picture_coverage(&id("root"), start, end, exact)
            .unwrap(),
        actual
    );
    assert!(exact.max_scopes > 0 && exact.max_comparisons > 0);
    for reduced in [
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
            plan.definition_picture_coverage(&id("root"), start, end, reduced),
            Err(PlanError::PictureQueryLimit(_))
        ));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn random_cutaway_neighborhoods_match_canonical_queries_after_rational_retime(
        host_frames in 4_i64..65,
        output_frames in 1_i64..41,
        mapping_seed in 0_i64..64,
        mapping_length_seed in 0_i64..64,
        cut_seed in 0_i64..64,
        cut_length_seed in 0_i64..64,
        selected_ticks in 501_i64..6007,
        fit in 0_usize..4,
        removed in any::<bool>(),
        denominator in 3_i64..12,
    ) {
        let map_start = mapping_seed % host_frames;
        let map_end = map_start + 1 + mapping_length_seed % (host_frames - map_start);
        let cut_start = cut_seed % host_frames;
        let cut_end = cut_start + 1 + cut_length_seed % (host_frames - cut_start);
        let mut host = source(host_frames, 0, host_frames * 1001);
        host.cutaways.push(Cutaway {
            range: range(cut_start, cut_end),
            asset: asset_id("video"),
            // Fractional local-frame lengths force Loop/Bounce boundaries away
            // from integer frames before the additional rational Retime.
            selection: ExactSourceSpan::from(span(30030, 30030 + selected_ticks)),
            fit: [CutawayFit::Hold, CutawayFit::Loop, CutawayFit::Gap, CutawayFit::Bounce][fit],
            removed,
        });
        let plan = RenderPlan::compile(&document(&["speed"], vec![
            ("host", host),
            ("speed", retime("host", output_frames, map_start, map_end)),
        ])).unwrap();
        let start = q(1, denominator);
        let end = q(output_frames, 1).checked_sub(start).unwrap();
        let actual = coverage(&plan, "root", start, end);
        // The generator's minimum 501-tick pass and maximum 64 local frames
        // keep every legal case below the real 512-span cap; no errors are
        // filtered or converted into successful property cases.
        prop_assert!(actual.spans.len() <= 132);
        assert_oracle(&plan, &actual);
        assert_exact_work_limits(&plan, start, end, &actual);
    }

    #[test]
    fn random_source_hold_repeat_mixes_preserve_affine_spans_and_work_bounds(
        source_frames in 1_i64..17,
        hold_frames in 1_i64..17,
        selected_frames in 1_i64..9,
        provider in 0_usize..5,
        plays in 1_u32..7,
        gap in 0_i64..5,
        output_frames in 1_i64..41,
        mapping_seed in 0_i64..300,
        mapping_length_seed in 0_i64..300,
        denominator in 3_i64..12,
    ) {
        let video = match provider {
            0 => HoldVideo::Background,
            1 => HoldVideo::Freeze { asset: asset_id("video"), timestamp: span(5005, 6006).start() },
            2 => HoldVideo::Play { asset: asset_id("video"), span: span(5005, 5005 + selected_frames * 1001) },
            3 => HoldVideo::Reverse { asset: asset_id("video"), span: span(5005, 5005 + selected_frames * 1001) },
            _ => HoldVideo::Accepted { asset: asset_id("video"), frames: range(10, 10 + hold_frames) },
        };
        let child_frames = source_frames + hold_frames;
        let repeated_frames = child_frames * i64::from(plays) + gap * i64::from(plays - 1);
        let map_start = mapping_seed % repeated_frames;
        let map_end = map_start + 1 + mapping_length_seed % (repeated_frames - map_start);
        let plan = RenderPlan::compile(&document(&["prefix", "speed", "suffix"], vec![
            ("prefix", source(2, 0, 2002)),
            ("speed", retime("repeat", output_frames, map_start, map_end)),
            ("repeat", repeat("local", plays, gap, "plays")),
            ("local", BeatNode::sequence("Local", vec![id("source"), id("hold")])),
            ("source", source(source_frames, 20020, 20020 + source_frames * 1001)),
            ("hold", node(NodeKind::Hold { recipe: HoldRecipe { duration: duration(hold_frames),
                video, picture_context: None, audio: HoldAudio::Silence } })),
            ("suffix", source(2, 40040, 42042)),
        ])).unwrap();
        let start = q(1, denominator);
        let end = q(output_frames + 4, 1).checked_sub(start).unwrap();
        let actual = coverage(&plan, "root", start, end);
        prop_assert!(actual.spans.len() <= 26);
        assert_oracle(&plan, &actual);
        assert_exact_work_limits(&plan, start, end, &actual);
    }
}

fn vfr_picture(point: ExactRatio, endpoints: EndpointPolicy) -> Picture {
    Picture::Source {
        asset: asset_id("video"),
        point: SourcePoint {
            ticks: point,
            time_base: clock(),
        },
        span: span(0, 24),
        selection: ExactSourceSpan::from(span(0, 24)),
        endpoints,
    }
}

#[test]
fn measured_span_ordinals_preserve_vfr_forward_exclusion_reverse_and_zero_slope() {
    let measured = index("video", clock(), &[0, 3, 11, 20], 24);
    for (start, rate, distance, first, last) in [
        (q(0, 1), q(1, 1), q(11, 1), 0, 1),
        (q(3, 1), q(1, 1), q(8, 1), 1, 1),
        (q(3, 1), q(1, 1), q(8001, 1000), 1, 2),
        (q(20, 1), q(-1, 1), q(9, 1), 3, 2),
        (q(19, 1), q(-1, 1), q(8, 1), 2, 2),
        (q(11, 1), q(-1, 1), q(8, 1), 2, 1),
        (q(11, 1), q(0, 1), q(40, 1), 2, 2),
        (q(20, 1), q(1, 1), q(0, 1), 3, 3),
        (q(0, 1), q(1, 1), q(24, 1), 0, 3),
    ] {
        let actual = PictureClockSlope::SourceTicks(rate)
            .source_ordinals(
                &vfr_picture(start, EndpointPolicy::Reject),
                distance,
                &measured,
            )
            .unwrap();
        assert_eq!(
            actual,
            (SourceFrameId(first), SourceFrameId(last)),
            "start={start:?}, rate={rate:?}, distance={distance:?}"
        );
    }
    let freeze = Picture::Freeze {
        asset: asset_id("video"),
        point: SourcePoint {
            ticks: q(9, 1),
            time_base: clock(),
        },
    };
    assert_eq!(
        PictureClockSlope::Constant
            .source_ordinals(&freeze, q(100, 1), &measured)
            .unwrap(),
        (SourceFrameId(1), SourceFrameId(1))
    );
    assert!(
        PictureClockSlope::SourceTicks(q(0, 1))
            .source_ordinals(&freeze, q(1, 1), &measured)
            .is_err()
    );
}

#[test]
fn measured_span_support_holds_only_selected_picture_intervals() {
    let measured = index("video", clock(), &[0, 3, 11, 20], 24);
    for (start, slope, expected) in [(q(-10, 1), q(50, 1), (1, 2)), (q(40, 1), q(-50, 1), (2, 1))] {
        let mut picture = vfr_picture(start, EndpointPolicy::HoldAdjacent);
        let Picture::Source { selection, .. } = &mut picture else {
            unreachable!()
        };
        *selection = ExactSourceSpan::from(span(3, 20));
        assert_eq!(
            PictureClockSlope::SourceTicks(slope)
                .source_ordinals(&picture, q(1, 1), &measured)
                .unwrap(),
            (SourceFrameId(expected.0), SourceFrameId(expected.1))
        );
    }
    for (selection, expected) in [(span(0, 11), 1), (span(0, 12), 2)] {
        let mut picture = vfr_picture(q(0, 1), EndpointPolicy::HoldAdjacent);
        let Picture::Source {
            selection: selected,
            ..
        } = &mut picture
        else {
            unreachable!()
        };
        *selected = selection.into();
        assert_eq!(
            PictureClockSlope::SourceTicks(q(1, 1))
                .source_ordinals(&picture, q(20, 1), &measured)
                .unwrap()
                .1,
            SourceFrameId(expected)
        );
    }
    let reject = vfr_picture(q(3, 1), EndpointPolicy::Reject);
    for (slope, distance) in [
        (q(1, 1), q(22, 1)),
        (q(-1, 1), q(4, 1)),
        (q(1, 1), q(-1, 1)),
    ] {
        assert!(
            PictureClockSlope::SourceTicks(slope)
                .source_ordinals(&reject, distance, &measured)
                .is_err()
        );
    }
    assert!(
        PictureClockSlope::AcceptedFrames(q(1, 1))
            .source_ordinals(&reject, q(1, 1), &measured)
            .is_err()
    );
    let wrong_asset = index("other", clock(), &[0, 3, 11, 20], 24);
    assert!(matches!(
        PictureClockSlope::Constant.source_ordinals(&reject, q(0, 1), &wrong_asset),
        Err(PlanError::IndexAssetMismatch { .. })
    ));
    let wrong_clock = index(
        "video",
        SourceTimeBase::new(1, 100).unwrap(),
        &[0, 3, 11, 20],
        24,
    );
    assert!(matches!(
        PictureClockSlope::Constant.source_ordinals(&reject, q(0, 1), &wrong_clock),
        Err(PlanError::IndexClockMismatch { .. })
    ));
}

#[test]
fn measured_span_wrapper_keeps_the_closed_terminal_distinct() {
    let plan =
        RenderPlan::compile(&document(&["source"], vec![("source", source(24, 0, 24))])).unwrap();
    let actual = coverage(&plan, "root", q(0, 1), q(20, 1));
    let measured = index("video", clock(), &[0, 3, 11, 20], 24);
    assert_eq!(actual.spans.len(), 1);
    assert_eq!(
        actual.spans[0].source_ordinals(&measured).unwrap(),
        (SourceFrameId(0), SourceFrameId(2))
    );
    let terminal = deadpan_plan::DefinitionPictureSpan::observation(
        actual.terminal,
        q(20, 1),
        PictureClockSlope::Constant,
    );
    assert_eq!(
        terminal.source_ordinals(&measured).unwrap(),
        (SourceFrameId(3), SourceFrameId(3))
    );
}

#[test]
fn accepted_support_uses_exact_output_extent_and_rejects_noncanonical_descriptors() {
    let picture = Picture::Accepted {
        asset: asset_id("video"),
        generated: None,
        time_base: clock(),
        position: q(1, 2),
        frame: SourceFrameId(0),
    };
    let moving = PictureClockSlope::AcceptedFrames(q(3, 2));
    assert_eq!(
        moving
            .accepted_ordinals(&picture, q(11, 3), duration(6))
            .unwrap(),
        (SourceFrameId(0), SourceFrameId(5))
    );
    assert!(
        moving
            .accepted_ordinals(&picture, q(11001, 3000), duration(6))
            .is_err()
    );
    assert_eq!(
        moving
            .accepted_ordinals(&picture, ExactRatio::ZERO, duration(6))
            .unwrap(),
        (SourceFrameId(0), SourceFrameId(0))
    );
    assert_eq!(
        PictureClockSlope::Constant
            .accepted_ordinals(&picture, q(100, 1), duration(6))
            .unwrap(),
        (SourceFrameId(0), SourceFrameId(0))
    );
    assert!(
        PictureClockSlope::AcceptedFrames(q(-1, 1))
            .accepted_ordinals(&picture, q(1, 1), duration(6))
            .is_err()
    );
    assert!(
        PictureClockSlope::SourceTicks(q(1, 1))
            .accepted_ordinals(&picture, q(1, 1), duration(6))
            .is_err()
    );
    assert!(
        moving
            .accepted_ordinals(&picture, q(-1, 1), duration(6))
            .is_err()
    );
    assert!(
        moving
            .accepted_ordinals(&picture, q(0, 1), FrameDuration::ZERO)
            .is_err()
    );
    let mut wrong = picture.clone();
    let Picture::Accepted { frame, .. } = &mut wrong else {
        unreachable!()
    };
    *frame = SourceFrameId(1);
    assert!(
        moving
            .accepted_ordinals(&wrong, q(1, 1), duration(6))
            .is_err()
    );
    let measured = index("video", clock(), &[0, 3, 11, 20, 21, 22], 24);
    assert_eq!(
        moving
            .source_ordinals(&picture, q(11, 3), &measured)
            .unwrap(),
        moving
            .accepted_ordinals(&picture, q(11, 3), duration(6))
            .unwrap()
    );
}

#[test]
fn accepted_index_support_requires_the_retained_sampled_count() {
    let (artifact, _) = generated_fixture("coverage-", ['a', 'b', 'c']);
    let picture = Picture::Accepted {
        asset: artifact.sampled_asset.clone(),
        generated: Some(std::sync::Arc::new(artifact)),
        time_base: clock(),
        position: q(29, 1),
        frame: SourceFrameId(29),
    };
    let measured = |frames| {
        let pts: Vec<_> = (0..frames).map(|ordinal| ordinal * 1001).collect();
        index("coverage-sampled", clock(), &pts, frames * 1001)
    };
    let constant = PictureClockSlope::Constant;
    assert_eq!(
        constant
            .source_ordinals(&picture, q(1, 1), &measured(30))
            .unwrap(),
        (SourceFrameId(29), SourceFrameId(29))
    );
    assert!(
        constant
            .source_ordinals(&picture, q(1, 1), &measured(25))
            .is_err()
    );
    // An extra measured picture also disagrees, even though the requested
    // ordinal itself exists. The sampled master is exactly thirty pictures.
    assert!(
        constant
            .source_ordinals(&picture, q(1, 1), &measured(31))
            .is_err()
    );
}

fn generated_coverage(freeze: bool) -> (ProjectDocument, RenderPlan, DefinitionPictureCoverage) {
    let mut pause = hold(30);
    if freeze {
        let NodeKind::Hold { recipe } = &mut pause.kind else {
            unreachable!()
        };
        recipe.video = HoldVideo::Freeze {
            asset: asset_id("video"),
            timestamp: span(1001, 2002).start(),
        };
    }
    pause.cutaways.push(Cutaway {
        range: range(5, 6),
        asset: asset_id("video"),
        selection: span(20020, 21021).into(),
        fit: CutawayFit::Hold,
        removed: false,
    });
    let before = document(&["pause"], vec![("pause", pause)]);
    let (artifact, assets) = generated_fixture("fallback-", ['a', 'b', 'c']);
    let command = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: revision("accepted"),
        command: Command::AcceptGeneratedHold {
            node: id("pause"),
            artifact,
            assets,
        },
    };
    let accepted = deadpan_core::apply(&before, &command)
        .unwrap()
        .forward
        .apply(&before)
        .unwrap();
    let plan = RenderPlan::compile(&accepted).unwrap();
    let spans = coverage(&plan, "root", q(0, 1), q(10, 1));
    (accepted, plan, spans)
}

#[test]
fn whole_span_fallback_preserves_interval_and_refuses_a_generated_holds_cutaway() {
    for freeze in [false, true] {
        let (_, plan, coverage) = generated_coverage(freeze);
        assert_eq!(coverage.spans.len(), 3);
        for index in [0, 2] {
            let span = &coverage.spans[index];
            let fallback = plan.definition_hold_fallback_span(span).unwrap().unwrap();
            assert_eq!(fallback.start.position, span.start.position);
            assert_eq!(fallback.end_exclusive, span.end_exclusive);
            assert_eq!(fallback.clock, PictureClockSlope::Constant);
            assert_eq!(
                fallback.start.picture,
                if freeze {
                    Picture::Freeze {
                        asset: asset_id("video"),
                        point: SourcePoint {
                            ticks: q(1001, 1),
                            time_base: clock(),
                        },
                    }
                } else {
                    Picture::Background
                }
            );
            assert!(
                plan.definition_hold_fallback_span(&fallback).is_err(),
                "substitution grants no new canonical provider proof"
            );
        }
        assert_eq!(
            plan.definition_hold_fallback_span(&coverage.spans[1])
                .unwrap(),
            None
        );
    }
}

#[test]
fn whole_span_fallback_rejects_changed_public_fields_and_another_compiled_plan() {
    let (document, plan, coverage) = generated_coverage(false);
    let original = &coverage.spans[0];
    let mutations: [fn(&mut deadpan_plan::DefinitionPictureSpan); 12] = [
        |span| span.end_exclusive = q(6, 1), // crosses the hidden cutaway
        |span| span.end_exclusive = q(4, 1), // even a forged subinterval
        |span| span.clock = PictureClockSlope::Constant,
        |span| span.start.position = q(1, 1),
        |span| span.start.local_position = q(1, 1),
        |span| span.start.definition = id("pause"),
        |span| span.start.instance.node = id("other"),
        |span| span.start.revision_id = revision("later"),
        |span| span.start.project_id = ProjectId::new("other").unwrap(),
        |span| span.start.picture = Picture::Background,
        |span| span.start.framing.clear(),
        |span| span.start.lookup.visited_nodes += 1,
    ];
    assert!(!original.start.framing.is_empty());
    for mutate in mutations {
        let mut forged = original.clone();
        mutate(&mut forged);
        assert!(plan.definition_hold_fallback_span(&forged).is_err());
    }
    let separately_compiled = RenderPlan::compile(&document).unwrap();
    assert!(
        separately_compiled
            .definition_hold_fallback_span(original)
            .is_err()
    );
    let mut transplanted = original.clone();
    transplanted.start = separately_compiled
        .definition_picture_coverage(&id("root"), q(0, 1), q(10, 1), ample())
        .unwrap()
        .spans[0]
        .start
        .clone();
    assert!(plan.definition_hold_fallback_span(&transplanted).is_err());
    assert!(
        separately_compiled
            .definition_hold_fallback_span(&transplanted)
            .is_err()
    );
    let raw = deadpan_plan::DefinitionPictureSpan::observation(
        original.start.clone(),
        original.end_exclusive,
        original.clock,
    );
    assert!(plan.definition_hold_fallback_span(&raw).is_err());
    assert_eq!(
        serde_json::to_value(original).unwrap(),
        serde_json::to_value(raw).unwrap(),
        "private capability is absent from inspection JSON"
    );
}
