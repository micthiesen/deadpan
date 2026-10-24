//! Exact independent coordinate witnesses for retained target projection.

use super::*;
use deadpan_core::framing_zoom::ZoomCurve;
use deadpan_plan::{follow_pose, follow_pose_in_context};

fn q(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn target(x: u32, y: u32) -> AttentionTarget {
    AttentionTarget {
        label: "Point".into(),
        asset: asset_id("video"),
        span: span(0, 10010),
        region: TargetPoint { center: [x, y] }.into(),
        samples: vec![],
        corrections: vec![],
        provenance: None,
        display_aspect: Some(q(2, 1)),
    }
}
fn stage(
    width: u32,
    height: u32,
    fit: CapturedFit,
    layers: Vec<Option<FramingPose>>,
) -> CapturedCanvas {
    CapturedCanvas {
        width,
        height,
        fit,
        layers,
    }
}
fn context(stages: Vec<CapturedCanvas>) -> CapturedFraming {
    CapturedFraming::new(stages).unwrap()
}
fn point() -> SourcePoint {
    SourcePoint {
        ticks: q(1001, 1),
        time_base: clock(),
    }
}
fn pose(x: ExactRatio, y: ExactRatio, scale: ExactRatio) -> FramingPose {
    FramingPose::new(x, y, scale).unwrap()
}
fn projected(
    target: &AttentionTarget,
    context: &CapturedFraming,
    canvas: [u32; 2],
    inner: &[Option<FramingPose>],
) -> Option<FramingPose> {
    follow_pose_in_context(
        target,
        &asset_id("video"),
        point(),
        canvas,
        Some(context),
        inner.iter().copied(),
        ExactRatio::ONE,
    )
}
fn expected(x: ExactRatio, y: ExactRatio) -> FramingPose {
    pose(x, y, ExactRatio::ONE).quantized().unwrap()
}

#[test]
fn captured_follow_fit_fill_and_first_scope_clip_use_the_render_order() {
    // 2:1 source fitted into a square: source (1/4,1/4) -> (1/4,3/8).
    let value = target(250000, 250000);
    let fitted = context(vec![stage(100, 100, CapturedFit::Fit, vec![])]);
    assert_eq!(
        projected(&value, &fitted, [100, 100], &[]),
        Some(expected(q(1, 4), q(3, 8)))
    );
    // Fill uses twice the normalized source width: (1/4,1/4)->(0,1/4).
    let filled = context(vec![stage(100, 100, CapturedFit::Fill, vec![])]);
    assert_eq!(
        projected(&value, &filled, [100, 100], &[]),
        Some(expected(q(0, 1), q(1, 4)))
    );
    assert_eq!(
        projected(&target(0, 500000), &filled, [100, 100], &[]),
        None
    );
    let mut portrait = value.clone();
    portrait.display_aspect = Some(q(1, 2));
    assert_eq!(
        projected(&portrait, &fitted, [100, 100], &[]),
        Some(expected(q(3, 8), q(1, 4)))
    );
    assert_eq!(
        projected(&portrait, &filled, [100, 100], &[]),
        Some(expected(q(1, 4), ExactRatio::ZERO))
    );

    // The source-left edge initially lands at -1/2 under Fill. A first pose
    // centered at zero brings it to zero BEFORE the first clip.
    let bring_in = pose(q(0, 1), q(1, 2), q(1, 1));
    let first_pose = context(vec![stage(
        100,
        100,
        CapturedFit::Fill,
        vec![Some(bring_in)],
    )]);
    assert_eq!(
        projected(&target(0, 500000), &first_pose, [100, 100], &[]),
        Some(expected(q(0, 1), q(1, 2)))
    );
    let earlier_clip = context(vec![stage(
        100,
        100,
        CapturedFit::Fill,
        vec![None, Some(bring_in)],
    )]);
    assert_eq!(
        projected(&target(0, 500000), &earlier_clip, [100, 100], &[]),
        None
    );
}

fn nested_context() -> CapturedFraming {
    context(vec![
        stage(
            100,
            100,
            CapturedFit::Fit,
            vec![Some(pose(q(1, 2), q(1, 2), q(2, 1)))],
        ),
        stage(
            200,
            100,
            CapturedFit::Fill,
            vec![Some(pose(q(1, 2), q(1, 2), q(1, 2)))],
        ),
    ])
}

#[test]
fn captured_follow_maps_complete_canvases_between_stages_and_honors_all_clips() {
    // Independent hand fractions: source(1/4,1/4), Fit square ->(1/4,3/8),
    // zoom2 ->(0,1/4), Fill 2:1 ->(0,0), zoom1/2 ->(1/4,1/4),
    // Fit the complete wide canvas into final square ->(1/4,3/8).
    assert_eq!(
        projected(&target(250000, 250000), &nested_context(), [100, 100], &[]),
        Some(expected(q(1, 4), q(3, 8)))
    );
    let lost = context(vec![
        stage(100, 100, CapturedFit::Fill, vec![]),
        stage(
            100,
            100,
            CapturedFit::Fit,
            vec![Some(pose(q(1, 2), q(1, 2), q(1, 2)))],
        ),
    ]);
    assert_eq!(projected(&target(0, 500000), &lost, [100, 100], &[]), None);

    let identity = context(vec![stage(100, 100, CapturedFit::Fit, vec![])]);
    let mut square = target(100000, 500000);
    square.display_aspect = Some(ExactRatio::ONE);
    let inner = [
        Some(pose(q(1, 2), q(1, 2), q(2, 1))),
        Some(pose(q(1, 2), q(1, 2), q(1, 2))),
    ];
    // The first live child crop removes x=.1; an outer zoom-out cannot recover it.
    assert_eq!(projected(&square, &identity, [100, 100], &inner), None);
    // The context-free API observes the same irreversible live clip.
    assert_eq!(
        follow_pose(
            &square,
            &asset_id("video"),
            point(),
            [100, 100],
            inner,
            ExactRatio::ONE
        ),
        None
    );
}

#[test]
fn captured_follow_refuses_missing_identity_invalid_context_and_exact_overflow() {
    let identity = context(vec![stage(100, 100, CapturedFit::Fit, vec![])]);
    let mut value = target(250000, 250000);
    value.display_aspect = None;
    assert_eq!(projected(&value, &identity, [100, 100], &[]), None);
    assert_eq!(
        follow_pose(
            &value,
            &asset_id("video"),
            point(),
            [100, 100],
            [],
            ExactRatio::ONE
        ),
        Some(expected(q(1, 4), q(1, 4)))
    );
    value.display_aspect = Some(q(2, 1));
    assert_eq!(
        follow_pose_in_context(
            &value,
            &asset_id("other"),
            point(),
            [100, 100],
            Some(&identity),
            [],
            ExactRatio::ONE
        ),
        None
    );
    let outside = SourcePoint {
        ticks: q(10010, 1),
        ..point()
    };
    assert_eq!(
        follow_pose_in_context(
            &value,
            &asset_id("video"),
            outside,
            [100, 100],
            Some(&identity),
            [],
            ExactRatio::ONE
        ),
        None
    );
    let invalid = CapturedFraming { canvases: vec![] };
    assert_eq!(projected(&value, &invalid, [100, 100], &[]), None);
    assert_eq!(projected(&value, &identity, [0, 100], &[]), None);
    let tiny = pose(q(1, i128::MAX), q(1, 2), ExactRatio::ONE);
    let overflow = context(vec![stage(100, 100, CapturedFit::Fit, vec![Some(tiny)])]);
    assert_eq!(projected(&value, &overflow, [100, 100], &[]), None);
}

#[test]
fn captured_hold_follow_uses_frozen_source_point_and_complete_output_clock() {
    let retained = nested_context();
    let recipe = HoldRecipe {
        duration: duration(12),
        video: HoldVideo::Freeze {
            asset: asset_id("video"),
            timestamp: SourceTimestamp {
                ticks: 1001,
                time_base: clock(),
            },
        },
        picture_context: Some(retained.clone()),
        audio: HoldAudio::Silence,
    };
    let follow = |scale_transition| Framing {
        clock: FramingClock::OwnerOutput,
        value: FramingValue::Follow {
            target: TargetId::new("point").unwrap(),
            scale: ExactRatio::ONE,
            fallback: pose(q(1, 3), q(2, 3), q(1, 2)),
            scale_transition,
        },
    };
    let held = node(NodeKind::Hold {
        recipe: recipe.clone(),
    });
    let group = BeatNode::sequence("Group", vec![id("h")]);
    let mut wire = serde_json::to_value(document(&["g"], vec![("g", group), ("h", held)])).unwrap();
    wire["nodes"]["h"]["framing"] = serde_json::to_value(follow(Some(FollowScaleTransition {
        factor: q(2, 1),
        curve: ZoomCurve::Linear,
    })))
    .unwrap();
    wire["nodes"]["g"]["framing"] = serde_json::to_value(follow(None)).unwrap();
    wire["presentation_basis"]["width"] = 100.into();
    wire["presentation_basis"]["height"] = 100.into();
    wire["targets"] = serde_json::to_value(BTreeMap::from([(
        TargetId::new("point").unwrap(),
        target(250000, 250000),
    )]))
    .unwrap();
    let original = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = RenderPlan::compile(&original).unwrap();
    for frame in 0..12 {
        let sample = plan.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(sample.picture_context.as_deref(), Some(&retained));
        let (asset, source) = sample.picture.follow_point().unwrap();
        assert_eq!(asset, &asset_id("video"));
        assert_eq!(source, point());
        let own = sample
            .framing
            .iter()
            .find(|v| v.instance.node == id("h"))
            .unwrap();
        assert_eq!(own.local_position, q(2 * i128::from(frame) + 1, 2));
        assert_eq!(own.effective_duration, q(12, 1));
        let scale = ExactRatio::ONE
            .checked_add(q(2 * i128::from(frame) + 1, 24))
            .unwrap();
        assert_eq!(
            own.pose,
            Some(pose(q(1, 4), q(3, 8), scale).quantized().unwrap())
        );
        // The outer Follow sees the target centered by the inner Hold Follow.
        let outer = sample
            .framing
            .iter()
            .find(|v| v.instance.node == id("g"))
            .unwrap();
        assert_eq!(outer.pose, Some(expected(q(1, 2), q(1, 2))));
    }
    assert!(
        matches!(&original.nodes()[&id("h")].kind,NodeKind::Hold {recipe:stored} if stored==&recipe)
    );
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        rng_seed: proptest::test_runner::RngSeed::Fixed(0xdead_484f_4c44),
        failure_persistence: None,
        ..ProptestConfig::default()
    })]
    #[test]
    fn captured_follow_identity_fit_matches_uncaptured_target_for_both_shapes(x in 0u32..=1_000_000,y in 0u32..=1_000_000,width in 1u32..4000,height in 1u32..4000,point_kind in any::<bool>()) {
        // Fitting the same-aspect input canvas to the live canvas is identical
        // to directly fitting the source, independently of point/box encoding.
        let mut value=target(x,y);
        if !point_kind {value.region=TargetRegion {center:[x,y],size:[10000,10000]}.into();}
        let retained=context(vec![stage(200,100,CapturedFit::Fit,vec![])]);
        let old=follow_pose(&value,&asset_id("video"),point(),[width,height],[],ExactRatio::ONE);
        let new=projected(&value,&retained,[width,height],&[]);
        prop_assert_eq!(new,old);
    }
}
