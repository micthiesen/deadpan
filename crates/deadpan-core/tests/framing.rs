use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};

use deadpan_core::*;
use serde_json::json;

fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn duration(n: i64) -> FrameDuration {
    FrameDuration::new(n).unwrap()
}
fn id(s: &str) -> NodeId {
    NodeId::new(s).unwrap()
}
fn pose(scale: i64) -> FramingPose {
    FramingPose::new(ratio(1, 2), ratio(1, 2), ExactRatio::integer(scale)).unwrap()
}
fn creep(curve: FramingCurve) -> Framing {
    Framing::creep(pose(1), pose(2), curve).unwrap()
}
fn hold(frames: i64) -> BeatNode {
    BeatNode::hold(
        "Hold",
        HoldRecipe {
            duration: duration(frames),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}
fn document() -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("framing").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (id("root"), BeatNode::sequence("Root", vec![id("hold")])),
        (id("hold"), hold(8)),
    ]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn request(doc: &ProjectDocument, command: Command) -> CommandRequest {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    CommandRequest {
        project_id: doc.project_id().clone(),
        expected_revision: doc.revision_id().clone(),
        new_revision: RevisionId::new(format!("edit-{}", NEXT.fetch_add(1, Ordering::Relaxed)))
            .unwrap(),
        command,
    }
}
fn edit(doc: &ProjectDocument, command: Command) -> ProjectDocument {
    let tx = apply(doc, &request(doc, command)).unwrap();
    let after = tx.forward.apply(doc).unwrap();
    assert_eq!(tx.inverse.apply(&after).unwrap(), *doc);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    after
}
fn split(doc: &ProjectDocument, node: &NodeId, at: i64, prefix: &str) -> ProjectDocument {
    edit(
        doc,
        Command::Split {
            node: node.clone(),
            at: duration(at),
            identities: SplitIdentities {
                nodes: (0..doc.nodes().len() + 4)
                    .map(|i| id(&format!("{prefix}-{i}")))
                    .collect(),
            },
        },
    )
}

#[test]
fn all_curves_use_explicit_frame_edge_progress_and_exact_endpoints() {
    for (curve, expected) in [
        (FramingCurve::Step, ratio(1, 1)),
        (FramingCurve::Linear, ratio(5, 4)),
        (FramingCurve::Smoothstep, ratio(37, 32)),
        (
            FramingCurve::Cubic {
                control1: pose(1),
                control2: pose(1),
            },
            ratio(65, 64),
        ),
    ] {
        let framing = creep(curve);
        assert_eq!(
            framing.evaluate(ratio(1, 2), duration(2)).unwrap().scale,
            expected
        );
        assert_eq!(
            framing.evaluate(ExactRatio::ZERO, duration(2)).unwrap(),
            pose(1)
        );
        assert_eq!(
            framing
                .evaluate(ExactRatio::integer(2), duration(2))
                .unwrap(),
            pose(2)
        );
        assert!(framing.evaluate(ExactRatio::ZERO, duration(0)).is_err());
        assert!(framing.evaluate(ratio(-1, 2), duration(2)).is_err());
    }
    assert_eq!(
        creep(FramingCurve::Smoothstep)
            .evaluate(ratio(1, 2), duration(1))
            .unwrap()
            .scale,
        ratio(3, 2)
    );
}

#[test]
fn step_boundary_and_quantization_do_not_move_segment_selection() {
    let framing = Framing {
        value: FramingValue::Envelope {
            envelope: FramingEnvelope {
                initial: pose(1),
                segments: vec![
                    FramingSegment {
                        end: ratio(1, 2),
                        pose: pose(2),
                        curve: FramingCurve::Step,
                    },
                    FramingSegment {
                        end: ExactRatio::ONE,
                        pose: pose(3),
                        curve: FramingCurve::Step,
                    },
                ],
            },
        },
    };
    assert_eq!(
        framing
            .evaluate(ratio(i128::MAX - 1, i128::MAX), duration(2))
            .unwrap(),
        pose(1)
    );
    assert_eq!(
        framing.evaluate(ExactRatio::ONE, duration(2)).unwrap(),
        pose(2)
    );
    assert_eq!(
        framing
            .evaluate(ratio(i128::MAX, i128::MAX - 1), duration(2))
            .unwrap(),
        pose(2)
    );
    let p = FramingPose::new(
        ratio(-1, 2 * i128::from(FRAMING_NUMERIC_SCALE)),
        ratio(3, 2 * i128::from(FRAMING_NUMERIC_SCALE)),
        ExactRatio::ONE,
    )
    .unwrap()
    .quantized()
    .unwrap();
    assert_eq!(p.center_x, ExactRatio::ZERO);
    assert_eq!(p.center_y, ratio(2, i128::from(FRAMING_NUMERIC_SCALE)));
}

#[test]
fn wide_local_time_and_small_segment_denominators_do_not_overflow() {
    // Python Fraction oracle: nearly one frame in a two-frame host is below
    // the midpoint by <2^-126 and rounds to exactly Q32/2, not a new segment.
    for curve in [
        FramingCurve::Linear,
        FramingCurve::Smoothstep,
        FramingCurve::Cubic {
            control1: pose(1),
            control2: pose(2),
        },
    ] {
        assert_eq!(
            creep(curve)
                .evaluate(ratio(i128::MAX - 1, i128::MAX), duration(2))
                .unwrap()
                .scale,
            ratio(3, 2)
        );
    }
    let framing = Framing {
        value: FramingValue::Envelope {
            envelope: FramingEnvelope {
                initial: pose(1),
                segments: vec![
                    FramingSegment {
                        end: ratio(499_999, 1_000_000),
                        pose: pose(2),
                        curve: FramingCurve::Linear,
                    },
                    FramingSegment {
                        end: ratio(500_001, 1_000_000),
                        pose: pose(3),
                        curve: FramingCurve::Linear,
                    },
                    FramingSegment {
                        end: ExactRatio::ONE,
                        pose: pose(4),
                        curve: FramingCurve::Linear,
                    },
                ],
            },
        },
    };
    assert_eq!(
        framing
            .evaluate(ratio(i128::MAX - 1, i128::MAX), duration(2))
            .unwrap()
            .scale,
        ratio(5, 2)
    );
    assert_eq!(
        framing
            .evaluate(ratio(i128::from(i64::MAX), 2), duration(i64::MAX))
            .unwrap()
            .scale,
        ratio(5, 2)
    );
}

#[test]
fn interpolation_matches_independent_fraction_oracles_at_extreme_coordinates() {
    // The retained fixture was calculated with Python's independent arbitrary
    // precision Fraction arithmetic, not the production fixed-width helper.
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/framing-numeric-oracles.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let field = |name: &str| case[name].as_i64().unwrap();
        let start = ratio(i128::from(field("start_n")), i128::from(field("start_d")));
        let end = ratio(i128::from(field("end_n")), i128::from(field("end_d")));
        let mut segments = Vec::new();
        if start != ExactRatio::ZERO {
            segments.push(FramingSegment {
                end: start,
                pose: pose(1),
                curve: FramingCurve::Step,
            });
        }
        segments.push(FramingSegment {
            end,
            pose: pose(2),
            curve: FramingCurve::Linear,
        });
        if end != ExactRatio::ONE {
            segments.push(FramingSegment {
                end: ExactRatio::ONE,
                pose: pose(2),
                curve: FramingCurve::Step,
            });
        }
        let framing = Framing {
            value: FramingValue::Envelope {
                envelope: FramingEnvelope {
                    initial: pose(1),
                    segments,
                },
            },
        };
        let local = ratio(
            case["local_n"].as_str().unwrap().parse().unwrap(),
            case["local_d"].as_str().unwrap().parse().unwrap(),
        );
        let actual = framing
            .evaluate(local, duration(field("frames")))
            .unwrap()
            .scale;
        let expected = ratio(
            i128::from(FRAMING_NUMERIC_SCALE) + i128::from(case["progress"].as_u64().unwrap()),
            i128::from(FRAMING_NUMERIC_SCALE),
        );
        assert_eq!(actual, expected, "{case}");
    }
}

