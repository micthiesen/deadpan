use std::collections::BTreeMap;

use deadpan_core::*;
use deadpan_plan::{Picture, PlanError, RenderPlan};
use proptest::prelude::*;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn asset_id(value: &str) -> AssetId {
    AssetId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn duration(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}
fn clock() -> SourceTimeBase {
    SourceTimeBase::new(1, 30_000).unwrap()
}
fn span(start: i64, end: i64) -> SourceSpan {
    SourceSpan::new(
        SourceTimestamp {
            ticks: start,
            time_base: clock(),
        },
        SourceTimestamp {
            ticks: end,
            time_base: clock(),
        },
    )
    .unwrap()
}
fn node(kind: NodeKind) -> BeatNode {
    BeatNode {
        framing: None,
        audio_edges: Default::default(),
        label: "Fixture".into(),
        kind,
    }
}
fn background(frames: i64) -> HoldRecipe {
    HoldRecipe {
        duration: duration(frames),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    }
}
fn hold(frames: i64) -> BeatNode {
    node(NodeKind::Hold {
        recipe: background(frames),
    })
}
fn source(frames: i64, start: i64, end: i64) -> BeatNode {
    source_with_mapping(frames, start, end, SourceVideoMapping::FitBeat)
}
fn source_with_mapping(
    frames: i64,
    start: i64,
    end: i64,
    video_mapping: SourceVideoMapping,
) -> BeatNode {
    node(NodeKind::Source {
        source: SourceNode {
            duration: duration(frames),
            video_mapping,
            video: SourceVideo::Stream {
                asset: asset_id("video"),
                span: span(start, end),
            },
            audio: None,
            link: LinkRelation::Independent,
            audio_mapping: SourceAudioMapping::FitBeat,
            audio_offset: AudioSample(0),
        },
    })
}
fn retime(child: &str, frames: i64, start: i64, end: i64) -> BeatNode {
    node(NodeKind::Retime {
        purpose: deadpan_core::RetimePurpose::Edit,
        child: id(child),
        duration: duration(frames),
        mapping: range(start, end),
        pitch: PitchPolicy::Preserve,
    })
}
fn repeat(child: &str, plays: u32, gap: i64, allocation: &str) -> BeatNode {
    node(NodeKind::Repeat {
        child: id(child),
        iterations: IterationOrder::new(revision(allocation), plays).unwrap(),
        gap: (gap > 0).then(|| background(gap)),
    })
}
fn document(roots: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("project").unwrap(),
        revision("initial"),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut value = serde_json::to_value(empty).unwrap();
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(name, node)| (id(name), node))
        .collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("Root", roots.iter().map(|name| id(name)).collect()),
    );
    value["nodes"] = serde_json::to_value(nodes).unwrap();
    let video = AssetRecord {
        source_qualification: None,
        label: "Video".into(),
        content_hash: "a".repeat(64),
        video: Some(span(-10000, 100000)),
        audio: Some(span(-10000, 100000)),
        still_image: false,
        frame_count: Some(duration(10000)),
    };
    let still = AssetRecord {
        source_qualification: None,
        label: "Still".into(),
        content_hash: "b".repeat(64),
        video: None,
        audio: None,
        still_image: true,
        frame_count: None,
    };
    value["assets"] = serde_json::to_value(BTreeMap::from([
        (asset_id("video"), video),
        (asset_id("still"), still),
    ]))
    .unwrap();
    ProjectDocument::from_json(&value.to_string()).unwrap()
}
fn index(asset: &str, time_base: SourceTimeBase, pts: &[i64], end: i64) -> SourceFrameIndex {
    SourceFrameIndex::new(
        asset_id(asset),
        time_base,
        pts.iter()
            .enumerate()
            .map(|(number, pts)| IndexedSourceFrame {
                identity: SourceFrameId(number as u64),
                pts: *pts,
                reported_duration: None,
                keyframe: number == 0,
                seek_from: Some(SourceFrameId(0)),
                decode_timestamp: None,
            })
            .collect(),
        end,
        TerminalProvenance::Explicit,
    )
    .unwrap()
}
fn ticks(picture: &Picture) -> ExactRatio {
    match picture {
        Picture::Source { point, .. } | Picture::Freeze { point, .. } => point.ticks,
        other => panic!("unexpected {other:?}"),
    }
}

fn framed(mut node: BeatNode, end_scale: i64) -> BeatNode {
    node.framing = Some(
        Framing::creep(
            FramingPose::identity(),
            FramingPose::new(
                ExactRatio::new(1, 2).unwrap(),
                ExactRatio::new(1, 2).unwrap(),
                ExactRatio::integer(end_scale),
            )
            .unwrap(),
            FramingCurve::Linear,
        )
        .unwrap(),
    );
    node
}

