use super::definition::target;
use super::*;
use deadpan_plan::{
    MAX_HOLD_CONTEXT_BATCH, MAX_HOLD_CONTEXT_FRAMES, ScopedHoldContext, ScopedHoldContextRequest,
};

fn ample() -> BoundaryQueryLimits {
    BoundaryQueryLimits {
        max_scopes: 1_000_000,
        max_comparisons: 1_000_000,
    }
}

fn request(direction: ExtensionDirection) -> ScopedHoldContextRequest {
    ScopedHoldContextRequest {
        target: target("pause", &[]),
        direction,
        native_rate: FrameRate::new(24, 1).unwrap(),
        frame_count: 9,
    }
}

fn ordinary() -> ProjectDocument {
    document(
        &["left", "pause", "right"],
        vec![
            ("left", source(20, 0, 20020)),
            ("pause", hold(3)),
            ("right", source(20, 20020, 40040)),
        ],
    )
}

fn work(contexts: &[ScopedHoldContext]) -> BoundaryQueryLimits {
    BoundaryQueryLimits {
        max_scopes: contexts.iter().map(|c| c.lookup.visited_nodes).sum(),
        max_comparisons: contexts
            .iter()
            .map(|c| c.lookup.sequence_comparisons + c.lookup.iteration_run_comparisons)
            .sum(),
    }
}

#[test]
fn context_uses_native_time_spacing_with_exact_fractional_project_rate() {
    let plan = RenderPlan::compile(&ordinary()).unwrap();
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let actual = plan
            .scoped_hold_context(&request(direction), ample())
            .unwrap();
        assert_eq!(actual.pictures.len(), 9);
        for (i, sample) in actual.pictures.iter().enumerate() {
            // Independent source-time oracle: 24 Hz is exactly 1250 ticks of
            // this 30000 Hz fixture. Neither rounded project ordinals nor a
            // 1001-tick (project-frame) step describe the supplied context.
            let first_twice_ticks = match direction {
                ExtensionDirection::FromLeft => 19_039,
                ExtensionDirection::FromRight => 41_041,
            };
            assert_eq!(
                ticks(&sample.picture),
                ExactRatio::new(first_twice_ticks + 2500 * i as i128, 2).unwrap()
            );
            assert_eq!(sample.definition, id("root"));
            assert!(sample.instance.repeats.is_empty());
            assert_eq!(
                sample,
                &plan
                    .definition_picture(&id("root"), sample.position, ample())
                    .unwrap()
            );
        }
        let (anchor, index) = match direction {
            ExtensionDirection::FromLeft => (actual.boundaries.left.as_ref().unwrap(), 8),
            ExtensionDirection::FromRight => (actual.boundaries.right.as_ref().unwrap(), 0),
        };
        assert_eq!(&actual.pictures[index], anchor);
    }
}

#[test]
fn thirty_fps_context_is_not_nine_consecutive_project_frames() {
    let mut wire = serde_json::to_value(ordinary()).unwrap();
    wire["presentation_basis"]["frame_rate"] = serde_json::json!({"numerator":30,"denominator":1});
    let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = RenderPlan::compile(&doc).unwrap();
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let actual = plan
            .scoped_hold_context(&request(direction), ample())
            .unwrap();
        let first = match direction {
            ExtensionDirection::FromLeft => 38,
            ExtensionDirection::FromRight => 94,
        };
        for (i, sample) in actual.pictures.iter().enumerate() {
            assert_eq!(
                sample.position,
                ExactRatio::new(first + 5 * i as i128, 4).unwrap()
            );
        }
    }
}

