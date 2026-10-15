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
            // Consecutive tracked samples interpolate, so the move is a
            // one-tick ramp just before frame 5.
            samples: [
                sample(0, 200_000),
                sample(5004, 200_000),
                sample(5005, 800_000),
            ]
            .into_iter()
            .filter(|sample| sample.at < end)
            .collect(),
            corrections: Vec::new(),
            provenance: None,
            display_aspect: None,
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

/// `document` on a `width`x`height` canvas, its target recording `aspect`
/// and centered at y = 1/4 of the picture.
fn on_canvas(
    document: ProjectDocument,
    width: u32,
    height: u32,
    aspect: Option<ExactRatio>,
) -> ProjectDocument {
    let mut value = serde_json::to_value(document).unwrap();
    value["presentation_basis"]["width"] = width.into();
    value["presentation_basis"]["height"] = height.into();
    let target = &mut value["targets"]["speaker"];
    target["display_aspect"] = serde_json::to_value(aspect).unwrap();
    target["region"]["center"][1] = 250_000.into();
    for sample in target["samples"].as_array_mut().unwrap() {
        sample["region"]["center"][1] = 250_000.into();
    }
    if aspect.is_none() {
        target.as_object_mut().unwrap().remove("display_aspect");
    }
    ProjectDocument::from_json(&value.to_string()).unwrap()
}

#[test]
fn a_follow_centers_the_target_where_the_picture_is_fitted_on_the_canvas() {
    let widescreen = || {
        with_target(
            document(&["a"], vec![("a", source(10, 0, 10010))]),
            10010,
            &[("a", 2)],
        )
    };
    // A 16:9 picture on a vertical 1080x1920 canvas fills its width and
    // covers 81/256 of its height: y = 1/4 lands at 1/2 - 81/1024.
    let vertical = on_canvas(widescreen(), 1080, 1920, Some(ratio(16, 9)));
    let plan = RenderPlan::compile(&vertical).unwrap();
    let centered = pose(&plan, 0, 0);
    assert_eq!(centered.center_x, grid(1, 5));
    assert_eq!(centered.center_y, grid(431, 1024));
    assert_eq!(pose(&plan, 6, 0).center_x, grid(4, 5));
    // On a canvas of the picture's own aspect nothing moves.
    let same = on_canvas(widescreen(), 1920, 1080, Some(ratio(16, 9)));
    let plan = RenderPlan::compile(&same).unwrap();
    assert_eq!(pose(&plan, 0, 0).center_y, grid(1, 4));
    // A target recorded without an aspect keeps source coordinates.
    let legacy = on_canvas(widescreen(), 1080, 1920, None);
    let plan = RenderPlan::compile(&legacy).unwrap();
    assert_eq!(pose(&plan, 0, 0).center_y, grid(1, 4));
    // A 9:16 picture on a 16:9 canvas is pillarboxed: x = 1/5 lands at
    // 1/2 - (3/10)(81/256).
    let pillarboxed = on_canvas(widescreen(), 1920, 1080, Some(ratio(9, 16)));
    let plan = RenderPlan::compile(&pillarboxed).unwrap();
    assert_eq!(pose(&plan, 0, 0).center_x, grid(1037, 2560));
    assert_eq!(pose(&plan, 0, 0).center_y, grid(1, 4));
}

#[test]
fn an_escalating_zoom_moves_toward_its_target_and_keeps_the_view_inside_the_picture() {
    let mut escalating = repeat("source", 3, 1, "plays");
    let NodeKind::Repeat { escalation, .. } = &mut escalating.kind else {
        unreachable!()
    };
    *escalation = Some(RepeatEscalation {
        gain_step: GainDb::UNITY,
        zoom: Some(ZoomStep {
            step: ExactRatio::ONE,
            progression: ZoomProgression::Add,
            target: None,
        }),
    });
    let document = with_target(
        document(
            &["repeat"],
            vec![("repeat", escalating), ("source", source(4, 0, 4000))],
        ),
        10010,
        &[],
    );
    let mut value = serde_json::to_value(&document).unwrap();
    value["nodes"]["repeat"]["kind"]["escalation"]["zoom"]["target"] = "speaker".into();
    let document = ProjectDocument::from_json(&value.to_string()).unwrap();
    let plan = RenderPlan::compile(&document).unwrap();
    let escalated = |frame: i64| {
        plan.picture(ProjectFrame(frame))
            .unwrap()
            .framing
            .iter()
            .find(|layer| layer.escalation)
            .and_then(|layer| layer.pose)
    };
    // Plays occupy 0..4, 5..9 and 10..14; the subject sits at x = 0.2.
    assert_eq!(escalated(1), None, "the first play is unchanged");
    // At 2× the view's left edge would cross the picture at center 0.2, so it
    // stops at 1/4.
    let second = escalated(6).unwrap();
    assert_eq!(second.scale, ExactRatio::integer(2));
    assert_eq!(second.center_x, grid(1, 4));
    assert_eq!(second.center_y, grid(1, 2));
    // At 3× the subject itself fits.
    let third = escalated(12).unwrap();
    assert_eq!(third.scale, ExactRatio::integer(3));
    assert_eq!(third.center_x, grid(1, 5));
    // A target missing from the document refuses the escalation.
    let mut value = serde_json::to_value(&document).unwrap();
    value.as_object_mut().unwrap().remove("targets");
    assert!(ProjectDocument::from_json(&value.to_string()).is_err());
}
