//! A rewind Hold shows the evaluated pictures of the passage before its
//! occurrence in reverse order, with the framing it does not share.

use super::*;

fn rewind(frames: i64, bounce: bool) -> BeatNode {
    node(NodeKind::Hold {
        recipe: HoldRecipe {
            duration: duration(frames),
            video: HoldVideo::Rewind { bounce },
            picture_context: None,
            audio: HoldAudio::Rewind { bounce },
        },
    })
}

fn freeze(frames: i64, ticks: i64) -> BeatNode {
    node(NodeKind::Hold {
        recipe: HoldRecipe {
            duration: duration(frames),
            video: HoldVideo::Freeze {
                asset: asset_id("video"),
                timestamp: SourceTimestamp {
                    ticks,
                    time_base: clock(),
                },
            },
            picture_context: None,
            audio: HoldAudio::Silence,
        },
    })
}

/// Three Original frames, a cut to a two-frame freeze, a framed group of two
/// frames and a 2x speed change of two output frames: nine frames.
fn passage(tail: Vec<(&'static str, BeatNode)>, roots: &[&str]) -> ProjectDocument {
    let mut nodes = vec![
        ("a", source(3, 0, 3003)),
        ("pause", freeze(2, 50050)),
        (
            "group",
            framed(BeatNode::sequence("Group", vec![id("inside")]), 2),
        ),
        ("inside", source(2, 10010, 12012)),
        ("fast", retime("fast_source", 2, 0, 4)),
        ("fast_source", source(4, 20020, 24024)),
    ];
    nodes.extend(tail);
    document(roots, nodes)
}

/// The framing a forward sample shows below the root, as a freeze would
/// capture it on the canvas.
fn captured(sample: &deadpan_plan::PictureSample) -> CapturedFraming {
    let below_root = sample.framing.len() - 1;
    CapturedFraming::capture(
        sample.picture_context.as_deref(),
        CapturedCanvas {
            width: 1920,
            height: 1080,
            fit: CapturedFit::Fit,
            layers: sample.framing[..below_root]
                .iter()
                .map(|scope| scope.pose)
                .collect(),
        },
    )
    .unwrap()
}

#[test]
fn a_rewind_shows_every_evaluated_picture_of_a_composite_passage_in_reverse() {
    let document = passage(
        vec![("rewind", rewind(9, false))],
        &["a", "pause", "group", "fast", "rewind"],
    );
    let plan = RenderPlan::compile(&document).unwrap();
    assert_eq!(plan.duration(), duration(18));
    for k in 0..9 {
        let forward = plan.picture(ProjectFrame(8 - k)).unwrap();
        let back = plan.picture(ProjectFrame(9 + k)).unwrap();
        assert_eq!(back.picture, forward.picture, "frame {k}");
        assert_eq!(back.instance.node.as_str(), "rewind");
        assert_eq!(
            back.picture_context.as_deref(),
            Some(&captured(&forward)),
            "frame {k} keeps the passage's own framing"
        );
    }
    // The 2x speed change shows every other source picture, so its reversal
    // steps back two pictures per frame; the framed group keeps its creep.
    let source_ticks = |frame| ticks(&plan.picture(ProjectFrame(frame)).unwrap().picture);
    assert_eq!(
        (9..11).map(source_ticks).collect::<Vec<_>>(),
        [ExactRatio::integer(23023), ExactRatio::integer(21021)]
    );
    let group = plan.picture(ProjectFrame(11)).unwrap();
    assert!(
        group.picture_context.as_ref().unwrap().canvases[0]
            .layers
            .iter()
            .any(|layer| layer.is_some()),
        "the group's framing is captured"
    );
}

#[test]
fn a_bounced_rewind_omits_the_turning_picture_and_black_precedes_the_start() {
    let document = passage(
        vec![("rewind", rewind(10, true))],
        &["a", "pause", "group", "fast", "rewind"],
    );
    let plan = RenderPlan::compile(&document).unwrap();
    for k in 0..8 {
        assert_eq!(
            plan.picture(ProjectFrame(9 + k)).unwrap().picture,
            plan.picture(ProjectFrame(7 - k)).unwrap().picture,
            "frame {k}"
        );
    }
    // Past the project start a rewind shows black.
    for frame in 17..19 {
        let sample = plan.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(sample.picture, Picture::Background);
        assert!(sample.picture_context.is_none());
    }
}

#[test]
fn rewinds_do_not_nest_and_have_no_picture_off_the_edit_clock() {
    let document = passage(
        vec![("first", rewind(2, false)), ("second", rewind(4, false))],
        &["a", "pause", "group", "fast", "first", "second"],
    );
    let plan = RenderPlan::compile(&document).unwrap();
    // The second rewind's passage holds the first rewind, which reads black.
    for frame in 11..13 {
        assert_eq!(
            plan.picture(ProjectFrame(frame)).unwrap().picture,
            Picture::Background
        );
    }
    assert_eq!(
        plan.picture(ProjectFrame(13)).unwrap().picture,
        plan.picture(ProjectFrame(8)).unwrap().picture
    );

    // Inside a speed change there is no edit position: invalid.
    let unity = passage(
        vec![
            ("slow", retime("slow_hold", 2, 0, 2)),
            ("slow_hold", rewind(2, false)),
        ],
        &["a", "pause", "group", "fast", "slow"],
    );
    let mut wire = serde_json::to_value(&unity).unwrap();
    wire["nodes"]["slow"]["kind"]["duration"] = serde_json::json!(4);
    let error = ProjectDocument::from_json(&wire.to_string()).unwrap_err();
    assert!(error.to_string().contains("speed change"), "{error}");
}
