//! Literal signed VFR picture coordinates for the complete authoring command.
use super::*;

fn owner(context: SourceSpan, length: i64, start: i64, end: ExactRatio) -> BeatNode {
    let mut result = source(
        context,
        SourceVideoMapping::SelectedPlacement {
            start: ExactRatio::integer(start),
            frames: ExactRatio::integer(6),
            selection: ExactFrameRange::new(ExactRatio::ZERO, end).unwrap(),
            endpoints: EndpointPolicy::HoldAdjacent,
        },
    );
    let NodeKind::Source { source } = &mut result.kind else {
        unreachable!()
    };
    source.duration = duration(length);
    source.edit_window = Some(SourceEditWindow::new(ExactRatio::ZERO, end).unwrap());
    result
}

fn crop(child: &str, start: i64, end: i64) -> BeatNode {
    beat(NodeKind::Retime {
        child: node_id(child),
        duration: duration(end - start),
        mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
        pitch: PitchPolicy::FollowSpeed,
        purpose: RetimePurpose::Partition,
    })
}

fn qualified(document: ProjectDocument) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["assets"]["video"]["source_qualification"] =
        serde_json::to_value(SourceQualificationId::new("b".repeat(64)).unwrap()).unwrap();
    wire["assets"]["video"]["frame_count"] = serde_json::to_value(duration(7)).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn apply_intent(
    before: &ProjectDocument,
    left: &str,
    right: &str,
    intent: SourceTrimIntent,
) -> (ProjectDocument, SourceTrimEditResolution) {
    let r = before
        .source_trim_edit(
            &node_id("root"),
            &node_id(left),
            Some(&node_id(right)),
            intent,
        )
        .unwrap();
    let allocation = revision("combined-picture");
    let tx = apply(
        before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: allocation.clone(),
            command: Command::ApplySourceTrim {
                parent: node_id("root"),
                node: node_id(left),
                right: Some(node_id(right)),
                intent,
                resources: SourceTrimResources {
                    target_wrapper: r
                        .required_target_wrapper
                        .then(|| node_id("new-left-window")),
                    right_wrapper: r
                        .required_right_wrapper
                        .then(|| node_id("new-right-window")),
                    split: SplitIdentities {
                        nodes: (0..r.required_split_nodes)
                            .map(|n| node_id(&format!("split-{n}")))
                            .collect(),
                    },
                    fillers: (0..r.required_filler_nodes)
                        .map(|n| node_id(&format!("filler-{n}")))
                        .collect(),
                    timing: (r.capture != SourceTrimCapture::None).then_some(AudioTimingId {
                        allocation,
                        ordinal: 0,
                    }),
                },
            },
        },
    )
    .unwrap();
    let after = tx.forward.apply(before).unwrap();
    assert_eq!(tx.inverse.apply(&after).unwrap(), *before);
    assert_eq!(tx.forward.apply(before).unwrap(), after);
    (after, r)
}