#[test]
fn setters_are_atomic_reversible_and_duration_changes_reflow_the_host() {
    let initial = document();
    let framing = creep(FramingCurve::Smoothstep);
    let framed = edit(
        &initial,
        Command::SetFraming {
            node: id("hold"),
            framing: Some(framing.clone()),
        },
    );
    assert_eq!(framed.audio_bindings(), initial.audio_bindings());
    assert_eq!(framed.marks(), initial.marks());
    let changed = edit(
        &framed,
        Command::SetHoldDuration {
            node: id("hold"),
            duration: duration(16),
        },
    );
    assert_eq!(changed.nodes()[&id("hold")].framing, Some(framing.clone()));
    assert_eq!(
        framing
            .evaluate(ExactRatio::integer(4), duration(8))
            .unwrap(),
        framing
            .evaluate(ExactRatio::integer(8), duration(16))
            .unwrap()
    );
    let mut stale = request(
        &framed,
        Command::SetFraming {
            node: id("hold"),
            framing: None,
        },
    );
    stale.expected_revision = initial.revision_id().clone();
    assert!(apply(&framed, &stale).is_err());
    let invalid = Framing {
        value: FramingValue::Static {
            pose: FramingPose {
                scale: ExactRatio::ZERO,
                ..pose(1)
            },
        },
    };
    assert!(
        apply(
            &framed,
            &request(
                &framed,
                Command::SetFraming {
                    node: id("hold"),
                    framing: Some(invalid)
                }
            )
        )
        .is_err()
    );
    assert_eq!(framed.nodes()[&id("hold")].framing, Some(framing));
}

