//! Retime output scale time stays separate from mapped exact provider time.

use super::*;
use deadpan_core::framing_zoom::ZoomCurve;
use deadpan_plan::{AudioContent, AudioQueryLimits};

const Q: i128 = 4_294_967_296;
const OWNER_START: i64 = 2;

fn r(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}

/// Exact, small nonnegative integer quotient with ties to even. Expected
/// values never call Framing evaluation, target lookup, or pose quantization.
fn even_div(n: i128, d: i128) -> i128 {
    let (whole, rest) = (n / d, n % d);
    whole + i128::from(2 * rest > d || (2 * rest == d && whole % 2 != 0))
}

fn grid(n: i128, d: i128) -> ExactRatio {
    r(even_div(n * Q, d), Q)
}

fn expected_scale(
    frame: i64,
    fallback: bool,
    offset: i128,
    authored_duration: i128,
    curve: ZoomCurve,
) -> ExactRatio {
    // The authored clock is f+1/2+offset. Its retained endpoints hold;
    // provider child positions and decoded frame ordinals never enter here.
    let n = (2 * i128::from(frame) + 1 + 2 * offset).clamp(0, 2 * authored_duration);
    let t = even_div(n * Q, 2 * authored_duration);
    let weight = match curve {
        ZoomCurve::Linear => t,
        ZoomCurve::Smoothstep => even_div(t * t * (3 * Q - 2 * t), Q * Q),
        ZoomCurve::Step => {
            if t == Q {
                Q
            } else {
                0
            }
        }
    };
    // Live3/2->5/2, fallback3/4->5/4, factor5/3. Endpoints are exact Q32.
    r(
        if fallback {
            3 * Q / 4 + even_div(weight, 2)
        } else {
            3 * Q / 2 + weight
        },
        Q,
    )
}

fn geometry(point: bool, x: u32) -> TargetGeometry {
    let center = [x, 500_000];
    if point {
        TargetPoint { center }.into()
    } else {
        TargetRegion {
            center,
            size: [100_000, 100_000],
        }
        .into()
    }
}

fn target(point: bool) -> AttentionTarget {
    let sample = |at, x, state| TargetSample {
        at,
        region: geometry(point, x),
        confidence: if state == TrackState::Lost { 0 } else { 950 },
        state,
    };
    AttentionTarget {
        label: "Retime provider path".into(),
        asset: asset_id("video"),
        span: span(0, 22022),
        region: geometry(point, 250_000),
        samples: vec![
            sample(0, 250_000, TrackState::Tracked),
            sample(4004, 750_000, TrackState::Tracked),
            sample(5005, 750_000, TrackState::Lost),
            sample(21021, 625_000, TrackState::Tracked),
        ],
        corrections: vec![TargetCorrection {
            at: 20020,
            region: geometry(point, 875_000),
        }],
        provenance: None,
        display_aspect: Some(r(16, 9)),
    }
}

fn pose(x: ExactRatio, scale: ExactRatio) -> FramingPose {
    FramingPose::new(x, r(1, 2), scale).unwrap()
}

fn captured() -> CapturedFraming {
    CapturedFraming::new(vec![CapturedCanvas {
        width: 1920,
        height: 1080,
        fit: CapturedFit::Fit,
        layers: vec![Some(pose(r(1, 2), r(1, 2)))],
    }])
    .unwrap()
}

fn voiced(frames: i64, start: i64, end: i64) -> BeatNode {
    let mut child = source(frames, start, end);
    let NodeKind::Source { source } = &mut child.kind else {
        unreachable!()
    };
    source.audio = Some(SourceAudio {
        asset: asset_id("video"),
        span: span(start, end),
    });
    child
}

