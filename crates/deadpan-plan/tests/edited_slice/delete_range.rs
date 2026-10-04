//! Indexed coordinates and owner clocks, not decoded or composed GPU pixels.
use super::*;

fn window(child: &str, start: i64, end: i64) -> BeatNode {
    BeatNode {
        label: "Window".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id(child),
            duration: duration(end - start),
            mapping: range(start, end),
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Partition,
        },
        cutaways: Vec::new(),
    }
}

fn cut(before: &ProjectDocument, parent: &str, start: i64, end: i64) -> ProjectDocument {
    let selected = range(start, end);
    let query = before.range_deletion(&id(parent), selected).unwrap();
    let tx = apply(
        before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision("nested-delete"),
            command: Command::DeleteRange {
                parent: id(parent),
                range: selected,
                identities: SplitIdentities {
                    nodes: (0..query.required_ids)
                        .map(|n| id(&format!("delete-{n}")))
                        .collect(),
                },
                timing: timing("nested-delete"),
            },
        },
    )
    .unwrap();
    let after = tx.forward.apply(before).unwrap();
    assert_eq!(tx.inverse.apply(&after).unwrap(), *before);
    assert_eq!(
        after.duration().unwrap().frames(),
        before.duration().unwrap().frames() - (end - start)
    );
    after
}

fn index() -> SourceFrameIndex {
    SourceFrameIndex::new(
        asset(),
        clock(),
        (0_u64..30)
            .map(|ordinal| IndexedSourceFrame {
                identity: SourceFrameId(ordinal),
                pts: i64::try_from(ordinal).unwrap() * 1001,
                reported_duration: None,
                keyframe: ordinal == 0,
                seek_from: Some(SourceFrameId(0)),
                decode_timestamp: None,
            })
            .collect(),
        30030,
        TerminalProvenance::Explicit,
    )
    .unwrap()
}

fn owner(
    sample: &PictureSample,
    document: &ProjectDocument,
    label: &str,
    local: ExactRatio,
    frames: i64,
    end_scale: i64,
) {
    let layers: Vec<_> = sample
        .framing
        .iter()
        .filter(|layer| {
            document.nodes()[&layer.instance.node].label == label && layer.pose.is_some()
        })
        .collect();
    assert_eq!(layers.len(), 1, "owner {label} must apply exactly once");
    let layer = layers[0];
    assert_eq!(layer.local_position, local);
    assert_eq!(layer.duration, duration(frames));
    assert_eq!(
        layer.pose.unwrap().scale,
        linear_scale_at(local, frames, end_scale)
    );
    layer.instance.validate(document).unwrap();
}

fn framed_root(before: &ProjectDocument) -> ProjectDocument {
    edit(
        before,
        "root-camera",
        Command::SetFraming {
            node: id("root"),
            framing: creep(BeatNode::sequence("", vec![]), 7).framing,
        },
    )
}

#[test]
fn two_and_three_nested_windows_keep_every_source_or_freeze_point_and_framing_clock() {
    let source_index = index();
    for depth in [2, 3] {
        for frozen in [false, true] {
            for framed in [false, true] {
                let frame = |node, scale| if framed { creep(node, scale) } else { node };
                let leaf = if frozen {
                    freeze(12, 5005)
                } else {
                    source(12, 0, 12012)
                };
                let mut nodes = vec![
                    ("leaf", frame(leaf, 5)),
                    ("inner", frame(window("leaf", 1, 12), 3)),
                ];
                if depth == 2 {
                    nodes.push(("outer", window("inner", 2, 9)));
                } else {
                    nodes.push(("middle", frame(window("inner", 1, 10), 4)));
                    nodes.push(("outer", window("middle", 1, 8)));
                }
                let before = framed_root(&document(&["outer"], nodes));
                let after = cut(&before, "root", 1, 3);
                let plan = RenderPlan::compile(&after).unwrap();
                let old = RenderPlan::compile(&before).unwrap();
                // Both chains expose leaf [3,10). The retained old global frames
                // are [0,3,4,5,6], independently of preflight and split identities.
                for (output, original) in [0, 3, 4, 5, 6].into_iter().enumerate().rev() {
                    let sample = plan
                        .picture(ProjectFrame(i64::try_from(output).unwrap()))
                        .unwrap();
                    let was = old.picture(ProjectFrame(original)).unwrap();
                    let leaf_position = ratio(i128::from(2 * original + 7), 2);
                    let ticks = if frozen {
                        ExactRatio::integer(5005)
                    } else {
                        leaf_position
                            .checked_mul(ExactRatio::integer(1001))
                            .unwrap()
                    };
                    assert_eq!(picture_ticks(&sample), ticks);
                    assert_eq!(sample.picture, was.picture);
                    assert_eq!(sample.local_position, leaf_position);
                    let indexed = sample.picture.select_source_frame(&source_index).unwrap();
                    let ordinal = if frozen { 5 } else { original + 3 };
                    assert_eq!(
                        indexed.identity,
                        SourceFrameId(u64::try_from(ordinal).unwrap())
                    );
                    assert_eq!(indexed.pts, ordinal * 1001);
                    assert_eq!(
                        sample.picture_context.as_deref(),
                        frozen.then_some(&captured_geometry())
                    );
                    if framed {
                        owner(&sample, &after, "leaf", leaf_position, 12, 5);
                        owner(
                            &sample,
                            &after,
                            "inner",
                            ratio(i128::from(2 * original + 5), 2),
                            11,
                            3,
                        );
                        if depth == 3 {
                            owner(
                                &sample,
                                &after,
                                "middle",
                                ratio(i128::from(2 * original + 3), 2),
                                9,
                                4,
                            );
                        }
                    } else {
                        // No treated physical owner is available to enable the
                        // historical admission exception for these nested chains.
                        assert!(sample.framing.iter().all(|layer| {
                            layer.instance.node == id("root") || layer.pose.is_none()
                        }));
                    }
                    owner(
                        &sample,
                        &after,
                        "root",
                        ratio(i128::try_from(2 * output + 1).unwrap(), 2),
                        5,
                        7,
                    );
                    sample.instance.validate(&after).unwrap();
                }
            }
        }
    }
}

