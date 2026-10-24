from pathlib import Path
path = Path("/Users/michael/Code/deadpan/crates/deadpan-plan/tests/picture_plan/sequence_follow_creep.rs")
assert not path.exists()
path.write_text('''//! A Sequence Follow resolves each child provider but keeps one output clock.

use super::*;
use deadpan_core::framing_zoom::ZoomCurve;
use deadpan_plan::{AudioContent, AudioQueryLimits};

const Q32: i128 = 4_294_967_296;
const GROUP_FRAMES: i64 = 15;
const GROUP_START: i64 = 2;

fn r(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}

/// Small positive integer quotient, ties to even. This oracle does not call
/// Framing evaluation, pose quantization, or exact-ratio rounding.
fn even_quotient(n: i128, d: i128) -> i128 {
    let whole = n / d;
    match (2 * (n % d)).cmp(&d) {
        std::cmp::Ordering::Greater => whole + 1,
        std::cmp::Ordering::Equal if whole % 2 != 0 => whole + 1,
        _ => whole,
    }
}

fn expected_scale(frame: i64, fallback: bool, curve: ZoomCurve) -> ExactRatio {
    // Sample centers are (2f+1)/30 of the WHOLE group, regardless of the
    // selected source interval, child duration, tracked state or target cover.
    let t = even_quotient((2 * i128::from(frame) + 1) * Q32, 30);
    let weight = match curve {
        ZoomCurve::Linear => t,
        ZoomCurve::Smoothstep => even_quotient(t * t * (3 * Q32 - 2 * t), Q32 * Q32),
        ZoomCurve::Step => 0,
    };
    // Live 3/2 -> 5/2; fallback 3/4 -> 5/4. All endpoints are exact Q32
    // integers, so only the half-unit fallback delta needs another rounding.
    let units = if fallback {
        3 * Q32 / 4 + even_quotient(weight, 2)
    } else {
        3 * Q32 / 2 + weight
    };
    r(units, Q32)
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
        label: "Across the cut".into(),
        asset: asset_id("video"),
        // b's last sampled point is EXACTLY this excluded endpoint.
        span: span(0, 33033),
        region: geometry(point, 250_000),
        samples: vec![
            sample(0, 250_000, TrackState::Tracked),
            sample(4004, 750_000, TrackState::Tracked),
            sample(5005, 750_000, TrackState::Lost),
            sample(20020, 625_000, TrackState::Tracked),
            sample(24024, 375_000, TrackState::Tracked),
            sample(25025, 375_000, TrackState::Lost),
        ],
        corrections: vec![
            TargetCorrection { at: 7007, region: geometry(point, 375_000) },
            TargetCorrection { at: 30030, region: geometry(point, 875_000) },
        ],
        provenance: None,
        display_aspect: Some(r(16, 9)),
    }
}

fn child(frames: i64, start: i64, end: i64, other_asset: bool, center: ExactRatio) -> BeatNode {
    let mut child = source(frames, start, end);
    let NodeKind::Source { source } = &mut child.kind else { unreachable!() };
    if other_asset {
        let SourceVideo::Stream { asset, .. } = &mut source.video else { unreachable!() };
        *asset = asset_id("other");
    }
    source.audio = Some(SourceAudio { asset: asset_id("video"), span: span(start, end) });
    child.framing = Some(Framing::static_pose(
        FramingPose::new(center, r(1, 2), r(1, 2)).unwrap()
    ).unwrap());
    child
}

/// Two real Original ranges separated by a cut, then a different asset whose
/// points would otherwise be inside the target. Neighbors lie outside owner g.
fn fixture(point: bool, curve: ZoomCurve) -> (ProjectDocument, ProjectDocument) {
    let group = BeatNode::sequence("Group", vec![id("a"), id("b"), id("c")]);
    let initial = document(
        &["before", "g", "after"],
        vec![
            ("before", hold(2)),
            ("g", group),
            ("a", child(5, 0, 10010, false, r(1, 2))),
            ("b", child(7, 20020, 34034, false, r(1, 4))),
            ("c", child(3, 0, 6006, true, r(1, 2))),
            ("after", hold(2)),
        ],
    );
    // The generic document helper validates before its catalog can be extended.
    // c is switched to the other asset only after that extension, below.
    let mut wire = serde_json::to_value(initial).unwrap();
    wire["assets"]["other"] = wire["assets"]["video"].clone();
    wire["targets"] = serde_json::to_value(BTreeMap::from([(
        TargetId::new("subject").unwrap(), target(point)
    )])).unwrap();
    let follow = Framing {
        clock: FramingClock::OwnerOutput,
        value: FramingValue::Follow {
            target: TargetId::new("subject").unwrap(),
            scale: r(3, 2),
            fallback: FramingPose::new(r(1, 8), r(7, 8), r(3, 4)).unwrap(),
            scale_transition: None,
        },
    };
    wire["nodes"]["g"]["framing"] = serde_json::to_value(&follow).unwrap();
    let fixed = ProjectDocument::from_json(&wire.to_string()).unwrap();
    wire["nodes"]["g"]["framing"]["value"]["scale_transition"] =
        serde_json::to_value(FollowScaleTransition { factor: r(5, 3), curve }).unwrap();
    let animated = ProjectDocument::from_json(&wire.to_string()).unwrap();
    (fixed, animated)
}

#[test]
fn sequence_follow_creep_resolves_every_provider_sample_and_preserves_child_audio_and_framing() {
    for point in [false, true] {
        let (fixed, animated) = fixture(point, ZoomCurve::Linear);
        let old = RenderPlan::compile(&fixed).unwrap();
        let new = RenderPlan::compile(&animated).unwrap();
        assert_eq!(new.duration(), duration(19));
        assert_eq!(new.duration(), old.duration());
        for name in ["root", "before", "a", "b", "c", "after"] {
            assert_eq!(fixed.nodes()[&id(name)], animated.nodes()[&id(name)]);
        }
        let mut group_before = fixed.nodes()[&id("g")].clone();
        group_before.framing = animated.nodes()[&id("g")].framing.clone();
        assert_eq!(group_before, animated.nodes()[&id("g")]);

        // Static hand fractions, after each child's independent transform:
        // a: x -> x/2+1/4; b: x -> x/2+3/8.
        let centers = [
            r(7,16), r(9,16), r(5,8), r(7,16), r(7,16),
            r(21,32), r(19,32), r(9,16), r(9,16), r(9,16), r(13,16),
            r(1,8), r(1,8), r(1,8), r(1,8),
        ];
        let original_ticks = [
            1001, 3003, 5005, 7007, 9009,
            21021, 23023, 25025, 27027, 29029, 31031, 33033,
            1001, 3003, 5005,
        ];
        for frame in 0..19 {
            let prior = old.picture(ProjectFrame(frame)).unwrap();
            let sample = new.picture(ProjectFrame(frame)).unwrap();
            assert_eq!(sample.picture, prior.picture);
            assert_eq!(sample.picture_context, prior.picture_context);
            assert_eq!(
                sample.framing.iter().filter(|v| v.instance.node != id("g")).collect::<Vec<_>>(),
                prior.framing.iter().filter(|v| v.instance.node != id("g")).collect::<Vec<_>>()
            );
            if !(GROUP_START..GROUP_START + GROUP_FRAMES).contains(&frame) {
                assert_eq!(sample.framing, prior.framing, "half-open neighbor {frame}");
                assert!(sample.framing.iter().all(|v| v.instance.node != id("g")));
                continue;
            }
            let local = frame - GROUP_START;
            let index = usize::try_from(local).unwrap();
            let (asset, source) = sample.picture.follow_point().unwrap();
            assert_eq!(asset, &asset_id(if local < 12 { "video" } else { "other" }));
            assert_eq!(source.ticks, ExactRatio::integer(original_ticks[index]));
            assert_eq!(source.time_base, clock());
            let owner = sample.framing.iter().find(|v| v.instance.node == id("g")).unwrap();
            assert_eq!(owner.local_position, r(2 * i128::from(local) + 1, 2));
            assert_eq!(owner.effective_duration, r(15, 1));
            let pose = owner.pose.unwrap();
            assert_eq!(pose.center_x, centers[index], "point={point}, local={local}");
            assert_eq!(pose.center_y, if local < 11 { r(1,2) } else { r(7,8) });
            assert_eq!(pose.scale, expected_scale(local, local >= 11, ZoomCurve::Linear));
        }
        // Explicit first/last/seam observations make wrong provider ownership
        // and a transition restarting on b independently visible.
        let at = |local| new.picture(ProjectFrame(GROUP_START + local)).unwrap()
            .framing.into_iter().find(|v| v.instance.node == id("g")).unwrap().pose.unwrap();
        assert_eq!(at(0).center_x, r(7,16));
        assert_eq!(at(4).center_x, r(7,16));
        assert_eq!(at(5).center_x, r(21,32));
        assert_eq!(at(10).center_x, r(13,16));
        assert_eq!(at(11).center_x, r(1,8), "target end is excluded");
        assert_eq!(at(12).center_x, r(1,8), "other asset is excluded");
        assert_eq!(at(14).scale, expected_scale(14, true, ZoomCurve::Linear));

        let audio_end = old.audio_duration().unwrap();
        assert_eq!(audio_end, new.audio_duration().unwrap());
        let before_audio = old.audio(AudioSample(0)..audio_end, AudioQueryLimits::default()).unwrap();
        let after_audio = new.audio(AudioSample(0)..audio_end, AudioQueryLimits::default()).unwrap();
        assert!(before_audio.spans.iter().any(|v| matches!(v.content, AudioContent::Source { .. })));
        assert_eq!(before_audio.spans, after_audio.spans);
    }
}

#[test]
fn sequence_follow_creep_curves_share_the_complete_sequence_clock_including_fallback() {
    for curve in [ZoomCurve::Linear, ZoomCurve::Smoothstep, ZoomCurve::Step] {
        let (_, document) = fixture(true, curve);
        let plan = RenderPlan::compile(&document).unwrap();
        for local in 0..GROUP_FRAMES {
            let sample = plan.picture(ProjectFrame(GROUP_START + local)).unwrap();
            let owner = sample.framing.iter().find(|v| v.instance.node == id("g")).unwrap();
            assert_eq!(owner.pose.unwrap().scale, expected_scale(local, local >= 11, curve),
                "curve={curve:?}, local={local}");
        }
    }
}
''')