#[test]
fn framing_retains_every_owner_clock_and_repeat_scope_in_composition_order() {
    let document = document(
        &["lead", "group"],
        vec![
            ("lead", hold(3)),
            ("group", BeatNode::sequence("group", vec![id("repeat")])),
            ("repeat", framed(repeat("retime", 3, 1, "plays"), 2)),
            ("retime", framed(retime("source", 4, 0, 8), 2)),
            ("source", framed(source(8, 0, 8000), 3)),
        ],
    );
    let plan = RenderPlan::compile(&document).unwrap();
    let sample = plan.picture(ProjectFrame(9)).unwrap();
    let layers = &sample.framing;
    assert_eq!(
        layers
            .iter()
            .map(|l| l.instance.node.as_str())
            .collect::<Vec<_>>(),
        ["source", "retime", "repeat", "group", "root"]
    );
    assert_eq!(
        layers.iter().map(|l| l.local_position).collect::<Vec<_>>(),
        [
            ExactRatio::integer(3),
            ExactRatio::new(3, 2).unwrap(),
            ExactRatio::new(13, 2).unwrap(),
            ExactRatio::new(13, 2).unwrap(),
            ExactRatio::new(19, 2).unwrap()
        ]
    );
    assert_eq!(
        layers
            .iter()
            .map(|l| l.duration.frames())
            .collect::<Vec<_>>(),
        [8, 4, 14, 14, 17]
    );
    assert_eq!(
        layers[0].pose.unwrap().scale,
        ExactRatio::new(7, 4).unwrap()
    );
    assert_eq!(
        layers[1].pose.unwrap().scale,
        ExactRatio::new(11, 8).unwrap()
    );
    assert_eq!(layers[0].instance.repeats[0].iteration.ordinal, 1);
    assert_eq!(layers[1].instance.repeats[0].iteration.ordinal, 1);
    assert!(layers[2].instance.repeats.is_empty());
    assert!(layers[3].pose.is_none() && layers[4].pose.is_none());
    for layer in layers {
        layer.instance.validate(&document).unwrap();
    }
    assert!(sample.lookup.visited_nodes <= 5);

    let gap = plan.picture(ProjectFrame(7)).unwrap();
    assert_eq!(gap.gap_after.unwrap().ordinal, 0);
    assert_eq!(
        gap.framing
            .iter()
            .map(|l| l.instance.node.as_str())
            .collect::<Vec<_>>(),
        ["repeat", "group", "root"]
    );
    assert_eq!(gap.local_position, ExactRatio::new(1, 2).unwrap());
    assert_eq!(
        gap.framing[0].local_position,
        ExactRatio::new(9, 2).unwrap()
    );
    assert_eq!(gap.framing[0].duration, duration(14));
}

#[test]
fn freeze_picture_identity_stays_fixed_while_its_framing_clock_advances() {
    let freeze = node(NodeKind::Hold {
        recipe: HoldRecipe {
            duration: duration(8),
            video: HoldVideo::Freeze {
                asset: asset_id("video"),
                timestamp: SourceTimestamp {
                    ticks: 1001,
                    time_base: clock(),
                },
            },
            audio: HoldAudio::Silence,
        },
    });
    let document = document(&["hold"], vec![("hold", framed(freeze, 3))]);
    let plan = RenderPlan::compile(&document).unwrap();
    let first = plan.picture(ProjectFrame(0)).unwrap();
    let last = plan.picture(ProjectFrame(7)).unwrap();
    assert_eq!(first.picture, last.picture);
    assert_eq!(
        first.framing[0].pose.unwrap().scale,
        ExactRatio::new(9, 8).unwrap()
    );
    assert_eq!(
        last.framing[0].pose.unwrap().scale,
        ExactRatio::new(23, 8).unwrap()
    );
}

#[test]
fn source_framing_envelope_survives_actual_split_without_reset_or_duplication() {
    let before = document(&["source"], vec![("source", framed(source(9, 0, 9009), 3))]);
    let original = RenderPlan::compile(&before).unwrap();
    let transaction = apply(
        &before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision("split-framing"),
            command: Command::Split {
                node: id("source"),
                at: duration(4),
                identities: SplitIdentities {
                    nodes: (0..10).map(|n| id(&format!("part-{n}"))).collect(),
                },
            },
        },
    )
    .unwrap();
    let divided = transaction.forward.apply(&before).unwrap();
    let after = RenderPlan::compile(&divided).unwrap();
    for frame in [8, 0, 3, 4, 7, 1, 5, 2, 6] {
        let old = original.picture(ProjectFrame(frame)).unwrap();
        let new = after.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(old.picture, new.picture);
        assert_eq!(
            old.framing
                .iter()
                .filter_map(|l| l.pose)
                .collect::<Vec<_>>(),
            new.framing
                .iter()
                .filter_map(|l| l.pose)
                .collect::<Vec<_>>()
        );
        let old = old.framing.iter().find(|l| l.pose.is_some()).unwrap();
        let new = new.framing.iter().find(|l| l.pose.is_some()).unwrap();
        assert_eq!(
            (old.local_position, old.duration),
            (new.local_position, new.duration)
        );
    }
}

#[test]
fn fractional_source_centers_and_vfr_selection_preserve_original_pts() {
    let document = document(&["source"], vec![("source", source(6, -2002, 5005))]);
    let plan = RenderPlan::compile(&document).unwrap();
    let index = index("video", clock(), &[-2002, -1001, 1001, 4004], 5005);
    let expected = [0, 1, 1, 2, 2, 3];
    for (frame, expected) in expected.into_iter().enumerate() {
        let sample = plan.picture(ProjectFrame(frame as i64)).unwrap();
        assert_eq!(
            ticks(&sample.picture),
            ExactRatio::new(-2002 * 12 + (2 * frame as i128 + 1) * 7007, 12).unwrap()
        );
        assert_eq!(
            sample.picture.select_source_frame(&index).unwrap().identity,
            SourceFrameId(expected)
        );
        sample.instance.validate(&document).unwrap();
    }
    assert_eq!(
        plan.metadata().presentation_basis.frame_rate,
        FrameRate::new(30000, 1001).unwrap()
    );
}

