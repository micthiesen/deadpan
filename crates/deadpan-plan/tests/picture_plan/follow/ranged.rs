//! Ranged Follow keeps provider motion outside its independent scale window.
use super::*;
use deadpan_core::framing_zoom::{Center, Shot, WholeFollowOwner, ZoomCurve, ZoomKind};

const Q: i128 = 1 << 32;

// Positive integer round-to-even, independent of framing's numeric helpers.
fn even(n: i128, d: i128) -> i128 {
    let whole = n / d;
    whole + i128::from(2 * (n % d) > d || (2 * (n % d) == d && whole % 2 != 0))
}

fn expected_scale(
    frame: i64,
    start: i64,
    end: i64,
    fallback: bool,
    curve: ZoomCurve,
) -> ExactRatio {
    let (base, from, to) = if fallback {
        (Q, 3 * Q / 2, 2 * Q)
    } else {
        (2 * Q, 3 * Q, 4 * Q)
    };
    let units = if frame < start {
        base
    } else if frame >= end {
        to
    } else {
        let t = even(
            (2 * i128::from(frame - start) + 1) * Q,
            2 * i128::from(end - start),
        );
        let t = match curve {
            ZoomCurve::Linear => t,
            ZoomCurve::Smoothstep => even(t * t * (3 * Q - 2 * t), Q * Q),
            ZoomCurve::Step => unreachable!("authoring admits only eased creep"),
        };
        even(from * (Q - t) + to * t, Q)
    };
    ratio(units, Q)
}