#[test]
fn definition_edges_are_absent_and_short_context_is_never_padded() {
    for (direction, roots) in [
        (ExtensionDirection::FromLeft, vec!["pause", "left"]),
        (ExtensionDirection::FromRight, vec!["left", "pause"]),
    ] {
        let doc = document(
            &roots,
            vec![("left", source(20, 0, 20020)), ("pause", hold(3))],
        );
        let plan = RenderPlan::compile(&doc).unwrap();
        assert!(matches!(
            plan.scoped_hold_context(&request(direction), ample()),
            Err(PlanError::InvalidScopedHold(
                "the requested extension anchor is absent"
            ))
        ));
    }
    let doc = document(
        &["outer"],
        vec![
            ("outer", repeat("local", 1_000_000, 0, "outer")),
            (
                "local",
                BeatNode::sequence("Local", vec![id("left"), id("pause"), id("right")]),
            ),
            ("left", source(2, 0, 2002)),
            ("pause", hold(3)),
            ("right", source(2, 2002, 4004)),
        ],
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let mut query = request(direction);
        query.target = target("pause", &[("outer", Some(500_000))]);
        assert!(
            matches!(plan.scoped_hold_context(&query, ample()), Err(PlanError::DefinitionPictureOutOfRange { definition, .. }) if definition == id("local"))
        );
        query.frame_count = 1;
        assert_eq!(
            plan.scoped_hold_context(&query, ample())
                .unwrap()
                .pictures
                .len(),
            1
        );
    }
}

fn nested_context() -> ProjectDocument {
    document(
        &["speed"],
        vec![
            ("speed", retime("outer", 43, 0, 86)),
            ("outer", repeat("local", 2, 0, "outer")),
            (
                "local",
                BeatNode::sequence("Local", vec![id("left"), id("pause"), id("right")]),
            ),
            ("left", source(20, 0, 20020)),
            ("pause", hold(3)),
            ("right", source(20, 20020, 40040)),
        ],
    )
}

#[test]
fn outer_retime_and_shared_repeat_do_not_change_the_authored_context_clock() {
    let doc = nested_context();
    let plan = RenderPlan::compile(&doc).unwrap();
    let ordinary = RenderPlan::compile(&ordinary()).unwrap();
    for branch in [None, Some(0), Some(1)] {
        for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
            let mut query = request(direction);
            query.target = target("pause", &[("outer", branch)]);
            let actual = plan.scoped_hold_context(&query, ample()).unwrap();
            let expected = ordinary
                .scoped_hold_context(&request(direction), ample())
                .unwrap();
            assert_eq!(actual.boundaries.definition, id("local"));
            assert_eq!(actual.boundaries.range, expected.boundaries.range);
            for (left, right) in actual.pictures.iter().zip(&expected.pictures) {
                assert_eq!(left.position, right.position);
                assert_eq!(left.picture, right.picture);
            }
            if branch.is_some() {
                assert!(matches!(
                    plan.scoped_hold_context_batch(&[query], ample()),
                    Err(PlanError::InvalidScopedHold(_))
                ));
            }
        }
    }
}

#[test]
fn dormant_defaults_and_owned_play_context_keep_their_distinct_scope() {
    let mut wire = serde_json::to_value(nested_context()).unwrap();
    for n in 0..2 {
        let name = format!("owned-{n}");
        wire["nodes"][&name] = serde_json::to_value(hold(43)).unwrap();
    }
    wire["overrides"]["outer"] = serde_json::json!([
        {"iteration":{"allocation":"outer","ordinal":0},"root":"owned-0"},
        {"iteration":{"allocation":"outer","ordinal":1},"root":"owned-1"},
    ]);
    let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = RenderPlan::compile(&doc).unwrap();
    let mut query = request(ExtensionDirection::FromLeft);
    query.target = target("pause", &[("outer", None)]);
    assert_eq!(
        plan.scoped_hold_context_batch(&[query.clone()], ample())
            .unwrap()[0]
            .pictures
            .len(),
        9
    );
    query.target = target("pause", &[("outer", Some(0))]);
    assert!(matches!(
        plan.scoped_hold_context(&query, ample()),
        Err(PlanError::InvalidScopedHold(_))
    ));
    query.target = target("owned-0", &[("outer", Some(0))]);
    assert!(matches!(
        plan.scoped_hold_context_batch(&[query], ample()),
        Err(PlanError::InvalidScopedHold(
            "the requested extension anchor is absent"
        ))
    ));
}