#[test]
fn retained_partitions_preserve_exact_vfr_picture_mapping_through_fractional_retime() {
    let original = document(
        &["retime"],
        vec![
            ("source", source(6, -2002, 5005)),
            ("retime", retime("source", 9, 0, 6)),
        ],
    );
    let partition = |child: &str, start, end| {
        let mut node = retime(child, end - start, start, end);
        let NodeKind::Retime { purpose, .. } = &mut node.kind else {
            unreachable!()
        };
        *purpose = RetimePurpose::Partition;
        node
    };
    let divided = document(
        &["left", "right"],
        vec![
            ("left", partition("retime-left", 0, 4)),
            ("right", partition("retime-right", 4, 9)),
            ("source-left", source(6, -2002, 5005)),
            ("source-right", source(6, -2002, 5005)),
            ("retime-left", retime("source-left", 9, 0, 6)),
            ("retime-right", retime("source-right", 9, 0, 6)),
        ],
    );
    let original = RenderPlan::compile(&original).unwrap();
    let plan = RenderPlan::compile(&divided).unwrap();
    let index = index("video", clock(), &[-2002, -1001, 1001, 4004], 5005);
    assert_eq!(plan.duration(), original.duration());
    for frame in [8, 0, 3, 4, 7, 1, 5, 2, 6] {
        let before = original.picture(ProjectFrame(frame)).unwrap();
        let after = plan.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(after.picture, before.picture);
        assert_eq!(
            after.picture.select_source_frame(&index).unwrap(),
            before.picture.select_source_frame(&index).unwrap()
        );
        assert_eq!(
            ticks(&after.picture),
            ExactRatio::new(-2002 * 18 + (2 * i128::from(frame) + 1) * 7007, 18).unwrap()
        );
        after.instance.validate(&divided).unwrap();
    }
}

#[test]
fn actual_split_and_refinement_keep_every_vfr_picture_and_exact_source_coordinate() {
    let original = document(
        &["retime"],
        vec![
            ("source", source(6, -2002, 5005)),
            ("retime", retime("source", 9, 0, 6)),
        ],
    );
    let expected = RenderPlan::compile(&original).unwrap();
    let source_index = index("video", clock(), &[-2002, -1001, 1001, 4004], 5005);
    let mut divided = original;
    let mut target = id("retime");
    for (cut, boundary) in [("first", 4), ("second", 2)] {
        let transaction = apply(
            &divided,
            &CommandRequest {
                project_id: divided.project_id().clone(),
                expected_revision: divided.revision_id().clone(),
                new_revision: revision(cut),
                command: Command::Split {
                    node: target,
                    at: duration(boundary),
                    identities: SplitIdentities {
                        nodes: (0..10)
                            .map(|number| id(&format!("{cut}-{number}")))
                            .collect(),
                    },
                },
            },
        )
        .unwrap();
        let next = transaction.forward.apply(&divided).unwrap();
        assert_eq!(transaction.duration_delta, 0);
        assert_eq!(transaction.inverse.apply(&next).unwrap(), divided);
        divided = next;
        let NodeKind::Sequence { children } = &divided.nodes()[divided.root()].kind else {
            unreachable!()
        };
        target = children.last().unwrap().clone();
        let plan = RenderPlan::compile(&divided).unwrap();
        for frame in [8, 0, 3, 4, 7, 1, 5, 2, 6] {
            let before = expected.picture(ProjectFrame(frame)).unwrap();
            let after = plan.picture(ProjectFrame(frame)).unwrap();
            assert_eq!(after.picture, before.picture);
            assert_eq!(
                after.picture.select_source_frame(&source_index).unwrap(),
                before.picture.select_source_frame(&source_index).unwrap()
            );
            after.instance.validate(&divided).unwrap();
        }
    }
}

#[test]
fn natural_video_duration_preserves_vfr_selection_after_beat_rounding() {
    let mapping = SourceVideoMapping::natural_rate(
        span(0, 30_000),
        FrameRate::new(30_000, 1001).unwrap(),
        EndpointPolicy::Reject,
    )
    .unwrap();
    let natural = RenderPlan::compile(&document(
        &["source"],
        vec![("source", source_with_mapping(30, 0, 30_000, mapping))],
    ))
    .unwrap();
    let fitted = RenderPlan::compile(&document(
        &["source"],
        vec![("source", source(30, 0, 30_000))],
    ))
    .unwrap();
    let source_index = index("video", clock(), &[0, 14_014, 15_510, 16_016], 30_000);
    let natural_picture = natural.picture(ProjectFrame(15)).unwrap().picture;
    let fitted_picture = fitted.picture(ProjectFrame(15)).unwrap().picture;
    assert_eq!(ticks(&natural_picture), ExactRatio::new(31_031, 2).unwrap());
    assert_eq!(ticks(&fitted_picture), ExactRatio::integer(15_500));
    assert_eq!(
        natural_picture
            .select_source_frame(&source_index)
            .unwrap()
            .identity,
        SourceFrameId(2)
    );
    assert_eq!(
        fitted_picture
            .select_source_frame(&source_index)
            .unwrap()
            .identity,
        SourceFrameId(1)
    );
    assert_eq!(natural.duration(), duration(30));
}

#[test]
fn rounded_video_tail_uses_the_authored_policy_and_selected_span() {
    // The exact 3/2-frame selection rounds ties-to-even to two frames. The
    // second frame center reaches its exclusive end, where later media exists.
    let source_index = index("video", clock(), &[0, 500, 1000, 1500, 2000], 2500);
    for endpoints in [EndpointPolicy::Reject, EndpointPolicy::HoldAdjacent] {
        let mapping = SourceVideoMapping::Duration {
            frames: ExactRatio::new(3, 2).unwrap(),
            endpoints,
        };
        let plan = RenderPlan::compile(&document(
            &["source"],
            vec![("source", source_with_mapping(2, 0, 1500, mapping))],
        ))
        .unwrap();
        let first = plan.picture(ProjectFrame(0)).unwrap().picture;
        assert_eq!(ticks(&first), ExactRatio::integer(500));
        assert_eq!(
            first.select_source_frame(&source_index).unwrap().identity,
            SourceFrameId(1)
        );
        let last = plan.picture(ProjectFrame(1)).unwrap().picture;
        assert_eq!(ticks(&last), ExactRatio::integer(1500));
        let selected = last.select_source_frame(&source_index);
        match endpoints {
            EndpointPolicy::Reject => assert!(selected.is_err()),
            EndpointPolicy::HoldAdjacent => {
                assert_eq!(selected.unwrap().identity, SourceFrameId(2));
            }
        }
        let retimed = RenderPlan::compile(&document(
            &["outer"],
            vec![
                ("source", source_with_mapping(2, 0, 1500, mapping)),
                ("inner", retime("source", 4, 0, 2)),
                ("outer", retime("inner", 8, 0, 4)),
            ],
        ))
        .unwrap();
        let tail = retimed.picture(ProjectFrame(7)).unwrap().picture;
        assert_eq!(ticks(&tail), ExactRatio::integer(1875));
        let selected = tail.select_source_frame(&source_index);
        match endpoints {
            EndpointPolicy::Reject => assert!(selected.is_err()),
            EndpointPolicy::HoldAdjacent => {
                assert_eq!(selected.unwrap().identity, SourceFrameId(2));
            }
        }
    }
}

