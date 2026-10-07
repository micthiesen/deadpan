use super::*;
use deadpan_plan::{DefinitionPictureSample, PictureSample};

fn target(node: &str, repeats: &[(&str, Option<u32>)]) -> ScopedNodeTarget {
    ScopedNodeTarget {
        node: id(node),
        repeats: repeats
            .iter()
            .map(|(name, ordinal)| RepeatEditStep {
                repeat: id(name),
                branch: ordinal.map_or(RepeatEditBranch::Default, |ordinal| {
                    RepeatEditBranch::Play {
                        iteration: IterationId {
                            allocation: revision(name),
                            ordinal,
                        },
                    }
                }),
            })
            .collect(),
    }
}

fn limits() -> BoundaryQueryLimits {
    BoundaryQueryLimits::default()
}

fn same_picture(local: &DefinitionPictureSample, root: &PictureSample) {
    assert_eq!(local.project_id, root.project_id);
    assert_eq!(local.revision_id, root.revision_id);
    assert_eq!(local.instance, root.instance);
    assert_eq!(local.local_position, root.local_position);
    assert_eq!(local.gap_after, root.gap_after);
    assert_eq!(local.picture, root.picture);
    assert_eq!(local.picture_context, root.picture_context);
    assert_eq!(local.framing, root.framing);
    assert_eq!(local.captions, root.captions);
    assert_eq!(local.lookup, root.lookup);
}

fn nested(plays: u32) -> ProjectDocument {
    document(
        &["outer"],
        vec![
            ("outer", repeat("outer-group", plays, 0, "outer")),
            (
                "outer-group",
                BeatNode::sequence("Outer group", vec![id("inner")]),
            ),
            ("inner", repeat("local", 3, 0, "inner")),
            (
                "local",
                BeatNode::sequence("Local", vec![id("left"), id("pause"), id("right")]),
            ),
            ("left", framed(source(4, 0, 4004), 2)),
            ("pause", hold(3)),
            ("right", source(4, 4004, 8008)),
        ],
    )
}

#[test]
fn definition_samples_preserve_every_root_picture_field() {
    let doc = nested(2);
    let plan = RenderPlan::compile(&doc).unwrap();
    for frame in 0..plan.duration().frames() {
        let local = plan
            .definition_picture(
                doc.root(),
                ExactRatio::new(i128::from(frame) * 2 + 1, 2).unwrap(),
                limits(),
            )
            .unwrap();
        same_picture(&local, &plan.picture(ProjectFrame(frame)).unwrap());
        assert_eq!(&local.definition, doc.root());
        let wire = serde_json::to_value(&local).unwrap();
        assert!(wire.get("project_frame").is_none());
        assert_eq!(wire["definition"], "root");
    }
}

