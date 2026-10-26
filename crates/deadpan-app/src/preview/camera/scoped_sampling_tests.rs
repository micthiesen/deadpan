//! Root Edit samples retain their exact provider and occurrence geometry.
use super::*;
use deadpan_core::framing_zoom::{Center, Shot, WholeFollowOwner, ZoomCurve, ZoomKind};
use deadpan_core::*;
use std::collections::BTreeMap;

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn q(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn frames(n: i64) -> FrameDuration {
    FrameDuration::new(n).unwrap()
}
fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}
fn stamp(ticks: i64) -> SourceTimestamp {
    SourceTimestamp {
        ticks,
        time_base: SourceTimeBase::new(1, 30).unwrap(),
    }
}
fn pose(x: ExactRatio, scale: ExactRatio) -> FramingPose {
    FramingPose::new(x, q(1, 2), scale).unwrap()
}
fn projected(x: ExactRatio) -> [ExactRatio; 2] {
    let pose = pose(x, ExactRatio::ONE).quantized().unwrap();
    [pose.center_x, pose.center_y]
}
fn creep() -> ZoomKind {
    ZoomKind::Creep {
        from: None,
        to: q(2, 1),
        curve: ZoomCurve::Linear,
    }
}
fn smash() -> ZoomKind {
    ZoomKind::Smash { scale: q(2, 1) }
}