#[test]
fn exact_video_mapping_composes_nested_retimes_before_pts_selection() {
    let mapping = SourceVideoMapping::natural_rate(
        span(-2002, 27998),
        FrameRate::new(30_000, 1001).unwrap(),
        EndpointPolicy::Reject,
    )
    .unwrap();
    let plan = RenderPlan::compile(&document(
        &["outer"],
        vec![
            ("source", source_with_mapping(30, -2002, 27998, mapping)),
            ("inner", retime("source", 20, 0, 30)),
            ("outer", retime("inner", 30, 0, 20)),
        ],
    ))
    .unwrap();
    let source_index = index("video", clock(), &[-2002, 12012, 13508, 14014], 27998);
    let picture = plan.picture(ProjectFrame(15)).unwrap().picture;
    assert_eq!(ticks(&picture), ExactRatio::new(27027, 2).unwrap());
    assert_eq!(
        picture.select_source_frame(&source_index).unwrap().identity,
        SourceFrameId(2)
    );
}

#[test]
fn source_index_checks_asset_clock_and_measured_coverage() {
    let plan =
        RenderPlan::compile(&document(&["source"], vec![("source", source(2, 0, 1001))])).unwrap();
    let picture = plan.picture(ProjectFrame(0)).unwrap().picture;
    assert!(matches!(
        picture.select_source_frame(&index("other", clock(), &[0], 1001)),
        Err(PlanError::IndexAssetMismatch { .. })
    ));
    assert!(matches!(
        picture.select_source_frame(&index(
            "video",
            SourceTimeBase::new(1, 1000).unwrap(),
            &[0],
            1001
        )),
        Err(PlanError::IndexClockMismatch { .. })
    ));
    let shortened = index("video", clock(), &[500], 1001);
    assert!(picture.select_source_frame(&shortened).is_err());
    let mut held = picture;
    let Picture::Source { endpoints, .. } = &mut held else {
        panic!("source")
    };
    *endpoints = EndpointPolicy::HoldAdjacent;
    assert!(held.select_source_frame(&shortened).is_err());
    assert!(matches!(
        Picture::Background.select_source_frame(&shortened),
        Err(PlanError::NoSourceFrame)
    ));
}

#[test]
fn generated_hold_resize_reuses_materialized_frames_and_preserves_following_source() {
    let initial = document(
        &["hold", "source"],
        vec![("hold", hold(30)), ("source", source(10, 10_000, 20_010))],
    );
    let object = |digit: char| {
        GeneratedObjectRef::new(
            GeneratedContentId::new(digit.to_string().repeat(64)).unwrap(),
            1024,
        )
        .unwrap()
    };
    let artifact = GeneratedArtifact {
        sampled_asset: asset_id("sampled"),
        sampled_object: object('c'),
        native_asset: asset_id("native"),
        native_object: object('d'),
        provenance: object('e'),
        sampling: BridgeSamplingMap::new(
            FrameRate::new(30_000, 1001).unwrap(),
            FrameRate::new(24, 1).unwrap(),
            duration(25),
            duration(30),
            BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
        )
        .unwrap(),
    };
    let record = |object: &GeneratedObjectRef, frames: i64| AssetRecord {
        source_qualification: None,
        label: "Generated".into(),
        content_hash: object.content().to_string(),
        video: Some(span(0, frames * 1001)),
        audio: None,
        still_image: false,
        frame_count: Some(duration(frames)),
    };
    let assets = BTreeMap::from([
        (
            artifact.sampled_asset.clone(),
            record(&artifact.sampled_object, 30),
        ),
        (
            artifact.native_asset.clone(),
            record(&artifact.native_object, 25),
        ),
    ]);
    let edit = |before: &ProjectDocument, next: &str, command| {
        let transaction = deadpan_core::apply(
            before,
            &CommandRequest {
                project_id: before.project_id().clone(),
                expected_revision: before.revision_id().clone(),
                new_revision: revision(next),
                command,
            },
        )
        .unwrap();
        transaction.forward.apply(before).unwrap()
    };
    let accepted = edit(
        &initial,
        "accepted",
        Command::AcceptGeneratedHold {
            node: id("hold"),
            artifact,
            assets,
        },
    );
    let full_plan = RenderPlan::compile(&accepted).unwrap();
    let following = full_plan.picture(ProjectFrame(30)).unwrap().picture;
    let shorter = edit(
        &accepted,
        "shorter",
        Command::SetHoldDuration {
            node: id("hold"),
            duration: duration(12),
        },
    );
    let shorter_plan = RenderPlan::compile(&shorter).unwrap();
    for frame in 0..12 {
        let picture = shorter_plan.picture(ProjectFrame(frame)).unwrap().picture;
        assert_eq!(
            picture,
            full_plan.picture(ProjectFrame(frame)).unwrap().picture
        );
        assert!(
            matches!(picture, Picture::Accepted { asset, frame: SourceFrameId(number), .. }
            if asset == asset_id("sampled") && number == u64::try_from(frame).unwrap())
        );
    }
    assert_eq!(
        shorter_plan.picture(ProjectFrame(12)).unwrap().picture,
        following
    );
    let reused = edit(
        &shorter,
        "reused",
        Command::SetHoldDuration {
            node: id("hold"),
            duration: duration(30),
        },
    );
    let reused_plan = RenderPlan::compile(&reused).unwrap();
    for frame in 0..40 {
        assert_eq!(
            reused_plan.picture(ProjectFrame(frame)).unwrap().picture,
            full_plan.picture(ProjectFrame(frame)).unwrap().picture
        );
    }
    let extended = edit(
        &reused,
        "extended",
        Command::SetHoldDuration {
            node: id("hold"),
            duration: duration(31),
        },
    );
    let fallback = RenderPlan::compile(&extended).unwrap();
    assert_eq!(
        fallback.picture(ProjectFrame(30)).unwrap().picture,
        Picture::Background
    );
    assert_eq!(
        fallback.picture(ProjectFrame(31)).unwrap().picture,
        following
    );
    assert_eq!(
        full_plan.duration(),
        duration(40),
        "compiled revision stays immutable"
    );
    assert!(matches!(
        full_plan.picture(ProjectFrame(29)).unwrap().picture,
        Picture::Accepted { .. }
    ));
}