#[test]
fn endpoints_in_different_nested_children_preserve_context_live_ancestors_and_mark_bias() {
    let source_index = index();
    let mut before = framed_root(&document(
        &["lead", "scope", "tail"],
        vec![
            ("lead", source(2, 0, 2002)),
            (
                "scope",
                creep(
                    BeatNode::sequence("", vec![id("a-window"), id("b-window")]),
                    6,
                ),
            ),
            ("a-window", window("a-inner", 2, 9)),
            ("a-inner", creep(window("a-leaf", 1, 12), 3)),
            ("a-leaf", creep(source(12, 0, 12012), 5)),
            ("b-window", window("b-middle", 1, 8)),
            ("b-middle", creep(window("b-inner", 1, 10), 4)),
            ("b-inner", creep(window("b-leaf", 1, 12), 3)),
            ("b-leaf", creep(freeze(12, 5005), 5)),
            ("tail", source(2, 20020, 22022)),
        ],
    ));
    for (name, position, bias) in [
        ("prefix", 1, InsertionBias::Right),
        ("start-left", 4, InsertionBias::Left),
        ("end-right", 12, InsertionBias::Right),
        ("suffix", 17, InsertionBias::Right),
        ("deleted", 8, InsertionBias::Right),
    ] {
        before = edit(
            &before,
            name,
            Command::SetMark {
                id: MarkId::new(name).unwrap(),
                owner: id("root"),
                label: name.into(),
                boundary: BoundaryAnchor {
                    coordinate: Anchor::Local {
                        node: id("root"),
                        position: ExactRatio::integer(position),
                    },
                    bias,
                },
                loss_policy: AnchorLossPolicy::KeepUnresolved,
            },
        );
    }
    let after = cut(&before, "scope", 4, 12);
    let plan = RenderPlan::compile(&after).unwrap();
    let old = RenderPlan::compile(&before).unwrap();
    for (output, original) in [0, 1, 2, 3, 12, 13, 14, 15, 16, 17]
        .into_iter()
        .enumerate()
        .rev()
    {
        let output = i64::try_from(output).unwrap();
        let sample = plan.picture(ProjectFrame(output)).unwrap();
        let was = old.picture(ProjectFrame(original)).unwrap();
        assert_eq!(sample.picture, was.picture);
        assert_eq!(sample.picture_context, was.picture_context);
        let ordinal = match output {
            0..=1 => output,
            2..=3 => output + 1,
            4..=7 => 5,
            _ => output + 12,
        };
        let indexed = sample.picture.select_source_frame(&source_index).unwrap();
        assert_eq!(
            indexed.identity,
            SourceFrameId(u64::try_from(ordinal).unwrap())
        );
        assert_eq!(indexed.pts, ordinal * 1001);
        owner(
            &sample,
            &after,
            "root",
            ratio(i128::from(2 * output + 1), 2),
            10,
            7,
        );
        if (2..8).contains(&output) {
            owner(
                &sample,
                &after,
                "scope",
                ratio(i128::from(2 * output - 3), 2),
                6,
                6,
            );
            let (prefix, local, inner, middle) = if output < 4 {
                ("a", output + 1, output, None)
            } else {
                ("b", output + 2, output + 1, Some(output))
            };
            owner(
                &sample,
                &after,
                &format!("{prefix}-leaf"),
                ratio(i128::from(2 * local + 1), 2),
                12,
                5,
            );
            owner(
                &sample,
                &after,
                &format!("{prefix}-inner"),
                ratio(i128::from(2 * inner + 1), 2),
                11,
                3,
            );
            if let Some(middle) = middle {
                owner(
                    &sample,
                    &after,
                    "b-middle",
                    ratio(i128::from(2 * middle + 1), 2),
                    9,
                    4,
                );
            }
        }
    }
    assert_eq!(
        after.marks().keys().collect::<Vec<_>>(),
        before.marks().keys().collect::<Vec<_>>()
    );
    for (name, at) in [
        ("prefix", 1),
        ("start-left", 4),
        ("end-right", 4),
        ("suffix", 9),
    ] {
        let key = MarkId::new(name).unwrap();
        let mark = &after.marks()[&key];
        assert_eq!(mark.state, MarkState::Bound);
        assert_eq!(
            mark.boundary.coordinate,
            Anchor::Local {
                node: id("root"),
                position: ExactRatio::integer(at)
            }
        );
        assert_eq!(mark.boundary.bias, before.marks()[&key].boundary.bias);
    }
    assert!(matches!(
        after.marks()[&MarkId::new("deleted").unwrap()].state,
        MarkState::Unresolved { .. }
    ));
}
