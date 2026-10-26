//! Camera drafts must use the same target projection as a committed plan.
use super::*;
use deadpan_core::*;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn q(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn span() -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 30).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 8,
            time_base,
        },
    )
    .unwrap()
}
fn follow(centering: FollowCentering) -> Framing {
    Framing {
        clock: FramingClock::OwnerOutput,
        value: FramingValue::Follow {
            target: TargetId::new("subject").unwrap(),
            scale: q(3, 2),
            fallback: FramingPose::new(q(2, 5), q(1, 2), q(3, 2)).unwrap(),
            centering,
            scale_transition: None,
        },
    }
}
fn fixture(x: u32) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("camera-targeted-escalation").unwrap(),
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
    let mut source = BeatNode::sequence("Source", Vec::new());
    source.kind = NodeKind::Source {
        source: SourceNode {
            duration: frames(8),
            video: SourceVideo::Stream {
                asset: AssetId::new("video").unwrap(),
                span: span(),
            },
            video_mapping: SourceVideoMapping::FitBeat,
            audio: None,
            audio_mapping: SourceAudioMapping::FitBeat,
            audio_offset: AudioSample(0),
            link: LinkRelation::Independent,
            edit_window: None,
        },
    };
    let repeat = |name: &str, child: &str, plays| {
        let mut node = BeatNode::sequence(name, Vec::new());
        node.kind = NodeKind::Repeat {
            child: id(child),
            iterations: IterationOrder::new(RevisionId::new(name).unwrap(), plays).unwrap(),
            gap: None,
            escalation: Some(RepeatEscalation {
                gain_step: GainDb::UNITY,
                zoom: Some(ZoomStep {
                    step: ExactRatio::ONE,
                    progression: ZoomProgression::Add,
                    target: Some(TargetId::new("subject").unwrap()),
                }),
            }),
        };
        node.framing = Some(follow(FollowCentering::Unconstrained));
        node
    };
    let mut retime = BeatNode::sequence("Retime", Vec::new());
    retime.kind = NodeKind::Retime {
        child: id("outer"),
        duration: frames(7),
        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(48)).unwrap(),
        pitch: PitchPolicy::FollowSpeed,
        purpose: RetimePurpose::Edit,
    };
    retime.framing = Some(follow(FollowCentering::CanvasClamped));
    let nodes = std::collections::BTreeMap::from([
        (id("root"), BeatNode::sequence("Root", vec![id("retime")])),
        (id("retime"), retime),
        (id("outer"), repeat("outer", "inner", 2)),
        (id("inner"), repeat("inner", "source", 3)),
        (id("source"), source),
    ]);
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"] = serde_json::to_value(std::collections::BTreeMap::from([(
        AssetId::new("video").unwrap(),
        AssetRecord {
            label: "Original".into(),
            content_hash: "a".repeat(64),
            source_qualification: None,
            video: Some(span()),
            audio: None,
            still_image: false,
            frame_count: Some(frames(8)),
        },
    )]))
    .unwrap();
    wire["targets"] = serde_json::to_value(std::collections::BTreeMap::from([(
        TargetId::new("subject").unwrap(),
        AttentionTarget {
            label: "Subject".into(),
            asset: AssetId::new("video").unwrap(),
            span: span(),
            region: TargetPoint {
                center: [x, TARGET_UNITS / 2],
            }
            .into(),
            samples: Vec::new(),
            corrections: Vec::new(),
            provenance: None,
            display_aspect: Some(ExactRatio::ONE),
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn with_framing(document: &ProjectDocument, framing: &Framing) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"]["source"]["framing"] = serde_json::to_value(framing).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn picture(sample: &deadpan_plan::PictureSample) -> crate::worker::Picture {
    crate::worker::Picture {
        summary: None,
        id: SourceFrameId(0),
        frame: None,
        canvas: Some((16, 16)),
        framing: sample.framing.clone(),
        framing_gap: sample.gap_after.is_some(),
        picture_context: sample.picture_context.clone(),
        follow_point: sample
            .picture
            .follow_point()
            .map(|(asset, point)| (asset.clone(), point)),
        captions: Vec::new(),
        tier: crate::worker::PictureTier::Original,
    }
}

#[test]
fn child_camera_reprojects_targeted_escalations_and_follow_in_plan_order() {
    let mut saw_fraction = false;
    let mut changed_escalations = 0;
    let mut saw_hidden_fallback = false;
    let mut saw_edge_clamp = false;
    // Interior and canvas-edge subjects, plus a crop hiding either one.
    for x in [400_000, 20_000] {
        let document = fixture(x);
        let plan = deadpan_plan::RenderPlan::compile(&document).unwrap();
        for hidden in [false, true] {
            let pose = if hidden {
                FramingPose::new(q(4, 5), q(1, 2), q(2, 1)).unwrap()
            } else {
                FramingPose::new(q(3, 10), q(1, 2), ExactRatio::ONE).unwrap()
            };
            let framing = Framing::static_pose(pose).unwrap();
            let committed = with_framing(&document, &framing);
            let committed_plan = deadpan_plan::RenderPlan::compile(&committed).unwrap();
            for frame in 0..7 {
                let before = plan.picture(ProjectFrame(frame)).unwrap();
                let expected = committed_plan.picture(ProjectFrame(frame)).unwrap();
                let picture = picture(&before);
                let local = picture.framing[0].local_position;
                saw_fraction |= local.denominator() != 1;
                let updates = layer_poses(&document, &picture, 0, Some(&framing), local).unwrap();
                let mut actual = picture.framing.clone();
                for (instance, escalation, pose) in updates {
                    actual
                        .iter_mut()
                        .find(|layer| layer.instance == instance && layer.escalation == escalation)
                        .unwrap()
                        .pose = pose;
                }
                assert_eq!(
                    actual, expected.framing,
                    "x={x}, hidden={hidden}, frame={frame}"
                );
                for (old, new) in before.framing.iter().zip(&actual) {
                    if new.escalation {
                        changed_escalations += usize::from(old.pose != new.pose);
                        let pose = new.pose.unwrap();
                        saw_hidden_fallback |= hidden && pose.center_x == q(1, 2);
                        saw_edge_clamp |= !hidden
                            && x == 20_000
                            && pose.center_x
                                == q(1, 1)
                                    .checked_div(pose.scale.checked_mul(q(2, 1)).unwrap())
                                    .unwrap();
                    }
                }
            }
        }
    }
    assert!(saw_fraction, "exercise exact fractional owner coordinates");
    assert!(
        changed_escalations > 0,
        "the old escalation skip must fail this regression"
    );
    assert!(
        saw_hidden_fallback,
        "hidden subjects retain the centered escalation fallback"
    );
    assert!(
        saw_edge_clamp,
        "edge subjects keep the view inside the escalation canvas"
    );
}

#[test]
fn clamped_follow_camera_adjustments_and_replacement_preserve_policy() {
    let entry = follow(FollowCentering::CanvasClamped);
    let FramingValue::Follow {
        target,
        scale,
        fallback,
        centering,
        scale_transition,
    } = &entry.value
    else {
        unreachable!()
    };
    let draft = FollowDraft {
        target: target.clone(),
        center: [q(1, 3), q(1, 2)],
        base_scale: *scale,
        base_live: *scale,
        base_fallback: *fallback,
        centering: *centering,
        clock: entry.clock,
        scale_transition: *scale_transition,
    };
    let handle = FramingPose::new(draft.center[0], draft.center[1], *scale).unwrap();
    assert_eq!(followed_framing(&draft, handle).unwrap(), entry);
    let adjusted = followed_framing(
        &draft,
        FramingPose {
            scale: q(2, 1),
            ..handle
        },
    )
    .unwrap();
    assert!(matches!(
        adjusted.value,
        FramingValue::Follow {
            centering: FollowCentering::CanvasClamped,
            ..
        }
    ));
    let mut replacement = draft.clone();
    replacement.target = TargetId::new("replacement").unwrap();
    assert!(
        matches!(followed_framing(&replacement, handle).unwrap().value,
        FramingValue::Follow { centering: FollowCentering::CanvasClamped, target, .. } if target.as_str() == "replacement")
    );
    assert!(matches!(
        adjusted_framing(Some(&entry), handle, handle, false)
            .unwrap()
            .unwrap()
            .value,
        FramingValue::Static { .. }
    ));
}