#[test]
fn exact_retime_boundary_selects_the_right_sequence_child() {
    let document = document(
        &["retime"],
        vec![
            ("retime", retime("sequence", 1, 0, 2)),
            (
                "sequence",
                BeatNode::sequence("Sequence", vec![id("left"), id("empty"), id("right")]),
            ),
            ("left", source(1, -1001, 0)),
            ("empty", BeatNode::sequence("Empty", vec![])),
            ("right", source(1, 0, 1001)),
        ],
    );
    let sample = RenderPlan::compile(&document)
        .unwrap()
        .picture(ProjectFrame(0))
        .unwrap();
    assert_eq!(sample.instance.node, id("right"));
    assert_eq!(sample.local_position, ExactRatio::ZERO);
    assert_eq!(ticks(&sample.picture), ExactRatio::ZERO);
}

#[test]
fn nested_retimes_repeat_paths_and_gap_coordinates_stay_exact() {
    let mut inner = repeat("inner-retime", 2, 1, "inner-plays");
    if let NodeKind::Repeat { gap: Some(gap), .. } = &mut inner.kind {
        gap.video = HoldVideo::Freeze {
            asset: asset_id("video"),
            timestamp: SourceTimestamp {
                ticks: 10,
                time_base: clock(),
            },
        };
    }
    let document = document(
        &["outer"],
        vec![
            ("source", source(6, -1001, 5005)),
            ("inner-retime", retime("source", 4, 0, 6)),
            ("inner", inner),
            ("outer-retime", retime("inner", 6, 1, 9)),
            ("outer", repeat("outer-retime", 3, 2, "outer-plays")),
        ],
    );
    let plan = RenderPlan::compile(&document).unwrap();
    assert_eq!(plan.duration(), duration(22));
    for (frame, outer_ordinal) in [(0, 0), (8, 1), (16, 2)] {
        let sample = plan.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(sample.local_position, ExactRatio::new(5, 2).unwrap());
        assert_eq!(ticks(&sample.picture), ExactRatio::new(3003, 2).unwrap());
        assert_eq!(
            sample
                .instance
                .repeats
                .iter()
                .map(|r| (&r.node, r.iteration.ordinal))
                .collect::<Vec<_>>(),
            vec![(&id("outer"), outer_ordinal), (&id("inner"), 0)]
        );
        assert_eq!(sample.gap_after, None);
        sample.instance.validate(&document).unwrap();
    }
    let inner_gap = plan.picture(ProjectFrame(2)).unwrap();
    assert_eq!(inner_gap.instance.node, id("inner"));
    assert_eq!(inner_gap.instance.repeats.len(), 1);
    assert_eq!(
        inner_gap.gap_after.unwrap(),
        IterationId {
            allocation: revision("inner-plays"),
            ordinal: 0
        }
    );
    assert_eq!(inner_gap.local_position, ExactRatio::new(1, 3).unwrap());
    assert_eq!(ticks(&inner_gap.picture), ExactRatio::integer(10));
    inner_gap.instance.validate(&document).unwrap();
    for frame in [6, 7] {
        let outer_gap = plan.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(outer_gap.instance.node, id("outer"));
        assert!(outer_gap.instance.repeats.is_empty());
        assert_eq!(outer_gap.gap_after.unwrap().ordinal, 0);
        assert_eq!(outer_gap.picture, Picture::Background);
        outer_gap.instance.validate(&document).unwrap();
    }
    assert!(plan.picture(ProjectFrame(21)).unwrap().gap_after.is_none());
    assert!(plan.picture(ProjectFrame(22)).is_err());
}

#[test]
fn accepted_frames_floor_only_after_composed_retimes_and_validate_index() {
    let mut recipe = background(6);
    recipe.video = HoldVideo::Accepted {
        asset: asset_id("video"),
        frames: range(3, 9),
    };
    let document = document(
        &["outer"],
        vec![
            ("accepted", node(NodeKind::Hold { recipe })),
            ("inner", retime("accepted", 4, 0, 6)),
            ("outer", retime("inner", 6, 0, 4)),
        ],
    );
    let plan = RenderPlan::compile(&document).unwrap();
    let source_index = index(
        "video",
        clock(),
        &[0, 100, 200, 300, 400, 500, 600, 700, 800, 900],
        1000,
    );
    for frame in 0..6 {
        let sample = plan.picture(ProjectFrame(frame)).unwrap();
        let Picture::Accepted {
            position,
            frame: source_frame,
            ..
        } = sample.picture
        else {
            panic!("accepted")
        };
        assert_eq!(
            position,
            ExactRatio::new(2 * i128::from(frame + 3) + 1, 2).unwrap()
        );
        assert_eq!(source_frame, SourceFrameId((frame + 3) as u64));
        assert_eq!(
            sample
                .picture
                .select_source_frame(&source_index)
                .unwrap()
                .identity,
            source_frame
        );
    }
    let last = plan.picture(ProjectFrame(5)).unwrap().picture;
    assert!(matches!(
        last.select_source_frame(&index("video", clock(), &[0], 1000)),
        Err(PlanError::MissingSourceFrame {
            frame: SourceFrameId(8)
        })
    ));
    assert!(matches!(
        last.select_source_frame(&index("wrong", clock(), &[0], 1000)),
        Err(PlanError::IndexAssetMismatch { .. })
    ));
}

