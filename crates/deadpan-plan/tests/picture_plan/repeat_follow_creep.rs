//! Provider clocks restart per play; a Repeat Follow's output clock does not.

use super::*;
use deadpan_core::framing_zoom::ZoomCurve;
use deadpan_plan::{AudioContent, AudioQueryLimits};

const Q: i128 = 4_294_967_296;
const REPEAT_START: i64 = 2;
const REPEAT_FRAMES: i64 = 17;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Escalation {
    None,
    Centered,
    Targeted,
}

fn r(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}

/// Independent small-integer round-to-even oracle; no production framing or
/// target projection/evaluation helper participates in expected values.
fn even_div(n: i128, d: i128) -> i128 {
    let (whole, rest) = (n / d, n % d);
    whole + i128::from(2 * rest > d || (2 * rest == d && whole % 2 != 0))
}

fn scale(frame: i64, fallback: bool, curve: ZoomCurve) -> ExactRatio {
    let t = even_div((2 * i128::from(frame) + 1) * Q, 34);
    let weight = match curve {
        ZoomCurve::Linear => t,
        ZoomCurve::Smoothstep => even_div(t * t * (3 * Q - 2 * t), Q * Q),
        ZoomCurve::Step => 0,
    };
    // Live 3/2 -> 5/2, fallback 3/4 -> 5/4, on the Q32 contract grid.
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
    let tracked = |at, x, state| TargetSample {
        at,
        region: geometry(point, x),
        confidence: if state == TrackState::Lost { 0 } else { 950 },
        state,
    };
    AttentionTarget {
        label: "Repeated motion".into(),
        asset: asset_id("video"),
        span: span(0, 8008),
        region: geometry(point, 0),
        samples: vec![
            tracked(0, 0, TrackState::Tracked),
            tracked(4004, 500_000, TrackState::Tracked),
            tracked(5005, 500_000, TrackState::Lost),
        ],
        corrections: vec![TargetCorrection {
            at: 7007,
            region: geometry(point, 875_000),
        }],
        provenance: None,
        display_aspect: Some(r(16, 9)),
    }
}

fn retained() -> CapturedFraming {
    CapturedFraming::new(vec![CapturedCanvas {
        width: 1920,
        height: 1080,
        fit: CapturedFit::Fit,
        layers: vec![Some(FramingPose::new(r(1, 2), r(1, 2), r(1, 2)).unwrap())],
    }])
    .unwrap()
}

fn freeze(frames: i64, ticks: i64, captured: bool) -> HoldRecipe {
    HoldRecipe {
        duration: duration(frames),
        video: HoldVideo::Freeze {
            asset: asset_id("video"),
            timestamp: SourceTimestamp {
                ticks,
                time_base: clock(),
            },
        },
        picture_context: captured.then(retained),
        audio: HoldAudio::Silence,
    }
}

