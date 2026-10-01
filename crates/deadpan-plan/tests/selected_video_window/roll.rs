//! Signed-origin VFR oracle for a fixed-duration Roll, including exact terminal
//! padding and every direct/neutral-Partition pairing. No decoding is involved.
use super::*;

fn owner(context: SourceSpan, frames: i64, start: i64, end: ExactRatio) -> BeatNode {
    let mut node = source(
        context,
        SourceVideoMapping::SelectedPlacement {
            start: ExactRatio::integer(start),
            frames: ExactRatio::integer(6),
            selection: ExactFrameRange::new(ExactRatio::ZERO, end).unwrap(),
            endpoints: EndpointPolicy::HoldAdjacent,
        },
    );
    let NodeKind::Source { source } = &mut node.kind else {
        unreachable!()
    };
    source.duration = duration(frames);
    source.edit_window = Some(SourceEditWindow::new(ExactRatio::ZERO, end).unwrap());
    node
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
fn fixture(origin: i64, left_partition: bool, right_partition: bool) -> ProjectDocument {
    let full = span(origin, origin + 12012);
    let left = if left_partition { "left-crop" } else { "left" };
    let right = if right_partition {
        "right-crop"
    } else {
        "right"
    };
    let mut nodes = vec![
        (
            "lead",
            beat(NodeKind::Hold {
                recipe: background(1),
            }),
        ),
        (
            "left",
            owner(
                full,
                if left_partition { 4 } else { 2 },
                0,
                ExactRatio::integer(if left_partition { 4 } else { 2 }),
            ),
        ),
        (
            "right",
            owner(
                full,
                if right_partition { 4 } else { 2 },
                if right_partition { 0 } else { -2 },
                if right_partition {
                    ratio(7, 2)
                } else {
                    ratio(3, 2)
                },
            ),
        ),
        ("tail", owner(full, 1, 0, ExactRatio::ONE)),
    ];
    if left_partition {
        nodes.push(("left-crop", crop("left", 0, 2)));
    }
    if right_partition {
        nodes.push(("right-crop", crop("right", 2, 4)));
    }
    let base = document(full, &["lead", left, right, "tail"], nodes);
    let mut record = base.assets()[&asset()].clone();
    record.source_qualification = Some(SourceQualificationId::new("b".repeat(64)).unwrap());
    record.frame_count = Some(duration(7));
    let mut wire = serde_json::to_value(base).unwrap();
    wire["assets"]["video"] = serde_json::to_value(record).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn roll_moves_exact_vfr_ownership_at_signed_origins_and_keeps_terminal_padding_and_suffix() {
    for origin in [-10010, 13013] {
        for left_partition in [false, true] {
            for right_partition in [false, true] {
                let before = fixture(origin, left_partition, right_partition);
                let left = if left_partition { "left-crop" } else { "left" };
                let right = if right_partition {
                    "right-crop"
                } else {
                    "right"
                };
                let old = RenderPlan::compile(&before).unwrap();
                let index = vfr_index(origin);
                for delta in [-1, 1] {
                    let result = before
                        .source_roll(&node_id("root"), &node_id(left), &node_id(right), delta)
                        .unwrap();
                    assert_eq!(result.applied_delta_frames, delta);
                    assert_eq!(
                        result.pair_output,
                        FrameRange::new(ProjectFrame(1), ProjectFrame(5)).unwrap()
                    );
                    assert_eq!(result.seam_before, ProjectFrame(3));
                    assert_eq!(result.seam_after, ProjectFrame(3 + delta));
                    assert_eq!(result.left.needs_wrapper, delta < 0 && !left_partition);
                    assert_eq!(result.right.needs_wrapper, delta > 0 && !right_partition);
                    assert_eq!(
                        result.right.physical_prefix,
                        duration(if delta < 0 && !right_partition { 1 } else { 0 })
                    );
                    let allocation = revision(if delta < 0 { "roll-left" } else { "roll-right" });
                    let tx = apply(
                        &before,
                        &CommandRequest {
                            project_id: before.project_id().clone(),
                            expected_revision: before.revision_id().clone(),
                            new_revision: allocation.clone(),
                            command: Command::RollSources {
                                parent: node_id("root"),
                                left: node_id(left),
                                right: node_id(right),
                                delta_frames: delta,
                                left_wrapper: result
                                    .left
                                    .needs_wrapper
                                    .then(|| node_id("new-left-crop")),
                                right_wrapper: result
                                    .right
                                    .needs_wrapper
                                    .then(|| node_id("new-right-crop")),
                                timing: AudioTimingId {
                                    allocation,
                                    ordinal: 0,
                                },
                            },
                        },
                    )
                    .unwrap();
                    let after = tx.forward.apply(&before).unwrap();
                    let plan = RenderPlan::compile(&after).unwrap();
                    assert_eq!(plan.duration(), duration(6));
                    let expected = if delta < 0 {
                        [(1, "left", 1001, 0, 0), (2, "right", 3003, 1, 1400)]
                    } else {
                        [(3, "left", 5005, 3, 4600), (4, "right", 7007, 4, 6200)]
                    };
                    for (frame, owner, ticks, ordinal, pts) in expected {
                        let sample = plan.picture(ProjectFrame(frame)).unwrap();
                        assert_eq!(sample.instance.node, node_id(owner));
                        let (point, context, _) = source_fields(&sample.picture);
                        assert_eq!(point.ticks, ExactRatio::integer(origin + ticks));
                        assert_eq!(context, span(origin, origin + 12012));
                        let selected = sample.picture.select_source_frame(&index).unwrap();
                        assert_eq!(selected.identity, SourceFrameId(ordinal));
                        assert_eq!(selected.pts, origin + pts);
                    }
                    // The exact right endpoint is7007, not the enclosing8008.
                    // Final frame center lands on it, so HoldAdjacent uses ordinal4
                    // ending at7007 rather than ordinal5 beginning at7007.
                    let terminal = plan.picture(ProjectFrame(4)).unwrap();
                    let (_, _, selected) = source_fields(&terminal.picture);
                    assert_eq!(selected.end().ticks, ExactRatio::integer(origin + 7007));
                    assert_eq!(
                        terminal
                            .picture
                            .select_source_frame(&index)
                            .unwrap()
                            .identity,
                        SourceFrameId(4)
                    );
                    let suffix = plan.picture(ProjectFrame(5)).unwrap();
                    let old_suffix = old.picture(ProjectFrame(5)).unwrap();
                    assert_eq!(suffix.picture, old_suffix.picture);
                    assert_eq!(suffix.instance, old_suffix.instance);
                    assert_eq!(suffix.local_position, old_suffix.local_position);
                    assert_eq!(suffix.framing, old_suffix.framing);
                    assert_eq!(tx.inverse.apply(&after).unwrap(), before);
                }
            }
        }
    }
}