#[test]
fn split_retains_framed_partition_as_a_complete_owned_effect_scope() {
    let initial = document();
    let once = split(&initial, &id("hold"), 4, "first");
    let framed = edit(
        &once,
        Command::SetFraming {
            node: id("first-1"),
            framing: Some(creep(FramingCurve::Smoothstep)),
        },
    );
    let twice = split(&framed, &id("first-1"), 2, "second");
    let mut retained = Vec::new();
    for node in twice.nodes().values() {
        if node.framing.is_some() {
            assert!(matches!(
                node.kind,
                NodeKind::Retime {
                    purpose: RetimePurpose::Partition,
                    ..
                }
            ));
            retained.push(node);
        }
    }
    assert_eq!(retained.len(), 2);
    assert_eq!(retained[0].framing, retained[1].framing);
    let contexts: Vec<_> = retained
        .iter()
        .map(|node| {
            let NodeKind::Retime {
                child,
                duration,
                mapping,
                ..
            } = &node.kind
            else {
                panic!()
            };
            assert_eq!(duration.frames(), 4);
            assert_eq!((mapping.start().0, mapping.end().0), (4, 8));
            child
        })
        .collect();
    assert_ne!(
        contexts[0], contexts[1],
        "physical children are independent copies"
    );
    for (id, node) in twice.nodes() {
        if id.as_str().starts_with("second-")
            && matches!(node.kind, NodeKind::Retime { duration: d, .. } if d.frames()==2)
        {
            assert!(node.framing.is_none());
        }
    }
    // Root framing must move into each full retained context, not be applied
    // again by the replacement root Sequence.
    let framed_root = edit(
        &initial,
        Command::SetFraming {
            node: id("root"),
            framing: Some(creep(FramingCurve::Linear)),
        },
    );
    let root_split = split(&framed_root, &id("root"), 4, "root-split");
    assert!(root_split.nodes()[root_split.root()].framing.is_none());
    assert_eq!(
        root_split
            .nodes()
            .values()
            .filter(|n| n.framing.is_some())
            .count(),
        2
    );
}