fn fixture(
    point: bool,
    background_gap: bool,
    escalation: Escalation,
    curve: ZoomCurve,
) -> (ProjectDocument, ProjectDocument) {
    let mut child = source(4, 0, 8008);
    let NodeKind::Source { source } = &mut child.kind else {
        unreachable!()
    };
    source.audio = Some(SourceAudio {
        asset: asset_id("video"),
        span: span(0, 8008),
    });
    // Source target x -> x-1/8, so play starts at the canvas-left edge.
    child.framing =
        Some(Framing::static_pose(FramingPose::new(r(5, 8), r(1, 2), r(1, 1)).unwrap()).unwrap());
    let repeated = node(NodeKind::Repeat {
        child: id("child"),
        iterations: IterationOrder::new(revision("creep-plays"), 3).unwrap(),
        gap: Some(if background_gap {
            background(2)
        } else {
            // Source6006 is Lost; its retained x=1/2 still resolves live.
            freeze(2, 6006, false)
        }),
        escalation: None,
    });
    let original = document(
        &["before", "repeat", "after"],
        vec![
            ("before", hold(2)),
            ("repeat", repeated),
            ("child", child),
            ("after", hold(2)),
        ],
    );
    let mut wire = serde_json::to_value(original).unwrap();
    let mut owned = node(NodeKind::Hold {
        recipe: freeze(3, 1001, true),
    });
    owned.framing =
        Some(Framing::static_pose(FramingPose::new(r(1, 2), r(1, 2), r(1, 2)).unwrap()).unwrap());
    wire["nodes"]["owned-gap"] = serde_json::to_value(owned).unwrap();
    wire["nodes"]["dormant-final-gap"] = serde_json::to_value(hold(9)).unwrap();
    wire["gap_overrides"] = serde_json::to_value(BTreeMap::from([(
        id("repeat"),
        PlayOverrides::try_from(vec![
            PlayOverride {
                iteration: IterationId {
                    allocation: revision("creep-plays"),
                    ordinal: 1,
                },
                root: id("owned-gap"),
            },
            PlayOverride {
                iteration: IterationId {
                    allocation: revision("creep-plays"),
                    ordinal: 2,
                },
                root: id("dormant-final-gap"),
            },
        ])
        .unwrap(),
    )]))
    .unwrap();
    wire["targets"] = serde_json::to_value(BTreeMap::from([(
        TargetId::new("subject").unwrap(),
        target(point),
    )]))
    .unwrap();
    if escalation != Escalation::None {
        wire["nodes"]["repeat"]["kind"]["escalation"] = serde_json::to_value(RepeatEscalation {
            gain_step: GainDb::new(1000).unwrap(),
            zoom: Some(ZoomStep {
                step: r(1, 1),
                progression: ZoomProgression::Add,
                target: (escalation == Escalation::Targeted)
                    .then(|| TargetId::new("subject").unwrap()),
            }),
        })
        .unwrap();
    }
    let follow = Framing {
        clock: FramingClock::OwnerOutput,
        value: FramingValue::Follow {
            centering: Default::default(),
            target: TargetId::new("subject").unwrap(),
            scale: r(3, 2),
            fallback: FramingPose::new(r(1, 8), r(7, 8), r(3, 4)).unwrap(),
            scale_transition: None,
        },
    };
    wire["nodes"]["repeat"]["framing"] = serde_json::to_value(follow).unwrap();
    let fixed = ProjectDocument::from_json(&wire.to_string()).unwrap();
    wire["nodes"]["repeat"]["framing"]["value"]["scale_transition"] =
        serde_json::to_value(FollowScaleTransition {
            factor: r(5, 3),
            curve,
            range: None,
        })
        .unwrap();
    let animated = ProjectDocument::from_json(&wire.to_string()).unwrap();
    (fixed, animated)
}

/// (owning play, leaf frame if a Source, gap kind). Default gap0 occupies4..6,
/// owned gap1 occupies10..13; the 9-frame last gap is dormant.
fn location(local: i64) -> (u32, Option<usize>, bool) {
    match local {
        0..=3 => (0, Some(usize::try_from(local).unwrap()), false),
        4..=5 => (0, None, false),
        6..=9 => (1, Some(usize::try_from(local - 6).unwrap()), false),
        10..=12 => (1, None, true),
        13..=16 => (2, Some(usize::try_from(local - 13).unwrap()), false),
        _ => panic!("outside Repeat"),
    }
}