#[test]
fn context_batch_shares_one_exact_work_budget_and_preserves_duplicates() {
    let plan = RenderPlan::compile(&ordinary()).unwrap();
    let queries = [
        request(ExtensionDirection::FromRight),
        request(ExtensionDirection::FromLeft),
        request(ExtensionDirection::FromRight),
    ];
    let expected = plan.scoped_hold_context_batch(&queries, ample()).unwrap();
    assert_eq!(expected[0], expected[2]);
    for (query, actual) in queries.iter().zip(&expected) {
        let mut independent = plan.scoped_hold_context(query, ample()).unwrap();
        independent.lookup = actual.lookup;
        independent.boundaries.lookup = actual.boundaries.lookup;
        assert_eq!(actual, &independent);
    }
    let exact = work(&expected);
    assert_eq!(
        plan.scoped_hold_context_batch(&queries, exact).unwrap(),
        expected
    );
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
            plan.scoped_hold_context_batch(&queries, reduced),
            Err(PlanError::PictureQueryLimit(_))
        ));
    }
    let single = plan.scoped_hold_context(&queries[0], ample()).unwrap();
    let single_budget = work(std::slice::from_ref(&single));
    assert_eq!(
        plan.scoped_hold_context(&queries[0], single_budget)
            .unwrap(),
        single
    );
    assert!(
        plan.scoped_hold_context(
            &queries[0],
            BoundaryQueryLimits {
                max_scopes: single_budget.max_scopes - 1,
                ..single_budget
            }
        )
        .is_err()
    );
}

#[test]
fn context_limits_reject_before_sampling_and_unknown_targets_never_retarget() {
    let plan = RenderPlan::compile(&ordinary()).unwrap();
    let mut query = request(ExtensionDirection::FromLeft);
    for count in [0, MAX_HOLD_CONTEXT_FRAMES + 1, u32::MAX] {
        query.frame_count = count;
        assert!(matches!(
            plan.scoped_hold_context(&query, ample()),
            Err(PlanError::PictureQueryLimit("context frames"))
        ));
        assert!(
            plan.scoped_hold_context_batch(&[query.clone()], ample())
                .is_err()
        );
    }
    query.frame_count = 9;
    assert!(matches!(
        plan.scoped_hold_context_batch(&vec![query.clone(); MAX_HOLD_CONTEXT_BATCH + 1], ample()),
        Err(PlanError::PictureQueryLimit("context requests"))
    ));
    query.target.node = id("missing");
    assert!(matches!(
        plan.scoped_hold_context(&query, ample()),
        Err(PlanError::InvalidScopedHold(_))
    ));
    query.target.node = id("left");
    assert!(matches!(
        plan.scoped_hold_context_batch(&[query], ample()),
        Err(PlanError::InvalidScopedHold(
            "the target is not an authored Hold"
        ))
    ));
}

