//! Explode retains source-time target evaluation and the escalation clamp.
use super::*;

#[derive(Clone, Copy, Debug)]
enum TargetCase {
    Moving,
    HiddenByChild,
    OtherAsset,
    OutsideSpan,
}

fn r(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}

fn pose(x: i128, scale: i128) -> FramingPose {
    FramingPose::new(r(x, 8), r(1, 2), r(scale, 4)).unwrap()
}

fn target(case: TargetCase) -> AttentionTarget {
    let region = |x| {
        TargetPoint {
            center: [x, 500_000],
        }
        .into()
    };
    let sample = |at, x, state| TargetSample {
        at,
        region: region(x),
        confidence: if state == TrackState::Lost { 0 } else { 950 },
        state,
    };
    let outside = matches!(case, TargetCase::OutsideSpan);
    AttentionTarget {
        label: "Moving subject".into(),
        asset: asset_id(if matches!(case, TargetCase::OtherAsset) {
            "other"
        } else {
            "video"
        }),
        span: span(0, if outside { 500 } else { 16016 }),
        region: region(0),
        samples: if outside {
            Vec::new()
        } else {
            vec![
                sample(0, 0, TrackState::Tracked),
                sample(1001, 0, TrackState::Tracked),
                sample(2002, 900_000, TrackState::Tracked),
                sample(3003, 900_000, TrackState::Lost),
                sample(9009, 750_000, TrackState::Tracked),
                sample(14014, 250_000, TrackState::Tracked),
            ]
        },
        corrections: if outside {
            Vec::new()
        } else {
            vec![TargetCorrection {
                at: 4504,
                region: region(50_000),
            }]
        },
        provenance: None,
        display_aspect: Some(r(16, 9)),
    }
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
        picture_context: captured.then(|| {
            CapturedFraming::new(vec![CapturedCanvas {
                width: 1920,
                height: 1080,
                fit: CapturedFit::Fit,
                layers: vec![Some(pose(4, 2))],
            }])
            .unwrap()
        }),
        audio: HoldAudio::Silence,
    }
}

fn fixture(
    progression: ZoomProgression,
    step: ExactRatio,
    case: TargetCase,
    background_gap: bool,
    nested_and_retimed: bool,
) -> ProjectDocument {
    let mut leaf = source(6, 0, 6006);
    leaf.framing = Some(
        Framing::static_pose(pose(
            5,
            if matches!(case, TargetCase::HiddenByChild) {
                16
            } else {
                4
            },
        ))
        .unwrap(),
    );
    leaf.captions.push(Caption {
        range: range(1, 6),
        text: "From the second play".into(),
        placement: CaptionPlacement::Top,
        reveal: std::num::NonZeroU32::new(2),
    });
    let mut owned_gap = node(NodeKind::Hold {
        recipe: freeze(3, 10510, true),
    });
    owned_gap.framing = Some(Framing::static_pose(pose(4, 2)).unwrap());
    owned_gap.captions.push(Caption {
        range: range(0, 3),
        text: "Owned gap".into(),
        placement: CaptionPlacement::Bottom,
        reveal: None,
    });
    let mut repeated = repeat(
        if nested_and_retimed { "inner" } else { "leaf" },
        3,
        0,
        "outer-plays",
    );
    if let NodeKind::Repeat { gap, .. } = &mut repeated.kind {
        *gap = Some(if background_gap {
            background(2)
        } else {
            freeze(2, 5505, false)
        });
    }
    let escalation = RepeatEscalation {
        gain_step: GainDb::new(1000).unwrap(),
        zoom: Some(ZoomStep {
            progression,
            step,
            target: Some(TargetId::new("subject").unwrap()),
        }),
    };
    let framing = Framing {
        clock: FramingClock::OwnerOutput,
        value: FramingValue::Follow {
            target: TargetId::new("subject").unwrap(),
            scale: r(3, 4),
            fallback: pose(3, 3),
            scale_transition: None,
            centering: FollowCentering::Unconstrained,
        },
    };
    let mut nodes = vec![
        ("before", hold(2)),
        ("r", repeated),
        ("leaf", leaf),
        ("after", hold(2)),
    ];
    if nested_and_retimed {
        nodes.push(("inner", repeat("leaf", 2, 0, "inner-plays")));
    }
    let mut wire = serde_json::to_value(document(&["before", "r", "after"], nodes)).unwrap();
    wire["nodes"]["r"]["kind"]["escalation"] = serde_json::to_value(escalation).unwrap();
    wire["nodes"]["r"]["framing"] = serde_json::to_value(framing).unwrap();
    wire["nodes"]["owned-gap"] = serde_json::to_value(owned_gap).unwrap();
    wire["nodes"]["dormant-gap"] = serde_json::to_value(hold(9)).unwrap();
    wire["nodes"]["root"]["framing"] =
        serde_json::to_value(Framing::static_pose(pose(4, 3)).unwrap()).unwrap();
    wire["assets"]["other"] = wire["assets"]["video"].clone();
    wire["targets"] = serde_json::to_value(BTreeMap::from([(
        TargetId::new("subject").unwrap(),
        target(case),
    )]))
    .unwrap();
    let play = |allocation, ordinal| IterationId {
        allocation: revision(allocation),
        ordinal,
    };
    wire["gap_overrides"] = serde_json::to_value(BTreeMap::from([(
        id("r"),
        PlayOverrides::try_from(vec![
            PlayOverride {
                iteration: play("outer-plays", 1),
                root: id("owned-gap"),
            },
            PlayOverride {
                iteration: play("outer-plays", 2),
                root: id("dormant-gap"),
            },
        ])
        .unwrap(),
    )]))
    .unwrap();
    if nested_and_retimed {
        let mut replacement = source(6, 6006, 12012);
        replacement.framing = Some(Framing::static_pose(pose(3, 4)).unwrap());
        wire["nodes"]["inner-second"] = serde_json::to_value(replacement).unwrap();
        wire["nodes"]["speed"] = serde_json::to_value(retime("r", 55, 0, 41)).unwrap();
        wire["nodes"]["root"]["kind"]["children"] =
            serde_json::to_value(vec![id("before"), id("speed"), id("after")]).unwrap();
        wire["overrides"] = serde_json::to_value(BTreeMap::from([(
            id("inner"),
            PlayOverrides::try_from(vec![PlayOverride {
                iteration: play("inner-plays", 1),
                root: id("inner-second"),
            }])
            .unwrap(),
        )]))
        .unwrap();
    }
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn changed(before: &ProjectDocument, name: &str, command: Command) -> ProjectDocument {
    let request = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: revision(name),
        command,
    };
    // Both the public command wire and the resulting authored wire retain policy.
    let request = serde_json::from_str(&serde_json::to_string(&request).unwrap()).unwrap();
    let edit = apply(before, &request).unwrap();
    let after = edit.forward.apply(before).unwrap();
    assert_eq!(edit.inverse.apply(&after).unwrap(), *before);
    ProjectDocument::from_json(&after.to_json().unwrap()).unwrap()
}