fn verify(point: bool, background_gap: bool, escalation: Escalation, curve: ZoomCurve) {
    let (fixed, animated) = fixture(point, background_gap, escalation, curve);
    let before = RenderPlan::compile(&fixed).unwrap();
    let after = RenderPlan::compile(&animated).unwrap();
    assert_eq!(before.duration(), duration(21));
    assert_eq!(after.duration(), before.duration());
    assert_eq!(fixed.gap_overrides(), animated.gap_overrides());
    assert_eq!(fixed.overrides(), animated.overrides());
    for name in [
        "root",
        "before",
        "child",
        "owned-gap",
        "dormant-final-gap",
        "after",
    ] {
        assert_eq!(fixed.nodes()[&id(name)], animated.nodes()[&id(name)]);
    }
    let mut prior_owner = fixed.nodes()[&id("repeat")].clone();
    prior_owner.framing = animated.nodes()[&id("repeat")].framing.clone();
    assert_eq!(prior_owner, animated.nodes()[&id("repeat")]);

    for frame in 0..21 {
        let prior = before.picture(ProjectFrame(frame)).unwrap();
        let sample = after.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(sample.picture, prior.picture);
        assert_eq!(sample.picture_context, prior.picture_context);
        assert_eq!(sample.instance, prior.instance);
        assert_eq!(sample.gap_after, prior.gap_after);
        assert_eq!(sample.local_position, prior.local_position);
        let outer =
            |v: &&deadpan_plan::PictureFraming| v.instance.node == id("repeat") && !v.escalation;
        assert_eq!(
            sample
                .framing
                .iter()
                .filter(|v| !outer(v))
                .collect::<Vec<_>>(),
            prior
                .framing
                .iter()
                .filter(|v| !outer(v))
                .collect::<Vec<_>>()
        );
        if !(REPEAT_START..REPEAT_START + REPEAT_FRAMES).contains(&frame) {
            assert_eq!(sample.framing, prior.framing, "half-open neighbor{frame}");
            assert!(
                sample
                    .framing
                    .iter()
                    .all(|v| v.instance.node != id("repeat"))
            );
            continue;
        }
        let local = frame - REPEAT_START;
        let (play, child_frame, owned_gap) = location(local);
        let default_gap = child_frame.is_none() && !owned_gap;
        let mut fallback = default_gap && background_gap;
        let base_units = if let Some(child_frame) = child_frame {
            let expected_ticks = [1001, 3003, 5005, 7007][child_frame];
            let (asset, source) = sample.picture.follow_point().unwrap();
            assert_eq!(asset, &asset_id("video"));
            assert_eq!(source.ticks, ExactRatio::integer(expected_ticks));
            assert_eq!(source.time_base, clock());
            assert_eq!(sample.instance.node, id("child"));
            assert_eq!(
                sample.instance.repeats.last().unwrap().iteration.ordinal,
                play
            );
            assert!(sample.gap_after.is_none());
            assert!(sample.picture_context.is_none());
            // Tracked interpolation then Lost continuation then correction,
            // carried through the child's shift by -1/8.
            [0, Q / 4, 3 * Q / 8, 3 * Q / 4][child_frame]
        } else if owned_gap {
            assert_eq!(sample.instance.node, id("owned-gap"));
            assert_eq!(sample.instance.repeats.last().unwrap().iteration.ordinal, 1);
            assert_eq!(sample.picture_context.as_deref(), Some(&retained()));
            assert_eq!(ticks(&sample.picture), r(1001, 1));
            // Source x=1/8, retained zoom1/2 ->5/16, live gap zoom1/2 ->13/32.
            13 * Q / 32
        } else {
            assert_eq!(sample.instance.node, id("repeat"));
            assert_eq!(sample.gap_after.as_ref().unwrap().ordinal, 0);
            assert!(sample.picture_context.is_none());
            if background_gap {
                assert_eq!(sample.picture, Picture::Background);
            } else {
                assert_eq!(ticks(&sample.picture), r(6006, 1));
            }
            Q / 2
        };
        let owner = sample.framing.iter().find(|v| outer(v)).unwrap();
        assert_eq!(owner.local_position, r(2 * i128::from(local) + 1, 2));
        assert_eq!(owner.effective_duration, r(17, 1));
        let escalated = sample.framing.iter().find(|v| v.escalation);
        let mut outer_units = base_units;
        if escalation != Escalation::None && play > 0 {
            let zoom = i128::from(play) + 1;
            let center = if escalation == Escalation::Centered {
                Q / 2
            } else if base_units * (2 * zoom) < Q {
                even_div(Q, 2 * zoom)
            } else if base_units * (2 * zoom) > (2 * zoom - 1) * Q {
                even_div((2 * zoom - 1) * Q, 2 * zoom)
            } else {
                base_units
            };
            let layer = escalated.unwrap();
            assert_eq!(layer.instance, owner.instance);
            assert_eq!(
                layer.pose.unwrap(),
                FramingPose::new(r(center, Q), r(1, 2), r(zoom, 1)).unwrap()
            );
            let outer_index = sample.framing.iter().position(|v| outer(&v)).unwrap();
            let escalation_index = sample.framing.iter().position(|v| v.escalation).unwrap();
            assert!(
                escalation_index < outer_index,
                "escalation remains inside authored Follow"
            );
            outer_units = (base_units - center) * zoom + Q / 2;
        } else {
            assert!(escalated.is_none());
        }
        // Each live scope clips before the next scope sees the picture.
        // Inclusive edge points remain visible; exterior centers do not.
        fallback |= !(0..=Q).contains(&outer_units);
        let pose = owner.pose.unwrap();
        assert_eq!(
            pose.center_x,
            if fallback { r(1, 8) } else { r(outer_units, Q) },
            "point={point} background={background_gap} escalation={escalation:?} curve={curve:?} local={local}"
        );
        assert_eq!(pose.center_y, if fallback { r(7, 8) } else { r(1, 2) });
        assert_eq!(pose.scale, scale(local, fallback, curve));
        if escalation == Escalation::Targeted && local == 13 {
            // At scale3, the left clamp1/6 rounds UP to715827883/Q.
            // Projecting source-left0 through that stored center gives -1/Q,
            // which is clipped, rather than an unquantized exact-clamp zero.
            assert_eq!(escalated.unwrap().pose.unwrap().center_x, r(715_827_883, Q));
            assert_eq!(outer_units, -1);
            assert_eq!(pose.center_x, r(1, 8));
        }
    }
    assert!(matches!(
        after.picture(ProjectFrame(21)),
        Err(PlanError::FrameOutOfRange { .. })
    ));
    let end = before.audio_duration().unwrap();
    assert_eq!(end, after.audio_duration().unwrap());
    let a = before
        .audio(AudioSample(0)..end, AudioQueryLimits::default())
        .unwrap();
    let b = after
        .audio(AudioSample(0)..end, AudioQueryLimits::default())
        .unwrap();
    assert!(
        a.spans
            .iter()
            .any(|v| matches!(v.content, AudioContent::Source { .. }))
    );
    assert_eq!(a.spans, b.spans);
}

