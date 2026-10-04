//! `Follow` framing centers on an attention target at the source time shown.

use super::*;

fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}
/// `numerator/denominator` on the framing numeric grid.
fn grid(numerator: i128, denominator: i128) -> ExactRatio {
    FramingPose::new(ratio(numerator, denominator), ratio(1, 2), ExactRatio::ONE)
        .unwrap()
        .quantized()
        .unwrap()
        .center_x
}
fn region(x: u32) -> TargetRegion {
    TargetRegion {
        center: [x, 500_000],
        size: [100_000, 100_000],
    }
}
fn follow(scale: i64) -> Framing {
    Framing {
        clock: FramingClock::OwnerOutput,
        value: FramingValue::Follow {
            target: TargetId::new("speaker").unwrap(),
            scale: ExactRatio::integer(scale),
            fallback: FramingPose::identity(),
        },
    }
}
/// The target moves from x=0.2 to x=0.8 at source frame 5 and ends at `end`;
/// `follows` gain framing that follows it.
fn with_target(document: ProjectDocument, end: i64, follows: &[(&str, i64)]) -> ProjectDocument {
    let mut value = serde_json::to_value(document).unwrap();
    for (node, scale) in follows {
        value["nodes"][node]["framing"] = serde_json::to_value(follow(*scale)).unwrap();
    }
    let sample = |at: i64, x: u32| TargetSample {
        at,
        region: region(x),
        confidence: 950,
        state: TrackState::Tracked,
    };
    value["targets"] = serde_json::to_value(BTreeMap::from([(
        TargetId::new("speaker").unwrap(),
        AttentionTarget {
            label: "Speaker".into(),
            asset: asset_id("video"),
            span: span(0, end),
            region: region(200_000),
            samples: [sample(0, 200_000), sample(5005, 800_000)]
                .into_iter()
                .filter(|sample| sample.at < end)
                .collect(),
            corrections: Vec::new(),
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&value.to_string()).unwrap()
}
fn pose(plan: &RenderPlan, frame: i64, layer: usize) -> FramingPose {
    plan.picture(ProjectFrame(frame)).unwrap().framing[layer]
        .pose
        .unwrap()
}

#[test]
fn a_follow_centers_on_the_target_at_each_pictures_source_time() {
    let document = with_target(
        document(&["a"], vec![("a", source(10, 0, 10010))]),
        10010,
        &[("a", 2)],
    );
    let plan = RenderPlan::compile(&document).unwrap();
    // Layers run leaf first: the Source's own follow, then the root.
    assert_eq!(pose(&plan, 0, 0).center_x, grid(1, 5));
    assert_eq!(pose(&plan, 0, 0).scale, ExactRatio::integer(2));
    assert_eq!(pose(&plan, 6, 0).center_x, grid(4, 5));
}

#[test]
fn outside_the_target_the_fallback_applies() {
    let document = with_target(
        document(&["a"], vec![("a", source(10, 0, 10010))]),
        3003,
        &[("a", 2)],
    );
    let plan = RenderPlan::compile(&document).unwrap();
    assert_eq!(pose(&plan, 1, 0).center_x, grid(1, 5));
    assert_eq!(pose(&plan, 5, 0), FramingPose::identity());
}

#[test]
fn an_outer_follow_sees_the_subject_through_inner_framing() {
    let mut zoomed = source(10, 0, 10010);
    zoomed.framing = Some(
        Framing::static_pose(
            FramingPose::new(ratio(1, 2), ratio(1, 2), ExactRatio::integer(2)).unwrap(),
        )
        .unwrap(),
    );
    let group = BeatNode::sequence("Group", vec![id("a")]);
    let document = with_target(
        document(&["g"], vec![("g", group), ("a", zoomed)]),
        10010,
        &[("g", 1)],
    );
    let plan = RenderPlan::compile(&document).unwrap();
    // Inner zoom 2× about the middle moves x=0.2 to (0.2-0.5)*2+0.5 = -0.1.
    assert_eq!(pose(&plan, 0, 1).center_x, grid(-1, 10));
    // A target removed from the document refuses the framing that follows it.
    let mut value = serde_json::to_value(&document).unwrap();
    value.as_object_mut().unwrap().remove("targets");
    assert!(ProjectDocument::from_json(&value.to_string()).is_err());
}