fn exploded(before: &ProjectDocument) -> ProjectDocument {
    let need = before.explode_requirements(&id("r")).unwrap();
    changed(
        before,
        "exploded",
        Command::Explode {
            node: id("r"),
            identities: OccurrenceIdentities {
                nodes: (0..need.nodes)
                    .map(|n| id(&format!("exploded-{n}")))
                    .collect(),
                marks: (0..need.marks)
                    .map(|n| MarkId::new(format!("exploded-mark-{n}")).unwrap())
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: revision("exploded"),
                ordinal: 0,
            },
        },
    )
}

fn poses(sample: &deadpan_plan::PictureSample) -> Vec<FramingPose> {
    sample
        .framing
        .iter()
        .filter_map(|layer| layer.pose)
        .collect()
}

fn same_pictures(before: &ProjectDocument, after: &ProjectDocument) {
    let old = RenderPlan::compile(before).unwrap();
    let new = RenderPlan::compile(after).unwrap();
    assert_eq!(new.duration(), old.duration());
    for frame in 0..old.duration().frames() {
        let a = old.picture(ProjectFrame(frame)).unwrap();
        let b = new.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(
            a.picture, b.picture,
            "provider and exact source coordinate at {frame}"
        );
        assert_eq!(
            a.picture_context, b.picture_context,
            "retained context at {frame}"
        );
        assert_eq!(poses(&a), poses(&b), "resolved framing at {frame}");
        assert_eq!(a.captions, b.captions, "caption reveals at {frame}");
    }
    assert!(new.picture(ProjectFrame(new.duration().frames())).is_err());
}

#[test]
fn targeted_explode_keeps_every_picture_through_motion_loss_correction_and_fallbacks() {
    for (progression, step) in [
        (ZoomProgression::Add, r(1, 1)),
        (ZoomProgression::Add, r(-1, 4)),
        (ZoomProgression::Multiply, r(3, 2)),
        (ZoomProgression::Multiply, r(1, 2)),
    ] {
        for case in [
            TargetCase::Moving,
            TargetCase::HiddenByChild,
            TargetCase::OtherAsset,
            TargetCase::OutsideSpan,
        ] {
            for background_gap in [false, true] {
                let before = fixture(progression, step, case, background_gap, false);
                let after = exploded(&before);
                same_pictures(&before, &after);
                assert_eq!(before.targets(), after.targets());
            }
        }
    }
}

#[test]
fn targeted_explode_keeps_nested_overrides_and_fractional_retime_pictures() {
    for background_gap in [false, true] {
        for (progression, step) in [
            (ZoomProgression::Add, r(1, 1)),
            (ZoomProgression::Multiply, r(1, 2)),
        ] {
            let before = fixture(progression, step, TargetCase::Moving, background_gap, true);
            same_pictures(&before, &exploded(&before));
        }
    }
}