#[test]
fn context_preserves_descendant_retime_cutaways_and_explicit_black() {
    let mut left = source(40, 0, 40040);
    left.cutaways.push(Cutaway {
        range: range(0, 20),
        asset: asset_id("video"),
        selection: ExactSourceSpan::from(span(60060, 80080)),
        fit: CutawayFit::Hold,
        removed: false,
    });
    let doc = document(
        &["speed", "pause", "black"],
        vec![
            ("speed", retime("left", 20, 0, 40)),
            ("left", left),
            ("pause", hold(3)),
            ("black", hold(20)),
        ],
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let left = plan
        .scoped_hold_context(&request(ExtensionDirection::FromLeft), ample())
        .unwrap();
    for sample in &left.pictures {
        assert_eq!(
            sample,
            &plan
                .definition_picture(&id("root"), sample.position, ample())
                .unwrap()
        );
    }
    assert!(
        ticks(&left.pictures[0].picture)
            .compare_integer(60060)
            .is_gt()
    );
    assert!(
        ticks(&left.pictures[8].picture)
            .compare_integer(40040)
            .is_lt()
    );
    let right = plan
        .scoped_hold_context(&request(ExtensionDirection::FromRight), ample())
        .unwrap();
    assert!(
        right
            .pictures
            .iter()
            .all(|sample| matches!(sample.picture, Picture::Background))
    );
    assert!(right.boundaries.right.is_some());
}

#[test]
fn temporal_context_keeps_fallback_witnesses_for_multiple_generated_neighbors() {
    let doc = document(
        &["left", "a", "b", "c", "pause"],
        vec![
            ("left", source(4, 0, 4004)),
            ("a", hold(4)),
            ("b", hold(4)),
            ("c", hold(4)),
            ("pause", hold(3)),
        ],
    );
    let mut wire = serde_json::to_value(doc).unwrap();
    for (name, hashes, tick) in [
        ("a", ['1', '2', '3'], 1001),
        ("b", ['4', '5', '6'], 2002),
        ("c", ['7', '8', '9'], 3003),
    ] {
        let (artifact, assets) = generated_fixture(name, hashes);
        wire["nodes"][name]["kind"]["recipe"]["video"] =
            serde_json::to_value(HoldVideo::Generated {
                accepted: Box::new(AcceptedGeneration {
                    artifact,
                    fallback: HoldFallback::Freeze {
                        asset: asset_id("video"),
                        timestamp: span(tick, tick + 1001).start(),
                    },
                }),
            })
            .unwrap();
        for (asset, record) in assets {
            wire["assets"][asset.as_str()] = serde_json::to_value(record).unwrap();
        }
    }
    let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = RenderPlan::compile(&doc).unwrap();
    let context = plan
        .scoped_hold_context(&request(ExtensionDirection::FromLeft), ample())
        .unwrap();
    let mut seen = std::collections::BTreeSet::new();
    for sample in &context.pictures {
        let name = sample.instance.node.as_str();
        seen.insert(name);
        let expected = match name {
            "a" => 1001,
            "b" => 2002,
            "c" => 3003,
            other => panic!("unexpected context {other}"),
        };
        assert_eq!(
            plan.definition_hold_fallback_picture(sample).unwrap(),
            Some(Picture::Freeze {
                asset: asset_id("video"),
                point: SourcePoint {
                    ticks: ExactRatio::integer(expected),
                    time_base: clock()
                },
            })
        );
        let mut forged = sample.clone();
        forged.position = ExactRatio::integer(0);
        assert!(plan.definition_hold_fallback_picture(&forged).is_err());
    }
    assert_eq!(seen, std::collections::BTreeSet::from(["a", "b", "c"]));
}

#[test]
fn deeply_nested_temporal_context_has_an_aggregate_metadata_limit() {
    let names: Vec<_> = (0..96)
        .map(|i| format!("repeat-{i:03}-{}", "x".repeat(117)))
        .collect();
    assert!(names.iter().all(|name| name.len() == 128));
    let mut nodes: Vec<(&str, BeatNode)> = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let child = names.get(i + 1).map(String::as_str).unwrap_or("left");
            (name.as_str(), repeat(child, 1, 0, name))
        })
        .collect();
    nodes.extend([("left", source(100, 0, 100000)), ("pause", hold(3))]);
    let doc = document(&[names[0].as_str(), "pause"], nodes);
    let plan = RenderPlan::compile(&doc).unwrap();
    let mut query = request(ExtensionDirection::FromLeft);
    query.frame_count = MAX_HOLD_CONTEXT_FRAMES;
    // All temporal samples and structural lookups fit. Their repeated owned
    // ancestry metadata alone would exceed 64 MiB without the separate ledger.
    for result in [
        plan.scoped_hold_context(&query, ample()).map(|_| ()),
        plan.scoped_hold_context_batch(&[query], ample())
            .map(|_| ()),
    ] {
        assert!(
            matches!(
                result,
                Err(PlanError::PictureQueryLimit("context metadata"))
            ),
            "{result:?}"
        );
    }
}