#[test]
fn complete_intent_moves_two_physical_origins_but_samples_literal_original_pts() {
    for origin in [-10010, 13013] {
        for partitions in [false, true] {
            let full = span(origin, origin + 12012);
            let length = if partitions { 4 } else { 2 };
            let (left, right) = if partitions {
                ("left-crop", "right-crop")
            } else {
                ("left", "right")
            };
            let mut nodes = vec![
                (
                    "lead",
                    beat(NodeKind::Hold {
                        recipe: background(3),
                    }),
                ),
                ("left", owner(full, length, -2, ExactRatio::integer(length))),
                (
                    "right",
                    owner(full, length, -2, ExactRatio::integer(length)),
                ),
                ("tail", owner(full, 1, 0, ExactRatio::ONE)),
            ];
            if partitions {
                nodes.push(("left-crop", crop("left", 1, 3)));
                nodes.push(("right-crop", crop("right", 2, 4)));
            }
            let before = qualified(document(full, &["lead", left, right, "tail"], nodes));
            let immutable = before.clone();
            let intent = if partitions {
                SourceTrimIntent {
                    in_frames: -2,
                    out_frames: 3,
                    slip_frames: 1,
                    roll_frames: -4,
                    policy: SourceTrimPolicy::Ripple,
                }
            } else {
                SourceTrimIntent {
                    in_frames: -1,
                    out_frames: 2,
                    slip_frames: 1,
                    roll_frames: -2,
                    policy: SourceTrimPolicy::Ripple,
                }
            };
            let (after, r) = apply_intent(&before, left, right, intent);
            assert_eq!(before, immutable);
            assert_eq!(r.geometry.target.physical_prefix, duration(1));
            assert_eq!(
                r.geometry.right.as_ref().unwrap().physical_prefix,
                duration(2)
            );
            let plan = RenderPlan::compile(&after).unwrap();
            let index = vfr_index(origin);
            let right_frames = if partitions { 6 } else { 4 };
            assert_eq!(plan.duration(), duration(7 + right_frames));
            // Each frame is2002 source ticks. A's selected physical centers are
            // −.5,.5,1.5 before its +1 prefix; its media start is−3 after Slip.
            for (frame, ticks, ordinal, pts) in
                [(3, 5005, 3, 4600), (4, 7007, 5, 7007), (5, 9009, 6, 8200)]
            {
                let sample = plan.picture(ProjectFrame(frame)).unwrap();
                assert_eq!(sample.instance.node, node_id("left"));
                let (point, context, selection) = source_fields(&sample.picture);
                assert_eq!(point.ticks, ExactRatio::integer(origin + ticks));
                assert_eq!(context, full);
                assert_eq!(selection.start().ticks, ExactRatio::integer(origin + 4004));
                assert_eq!(
                    selection.end().ticks,
                    ExactRatio::integer(origin + if partitions { 12012 } else { 10010 })
                );
                let actual = sample.picture.select_source_frame(&index).unwrap();
                assert_eq!(
                    (actual.identity, actual.pts),
                    (SourceFrameId(ordinal), origin + pts)
                );
            }
            for (offset, ticks, ordinal, pts) in [
                (0, 1001, 0, 0),
                (1, 3003, 1, 1400),
                (2, 5005, 3, 4600),
                (3, 7007, 5, 7007),
                (4, 9009, 6, 8200),
                (5, 11011, 6, 8200),
            ]
            .into_iter()
            .take(right_frames as usize)
            {
                let sample = plan.picture(ProjectFrame(6 + offset)).unwrap();
                assert_eq!(sample.instance.node, node_id("right"));
                let (point, context, _) = source_fields(&sample.picture);
                assert_eq!(context, full);
                assert_eq!(point.ticks, ExactRatio::integer(origin + ticks));
                let actual = sample.picture.select_source_frame(&index).unwrap();
                assert_eq!(
                    (actual.identity, actual.pts),
                    (SourceFrameId(ordinal), origin + pts)
                );
            }
            let old = RenderPlan::compile(&before)
                .unwrap()
                .picture(ProjectFrame(7))
                .unwrap();
            let tail = plan.picture(ProjectFrame(6 + right_frames)).unwrap();
            assert_eq!(tail.picture, old.picture);
            assert_eq!(tail.instance, old.instance);
            assert_eq!(tail.local_position, old.local_position);
        }
    }
}

#[test]
fn overwrite_keeps_a_real_b_owner_even_when_only_its_terminal_padding_survives() {
    for origin in [-10010, 13013] {
        let full = span(origin, origin + 12012);
        let before = qualified(document(
            full,
            &["lead", "left", "right", "tail"],
            vec![
                (
                    "lead",
                    beat(NodeKind::Hold {
                        recipe: background(1),
                    }),
                ),
                ("left", owner(full, 2, 0, ExactRatio::integer(2))),
                ("right", owner(full, 3, -2, ratio(3, 2))),
                ("tail", owner(full, 1, 0, ExactRatio::ONE)),
            ],
        ));
        let (after, r) = apply_intent(
            &before,
            "left",
            "right",
            SourceTrimIntent {
                out_frames: 2,
                policy: SourceTrimPolicy::Overwrite,
                ..Default::default()
            },
        );
        let b = r.right_after.unwrap();
        assert_eq!(
            b.output,
            FrameRange::new(ProjectFrame(5), ProjectFrame(6)).unwrap()
        );
        assert_eq!(
            b.allocation,
            FrameRange::new(ProjectFrame(2), ProjectFrame(3)).unwrap()
        );
        assert!(b.visible_selection.is_none());
        assert_eq!(
            after.nodes()[&node_id("right")].kind,
            before.nodes()[&node_id("right")].kind
        );
        let plan = RenderPlan::compile(&after).unwrap();
        assert_eq!(plan.duration(), duration(7));
        let sample = plan.picture(ProjectFrame(5)).unwrap();
        assert_eq!(sample.instance.node, node_id("right"));
        let (point, context, selection) = source_fields(&sample.picture);
        assert_eq!(point.ticks, ExactRatio::integer(origin + 9009));
        assert_eq!(context, full);
        assert_eq!(
            selection,
            exact_span(
                ExactRatio::integer(origin + 4004),
                ExactRatio::integer(origin + 7007)
            )
        );
        // The frame at source7007 exists but belongs to the excluded interval.
        // HoldAdjacent must retain ordinal4 ending there, not ordinal5 or6.
        let index = vfr_index(origin);
        let chosen = sample.picture.select_source_frame(&index).unwrap();
        assert_eq!(
            (chosen.identity, chosen.pts),
            (SourceFrameId(4), origin + 6200)
        );
        let old = RenderPlan::compile(&before).unwrap();
        assert_eq!(
            sample.picture,
            old.picture(ProjectFrame(5)).unwrap().picture
        );
        assert_eq!(
            plan.picture(ProjectFrame(6)).unwrap().picture,
            old.picture(ProjectFrame(6)).unwrap().picture
        );
    }
}