fn fixture(
    point: bool,
    output_frames: i64,
    clock: FramingClock,
    curve: ZoomCurve,
) -> (ProjectDocument, ProjectDocument) {
    let mut a = voiced(4, 0, 4004);
    // A maps source x to2x-1 and clips. At the first sampled source point
    // this is negative. The enclosing zoom-out cannot restore that target.
    a.framing = Some(Framing::static_pose(pose(r(3, 4), r(2, 1))).unwrap());
    let mut freeze = node(NodeKind::Hold {
        recipe: HoldRecipe {
            duration: duration(2),
            video: HoldVideo::Freeze {
                asset: asset_id("video"),
                timestamp: SourceTimestamp {
                    ticks: 6006,
                    time_base: super::clock(),
                },
            },
            picture_context: Some(captured()),
            audio: HoldAudio::Silence,
        },
    });
    freeze.framing = Some(Framing::static_pose(pose(r(1, 2), r(1, 2))).unwrap());
    let mut group = BeatNode::sequence("Provider changes", vec![id("a"), id("freeze"), id("b")]);
    group.framing = Some(Framing::static_pose(pose(r(1, 2), r(1, 2))).unwrap());
    let original = document(
        &["before", "retime", "after"],
        vec![
            ("before", hold(2)),
            ("retime", retime("group", output_frames, 1, 9)),
            ("group", group),
            ("a", a),
            ("freeze", freeze),
            ("b", voiced(4, 20020, 24024)),
            ("after", hold(2)),
        ],
    );
    let mut wire = serde_json::to_value(original).unwrap();
    wire["targets"] = serde_json::to_value(BTreeMap::from([(
        TargetId::new("subject").unwrap(),
        target(point),
    )]))
    .unwrap();
    let follow = Framing {
        clock,
        value: FramingValue::Follow {
            target: TargetId::new("subject").unwrap(),
            scale: r(3, 2),
            fallback: FramingPose::new(r(1, 8), r(7, 8), r(3, 4)).unwrap(),
            scale_transition: None,
        },
    };
    wire["nodes"]["retime"]["framing"] = serde_json::to_value(follow).unwrap();
    let fixed = ProjectDocument::from_json(&wire.to_string()).unwrap();
    wire["nodes"]["retime"]["framing"]["value"]["scale_transition"] =
        serde_json::to_value(FollowScaleTransition {
            factor: r(5, 3),
            curve,
        })
        .unwrap();
    let animated = ProjectDocument::from_json(&wire.to_string()).unwrap();
    (fixed, animated)
}

#[derive(Clone, Copy)]
struct Expected {
    input: (i128, i128),
    source: (i128, i128),
    decoded: u64,
    provider: &'static str,
    center: (i128, i128),
    fallback: bool,
}

fn fast() -> [Expected; 5] {
    [
        Expected {
            input: (9, 5),
            source: (9009, 5),
            decoded: 1,
            provider: "a",
            center: (1, 8),
            fallback: true,
        },
        Expected {
            input: (17, 5),
            source: (17017, 5),
            decoded: 3,
            provider: "a",
            center: (17, 40),
            fallback: false,
        },
        Expected {
            input: (5, 1),
            source: (6006, 1),
            decoded: 6,
            provider: "freeze",
            center: (17, 32),
            fallback: false,
        },
        Expected {
            input: (33, 5),
            source: (103103, 5),
            decoded: 20,
            provider: "b",
            center: (11, 16),
            fallback: false,
        },
        Expected {
            input: (41, 5),
            source: (111111, 5),
            decoded: 22,
            provider: "b",
            center: (1, 8),
            fallback: true,
        },
    ]
}

fn slow() -> [Expected; 12] {
    // A's interpolated centers round to416667,500000,583333,666667
    // millionths before child2x-1 then group(x/2+1/4), hence target-1/4.
    // Source6006 holds Lost3/4 through retained1/2, own1/2 and group1/2:
    // 3/4 ->5/8 ->9/16 ->17/32.
    // B's correction is7/8 through group1/2 ->11/16; later tracked5/8
    // becomes9/16. Its target ends at22022, excluding the exact endpoint.
    [
        Expected {
            input: (4, 3),
            source: (4004, 3),
            decoded: 1,
            provider: "a",
            center: (1, 8),
            fallback: true,
        },
        Expected {
            input: (2, 1),
            source: (2002, 1),
            decoded: 2,
            provider: "a",
            center: (1, 4),
            fallback: false,
        },
        Expected {
            input: (8, 3),
            source: (8008, 3),
            decoded: 2,
            provider: "a",
            center: (333333, 1000000),
            fallback: false,
        },
        Expected {
            input: (10, 3),
            source: (10010, 3),
            decoded: 3,
            provider: "a",
            center: (416667, 1000000),
            fallback: false,
        },
        Expected {
            input: (4, 1),
            source: (6006, 1),
            decoded: 6,
            provider: "freeze",
            center: (17, 32),
            fallback: false,
        },
        Expected {
            input: (14, 3),
            source: (6006, 1),
            decoded: 6,
            provider: "freeze",
            center: (17, 32),
            fallback: false,
        },
        Expected {
            input: (16, 3),
            source: (6006, 1),
            decoded: 6,
            provider: "freeze",
            center: (17, 32),
            fallback: false,
        },
        Expected {
            input: (6, 1),
            source: (20020, 1),
            decoded: 20,
            provider: "b",
            center: (11, 16),
            fallback: false,
        },
        Expected {
            input: (20, 3),
            source: (62062, 3),
            decoded: 20,
            provider: "b",
            center: (11, 16),
            fallback: false,
        },
        Expected {
            input: (22, 3),
            source: (64064, 3),
            decoded: 21,
            provider: "b",
            center: (9, 16),
            fallback: false,
        },
        Expected {
            input: (8, 1),
            source: (22022, 1),
            decoded: 22,
            provider: "b",
            center: (1, 8),
            fallback: true,
        },
        Expected {
            input: (26, 3),
            source: (68068, 3),
            decoded: 22,
            provider: "b",
            center: (1, 8),
            fallback: true,
        },
    ]
}