#[test]
fn ordinary_hold_boundaries_match_project_frames_with_retained_geometry() {
    let context = CapturedFraming::new(vec![CapturedCanvas {
        width: 640,
        height: 360,
        fit: CapturedFit::Fit,
        layers: vec![Some(FramingPose::identity())],
    }])
    .unwrap();
    let left = node(NodeKind::Hold {
        recipe: HoldRecipe {
            duration: duration(2),
            video: HoldVideo::Freeze {
                asset: asset_id("video"),
                timestamp: span(1001, 2002).start(),
            },
            audio: HoldAudio::Silence,
            picture_context: Some(context.clone()),
        },
    });
    let doc = document(
        &["group"],
        vec![
            (
                "group",
                framed(
                    BeatNode::sequence("Group", vec![id("left"), id("pause"), id("right")]),
                    2,
                ),
            ),
            ("left", left),
            ("pause", hold(3)),
            ("right", source(4, 4004, 8008)),
        ],
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let boundaries = plan
        .scoped_hold_boundaries(&target("pause", &[]), limits())
        .unwrap();
    assert_eq!(boundaries.definition, id("root"));
    assert_eq!(boundaries.duration, duration(3));
    assert_eq!(
        boundaries.range,
        ExactFrameRange::new(ExactRatio::integer(2), ExactRatio::integer(5)).unwrap()
    );
    same_picture(
        boundaries.left.as_ref().unwrap(),
        &plan.picture(ProjectFrame(1)).unwrap(),
    );
    same_picture(
        boundaries.right.as_ref().unwrap(),
        &plan.picture(ProjectFrame(5)).unwrap(),
    );
    assert_eq!(
        boundaries.left.as_ref().unwrap().picture_context.as_deref(),
        Some(&context)
    );
}

#[test]
fn nested_default_and_concrete_play_share_only_the_intrinsic_definition() {
    let doc = nested(2);
    let plan = RenderPlan::compile(&doc).unwrap();
    let mut previous = None;
    for outer in [None, Some(1)] {
        for inner in [None, Some(2)] {
            let selected = target("pause", &[("outer", outer), ("inner", inner)]);
            selected.validate(&doc).unwrap();
            let result = plan.scoped_hold_boundaries(&selected, limits()).unwrap();
            assert_eq!(result.target, selected);
            assert_eq!(result.definition, id("local"));
            assert_eq!(result.duration, duration(3));
            assert_eq!(result.range.start, ExactRatio::integer(4));
            assert_eq!(result.range.end, ExactRatio::integer(7));
            let left = result.left.unwrap();
            let right = result.right.unwrap();
            assert!(left.instance.repeats.is_empty());
            assert_eq!(
                left.framing
                    .iter()
                    .map(|entry| entry.instance.node.clone())
                    .collect::<Vec<_>>(),
                vec![id("left"), id("local")]
            );
            assert_eq!(
                ticks(&left.picture),
                ExactRatio::integer(3503)
                    .checked_add(ExactRatio::new(1, 2).unwrap())
                    .unwrap()
            );
            if let Some((prior_left, prior_right)) = previous {
                assert_eq!(left, prior_left);
                assert_eq!(right, prior_right);
            }
            previous = Some((left, right));
        }
    }
    let relative = plan
        .definition_picture(
            &id("outer-group"),
            ExactRatio::new(23, 2).unwrap(),
            limits(),
        )
        .unwrap();
    assert_eq!(relative.instance.node, id("left"));
    assert_eq!(
        relative.instance.repeats,
        vec![RepeatInstance {
            node: id("inner"),
            iteration: IterationId {
                allocation: revision("inner"),
                ordinal: 1
            },
        }]
    );
    assert!(
        relative.instance.validate(&doc).is_err(),
        "the branded relative address is not a project occurrence"
    );
}

#[test]
fn fully_overridden_default_remains_authored_without_a_root_occurrence() {
    let mut wire = serde_json::to_value(nested(2)).unwrap();
    for ordinal in 0..3 {
        wire["nodes"][format!("override-{ordinal}")] = serde_json::to_value(hold(1)).unwrap();
    }
    wire["overrides"]["inner"] = serde_json::json!((0..3).map(|ordinal| serde_json::json!({
        "iteration": {"allocation":"inner", "ordinal":ordinal}, "root":format!("override-{ordinal}")
    })).collect::<Vec<_>>());
    let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = RenderPlan::compile(&doc).unwrap();
    let selected = target("pause", &[("outer", None), ("inner", None)]);
    selected.validate(&doc).unwrap();
    let result = plan.scoped_hold_boundaries(&selected, limits()).unwrap();
    assert_eq!(result.duration, duration(3));
    assert!(result.left.is_some() && result.right.is_some());
    for frame in 0..plan.duration().frames() {
        assert_ne!(
            plan.picture(ProjectFrame(frame)).unwrap().instance.node,
            id("pause")
        );
    }
    assert!(matches!(
        plan.scoped_hold_boundaries(
            &target("pause", &[("outer", None), ("inner", Some(1))]),
            limits(),
        ),
        Err(PlanError::InvalidScopedHold(_))
    ));
    let selected = target("override-1", &[("outer", Some(1)), ("inner", Some(1))]);
    selected.validate(&doc).unwrap();
    let result = plan.scoped_hold_boundaries(&selected, limits()).unwrap();
    assert_eq!(result.definition, id("override-1"));
    assert!(result.left.is_none() && result.right.is_none());
}

#[test]
fn outer_retime_crop_cannot_shorten_authored_hold_or_retime_its_boundaries() {
    let mut previous = None;
    for (output, start, end) in [(11, 0, 11), (2, 4, 7), (1, 0, 2)] {
        let doc = document(
            &["speed"],
            vec![
                ("speed", framed(retime("local", output, start, end), 3)),
                (
                    "local",
                    BeatNode::sequence("Local", vec![id("left"), id("pause"), id("right")]),
                ),
                ("left", source(4, 0, 4004)),
                ("pause", hold(3)),
                ("right", source(4, 4004, 8008)),
            ],
        );
        let plan = RenderPlan::compile(&doc).unwrap();
        let result = plan
            .scoped_hold_boundaries(&target("pause", &[]), limits())
            .unwrap();
        assert_eq!(result.definition, id("local"));
        assert_eq!(result.duration, duration(3));
        if let Some(previous) = previous {
            assert_eq!(result, previous);
        }
        previous = Some(result);
    }
}

#[test]
fn internal_retime_and_fractional_vfr_points_stay_exact() {
    let doc = document(
        &["speed"],
        vec![
            ("speed", retime("local", 3, 0, 9)),
            (
                "local",
                BeatNode::sequence("Local", vec![id("left-speed"), id("pause"), id("right")]),
            ),
            ("left-speed", retime("left", 2, 2, 8)),
            ("left", source(8, 0, 8008)),
            ("pause", hold(3)),
            ("right", source(4, 8008, 12_012)),
        ],
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let result = plan
        .scoped_hold_boundaries(&target("pause", &[]), limits())
        .unwrap();
    assert_eq!(
        ticks(&result.left.unwrap().picture),
        ExactRatio::new(13_013, 2).unwrap()
    );
    assert_eq!(
        ticks(&result.right.unwrap().picture),
        ExactRatio::new(17_017, 2).unwrap()
    );
    let sample = plan
        .definition_picture(&id("left"), ExactRatio::new(1, 3).unwrap(), limits())
        .unwrap();
    assert_eq!(ticks(&sample.picture), ExactRatio::new(1001, 3).unwrap());
    let measured = index("video", clock(), &[0, 200, 1001, 2500, 6000, 7500], 8008);
    assert_eq!(
        sample
            .picture
            .select_source_frame(&measured)
            .unwrap()
            .identity,
        SourceFrameId(1)
    );
}

#[test]
fn neighboring_cutaways_and_removed_pictures_keep_root_semantics() {
    for removed in [false, true] {
        let mut left = source(2, 0, 2002);
        left.cutaways.push(Cutaway {
            range: range(1, 2),
            asset: asset_id("video"),
            selection: ExactSourceSpan::from(span(9009, 10_010)),
            fit: CutawayFit::Hold,
            removed,
        });
        let doc = document(
            &["left", "pause", "right"],
            vec![
                ("left", left),
                ("pause", hold(3)),
                ("right", source(2, 2002, 4004)),
            ],
        );
        let plan = RenderPlan::compile(&doc).unwrap();
        let result = plan
            .scoped_hold_boundaries(&target("pause", &[]), limits())
            .unwrap();
        let left = result.left.unwrap();
        same_picture(&left, &plan.picture(ProjectFrame(1)).unwrap());
        assert_ne!(
            left.picture,
            plan.provider_picture(ProjectFrame(1)).unwrap().picture
        );
        if removed {
            assert_eq!(left.picture, Picture::Background);
        } else {
            assert_eq!(ticks(&left.picture), ExactRatio::new(19_019, 2).unwrap());
        }
    }
}

#[test]
fn definition_edges_never_invent_neighbors_from_the_frozen_recipe() {
    for roots in [vec!["pause", "right"], vec!["left", "pause"], vec!["pause"]] {
        let mut nodes = vec![(
            "pause",
            node(NodeKind::Hold {
                recipe: HoldRecipe {
                    duration: duration(3),
                    video: HoldVideo::Freeze {
                        asset: asset_id("video"),
                        timestamp: span(1001, 2002).start(),
                    },
                    audio: HoldAudio::Silence,
                    picture_context: None,
                },
            }),
        )];
        if roots.contains(&"left") {
            nodes.push(("left", source(2, 0, 2002)));
        }
        if roots.contains(&"right") {
            nodes.push(("right", source(2, 2002, 4004)));
        }
        let doc = document(&roots, nodes);
        let plan = RenderPlan::compile(&doc).unwrap();
        let result = plan
            .scoped_hold_boundaries(&target("pause", &[]), limits())
            .unwrap();
        assert_eq!(result.left.is_some(), roots.contains(&"left"));
        assert_eq!(result.right.is_some(), roots.contains(&"right"));
    }
    let doc = document(
        &["before", "repeat", "after"],
        vec![
            ("before", source(2, 0, 2002)),
            ("repeat", repeat("pause", 3, 0, "repeat")),
            ("pause", hold(3)),
            ("after", source(2, 2002, 4004)),
        ],
    );
    let result = RenderPlan::compile(&doc)
        .unwrap()
        .scoped_hold_boundaries(&target("pause", &[("repeat", Some(1))]), limits())
        .unwrap();
    assert_eq!(result.definition, id("pause"));
    assert!(result.left.is_none() && result.right.is_none());
}

#[test]
fn explicit_owned_final_gap_is_a_definition_but_implicit_gap_is_not_a_hold() {
    let doc = document(
        &["repeat"],
        vec![
            ("repeat", repeat("play", 2, 1, "repeat")),
            ("play", hold(2)),
        ],
    );
    let mut wire = serde_json::to_value(doc).unwrap();
    wire["nodes"]["gap"] = serde_json::to_value(BeatNode::sequence(
        "Gap",
        vec![id("left"), id("pause"), id("right")],
    ))
    .unwrap();
    for (name, value) in [
        ("left", source(2, 0, 2002)),
        ("pause", hold(3)),
        ("right", source(2, 2002, 4004)),
    ] {
        wire["nodes"][name] = serde_json::to_value(value).unwrap();
    }
    wire["gap_overrides"]["repeat"] = serde_json::json!([{
        "iteration":{"allocation":"repeat", "ordinal":1}, "root":"gap"
    }]);
    let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = RenderPlan::compile(&doc).unwrap();
    let selected = target("pause", &[("repeat", Some(1))]);
    selected.validate(&doc).unwrap();
    let result = plan.scoped_hold_boundaries(&selected, limits()).unwrap();
    assert_eq!(result.definition, id("gap"));
    assert!(result.left.is_some() && result.right.is_some());
    for wrong in [
        target("pause", &[("repeat", None)]),
        target("pause", &[("repeat", Some(0))]),
        target("repeat", &[]),
    ] {
        assert!(matches!(
            plan.scoped_hold_boundaries(&wrong, limits()),
            Err(PlanError::InvalidScopedHold(_))
        ));
    }
    let implicit = plan
        .definition_picture(&id("repeat"), ExactRatio::new(5, 2).unwrap(), limits())
        .unwrap();
    assert_eq!(implicit.instance.node, id("repeat"));
    assert_eq!(implicit.gap_after.unwrap().ordinal, 0);
}

#[test]
fn scoped_ancestry_rejects_missing_extra_reordered_and_retired_choices() {
    let doc = nested(2);
    let plan = RenderPlan::compile(&doc).unwrap();
    for invalid in [
        target("missing", &[]),
        target("left", &[("outer", None), ("inner", None)]),
        target("pause", &[]),
        target("pause", &[("inner", None)]),
        target("pause", &[("inner", None), ("outer", None)]),
        target(
            "pause",
            &[("extra", None), ("outer", None), ("inner", None)],
        ),
        target("pause", &[("outer", Some(2)), ("inner", None)]),
    ] {
        assert!(
            matches!(
                plan.scoped_hold_boundaries(&invalid, limits()),
                Err(PlanError::InvalidScopedHold(_))
            ),
            "{invalid:?}"
        );
    }
    assert!(matches!(
        plan.definition_picture(&id("missing"), ExactRatio::ZERO, limits()),
        Err(PlanError::InvalidPictureDefinition(_))
    ));
    for position in [ExactRatio::new(-1, 2).unwrap(), ExactRatio::integer(11)] {
        assert!(matches!(
            plan.definition_picture(&id("local"), position, limits()),
            Err(PlanError::DefinitionPictureOutOfRange { .. })
        ));
    }
}

#[test]
fn one_shared_work_budget_is_independent_of_repeat_play_count() {
    let mut measured = None;
    let mut sampled = None;
    for plays in [2, u32::MAX] {
        let plan = RenderPlan::compile(&nested(plays)).unwrap();
        let selected = target("pause", &[("outer", Some(plays - 1)), ("inner", Some(2))]);
        let result = plan.scoped_hold_boundaries(&selected, limits()).unwrap();
        if let Some(previous) = measured {
            assert_eq!(result.lookup, previous);
        }
        measured = Some(result.lookup);
        let exact = BoundaryQueryLimits {
            max_scopes: result.lookup.visited_nodes,
            max_comparisons: result.lookup.sequence_comparisons
                + result.lookup.iteration_run_comparisons,
        };
        assert_eq!(
            plan.scoped_hold_boundaries(&selected, exact).unwrap(),
            result
        );
        assert!(matches!(
            plan.scoped_hold_boundaries(
                &selected,
                BoundaryQueryLimits {
                    max_scopes: exact.max_scopes - 1,
                    ..exact
                }
            ),
            Err(PlanError::PictureQueryLimit("node visits"))
        ));
        assert!(matches!(
            plan.scoped_hold_boundaries(
                &selected,
                BoundaryQueryLimits {
                    max_comparisons: exact.max_comparisons - 1,
                    ..exact
                }
            ),
            Err(PlanError::PictureQueryLimit("comparisons"))
        ));
        assert!(matches!(
            plan.definition_picture(
                &id("local"),
                ExactRatio::new(1, 2).unwrap(),
                BoundaryQueryLimits {
                    max_scopes: 0,
                    max_comparisons: 0
                }
            ),
            Err(PlanError::PictureQueryLimit("node visits"))
        ));
        assert!(matches!(
            plan.definition_picture(
                &id("local"),
                ExactRatio::new(1, 2).unwrap(),
                BoundaryQueryLimits {
                    max_scopes: 2,
                    max_comparisons: 0
                }
            ),
            Err(PlanError::PictureQueryLimit("comparisons"))
        ));
        let final_frame = plan.duration().frames() - 1;
        let picture = plan
            .definition_picture(
                &id("root"),
                ExactRatio::new(i128::from(final_frame) * 2 + 1, 2).unwrap(),
                limits(),
            )
            .unwrap();
        if let Some(previous) = sampled {
            assert_eq!(picture.lookup, previous);
        }
        sampled = Some(picture.lookup);
    }
}
