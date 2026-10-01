//! Slip changes indexed Original pictures while leaving delivery and camera clocks fixed.
use super::*;

fn camera(end_scale: i64) -> Framing {
    Framing::creep(
        FramingPose::identity(),
        FramingPose::new(ratio(1, 2), ratio(1, 2), ExactRatio::integer(end_scale)).unwrap(),
        FramingCurve::Linear,
    )
    .unwrap()
}

fn fixture(origin: i64, partition: bool) -> ProjectDocument {
    let full = span(origin, origin + 12012);
    let mut picture = source(full, natural_selection(EndpointPolicy::HoldAdjacent));
    let NodeKind::Source { source } = &mut picture.kind else {
        unreachable!()
    };
    source.edit_window = Some(SourceEditWindow::new(ExactRatio::ZERO, ratio(3, 2)).unwrap());
    picture.framing = Some(
        camera(2)
            .prepend_owner_frames(duration(1), duration(1))
            .unwrap(),
    );
    let target = if partition {
        source.duration = duration(6);
        source.edit_window = Some(SourceEditWindow::new(ExactRatio::ZERO, ratio(11, 2)).unwrap());
        source.video_mapping = SourceVideoMapping::SelectedPlacement {
            start: ExactRatio::ZERO,
            frames: ExactRatio::integer(6),
            selection: ExactFrameRange::new(ExactRatio::ZERO, ratio(11, 2)).unwrap(),
            endpoints: EndpointPolicy::HoldAdjacent,
        };
        picture.framing = Some(
            camera(2)
                .prepend_owner_frames(duration(2), duration(2))
                .unwrap(),
        );
        "crop"
    } else {
        "source"
    };
    let mut group = BeatNode::sequence("Live group camera", vec![node_id(target)]);
    group.framing = Some(camera(3));
    let mut nodes = vec![
        ("source", picture),
        ("group", group),
        (
            "lead",
            beat(NodeKind::Hold {
                recipe: background(3),
            }),
        ),
        (
            "tail",
            beat(NodeKind::Hold {
                recipe: background(2),
            }),
        ),
    ];
    if partition {
        nodes.push((
            "crop",
            beat(NodeKind::Retime {
                child: node_id("source"),
                duration: duration(2),
                mapping: FrameRange::new(ProjectFrame(2), ProjectFrame(4)).unwrap(),
                pitch: PitchPolicy::FollowSpeed,
                purpose: RetimePurpose::Partition,
            }),
        ));
    }
    let document = document(full, &["lead", "group", "tail"], nodes);
    let mut wire = serde_json::to_value(document).unwrap();
    wire["assets"]["video"]["source_qualification"] =
        serde_json::to_value(SourceQualificationId::new("b".repeat(64)).unwrap()).unwrap();
    wire["assets"]["video"]["frame_count"] = serde_json::to_value(duration(7)).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn slipped(before: &ProjectDocument, target: &str, requested: i64) -> ProjectDocument {
    let transaction = apply(
        before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision(&format!("{}-slip-{requested}", before.revision_id())),
            command: Command::SlipSource {
                parent: node_id("group"),
                node: node_id(target),
                delta_frames: requested,
            },
        },
    )
    .unwrap();
    let after = transaction.forward.apply(before).unwrap();
    let restored = transaction.inverse.apply(&after).unwrap();
    assert_eq!(restored, *before);
    let old = RenderPlan::compile(before).unwrap();
    let restored = RenderPlan::compile(&restored).unwrap();
    for frame in 0..7 {
        assert_eq!(
            restored.picture(ProjectFrame(frame)).unwrap(),
            old.picture(ProjectFrame(frame)).unwrap()
        );
    }
    ProjectDocument::from_json(&after.to_json().unwrap()).unwrap()
}

fn fixed_delivery_and_cameras(before: &RenderPlan, after: &RenderPlan) {
    assert_eq!(before.duration(), duration(7));
    assert_eq!(after.duration(), before.duration());
    for frame in [6, 3, 0, 4, 5, 2, 1] {
        let old = before.picture(ProjectFrame(frame)).unwrap();
        let new = after.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(new.project_id, old.project_id);
        assert_ne!(new.revision_id, old.revision_id);
        assert_eq!(new.project_frame, old.project_frame);
        assert_eq!(new.instance, old.instance);
        assert_eq!(new.local_position, old.local_position);
        assert_eq!(new.gap_after, old.gap_after);
        assert_eq!(new.picture_context, old.picture_context);
        assert_eq!(new.framing, old.framing);
        if !(3..5).contains(&frame) {
            assert_eq!(new.picture, old.picture);
        }
    }
    assert!(after.picture(ProjectFrame(7)).is_err());
}

