//! Exact indexed pictures and retained camera ownership through ripple edge changes.
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

fn trimmed(
    before: &ProjectDocument,
    target: &str,
    edge: SourceTrimEdge,
    delta: i64,
) -> ProjectDocument {
    let result = before
        .source_trim(
            &node_id("group"),
            &node_id(target),
            edge,
            delta,
            SourceTrimMode::Ripple,
        )
        .unwrap();
    let revision = revision(&format!("{}-trim-{delta}", before.revision_id()));
    let transaction = apply(
        before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision.clone(),
            command: Command::TrimSource {
                parent: node_id("group"),
                node: node_id(target),
                edge,
                delta_frames: delta,
                mode: SourceTrimMode::Ripple,
                wrapper: result.needs_wrapper.then(|| node_id("trim-crop")),
                timing: AudioTimingId {
                    allocation: revision,
                    ordinal: 0,
                },
            },
        },
    )
    .unwrap();
    let after = transaction.forward.apply(before).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *before);
    after
}

fn assert_pictures(
    plan: &RenderPlan,
    index: &SourceFrameIndex,
    origin: i64,
    selection: (i64, i64),
    expected: &[(i64, u64, i64)],
) {
    assert_eq!(
        plan.duration(),
        duration(5 + i64::try_from(expected.len()).unwrap())
    );
    for (offset, (ticks, ordinal, pts)) in expected.iter().enumerate() {
        let frame = ProjectFrame(3 + i64::try_from(offset).unwrap());
        let sample = plan.picture(frame).unwrap();
        let (point, context, selected) = source_fields(&sample.picture);
        assert_eq!(point.ticks, ExactRatio::integer(origin + ticks));
        assert_eq!(context, span(origin, origin + 12012));
        assert_eq!(
            selected,
            exact_span(
                ExactRatio::integer(origin + selection.0),
                ExactRatio::integer(origin + selection.1)
            )
        );
        let picture = sample.picture.select_source_frame(index).unwrap();
        assert_eq!(picture.identity, SourceFrameId(*ordinal));
        assert_eq!(picture.pts, origin + pts);
    }
}

#[test]
fn ripple_trim_signed_vfr_edges_keep_fractional_terminal_selection_and_clamp_handles() {
    for origin in [-10010, 13013] {
        let before = fixture(origin, false);
        let index = vfr_index(origin);
        for (edge, delta, selection, expected) in [
            (SourceTrimEdge::In, 1, (4004, 7007), vec![(7007, 4, 6200)]),
            (SourceTrimEdge::Out, -1, (4004, 5005), vec![(5005, 3, 4600)]),
            (
                SourceTrimEdge::In,
                -2,
                (0, 7007),
                vec![
                    (1001, 0, 0),
                    (3003, 1, 1400),
                    (5005, 3, 4600),
                    (7007, 4, 6200),
                ],
            ),
            (
                SourceTrimEdge::In,
                -100,
                (0, 7007),
                vec![
                    (1001, 0, 0),
                    (3003, 1, 1400),
                    (5005, 3, 4600),
                    (7007, 4, 6200),
                ],
            ),
            (
                SourceTrimEdge::Out,
                2,
                (4004, 11011),
                vec![
                    (5005, 3, 4600),
                    (7007, 5, 7007),
                    (9009, 6, 8200),
                    (11011, 6, 8200),
                ],
            ),
            (
                SourceTrimEdge::Out,
                100,
                (4004, 11011),
                vec![
                    (5005, 3, 4600),
                    (7007, 5, 7007),
                    (9009, 6, 8200),
                    (11011, 6, 8200),
                ],
            ),
        ] {
            let after = trimmed(&before, "source", edge, delta);
            let plan = RenderPlan::compile(&after).unwrap();
            assert_pictures(&plan, &index, origin, selection, &expected);
            for frame in [
                0,
                1,
                2,
                3 + i64::try_from(expected.len()).unwrap(),
                4 + i64::try_from(expected.len()).unwrap(),
            ] {
                assert!(matches!(
                    plan.picture(ProjectFrame(frame)).unwrap().picture,
                    Picture::Background
                ));
            }
            assert!(
                plan.picture(ProjectFrame(plan.duration().frames()))
                    .is_err()
            );
        }
    }
}

#[test]
fn ripple_trim_grown_camera_holds_endpoints_while_ancestor_uses_its_new_duration() {
    let before = fixture(-10010, false);
    for (edge, delta, source_scales) in [
        (
            SourceTrimEdge::In,
            -2,
            [
                ExactRatio::ONE,
                ExactRatio::ONE,
                ExactRatio::ONE,
                ratio(3, 2),
            ],
        ),
        (
            SourceTrimEdge::Out,
            2,
            [
                ExactRatio::ONE,
                ratio(3, 2),
                ExactRatio::integer(2),
                ExactRatio::integer(2),
            ],
        ),
    ] {
        let after = trimmed(&before, "source", edge, delta);
        let plan = RenderPlan::compile(&after).unwrap();
        for (i, source_scale) in source_scales.into_iter().enumerate() {
            let sample = plan
                .picture(ProjectFrame(3 + i64::try_from(i).unwrap()))
                .unwrap();
            assert_eq!(sample.framing[0].instance.node, node_id("source"));
            assert_eq!(sample.framing[0].duration, duration(4));
            assert_eq!(sample.framing[0].pose.unwrap().scale, source_scale);
            let group = &sample.framing[1];
            assert_eq!(group.instance.node, node_id("group"));
            assert_eq!(group.duration, duration(4));
            assert_eq!(
                group.pose.unwrap().scale,
                ratio(5 + 2 * i128::try_from(i).unwrap(), 4)
            );
        }
    }
}

#[test]
fn ripple_trim_partition_crop_retains_hidden_selection_and_source_camera() {
    let origin = 13013;
    let before = fixture(origin, true);
    let after = trimmed(&before, "crop", SourceTrimEdge::In, 1);
    let plan = RenderPlan::compile(&after).unwrap();
    assert_pictures(
        &plan,
        &vfr_index(origin),
        origin,
        (0, 11011),
        &[(7007, 5, 7007)],
    );
    let sample = plan.picture(ProjectFrame(3)).unwrap();
    assert_eq!(sample.local_position, ratio(7, 2));
    assert_eq!(sample.framing[0].duration, duration(6));
    assert_eq!(sample.framing[0].pose.unwrap().scale, ratio(7, 4));
    assert_eq!(sample.framing[1].instance.node, node_id("crop"));
    assert_eq!(sample.framing[1].pose, None);
    assert_eq!(sample.framing[2].instance.node, node_id("group"));
    assert_eq!(sample.framing[2].duration, duration(1));
    assert_eq!(
        sample.framing[2].pose.unwrap().scale,
        ExactRatio::integer(2)
    );
}