fn authored(
    before: &ProjectDocument,
    owner: &str,
    kind: WholeFollowOwner,
    start: u64,
    end: u64,
    curve: ZoomCurve,
) -> ProjectDocument {
    let entry = before.nodes()[&id(owner)].framing.as_ref().unwrap();
    let framing = framing_zoom::build(
        ZoomKind::Creep {
            from: Some(ratio(3, 1)),
            to: ratio(4, 1),
            curve,
        },
        &Center::Keep,
        &Shot {
            whole_owner: Some(kind),
            entry: Some(entry),
            current: FramingPose::identity(),
            frames: 10,
            range: Some((start, end)),
            explicit: false,
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(framing.clock, FramingClock::OwnerOutput);
    let mut wire = serde_json::to_value(before).unwrap();
    wire["nodes"][owner]["framing"] = serde_json::to_value(framing).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn fixture(curve: ZoomCurve) -> (ProjectDocument, ProjectDocument) {
    let before = with_target(
        document(&["a"], vec![("a", source(10, 0, 10010))]),
        8008,
        &[("a", 2)],
    );
    // A fixed Follow is independent of its old normalization clock. Authoring
    // the new window must normalize against the current ten-frame owner.
    let mut wire = serde_json::to_value(before).unwrap();
    wire["nodes"]["a"]["framing"]["clock"] = serde_json::to_value(FramingClock::RetainedOutput {
        offset: ratio(-4, 1),
        duration: ratio(6, 1),
    })
    .unwrap();
    wire["assets"]["video"]["source_qualification"] =
        serde_json::to_value(SourceQualificationId::new("b".repeat(64)).unwrap()).unwrap();
    wire["assets"]["video"]["video"] = serde_json::to_value(span(0, 10010)).unwrap();
    wire["assets"]["video"]["frame_count"] = serde_json::to_value(duration(10)).unwrap();
    wire["nodes"]["a"]["kind"]["source"]["edit_window"] =
        serde_json::to_value(SourceEditWindow::new(ExactRatio::ZERO, ratio(10, 1)).unwrap())
            .unwrap();
    wire["nodes"]["a"]["kind"]["source"]["video_mapping"] =
        serde_json::to_value(SourceVideoMapping::SelectedPlacement {
            start: ExactRatio::ZERO,
            frames: ratio(10, 1),
            selection: ExactFrameRange::new(ExactRatio::ZERO, ratio(10, 1)).unwrap(),
            endpoints: EndpointPolicy::HoldAdjacent,
        })
        .unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let after = authored(&before, "a", WholeFollowOwner::Source, 2, 6, curve);
    (before, after)
}

#[test]
fn ranged_follow_keeps_moving_centers_before_and_after_its_exact_scale_window() {
    for curve in [ZoomCurve::Linear, ZoomCurve::Smoothstep] {
        let (before, after) = fixture(curve);
        let old = RenderPlan::compile(&before).unwrap();
        let plan = RenderPlan::compile(&after).unwrap();
        assert_eq!(old.duration(), plan.duration());
        for frame in 0..10 {
            let old_picture = old.picture(ProjectFrame(frame)).unwrap();
            let picture = plan.picture(ProjectFrame(frame)).unwrap();
            assert_eq!(picture.picture, old_picture.picture);
            assert_eq!(picture.picture_context, old_picture.picture_context);
            let actual = pose(&plan, frame, 0);
            assert_eq!(actual.center_x, pose(&old, frame, 0).center_x);
            assert_eq!(actual.center_y, pose(&old, frame, 0).center_y);
            assert_eq!(
                actual.scale,
                expected_scale(frame, 2, 6, frame >= 8, curve),
                "frame {frame}, {curve:?}"
            );
        }
        assert_eq!(pose(&plan, 0, 0).center_x, grid(1, 5));
        assert_eq!(pose(&plan, 7, 0).center_x, grid(4, 5));
        assert_eq!(pose(&plan, 8, 0).center_x, ratio(1, 2));
    }
}

#[test]
fn ranged_follow_sequence_uses_group_window_and_each_childs_original_picture() {
    let before = with_target(
        document(
            &["g"],
            vec![
                ("g", BeatNode::sequence("Group", vec![id("a"), id("b")])),
                ("a", source(4, 0, 4004)),
                ("b", source(6, 4004, 10010)),
            ],
        ),
        8008,
        &[("g", 2)],
    );
    let after = authored(
        &before,
        "g",
        WholeFollowOwner::Sequence,
        3,
        7,
        ZoomCurve::Linear,
    );
    let old = RenderPlan::compile(&before).unwrap();
    let plan = RenderPlan::compile(&after).unwrap();
    for frame in 0..10 {
        let picture = plan.picture(ProjectFrame(frame)).unwrap();
        let old_picture = old.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(picture.picture, old_picture.picture);
        let actual = picture
            .framing
            .iter()
            .find(|layer| layer.instance.node == id("g"))
            .unwrap()
            .pose
            .unwrap();
        let prior = old_picture
            .framing
            .iter()
            .find(|layer| layer.instance.node == id("g"))
            .unwrap()
            .pose
            .unwrap();
        assert_eq!(
            [actual.center_x, actual.center_y],
            [prior.center_x, prior.center_y]
        );
        assert_eq!(
            actual.scale,
            expected_scale(frame, 3, 7, frame >= 8, ZoomCurve::Linear)
        );
    }
    for child in ["a", "b"] {
        assert_eq!(before.nodes()[&id(child)], after.nodes()[&id(child)]);
    }
}

fn voiced_source(frames: i64, end: i64) -> BeatNode {
    let mut child = source(frames, 0, end);
    let NodeKind::Source { source } = &mut child.kind else {
        unreachable!()
    };
    source.audio = Some(SourceAudio {
        asset: asset_id("video"),
        span: span(0, end),
    });
    child
}

fn assert_temporal_owner(kind: WholeFollowOwner) {
    for curve in [ZoomCurve::Linear, ZoomCurve::Smoothstep] {
        let (owner, children, target_end) = match kind {
            WholeFollowOwner::Hold => (
                node(NodeKind::Hold {
                    recipe: HoldRecipe {
                        duration: duration(10),
                        video: HoldVideo::Freeze {
                            asset: asset_id("video"),
                            timestamp: SourceTimestamp {
                                ticks: 6006,
                                time_base: clock(),
                            },
                        },
                        picture_context: None,
                        audio: HoldAudio::Silence,
                    },
                }),
                vec![],
                8008,
            ),
            WholeFollowOwner::Repeat => (
                repeat("child", 2, 2, "ranged-repeat-plays"),
                vec![("child", voiced_source(4, 8008))],
                8008,
            ),
            WholeFollowOwner::Retime => (
                retime("child", 10, 0, 7),
                vec![("child", voiced_source(7, 7007))],
                6006,
            ),
            _ => unreachable!("temporal-owner cases"),
        };
        let before = with_target(
            document(
                &["owner"],
                std::iter::once(("owner", owner)).chain(children).collect(),
            ),
            target_end,
            &[("owner", 2)],
        );
        let after = authored(&before, "owner", kind, 2, 6, curve);
        let mut only_framing = serde_json::to_value(&before).unwrap();
        only_framing["nodes"]["owner"]["framing"] =
            serde_json::to_value(&after.nodes()[&id("owner")].framing).unwrap();
        assert_eq!(serde_json::to_value(&after).unwrap(), only_framing);
        let old = RenderPlan::compile(&before).unwrap();
        let new = RenderPlan::compile(&after).unwrap();
        assert_eq!(new.duration(), duration(10));
        assert_eq!(new.duration(), old.duration());
        assert_eq!(new.audio_duration().unwrap(), old.audio_duration().unwrap());
        let audio_end = old.audio_duration().unwrap();
        let old_audio = old
            .audio(
                AudioSample(0)..audio_end,
                deadpan_plan::AudioQueryLimits::default(),
            )
            .unwrap();
        let new_audio = new
            .audio(
                AudioSample(0)..audio_end,
                deadpan_plan::AudioQueryLimits::default(),
            )
            .unwrap();
        assert_eq!(new_audio.spans, old_audio.spans);
        if kind != WholeFollowOwner::Hold {
            assert!(
                old_audio
                    .spans
                    .iter()
                    .any(|span| matches!(span.content, deadpan_plan::AudioContent::Source { .. }))
            );
        }
        for frame in 0..10 {
            let actual = new.picture(ProjectFrame(frame)).unwrap();
            let prior = old.picture(ProjectFrame(frame)).unwrap();
            assert_eq!(actual.picture, prior.picture);
            assert_eq!(actual.picture_context, prior.picture_context);
            let expected_ticks = match kind {
                WholeFollowOwner::Hold => Some(ratio(6006, 1)),
                WholeFollowOwner::Repeat if (4..6).contains(&frame) => None,
                WholeFollowOwner::Repeat => {
                    let local = if frame < 4 { frame } else { frame - 6 };
                    Some(ratio(1001 * i128::from(2 * local + 1), 1))
                }
                WholeFollowOwner::Retime => Some(ratio(7007 * i128::from(2 * frame + 1), 20)),
                _ => unreachable!(),
            };
            assert_eq!(
                actual.picture.follow_point().map(|(asset, point)| {
                    assert_eq!(asset, &asset_id("video"));
                    assert_eq!(point.time_base, clock());
                    point.ticks
                }),
                expected_ticks,
                "provider {kind:?}, {frame}"
            );
            let fallback = expected_ticks
                .is_none_or(|ticks| ticks.compare(ratio(i128::from(target_end), 1)).is_ge());
            let expected_x = if fallback {
                ratio(1, 2)
            } else if expected_ticks.unwrap().compare(ratio(5005, 1)).is_lt() {
                grid(1, 5)
            } else {
                grid(4, 5)
            };
            let layer = actual
                .framing
                .iter()
                .find(|layer| layer.instance.node == id("owner"))
                .unwrap();
            assert_eq!(layer.local_position, ratio(i128::from(2 * frame + 1), 2));
            assert_eq!(layer.effective_duration, ratio(10, 1));
            let pose = layer.pose.unwrap();
            assert_eq!(pose.center_x, expected_x, "center {kind:?}, {frame}");
            assert_eq!(
                pose.scale,
                expected_scale(frame, 2, 6, fallback, curve),
                "scale {kind:?}, {frame}, {curve:?}"
            );
        }
    }
}

#[test]
fn ranged_follow_hold_uses_one_frozen_original_point_and_owner_scale_window() {
    assert_temporal_owner(WholeFollowOwner::Hold);
}

#[test]
fn ranged_follow_repeat_keeps_gap_fallback_and_live_provider_reset_after_the_window() {
    assert_temporal_owner(WholeFollowOwner::Repeat);
}

#[test]
fn ranged_follow_retime_keeps_fractional_original_points_and_owner_scale_window() {
    assert_temporal_owner(WholeFollowOwner::Retime);
}

fn changed(before: &ProjectDocument, name: &str, command: Command) -> ProjectDocument {
    let tx = apply(
        before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision(name),
            command,
        },
    )
    .unwrap();
    let after = tx.forward.apply(before).unwrap();
    assert_eq!(tx.inverse.apply(&after).unwrap(), *before);
    ProjectDocument::from_json(&after.to_json().unwrap()).unwrap()
}

fn same_picture(old: &RenderPlan, old_frame: i64, new: &RenderPlan, new_frame: i64) {
    let a = old.picture(ProjectFrame(old_frame)).unwrap();
    let b = new.picture(ProjectFrame(new_frame)).unwrap();
    assert_eq!(a.picture.follow_point(), b.picture.follow_point());
    assert_eq!(a.picture_context, b.picture_context);
    assert_eq!(
        a.framing
            .iter()
            .filter_map(|layer| layer.pose)
            .collect::<Vec<_>>(),
        b.framing
            .iter()
            .filter_map(|layer| layer.pose)
            .collect::<Vec<_>>()
    );
}

#[test]
fn ranged_follow_split_and_ripple_trim_preserve_the_authored_window_on_surviving_frames() {
    let (_, before) = fixture(ZoomCurve::Smoothstep);
    let old = RenderPlan::compile(&before).unwrap();
    let divided = changed(
        &before,
        "split-ranged-follow",
        Command::Split {
            node: id("a"),
            at: duration(4),
            identities: SplitIdentities {
                nodes: (0..16).map(|n| id(&format!("ranged-split-{n}"))).collect(),
            },
        },
    );
    let split = RenderPlan::compile(&divided).unwrap();
    for frame in 0..10 {
        same_picture(&old, frame, &split, frame);
    }

    let resolved = before
        .source_trim(
            &id("root"),
            &id("a"),
            SourceTrimEdge::In,
            2,
            SourceTrimMode::Ripple,
        )
        .unwrap();
    assert_eq!(resolved.applied_delta_frames, 2);
    let trimmed = changed(
        &before,
        "trim-ranged-follow",
        Command::TrimSource {
            parent: id("root"),
            node: id("a"),
            edge: SourceTrimEdge::In,
            delta_frames: 2,
            mode: SourceTrimMode::Ripple,
            wrapper: resolved.needs_wrapper.then(|| id("ranged-trim-crop")),
            timing: AudioTimingId {
                allocation: revision("trim-ranged-follow"),
                ordinal: 0,
            },
        },
    );
    let trimmed = RenderPlan::compile(&trimmed).unwrap();
    assert_eq!(trimmed.duration(), duration(8));
    for frame in 0..8 {
        same_picture(&old, frame + 2, &trimmed, frame);
    }
}