fn indexed_oracle(
    plan: &RenderPlan,
    source_index: &SourceFrameIndex,
    origin: i64,
    selected: (i64, i64),
    samples: [(i64, u64, i64); 2],
) {
    let expected_selection = exact_span(
        ExactRatio::integer(origin + selected.0),
        ExactRatio::integer(origin + selected.1),
    );
    for (frame, (point_ticks, ordinal, frame_pts)) in [3, 4].into_iter().zip(samples) {
        let sample = plan.picture(ProjectFrame(frame)).unwrap();
        let (point, context, selection) = source_fields(&sample.picture);
        assert_eq!(point.ticks, ExactRatio::integer(origin + point_ticks));
        assert_eq!(point.time_base, clock());
        assert_eq!(context, span(origin, origin + 12012));
        assert_eq!(selection, expected_selection);
        let selected = sample.picture.select_source_frame(source_index).unwrap();
        assert_eq!(selected.identity, SourceFrameId(ordinal));
        assert_eq!(selected.pts, origin + frame_pts);
    }
}

#[test]
fn signed_vfr_slip_selects_known_pts_in_both_directions_and_holds_the_rounded_tail() {
    // Original intervals begin at offsets 0, 1400, 4004, 4600, 6200, 7007,
    // 8200 and end at 12012 ticks. One project frame is exactly 2002 ticks.
    // These fixed expectations do not use the slip resolver or mapping projection.
    for origin in [-10010, 13013] {
        let before = fixture(origin, false);
        let baseline = RenderPlan::compile(&before).unwrap();
        let source_index = vfr_index(origin);
        indexed_oracle(
            &baseline,
            &source_index,
            origin,
            (4004, 7007),
            [(5005, 3, 4600), (7007, 4, 6200)],
        );
        for (requested, selection, pictures) in [
            (-100, (0, 3003), [(1001, 0, 0), (3003, 1, 1400)]),
            (-1, (2002, 5005), [(3003, 1, 1400), (5005, 3, 4600)]),
            (1, (6006, 9009), [(7007, 5, 7007), (9009, 6, 8200)]),
            (100, (8008, 11011), [(9009, 6, 8200), (11011, 6, 8200)]),
        ] {
            let after = slipped(&before, "source", requested);
            let plan = RenderPlan::compile(&after).unwrap();
            fixed_delivery_and_cameras(&baseline, &plan);
            indexed_oracle(&plan, &source_index, origin, selection, pictures);
            for (frame, source_scale, group_scale) in [
                (3, ExactRatio::ONE, ratio(3, 2)),
                (4, ratio(3, 2), ratio(5, 2)),
            ] {
                let sample = plan.picture(ProjectFrame(frame)).unwrap();
                assert_eq!(sample.framing[0].instance.node, node_id("source"));
                assert_eq!(sample.framing[0].duration, duration(2));
                assert_eq!(sample.framing[0].pose.unwrap().scale, source_scale);
                assert_eq!(sample.framing[1].instance.node, node_id("group"));
                assert_eq!(sample.framing[1].pose.unwrap().scale, group_scale);
            }
        }
        // An authored +1 then -1 returns to the selected terminal boundary.
        // The full Original has frame 5 there, but rounded slack must hold 4.
        let later = slipped(&before, "source", 1);
        let returned = slipped(&later, "source", -1);
        let returned = RenderPlan::compile(&returned).unwrap();
        indexed_oracle(
            &returned,
            &source_index,
            origin,
            (4004, 7007),
            [(5005, 3, 4600), (7007, 4, 6200)],
        );
        let tail = returned.picture(ProjectFrame(4)).unwrap();
        let point = source_fields(&tail.picture).0;
        assert_eq!(
            source_index
                .select(point, EndpointPolicy::Reject)
                .unwrap()
                .identity,
            SourceFrameId(5)
        );
        assert_eq!(
            tail.picture
                .select_source_frame(&source_index)
                .unwrap()
                .identity,
            SourceFrameId(4)
        );
    }
}