#[test]
fn occurrence_copy_is_independent_and_framed_ungroup_is_refused() {
    let initial = document();
    let repeated = edit(
        &initial,
        Command::WrapRepeat {
            node: id("hold"),
            id: id("repeat"),
            plays: 3,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        },
    );
    let NodeKind::Repeat { iterations, .. } = &repeated.nodes()[&id("repeat")].kind else {
        panic!()
    };
    let edited = edit(
        &repeated,
        Command::EditOccurrence {
            instance: InstancePath {
                node: id("hold"),
                repeats: vec![RepeatInstance {
                    node: id("repeat"),
                    iteration: iterations.at(1).unwrap(),
                }],
            },
            edit: OccurrenceEdit::SetFraming {
                framing: Some(creep(FramingCurve::Linear)),
            },
            identities: OccurrenceIdentities {
                nodes: vec![id("copy")],
                marks: vec![],
            },
        },
    );
    assert!(edited.nodes()[&id("hold")].framing.is_none());
    assert!(edited.nodes()[&id("copy")].framing.is_some());
    let grouped = edit(
        &initial,
        Command::Group {
            parent: id("root"),
            start: 0,
            end: 1,
            id: id("group"),
            label: "Group".into(),
        },
    );
    let framed = edit(
        &grouped,
        Command::SetFraming {
            node: id("group"),
            framing: Some(creep(FramingCurve::Linear)),
        },
    );
    assert!(
        apply(
            &framed,
            &request(&framed, Command::Ungroup { node: id("group") })
        )
        .is_err()
    );
}

#[test]
fn explicit_pause_can_split_a_retained_framed_partition_without_losing_its_clock() {
    let initial = document();
    let split_doc = split(&initial, &id("hold"), 4, "partition");
    let framed = edit(
        &split_doc,
        Command::SetFraming {
            node: id("partition-1"),
            framing: Some(creep(FramingCurve::Linear)),
        },
    );
    let mut request = request(
        &framed,
        Command::InsertTime {
            at: ProjectFrame(6),
            hold: HoldRecipe {
                duration: duration(2),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            },
            id: id("pause"),
            identities: SplitIdentities {
                nodes: (0..20).map(|n| id(&format!("pause-split-{n}"))).collect(),
            },
            timing: AudioTimingId {
                allocation: RevisionId::new("placeholder").unwrap(),
                ordinal: 0,
            },
        },
    );
    let Command::InsertTime { timing, .. } = &mut request.command else {
        panic!()
    };
    timing.allocation = request.new_revision.clone();
    let tx = apply(&framed, &request).unwrap();
    let after = tx.forward.apply(&framed).unwrap();
    assert_eq!(tx.inverse.apply(&after).unwrap(), framed);
    assert_eq!(after.duration().unwrap(), duration(10));
    assert!(
        after.nodes()[&id("pause")].framing.is_none(),
        "explicit HoldRecipe does not snapshot a composed picture"
    );
    let owners: Vec<_> = after
        .nodes()
        .values()
        .filter(|node| node.framing.is_some())
        .collect();
    assert_eq!(owners.len(), 2);
    for node in owners {
        assert_eq!(node.framing, Some(creep(FramingCurve::Linear)));
        assert!(matches!(node.kind,NodeKind::Retime{duration:d,..} if d.frames()==4));
    }
    // Removing all modern framing from that nested allocation restores the old
    // explicit refusal, so the widened current helper cannot widen v17 replay.
    let mut wire = serde_json::to_value(&after).unwrap();
    for node in wire["nodes"].as_object_mut().unwrap().values_mut() {
        node.as_object_mut().unwrap().remove("framing");
    }
    let unframed = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut next = request.clone();
    next.expected_revision = unframed.revision_id().clone();
    next.new_revision = RevisionId::new("next-pause").unwrap();
    let Command::InsertTime {
        at,
        id: pause_id,
        identities,
        timing,
        ..
    } = &mut next.command
    else {
        panic!()
    };
    *at = ProjectFrame(9);
    *pause_id = id("new-pause");
    identities.nodes = (0..20).map(|n| id(&format!("next-copy-{n}"))).collect();
    timing.allocation = next.new_revision.clone();
    assert!(apply(&unframed, &next).is_err());
}