#[test]
fn repeat_follow_creep_keeps_one_clock_across_three_plays_default_and_owned_gaps() {
    for point in [false, true] {
        for background_gap in [false, true] {
            for curve in [ZoomCurve::Linear, ZoomCurve::Smoothstep, ZoomCurve::Step] {
                verify(point, background_gap, Escalation::None, curve);
            }
        }
    }
}

#[test]
fn repeat_follow_creep_composes_centered_and_target_clamped_escalation_in_play_order() {
    for point in [false, true] {
        for background_gap in [false, true] {
            for curve in [ZoomCurve::Linear, ZoomCurve::Smoothstep, ZoomCurve::Step] {
                for escalation in [Escalation::Centered, Escalation::Targeted] {
                    verify(point, background_gap, escalation, curve);
                }
            }
        }
    }
}

#[test]
fn repeat_follow_creep_cannot_restore_a_child_clipped_target_by_later_escalation_zoom_out() {
    for point in [false, true] {
        let (fixed, animated) = fixture(point, true, Escalation::None, ZoomCurve::Linear);
        let change = |document: ProjectDocument| {
            let mut wire = serde_json::to_value(document).unwrap();
            wire["nodes"]["child"]["framing"] = serde_json::to_value(
                Framing::static_pose(FramingPose::new(r(1, 2), r(1, 2), r(4, 1)).unwrap()).unwrap(),
            )
            .unwrap();
            wire["nodes"]["repeat"]["kind"]["escalation"] =
                serde_json::to_value(RepeatEscalation {
                    gain_step: GainDb::UNITY,
                    zoom: Some(ZoomStep {
                        step: r(1, 2),
                        progression: ZoomProgression::Multiply,
                        target: None,
                    }),
                })
                .unwrap();
            ProjectDocument::from_json(&wire.to_string()).unwrap()
        };
        let before = RenderPlan::compile(&change(fixed)).unwrap();
        let after = RenderPlan::compile(&change(animated)).unwrap();
        for local in 0..REPEAT_FRAMES {
            let at = ProjectFrame(REPEAT_START + local);
            let prior = before.picture(at).unwrap();
            let sample = after.picture(at).unwrap();
            assert_eq!(sample.picture, prior.picture);
            assert_eq!(sample.picture_context, prior.picture_context);
            let (play, child_frame, owned_gap) = location(local);
            let owner = sample
                .framing
                .iter()
                .find(|v| v.instance.node == id("repeat") && !v.escalation)
                .unwrap();
            let fallback = child_frame.is_none() && !owned_gap
                || child_frame.is_some_and(|index| index == 0 || index == 3);
            let x = if fallback {
                r(1, 8)
            } else if owned_gap {
                //13/32 through play1's centered zoom1/2 ->29/64.
                r(29, 64)
            } else {
                // Child zoom4 maps source [1/8,3/8,1/2,7/8] to[-1,0,1/2,2].
                // Only the middle two survive that clip.
                let child_x = if child_frame == Some(1) { 0 } else { Q / 2 };
                let divisor = 1i128 << play;
                r(Q / 2 + (child_x - Q / 2) / divisor, Q)
            };
            let pose = owner.pose.unwrap();
            assert_eq!(pose.center_x, x, "point={point}, local={local}");
            assert_eq!(pose.center_y, if fallback { r(7, 8) } else { r(1, 2) });
            assert_eq!(pose.scale, scale(local, fallback, ZoomCurve::Linear));
            if play > 0 {
                assert_eq!(
                    sample
                        .framing
                        .iter()
                        .find(|v| v.escalation)
                        .unwrap()
                        .pose
                        .unwrap()
                        .scale,
                    r(1, 1i128 << play),
                );
            }
            if local == 13 || local == 16 {
                // At play2, later zoom1/4 would place the discarded -1 or2
                // at1/8 or7/8. The earlier child clip is irreversible.
                assert!(fallback);
                assert_eq!(pose.center_x, r(1, 8));
            }
        }
    }
}