#[test]
fn exploded_target_remains_live_and_each_play_can_be_edited_independently() {
    let before = fixture(
        ZoomProgression::Add,
        r(1, 1),
        TargetCase::Moving,
        false,
        false,
    );
    let after = exploded(&before);
    let mut correction = target(TargetCase::Moving);
    correction.corrections.push(TargetCorrection {
        at: 5005,
        region: TargetPoint {
            center: [950_000, 500_000],
        }
        .into(),
    });
    let update = |document: &ProjectDocument| {
        changed(
            document,
            "retargeted",
            Command::SetTarget {
                id: TargetId::new("subject").unwrap(),
                target: correction.clone(),
            },
        )
    };
    let live_before = update(&before);
    let live_after = update(&after);
    same_pictures(&live_before, &live_after);
    let old = RenderPlan::compile(&after).unwrap();
    let live = RenderPlan::compile(&live_after).unwrap();
    assert!(
        (0..old.duration().frames()).any(|frame| {
            poses(&old.picture(ProjectFrame(frame)).unwrap())
                != poses(&live.picture(ProjectFrame(frame)).unwrap())
        }),
        "later target correction changes materialized Follow poses"
    );

    let NodeKind::Sequence { children } = &live_after.nodes()[&id("r")].kind else {
        panic!("exploded Repeat becomes an ordinary Sequence")
    };
    let second = children
        .iter()
        .find(|child| {
            matches!(
                &live_after.nodes()[*child].framing,
                Some(Framing {
                    value: FramingValue::Follow {
                        centering: FollowCentering::CanvasClamped,
                        ..
                    },
                    ..
                })
            )
        })
        .unwrap()
        .clone();
    let edited = changed(
        &live_after,
        "one-play",
        Command::SetFraming {
            node: second,
            framing: Some(Framing::static_pose(pose(4, 4)).unwrap()),
        },
    );
    let next = RenderPlan::compile(&edited).unwrap();
    let mut second_changed = false;
    for frame in 0..live.duration().frames() {
        let a = live.picture(ProjectFrame(frame)).unwrap();
        let b = next.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(a.picture, b.picture);
        assert_eq!(a.captions, b.captions);
        // Lead 2; play0+gap0 8; play1+owned-gap1 9; play2 6; tail 2.
        if (10..19).contains(&frame) {
            second_changed |= poses(&a) != poses(&b);
        } else {
            assert_eq!(
                poses(&a),
                poses(&b),
                "independent neighboring play at {frame}"
            );
        }
    }
    assert!(second_changed);
}

#[test]
fn exploded_follow_keeps_quantized_canvas_edges_and_leaves_zoom_out_unconstrained() {
    for (progression, step) in [
        (ZoomProgression::Add, r(1, 1)),
        (ZoomProgression::Multiply, r(1, 2)),
    ] {
        let mut wire =
            serde_json::to_value(fixture(progression, step, TargetCase::Moving, false, false))
                .unwrap();
        // Isolate the escalation's pose so the expected centers below do not
        // depend on the target-projection or centering implementation.
        for owner in ["root", "r", "leaf"] {
            wire["nodes"][owner]["framing"] = serde_json::Value::Null;
        }
        let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let after = exploded(&before);
        same_pictures(&before, &after);
        let plan = RenderPlan::compile(&after).unwrap();
        assert!(poses(&plan.picture(ProjectFrame(2)).unwrap()).is_empty());
        let actual = |frame| poses(&plan.picture(ProjectFrame(frame)).unwrap())[0];
        match progression {
            ZoomProgression::Add => {
                // Frame 10 starts play1 with target x=0; 12 and 13 retain
                // x=0.9 across Lost; 14 uses the correction x=0.05.
                for (frame, center) in [(10, r(1, 4)), (12, r(3, 4)), (13, r(3, 4)), (14, r(1, 4))]
                {
                    assert_eq!(
                        actual(frame),
                        FramingPose::new(center, r(1, 2), r(2, 1)).unwrap()
                    );
                }
                // Play2's scale3 lower edge is rounded UP on Q32. Using the
                // exact 1/6 here would change clipping in its outer scopes.
                assert_eq!(
                    actual(19),
                    FramingPose::new(r(715_827_883, 4_294_967_296), r(1, 2), r(3, 1)).unwrap()
                );
            }
            ZoomProgression::Multiply => {
                // CanvasClamped intentionally permits ordinary target centers
                // below scale1; it must not create an inverted clamp interval.
                assert_eq!(
                    actual(10),
                    FramingPose::new(r(0, 1), r(1, 2), r(1, 2)).unwrap()
                );
                assert_eq!(
                    actual(19),
                    FramingPose::new(r(0, 1), r(1, 2), r(1, 4)).unwrap()
                );
            }
        }
    }
}