#[test]
fn partition_slip_preserves_full_owner_context_and_retained_camera_domain() {
    for origin in [-10010, 13013] {
        let before = fixture(origin, true);
        let baseline = RenderPlan::compile(&before).unwrap();
        let source_index = vfr_index(origin);
        indexed_oracle(
            &baseline,
            &source_index,
            origin,
            (0, 11011),
            [(5005, 3, 4600), (7007, 5, 7007)],
        );
        for (requested, selection, pictures) in [
            (-100, (0, 7007), [(1001, 0, 0), (3003, 1, 1400)]),
            (100, (4004, 12012), [(9009, 6, 8200), (11011, 6, 8200)]),
        ] {
            let after = slipped(&before, "crop", requested);
            let plan = RenderPlan::compile(&after).unwrap();
            fixed_delivery_and_cameras(&baseline, &plan);
            indexed_oracle(&plan, &source_index, origin, selection, pictures);
            for (frame, local, source_scale, group_scale) in [
                (3, ratio(5, 2), ratio(5, 4), ratio(3, 2)),
                (4, ratio(7, 2), ratio(7, 4), ratio(5, 2)),
            ] {
                let sample = plan.picture(ProjectFrame(frame)).unwrap();
                assert_eq!(sample.local_position, local);
                assert_eq!(sample.framing[0].instance.node, node_id("source"));
                assert_eq!(sample.framing[0].local_position, local);
                assert_eq!(sample.framing[0].duration, duration(6));
                assert_eq!(sample.framing[0].pose.unwrap().scale, source_scale);
                assert_eq!(sample.framing[1].instance.node, node_id("crop"));
                assert_eq!(sample.framing[1].pose, None);
                assert_eq!(sample.framing[2].instance.node, node_id("group"));
                assert_eq!(sample.framing[2].pose.unwrap().scale, group_scale);
            }
            // The picture retains W intersected with full context, wider than
            // this allocation's selected two-frame source interval.
            let picture = plan.picture(ProjectFrame(3)).unwrap().picture;
            let selected = source_fields(&picture).2;
            let visible_start = if requested < 0 { 0 } else { 8008 };
            let visible_end = if requested < 0 { 4004 } else { 12012 };
            assert!(
                selected
                    .start()
                    .ticks
                    .compare_integer(origin + visible_start)
                    .is_lt()
                    || selected
                        .end()
                        .ticks
                        .compare_integer(origin + visible_end)
                        .is_gt()
            );
        }
    }
}

#[test]
fn indexed_slip_and_source_anchor_inverse_agree_without_rounding_vfr_pts() {
    let origin = -10010;
    let before = fixture(origin, false);
    let after = slipped(&before, "source", 1);
    let target = |offset| {
        source_target(
            SourceTimestamp {
                ticks: origin + offset,
                time_base: clock(),
            },
            InsertionBias::Right,
            vec![],
        )
    };
    // PTS 6200 was 1098/1001 frames into the Source; after +1 it is
    // 97/1001. The unchanged three-frame lead remains in both global values.
    let old = AnchorIndex::new(&before)
        .unwrap()
        .resolve_target(&target(6200))
        .unwrap();
    let new = AnchorIndex::new(&after)
        .unwrap()
        .resolve_target(&target(6200))
        .unwrap();
    assert_eq!(old.exact_frame, ratio(4101, 1001));
    assert_eq!(new.exact_frame, ratio(3100, 1001));
    assert_eq!(old.frame, ProjectFrame(4));
    assert_eq!(new.frame, ProjectFrame(3));
    let early = target(4600);
    assert_eq!(
        AnchorIndex::new(&before)
            .unwrap()
            .resolve_target(&early)
            .unwrap()
            .exact_frame,
        ratio(3301, 1001)
    );
    assert_eq!(
        AnchorIndex::new(&after)
            .unwrap()
            .resolve_target(&early)
            .unwrap_err()
            .code,
        AnchorErrorCode::OutsideMapping
    );
    let source_index = vfr_index(origin);
    let plan = RenderPlan::compile(&after).unwrap();
    // Frame-center sampling independently lands on Original frame 5 (PTS7007),
    // while the retained PTS6200 anchor stays fractionally before that center.
    indexed_oracle(
        &plan,
        &source_index,
        origin,
        (6006, 9009),
        [(7007, 5, 7007), (9009, 6, 8200)],
    );
}