#[test]
fn hostile_envelopes_layers_and_legacy_wire_are_rejected() {
    let initial = document();
    let mut long = match creep(FramingCurve::Linear).value {
        FramingValue::Envelope { envelope } => envelope,
        _ => panic!(),
    };
    long.segments = vec![long.segments[0].clone(); 65];
    assert!(
        serde_json::from_value::<FramingEnvelope>(serde_json::to_value(long).unwrap()).is_err()
    );
    let mut modern = serde_json::to_value(&initial).unwrap();
    modern["schema_version"] = json!(17);
    let old = legacy_v17::Document::from_json(&modern.to_string()).unwrap();
    let framed = edit(
        &initial,
        Command::SetFraming {
            node: id("hold"),
            framing: Some(creep(FramingCurve::Linear)),
        },
    );
    assert!(!old.matches(&framed));
    modern["nodes"]["hold"]["framing"] = serde_json::Value::Null;
    assert!(legacy_v17::Document::from_json(&modern.to_string()).is_err());
    assert!(
        legacy_v17::upgrade_request(
            &serde_json::to_string(&request(
                &initial,
                Command::SetFraming {
                    node: id("hold"),
                    framing: None
                }
            ))
            .unwrap()
        )
        .is_err()
    );
    let mut doc = initial;
    for n in 0..15 {
        let child = if n == 0 {
            id("hold")
        } else {
            id(&format!("group-{}", n - 1))
        };
        doc = edit(
            &doc,
            Command::WrapRepeat {
                node: child,
                id: id(&format!("group-{n}")),
                plays: 1,
                gap: None,
                anchor_policy: WrapAnchorPolicy::First,
            },
        );
        doc = edit(
            &doc,
            Command::SetFraming {
                node: id(&format!("group-{n}")),
                framing: Some(Framing::static_pose(pose(1)).unwrap()),
            },
        );
    }
    doc = edit(
        &doc,
        Command::SetFraming {
            node: id("hold"),
            framing: Some(Framing::static_pose(pose(1)).unwrap()),
        },
    );
    assert!(
        apply(
            &doc,
            &request(
                &doc,
                Command::SetFraming {
                    node: id("root"),
                    framing: Some(Framing::static_pose(pose(1)).unwrap())
                }
            )
        )
        .is_err()
    );
}

#[test]
fn typed_subtree_aggregate_framing_is_rejected_before_installation() {
    let doc = document();
    let envelope = Framing {
        value: FramingValue::Envelope {
            envelope: FramingEnvelope {
                initial: pose(1),
                segments: (1..=64)
                    .map(|i| FramingSegment {
                        end: ratio(i, 64),
                        pose: pose(1),
                        curve: FramingCurve::Cubic {
                            control1: pose(1),
                            control2: pose(1),
                        },
                    })
                    .collect(),
            },
        },
    };
    assert_eq!(envelope.record_count(), 193);
    let mut nodes = BTreeMap::new();
    let mut children = Vec::new();
    for n in 0..519 {
        let key = id(&format!("large-{n}"));
        children.push(key.clone());
        let mut node = hold(1);
        node.framing = Some(envelope.clone());
        nodes.insert(key, node);
    }
    nodes.insert(id("large-root"), BeatNode::sequence("Large", children));
    let request = request(
        &doc,
        Command::Insert {
            parent: id("root"),
            index: 0,
            subtree: Subtree {
                root: id("large-root"),
                nodes,
                overrides: BTreeMap::new(),
            },
        },
    );
    let error = apply(&doc, &request).unwrap_err();
    assert_eq!(error.code, EditErrorCode::LimitExceeded);
    assert_eq!(doc.nodes().len(), 2);
}