fn verify(
    point: bool,
    expected: &[Expected],
    clock: FramingClock,
    authored_offset: i128,
    authored_duration: i128,
    curve: ZoomCurve,
) -> RenderPlan {
    let frames = i64::try_from(expected.len()).unwrap();
    let (fixed, animated) = fixture(point, frames, clock, curve);
    let old = RenderPlan::compile(&fixed).unwrap();
    let new = RenderPlan::compile(&animated).unwrap();
    assert_eq!(old.duration(), duration(frames + 4));
    assert_eq!(new.duration(), old.duration());
    for name in ["root", "before", "group", "a", "freeze", "b", "after"] {
        assert_eq!(fixed.nodes()[&id(name)], animated.nodes()[&id(name)]);
    }
    let mut owner_before = fixed.nodes()[&id("retime")].clone();
    owner_before.framing = animated.nodes()[&id("retime")].framing.clone();
    assert_eq!(owner_before, animated.nodes()[&id("retime")]);
    assert_eq!(fixed.targets(), animated.targets());
    assert_eq!(fixed.overrides(), animated.overrides());
    assert_eq!(fixed.gap_overrides(), animated.gap_overrides());

    let source_index = index(
        "video",
        super::clock(),
        &(0..26).map(|ordinal| ordinal * 1001).collect::<Vec<_>>(),
        26026,
    );
    let mut fractional_source_witnesses = 0;
    for frame in 0..frames + 4 {
        let old_sample = old.picture(ProjectFrame(frame)).unwrap();
        let sample = new.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(sample.picture, old_sample.picture);
        assert_eq!(sample.instance, old_sample.instance);
        assert_eq!(sample.gap_after, old_sample.gap_after);
        assert_eq!(sample.local_position, old_sample.local_position);
        assert_eq!(sample.picture_context, old_sample.picture_context);
        let is_owner = |v: &&deadpan_plan::PictureFraming| v.instance.node == id("retime");
        assert_eq!(
            sample
                .framing
                .iter()
                .filter(|v| !is_owner(v))
                .collect::<Vec<_>>(),
            old_sample
                .framing
                .iter()
                .filter(|v| !is_owner(v))
                .collect::<Vec<_>>()
        );
        if !(OWNER_START..OWNER_START + frames).contains(&frame) {
            assert_eq!(
                sample.framing, old_sample.framing,
                "half-open neighbor{frame}"
            );
            assert!(
                sample
                    .framing
                    .iter()
                    .all(|v| v.instance.node != id("retime"))
            );
            continue;
        }
        let local = frame - OWNER_START;
        let oracle = expected[usize::try_from(local).unwrap()];
        assert_eq!(sample.instance.node, id(oracle.provider));
        assert!(sample.instance.repeats.is_empty());
        assert!(sample.gap_after.is_none());
        let (asset, source) = sample.picture.follow_point().unwrap();
        assert_eq!(asset, &asset_id("video"));
        assert_eq!(source.time_base, super::clock());
        assert_eq!(source.ticks, r(oracle.source.0, oracle.source.1));
        let decoded = sample.picture.select_source_frame(&source_index).unwrap();
        assert_eq!(decoded.identity.0, oracle.decoded);
        assert_eq!(decoded.pts, i64::try_from(oracle.decoded).unwrap() * 1001);
        if oracle.provider == "freeze" {
            assert!(matches!(sample.picture, Picture::Freeze { .. }));
            assert_eq!(sample.picture_context.as_deref(), Some(&captured()));
        } else {
            assert!(matches!(sample.picture, Picture::Source { .. }));
            assert!(sample.picture_context.is_none());
            if source.ticks != ExactRatio::integer(decoded.pts) {
                fractional_source_witnesses += 1;
            }
        }
        let input = sample
            .framing
            .iter()
            .find(|v| v.instance.node == id("group"))
            .unwrap();
        assert_eq!(input.local_position, r(oracle.input.0, oracle.input.1));
        assert_eq!(input.effective_duration, r(10, 1));
        let owner = sample.framing.iter().find(|v| is_owner(v)).unwrap();
        assert_eq!(owner.local_position, r(2 * i128::from(local) + 1, 2));
        assert_eq!(owner.effective_duration, r(i128::from(frames), 1));
        let shown = owner.pose.unwrap();
        assert_eq!(
            shown.center_x,
            grid(oracle.center.0, oracle.center.1),
            "point={point} local={local}"
        );
        assert_eq!(
            shown.center_y,
            if oracle.fallback { r(7, 8) } else { r(1, 2) }
        );
        assert_eq!(
            shown.scale,
            expected_scale(
                local,
                oracle.fallback,
                authored_offset,
                authored_duration,
                curve
            )
        );
    }
    assert!(
        fractional_source_witnesses >= 2,
        "must not qualify only decoded ordinals"
    );
    assert!(matches!(
        new.picture(ProjectFrame(frames + 4)),
        Err(PlanError::FrameOutOfRange { .. })
    ));
    let audio_end = old.audio_duration().unwrap();
    assert_eq!(audio_end, new.audio_duration().unwrap());
    let before_audio = old
        .audio(AudioSample(0)..audio_end, AudioQueryLimits::default())
        .unwrap();
    let after_audio = new
        .audio(AudioSample(0)..audio_end, AudioQueryLimits::default())
        .unwrap();
    assert!(
        before_audio
            .spans
            .iter()
            .any(|v| matches!(v.content, AudioContent::Source { .. }))
    );
    assert_eq!(before_audio.spans, after_audio.spans);
    new
}