#[test]
fn still_blank_freeze_and_background_are_distinct_picture_requests() {
    let still = node(NodeKind::Source {
        source: SourceNode {
            duration: duration(1),
            video_mapping: SourceVideoMapping::FitBeat,
            video: SourceVideo::Still {
                asset: asset_id("still"),
            },
            audio: None,
            link: LinkRelation::Independent,
            audio_mapping: SourceAudioMapping::FitBeat,
            audio_offset: AudioSample(0),
        },
    });
    let blank = node(NodeKind::Source {
        source: SourceNode {
            duration: duration(1),
            video_mapping: SourceVideoMapping::FitBeat,
            video: SourceVideo::Blank,
            audio: Some(SourceAudio {
                asset: asset_id("video"),
                span: span(0, 1001),
            }),
            link: LinkRelation::Independent,
            audio_mapping: SourceAudioMapping::FitBeat,
            audio_offset: AudioSample(0),
        },
    });
    let freeze = node(NodeKind::Hold {
        recipe: HoldRecipe {
            duration: duration(1),
            video: HoldVideo::Freeze {
                asset: asset_id("video"),
                timestamp: SourceTimestamp {
                    ticks: -1001,
                    time_base: clock(),
                },
            },
            audio: HoldAudio::Silence,
        },
    });
    let plan = RenderPlan::compile(&document(
        &["still", "blank", "freeze", "background"],
        vec![
            ("still", still),
            ("blank", blank),
            ("freeze", freeze),
            ("background", hold(1)),
        ],
    ))
    .unwrap();
    assert_eq!(
        plan.picture(ProjectFrame(0)).unwrap().picture,
        Picture::Still {
            asset: asset_id("still")
        }
    );
    assert_eq!(
        plan.picture(ProjectFrame(1)).unwrap().picture,
        Picture::Blank
    );
    let frozen = plan.picture(ProjectFrame(2)).unwrap().picture;
    assert_eq!(ticks(&frozen), ExactRatio::integer(-1001));
    assert_eq!(
        frozen
            .select_source_frame(&index("video", clock(), &[-2002, -1001, 0], 1001))
            .unwrap()
            .identity,
        SourceFrameId(1)
    );
    assert!(
        frozen
            .select_source_frame(&index("video", clock(), &[0], 1001))
            .is_err()
    );
    assert_eq!(
        plan.picture(ProjectFrame(3)).unwrap().picture,
        Picture::Background
    );
}

#[test]
fn empty_sequences_are_skipped_and_invalid_seeks_are_rejected() {
    let empty = document(&[], vec![]);
    let empty_plan = RenderPlan::compile(&empty).unwrap();
    assert_eq!(empty_plan.duration(), FrameDuration::ZERO);
    assert!(matches!(
        empty_plan.picture(ProjectFrame(0)),
        Err(PlanError::FrameOutOfRange { .. })
    ));
    let plan = RenderPlan::compile(&document(
        &["a", "b", "hold", "c"],
        vec![
            ("a", BeatNode::sequence("A", vec![])),
            ("b", BeatNode::sequence("B", vec![])),
            ("hold", hold(2)),
            ("c", BeatNode::sequence("C", vec![])),
        ],
    ))
    .unwrap();
    for frame in [0, 1] {
        assert_eq!(
            plan.picture(ProjectFrame(frame)).unwrap().instance.node,
            id("hold")
        );
    }
    for frame in [i64::MIN, -1, 2, i64::MAX] {
        assert!(matches!(
            plan.picture(ProjectFrame(frame)),
            Err(PlanError::FrameOutOfRange { .. })
        ));
    }
    assert_eq!(plan.node_duration(&id("a")), Some(FrameDuration::ZERO));
    assert_eq!(plan.node_duration(&id("missing")), None);
}

#[test]
fn maximum_play_count_and_last_frame_seek_do_not_expand_occurrences() {
    let document = document(
        &["repeat"],
        vec![
            ("child", hold(1)),
            ("repeat", repeat("child", u32::MAX, 1, "plays")),
        ],
    );
    let plan = RenderPlan::compile(&document).unwrap();
    let expected_duration = i64::from(u32::MAX) * 2 - 1;
    assert_eq!(plan.duration(), duration(expected_duration));
    assert_eq!(plan.metadata().storage.authored_nodes, 3);
    assert_eq!(plan.metadata().storage.sequence_prefix_entries, 1);
    assert_eq!(plan.metadata().storage.iteration_run_entries, 1);
    assert_eq!(
        plan.metadata().storage.referenced_plays,
        u64::from(u32::MAX)
    );
    let last = plan.picture(ProjectFrame(expected_duration - 1)).unwrap();
    assert_eq!(last.instance.repeats[0].iteration.ordinal, u32::MAX - 1);
    assert_eq!(last.lookup.iteration_run_comparisons, 1);
    assert_eq!(last.lookup.visited_nodes, 3);
    assert!(last.gap_after.is_none());
    last.instance.validate(&document).unwrap();
    assert!(plan.picture(ProjectFrame(expected_duration)).is_err());
    assert!(serde_json::to_string(&plan.inspect()).unwrap().len() < 2000);
}