/// A three-frame prefix, followed by two eight-frame owners. Optional enclosing
/// retiming maps sixteen frames to eleven, making the second owner's root
/// allocation 8.5..14 rather than an integral range.
fn fixture(retimed: bool, captured: bool, inner: FramingPose) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("scoped-sampling").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut provider = BeatNode::hold(
        "Provider",
        HoldRecipe {
            duration: frames(8),
            video: HoldVideo::Freeze {
                asset: AssetId::new("video").unwrap(),
                timestamp: stamp(2),
            },
            picture_context: captured.then(|| {
                CapturedFraming::new(vec![CapturedCanvas {
                    width: 16,
                    height: 16,
                    fit: CapturedFit::Fit,
                    layers: vec![Some(pose(q(1, 2), q(2, 1)))],
                }])
                .unwrap()
            }),
            audio: HoldAudio::Silence,
        },
    );
    if !captured {
        provider.kind = NodeKind::Source {
            source: SourceNode {
                duration: frames(8),
                video: SourceVideo::Stream {
                    asset: AssetId::new("video").unwrap(),
                    span: SourceSpan::new(stamp(0), stamp(8)).unwrap(),
                },
                video_mapping: SourceVideoMapping::FitBeat,
                audio: None,
                audio_mapping: SourceAudioMapping::FitBeat,
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
                edit_window: None,
            },
        };
    }
    provider.framing = Some(Framing::static_pose(inner).unwrap());
    let mut owner = BeatNode::sequence("Owner", vec![id("provider")]);
    owner.framing = Some(Framing::static_pose(pose(q(1, 4), q(3, 1))).unwrap());
    let mut repeat = BeatNode::sequence("Repeat", Vec::new());
    repeat.kind = NodeKind::Repeat {
        child: id("owner"),
        iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 2).unwrap(),
        gap: None,
        escalation: None,
    };
    let mut retime = BeatNode::sequence("Retime", Vec::new());
    retime.kind = NodeKind::Retime {
        child: id("repeat"),
        duration: frames(11),
        mapping: range(0, 16),
        pitch: PitchPolicy::FollowSpeed,
        purpose: RetimePurpose::Edit,
    };
    let prefix = BeatNode::hold(
        "Prefix",
        HoldRecipe {
            duration: frames(3),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        },
    );
    let mut root = BeatNode::sequence(
        "Root",
        vec![id("prefix"), id(if retimed { "retime" } else { "repeat" })],
    );
    root.framing = Some(Framing::static_pose(pose(q(3, 4), q(2, 1))).unwrap());
    let mut nodes = BTreeMap::from([
        (id("root"), root),
        (id("prefix"), prefix),
        (id("repeat"), repeat),
        (id("owner"), owner),
        (id("provider"), provider),
    ]);
    if retimed {
        nodes.insert(id("retime"), retime);
    }
    let target = AttentionTarget {
        label: "Moving subject".into(),
        asset: AssetId::new("video").unwrap(),
        span: SourceSpan::new(stamp(0), stamp(8)).unwrap(),
        region: TargetPoint {
            center: [300_000, 500_000],
        }
        .into(),
        samples: vec![
            TargetSample {
                at: 0,
                region: TargetPoint {
                    center: [300_000, 500_000],
                }
                .into(),
                confidence: 1000,
                state: TrackState::Tracked,
            },
            TargetSample {
                at: 7,
                region: TargetPoint {
                    center: [650_000, 500_000],
                }
                .into(),
                confidence: 1000,
                state: TrackState::Tracked,
            },
        ],
        corrections: Vec::new(),
        provenance: None,
        display_aspect: Some(ExactRatio::ONE),
    };
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        AssetId::new("video").unwrap(),
        AssetRecord {
            label: "Original".into(),
            content_hash: "a".repeat(64),
            source_qualification: None,
            video: Some(SourceSpan::new(stamp(0), stamp(8)).unwrap()),
            audio: None,
            still_image: false,
            frame_count: Some(frames(8)),
        },
    )]))
    .unwrap();
    wire["targets"] = serde_json::to_value(BTreeMap::from([(
        TargetId::new("subject").unwrap(),
        target,
    )]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn scope(ordinal: u32) -> InstancePath {
    InstancePath {
        node: id("owner"),
        repeats: vec![RepeatInstance {
            node: id("repeat"),
            iteration: IterationId {
                allocation: RevisionId::new("plays").unwrap(),
                ordinal,
            },
        }],
    }
}
fn sample(
    document: &ProjectDocument,
    plan: &deadpan_plan::RenderPlan,
    scope: &InstancePath,
    frame: ProjectFrame,
) -> Result<[ExactRatio; 2], String> {
    target_center_at_root(
        document,
        plan,
        scope,
        &TargetId::new("subject").unwrap(),
        frame,
        None,
    )
}

fn assert_provider_point(plan: &deadpan_plan::RenderPlan, frame: ProjectFrame, ticks: ExactRatio) {
    let picture = plan.picture(frame).unwrap();
    let asset = AssetId::new("video").unwrap();
    assert_eq!(
        picture.picture.follow_point(),
        Some((
            &asset,
            SourcePoint {
                ticks,
                time_base: stamp(0).time_base,
            },
        )),
        "root frame {} must retain its actual provider point",
        frame.0
    );
}

#[test]
fn smash_samples_root_in_and_creep_out_minus_one_independently_of_cursor() {
    let document = fixture(false, false, pose(q(1, 2), q(2, 1)));
    let plan = deadpan_plan::RenderPlan::compile(&document).unwrap();
    let selected = range(12, 17);
    let at_in = scoped_sample_frame(smash(), selected).unwrap();
    let at_out = scoped_sample_frame(creep(), selected).unwrap();
    assert_eq!(at_in, ProjectFrame(12));
    assert_eq!(at_out, ProjectFrame(16));
    // Root pictures sample at their centers. The second play starts at 11,
    // so frames 12, 16 and cursor 14 use source ticks 1.5, 5.5 and 3.5.
    assert_provider_point(&plan, at_in, q(3, 2));
    assert_provider_point(&plan, at_out, q(11, 2));
    assert_provider_point(&plan, ProjectFrame(14), q(7, 2));
    let in_center = sample(&document, &plan, &scope(1), at_in).unwrap();
    let out_center = sample(&document, &plan, &scope(1), at_out).unwrap();
    let cursor_center = sample(&document, &plan, &scope(1), ProjectFrame(14)).unwrap();
    // Inner 2x framing applies; owner and root framing do not.
    assert_eq!(in_center, projected(q(1, 4)));
    assert_eq!(out_center, projected(q(13, 20)));
    assert_eq!(cursor_center, projected(q(9, 20)));
    assert_ne!(in_center, cursor_center);
    assert_ne!(out_center, cursor_center);
    assert_eq!(scoped_sample_frame(ZoomKind::Off, selected), None);
    assert_eq!(
        scoped_sample_frame(creep(), range(12, 13)),
        Some(ProjectFrame(12))
    );
}

#[test]
fn retimed_owner_uses_exact_root_range_and_actual_fractional_provider() {
    let document = fixture(true, false, FramingPose::identity());
    let plan = deadpan_plan::RenderPlan::compile(&document).unwrap();
    let selected = range(9, 13);
    let mapping = plan
        .picture_owner_map(&scope(1), ProjectFrame(11), Default::default())
        .unwrap();
    assert_eq!(mapping.root_allocation, q(17, 2)..q(14, 1));
    assert_eq!(
        mapping.normalized_range(selected).unwrap(),
        (q(1, 11), q(9, 11))
    );
    let at_in = scoped_sample_frame(smash(), selected).unwrap();
    let at_out = scoped_sample_frame(creep(), selected).unwrap();
    // At each root picture center the source ticks are
    // (frame + 1/2 - prefix 3) * 16/11 - first play 8.
    assert_provider_point(&plan, at_in, q(16, 11));
    assert_provider_point(&plan, at_out, q(64, 11));
    assert_provider_point(&plan, ProjectFrame(11), q(48, 11));
    // Target x is 300000 + 50000 * source ticks. Interpolation rounds once
    // to target units before framing quantizes Q32.
    assert_eq!(
        sample(&document, &plan, &scope(1), at_in).unwrap(),
        projected(q(372_727, 1_000_000))
    );
    assert_eq!(
        sample(&document, &plan, &scope(1), at_out).unwrap(),
        projected(q(590_909, 1_000_000))
    );
    assert_eq!(
        sample(&document, &plan, &scope(1), ProjectFrame(11)).unwrap(),
        projected(q(518_182, 1_000_000))
    );
    assert!(
        mapping.normalized_range(range(8, 13)).is_err(),
        "a boundary before the fractional owner start cannot be rounded in"
    );
    assert!(
        sample(&document, &plan, &scope(0), at_in)
            .unwrap_err()
            .contains("not shown"),
        "the same node in another play grants no sample authority"
    );
}

#[test]
fn captured_provider_and_live_inner_geometry_both_project_the_fixed_sample() {
    let document = fixture(false, true, pose(q(1, 2), q(2, 1)));
    let plan = deadpan_plan::RenderPlan::compile(&document).unwrap();
    let selected = range(12, 17);
    for kind in [smash(), creep()] {
        let frame = scoped_sample_frame(kind, selected).unwrap();
        assert_provider_point(&plan, frame, q(2, 1));
        assert_eq!(
            sample(&document, &plan, &scope(1), frame).unwrap(),
            projected(q(1, 10)),
            "captured 2x and live inner 2x project source x=0.4 to x=0.1"
        );
    }
}

#[test]
fn clipped_missing_and_uncovered_saved_targets_refuse_without_a_fallback() {
    let document = fixture(false, false, pose(q(4, 5), q(4, 1)));
    let plan = deadpan_plan::RenderPlan::compile(&document).unwrap();
    assert!(
        sample(&document, &plan, &scope(1), ProjectFrame(12))
            .unwrap_err()
            .contains("not visible at frame 12")
    );
    assert!(
        target_center_at_root(
            &document,
            &plan,
            &scope(1),
            &TargetId::new("missing").unwrap(),
            ProjectFrame(12),
            None,
        )
        .unwrap_err()
        .contains("There is no saved target missing")
    );
    let document = fixture(false, true, FramingPose::identity());
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["targets"]["subject"]["span"] =
        serde_json::to_value(SourceSpan::new(stamp(3), stamp(8)).unwrap()).unwrap();
    wire["targets"]["subject"]["samples"] = serde_json::json!([]);
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = deadpan_plan::RenderPlan::compile(&document).unwrap();
    assert!(
        sample(&document, &plan, &scope(1), ProjectFrame(12))
            .unwrap_err()
            .contains("not visible"),
        "a captured provider at source tick 2 is outside this target's span"
    );
}

#[test]
fn full_owner_normalized_sample_is_fixed_and_never_promoted_to_follow() {
    let document = fixture(false, false, FramingPose::identity());
    let plan = deadpan_plan::RenderPlan::compile(&document).unwrap();
    let selected = range(11, 19);
    let window = plan
        .picture_owner_map(&scope(1), ProjectFrame(14), Default::default())
        .unwrap()
        .normalized_range(selected)
        .unwrap();
    assert_eq!(window, (ExactRatio::ZERO, ExactRatio::ONE));
    for kind in [smash(), creep()] {
        let frame = scoped_sample_frame(kind, selected).unwrap();
        let point = sample(&document, &plan, &scope(1), frame).unwrap();
        let shot = Shot {
            whole_owner: Some(WholeFollowOwner::Sequence),
            entry: None,
            current: FramingPose::identity(),
            frames: 8,
            range: None,
            explicit: true,
        };
        let framing = deadpan_core::framing_zoom::build_normalized_range(
            kind,
            &Center::Point(point),
            &shot,
            window,
        )
        .unwrap()
        .unwrap();
        assert!(!matches!(framing.value, FramingValue::Follow { .. }));
        let arrival = framing
            .evaluate_with_follow_exact(q(8, 1), q(8, 1))
            .unwrap();
        assert_eq!([arrival.pose.center_x, arrival.pose.center_y], point);
        assert!(arrival.follow.is_none());
    }
}

#[test]
fn sampled_range_preserves_outside_path_refusals_and_keep_follow_stays_live() {
    let point = Center::Point(projected(q(1, 3)));
    let follow = Framing {
        clock: FramingClock::OwnerOutput,
        value: FramingValue::Follow {
            target: TargetId::new("subject").unwrap(),
            scale: q(3, 2),
            fallback: FramingPose::identity(),
            centering: Default::default(),
            scale_transition: None,
        },
    };
    let path = Framing::creep(
        FramingPose::identity(),
        pose(q(1, 3), q(2, 1)),
        FramingCurve::Linear,
    )
    .unwrap();
    let window = (q(1, 11), q(9, 11));
    for (entry, refusal) in [
        (&follow, framing_zoom::RANGED_FOLLOW_REFUSAL),
        (&path, framing_zoom::RANGED_PATH_REFUSAL),
    ] {
        let shot = Shot {
            whole_owner: Some(WholeFollowOwner::Sequence),
            entry: Some(entry),
            current: FramingPose::identity(),
            frames: 8,
            range: None,
            explicit: true,
        };
        for kind in [smash(), creep()] {
            assert_eq!(
                framing_zoom::build_normalized_range(kind, &point, &shot, window),
                Err(refusal.into())
            );
        }
    }
    let shot = Shot {
        whole_owner: Some(WholeFollowOwner::Sequence),
        entry: Some(&follow),
        current: FramingPose::identity(),
        frames: 8,
        range: None,
        explicit: false,
    };
    let kept = framing_zoom::build_normalized_range(creep(), &Center::Keep, &shot, window)
        .unwrap()
        .unwrap();
    let FramingValue::Follow {
        target,
        scale_transition: Some(transition),
        ..
    } = kept.value
    else {
        panic!("Keep must retain the live target");
    };
    assert_eq!(target, TargetId::new("subject").unwrap());
    let selected = transition.range.unwrap();
    assert_eq!((selected.start, selected.end), window);
}

#[test]
fn sampled_creep_quantizes_ramp_but_holds_exact_authored_arrival_after_out() {
    let point = projected(q(1, 3));
    let shot = Shot {
        whole_owner: Some(WholeFollowOwner::Sequence),
        entry: None,
        current: FramingPose::identity(),
        frames: 36,
        range: None,
        explicit: true,
    };
    let framing = framing_zoom::build_normalized_range(
        ZoomKind::Creep {
            from: Some(q(3, 2)),
            to: q(11, 5),
            curve: ZoomCurve::Linear,
        },
        &Center::Point(point),
        &shot,
        (q(1, 6), q(2, 3)),
    )
    .unwrap()
    .unwrap();
    let evaluate = |local| {
        framing
            .evaluate_with_follow_exact(local, q(36, 1))
            .unwrap()
            .pose
    };
    assert_eq!(evaluate(q(5, 1)), FramingPose::identity());
    assert_eq!(evaluate(q(6, 1)), pose(q(1, 2), q(3, 2)));

    // Frame 15 lies halfway along the 6..24 ramp. Interpolation uses Q32,
    // including the center already projected from the sampled target.
    let midpoint_x = q(1, 2)
        .checked_add(point[0])
        .unwrap()
        .checked_div(q(2, 1))
        .unwrap();
    let midpoint = pose(midpoint_x, q(37, 20)).quantized().unwrap();
    assert_eq!(evaluate(q(15, 1)), midpoint);
    assert_ne!(midpoint.scale, q(37, 20));

    // Out is an authored segment boundary, and the suffix is a Step hold.
    // Both retain the exact 11/5 author intent rather than the ramp's Q32
    // terminal approximation. The target's projected center stays Q32.
    let arrival = pose(point[0], q(11, 5));
    let quantized_arrival = arrival.quantized().unwrap();
    assert_ne!(arrival.scale, quantized_arrival.scale);
    for local in [q(24, 1), q(49, 2), q(35, 1), q(36, 1)] {
        let actual = evaluate(local);
        assert_eq!(actual, arrival, "arrival and held suffix at {local:?}");
        assert_eq!([actual.center_x, actual.center_y], point);
        assert_ne!(actual.scale, quantized_arrival.scale);
    }
}