fn owner_pose(plan: &RenderPlan, local: i64) -> FramingPose {
    plan.picture(ProjectFrame(OWNER_START + local))
        .unwrap()
        .framing
        .into_iter()
        .find(|layer| layer.instance.node == id("retime"))
        .unwrap()
        .pose
        .unwrap()
}

#[test]
fn retime_follow_creep_fast_output_clock_keeps_fractional_target_points_and_child_clips() {
    for point in [false, true] {
        for curve in [ZoomCurve::Linear, ZoomCurve::Smoothstep, ZoomCurve::Step] {
            let plan = verify(point, &fast(), FramingClock::OwnerOutput, 0, 5, curve);
            // Original3.4 gives target.675 -> child.35 -> group.425.
            // Using decoded Original3 instead would give.375.
            assert_ne!(owner_pose(&plan, 1).center_x, grid(3, 8));
            // The first child crop removes -.05. A later zoom-out would
            // place it at.225, but the authored fallback must remain.
            assert_eq!(owner_pose(&plan, 0).center_x, r(1, 8));
            assert_ne!(owner_pose(&plan, 0).center_x, grid(9, 40));
        }
    }
}

#[test]
fn retime_follow_creep_slow_retained_clock_crosses_exact_freeze_and_source_boundaries() {
    for point in [false, true] {
        let plan = verify(
            point,
            &slow(),
            FramingClock::RetainedOutput {
                offset: r(2, 1),
                duration: r(18, 1),
            },
            2,
            18,
            ZoomCurve::Linear,
        );
        // Mapped input4 and6 land on exact half-open child seams.
        assert_eq!(
            plan.picture(ProjectFrame(OWNER_START + 4))
                .unwrap()
                .instance
                .node,
            id("freeze")
        );
        assert_eq!(
            plan.picture(ProjectFrame(OWNER_START + 7))
                .unwrap()
                .instance
                .node,
            id("b")
        );
        // The Freeze provider is fixed while the owner clock advances.
        assert_ne!(owner_pose(&plan, 4).scale, owner_pose(&plan, 6).scale);
        // First target416667millionths was clipped before the zoom-out.
        assert_ne!(owner_pose(&plan, 0).center_x, grid(166667, 1000000));
    }
}

#[test]
fn retime_follow_creep_retained_endpoints_hold_without_freezing_provider_target_time() {
    for point in [false, true] {
        let plan = verify(
            point,
            &slow(),
            FramingClock::RetainedOutput {
                offset: r(-2, 1),
                duration: r(4, 1),
            },
            -2,
            4,
            ZoomCurve::Smoothstep,
        );
        assert_eq!(owner_pose(&plan, 0).scale, r(3, 4));
        assert_eq!(owner_pose(&plan, 1).scale, r(3, 2));
        assert_eq!(owner_pose(&plan, 6).scale, r(5, 2));
        assert_eq!(owner_pose(&plan, 10).scale, r(5, 4));
    }
}