#[test]
fn single_play_ignores_unrepresentable_unused_period() {
    let document = document(
        &["repeat"],
        vec![
            ("child", hold(i64::MAX)),
            ("repeat", repeat("child", 1, i64::MAX, "plays")),
        ],
    );
    let plan = RenderPlan::compile(&document).unwrap();
    let sample = plan.picture(ProjectFrame(i64::MAX - 1)).unwrap();
    assert_eq!(sample.instance.node, id("child"));
    assert_eq!(sample.instance.repeats[0].iteration.ordinal, 0);
    assert!(sample.gap_after.is_none());
}

#[test]
fn exact_repeat_child_gap_and_next_play_boundaries_are_half_open() {
    for (start, end, in_gap) in [(3, 5, true), (4, 6, false)] {
        let document = document(
            &["retime"],
            vec![
                ("child", hold(4)),
                ("repeat", repeat("child", 2, 1, "plays")),
                ("retime", retime("repeat", 1, start, end)),
            ],
        );
        let plan = RenderPlan::compile(&document).unwrap();
        let sample = plan.picture(ProjectFrame(0)).unwrap();
        assert_eq!(sample.local_position, ExactRatio::ZERO);
        sample.instance.validate(&document).unwrap();
        if in_gap {
            assert_eq!(sample.instance.node, id("repeat"));
            assert_eq!(sample.gap_after.unwrap().ordinal, 0);
        } else {
            assert_eq!(sample.instance.node, id("child"));
            assert!(sample.gap_after.is_none());
            assert_eq!(sample.instance.repeats[0].iteration.ordinal, 1);
        }
    }
}

#[test]
fn moved_and_inserted_iteration_runs_use_binary_selection_and_stable_gap_identity() {
    let original = IterationOrder::new(revision("original"), 5).unwrap();
    let moved = original.moved(1, 3, 3).unwrap();
    let document = document(
        &["repeat"],
        vec![
            ("child", hold(1)),
            (
                "repeat",
                node(NodeKind::Repeat {
                    child: id("child"),
                    iterations: moved,
                    gap: Some(background(1)),
                }),
            ),
        ],
    );
    let plan = RenderPlan::compile(&document).unwrap();
    for (play, ordinal) in [0, 3, 4, 1, 2].into_iter().enumerate() {
        let child = plan.picture(ProjectFrame(play as i64 * 2)).unwrap();
        assert_eq!(
            child.instance.repeats[0].iteration,
            original.at(ordinal).unwrap()
        );
        if play < 4 {
            let gap = plan.picture(ProjectFrame(play as i64 * 2 + 1)).unwrap();
            assert_eq!(gap.gap_after, original.at(ordinal));
            gap.instance.validate(&document).unwrap();
        }
    }
    let mut many_runs = IterationOrder::new(revision("run0"), 1).unwrap();
    for number in 1..128 {
        many_runs = many_runs
            .inserted(number, 1, revision(&format!("run{number}")))
            .unwrap();
    }
    let many = document_with_repeat_order(many_runs);
    let plan = RenderPlan::compile(&many).unwrap();
    assert_eq!(plan.metadata().storage.iteration_run_entries, 128);
    for play in 0..128 {
        let sample = plan.picture(ProjectFrame(play)).unwrap();
        assert_eq!(
            sample.instance.repeats[0].iteration.allocation,
            revision(&format!("run{play}"))
        );
        assert!(sample.lookup.iteration_run_comparisons <= 8);
    }
}

fn document_with_repeat_order(iterations: IterationOrder) -> ProjectDocument {
    document(
        &["repeat"],
        vec![
            ("child", hold(1)),
            (
                "repeat",
                node(NodeKind::Repeat {
                    child: id("child"),
                    iterations,
                    gap: None,
                }),
            ),
        ],
    )
}

#[test]
fn compiled_revision_is_immutable_and_inverse_restores_deterministic_inspection() {
    let document = document(&["hold"], vec![("hold", hold(3))]);
    let original = RenderPlan::compile(&document).unwrap();
    let transaction = apply(
        &document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision("edit"),
            command: Command::SetHoldDuration {
                node: id("hold"),
                duration: duration(9),
            },
        },
    )
    .unwrap();
    let edited = transaction.forward.apply(&document).unwrap();
    let edited_plan = RenderPlan::compile(&edited).unwrap();
    assert_eq!(edited_plan.duration(), duration(9));
    assert_eq!(original.duration(), duration(3));
    assert_eq!(original.metadata().revision_id, revision("initial"));
    assert_eq!(
        edited_plan.picture(ProjectFrame(2)).unwrap().revision_id,
        revision("edit")
    );
    let restored = RenderPlan::compile(&transaction.inverse.apply(&edited).unwrap()).unwrap();
    assert_eq!(restored.inspect(), original.inspect());
    assert_eq!(
        serde_json::to_string(&restored.inspect()).unwrap(),
        serde_json::to_string(&original.inspect()).unwrap()
    );
    assert_eq!(
        restored.picture(ProjectFrame(2)).unwrap(),
        original.picture(ProjectFrame(2)).unwrap()
    );
}

#[test]
fn accepted_repeat_gap_maps_exact_fractional_positions_without_a_trailing_gap() {
    let recipe = HoldRecipe {
        duration: duration(3),
        video: HoldVideo::Accepted {
            asset: asset_id("video"),
            frames: range(20, 23),
        },
        audio: HoldAudio::Silence,
    };
    let document = document(
        &["retime"],
        vec![
            ("child", hold(2)),
            (
                "repeat",
                node(NodeKind::Repeat {
                    child: id("child"),
                    iterations: IterationOrder::new(revision("plays"), 2).unwrap(),
                    gap: Some(recipe),
                }),
            ),
            ("retime", retime("repeat", 14, 0, 7)),
        ],
    );
    let plan = RenderPlan::compile(&document).unwrap();
    for (frame, expected) in [(4, 20), (5, 20), (6, 21), (7, 21), (8, 22), (9, 22)] {
        let sample = plan.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(sample.instance.node, id("repeat"));
        sample.instance.validate(&document).unwrap();
        assert_eq!(sample.gap_after.unwrap().ordinal, 0);
        let Picture::Accepted {
            position,
            frame: original,
            ..
        } = sample.picture
        else {
            panic!("accepted gap")
        };
        assert_eq!(original, SourceFrameId(expected));
        assert_eq!(
            position,
            ExactRatio::new(80 + i128::from(frame * 2 - 7), 4).unwrap()
        );
    }
    assert!(plan.picture(ProjectFrame(10)).unwrap().gap_after.is_none());
    assert!(plan.picture(ProjectFrame(13)).unwrap().gap_after.is_none());
    assert!(plan.picture(ProjectFrame(14)).is_err());
}

#[test]
fn unrepresentable_nested_exact_arithmetic_is_an_error_not_rounded_output() {
    let document = document(
        &["outer"],
        vec![
            ("child", hold(i64::MAX)),
            ("inner", retime("child", i64::MAX, 1, i64::MAX)),
            ("middle", retime("inner", i64::MAX, 1, i64::MAX)),
            ("outer", retime("middle", i64::MAX, 1, i64::MAX)),
        ],
    );
    let plan = RenderPlan::compile(&document).unwrap();
    assert!(matches!(
        plan.picture(ProjectFrame(0)),
        Err(PlanError::Time(TimeError::Overflow))
    ));
}

proptest! {
    #[test]
    fn prefix_index_matches_explicit_sequence_reference(lengths in proptest::collection::vec(0_i64..12, 0..80)) {
        let names: Vec<_> = (0..lengths.len()).map(|i| format!("node-{i}")).collect();
        let nodes = names.iter().zip(&lengths).map(|(name, frames)| (name.as_str(), if *frames == 0 { BeatNode::sequence("Empty", vec![]) } else { hold(*frames) })).collect();
        let roots: Vec<_> = names.iter().map(String::as_str).collect();
        let plan = RenderPlan::compile(&document(&roots, nodes)).unwrap();
        let mut cursor = 0;
        for (name, frames) in names.iter().zip(&lengths) {
            for local in 0..*frames {
                let sample = plan.picture(ProjectFrame(cursor + local)).unwrap();
                prop_assert_eq!(sample.instance.node, id(name));
                prop_assert_eq!(sample.local_position, ExactRatio::new(i128::from(local) * 2 + 1, 2).unwrap());
                prop_assert!(sample.lookup.sequence_comparisons <= 7);
            }
            cursor += frames;
        }
        prop_assert_eq!(plan.duration().frames(), cursor);
        prop_assert!(plan.picture(ProjectFrame(cursor)).is_err());
    }

    #[test]
    fn compact_repeat_matches_explicit_reference(child in 1_i64..10, gap in 0_i64..7, plays in 1_u32..40) {
        let doc = document(&["repeat"], vec![("child", hold(child)), ("repeat", repeat("child", plays, gap, "plays"))]);
        let plan = RenderPlan::compile(&doc).unwrap();
        for frame in 0..plan.duration().frames() {
            let play = (frame / (child + gap)) as u32;
            let local = frame % (child + gap);
            let sample = plan.picture(ProjectFrame(frame)).unwrap();
            if local < child {
                prop_assert_eq!(&sample.instance.node, &id("child"));
                prop_assert_eq!(sample.instance.repeats[0].iteration.ordinal, play);
                prop_assert!(sample.gap_after.is_none());
            } else {
                prop_assert!(play < plays - 1);
                prop_assert_eq!(&sample.instance.node, &id("repeat"));
                prop_assert_eq!(sample.gap_after.unwrap().ordinal, play);
            }
            sample.instance.validate(&doc).unwrap();
        }
    }
}

#[test]
fn placed_picture_holds_only_selected_endpoints_and_preserves_negative_starts() {
    let source_index = index("video", clock(), &[0, 500, 1000, 1500, 2000], 2500);
    for start in [
        ExactRatio::new(5, 4).unwrap(),
        ExactRatio::new(-3, 4).unwrap(),
    ] {
        for endpoints in [EndpointPolicy::Reject, EndpointPolicy::HoldAdjacent] {
            let frames = ExactRatio::new(3, 2).unwrap();
            let plan = RenderPlan::compile(&document(
                &["source"],
                vec![(
                    "source",
                    source_with_mapping(
                        4,
                        500,
                        1500,
                        SourceVideoMapping::Placement {
                            start,
                            frames,
                            endpoints,
                        },
                    ),
                )],
            ))
            .unwrap();
            for frame in 0..4 {
                let picture = plan.picture(ProjectFrame(frame)).unwrap().picture;
                let relative = ExactRatio::new(i128::from(frame) * 2 + 1, 2)
                    .unwrap()
                    .checked_sub(start)
                    .unwrap();
                let expected = ExactRatio::integer(500)
                    .checked_add(
                        relative
                            .checked_mul(ExactRatio::new(2000, 3).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                assert_eq!(ticks(&picture), expected);
                let outside = expected.compare_integer(500).is_lt()
                    || !expected.compare_integer(1500).is_lt();
                let selected = picture.select_source_frame(&source_index);
                if outside && endpoints == EndpointPolicy::Reject {
                    assert!(selected.is_err());
                } else {
                    let expected_id = if expected.compare_integer(1000).is_lt() {
                        1
                    } else {
                        2
                    };
                    assert_eq!(selected.unwrap().identity, SourceFrameId(expected_id));
                }
            }
        }
    }
}
