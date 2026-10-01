//! Pure indexed-picture witnesses for physical context growth, not media/GPU evidence.
use super::*;

#[test]
fn source_context_growth_retains_poses_pts_and_live_ancestor_clocks_through_split() {
    let original_source = framed(source(10, 0, 10_010), 2);
    let original_recipe = original_source.framing.clone().unwrap();
    let before = document(
        &["group"],
        vec![
            (
                "group",
                framed(BeatNode::sequence("Group", vec![id("source")]), 3),
            ),
            ("source", original_source),
        ],
    );
    let mut extended_source = source(15, -3003, 12_012);
    let retained = original_recipe
        .prepend_owner_frames(duration(3), duration(10))
        .unwrap();
    extended_source.framing = Some(retained.clone());
    let extended = document(&["source"], vec![("source", extended_source.clone())]);
    let mut crop = retime("source", 10, 3, 13);
    let NodeKind::Retime { purpose, .. } = &mut crop.kind else {
        unreachable!()
    };
    *purpose = RetimePurpose::Partition;
    let grown = document(
        &["group"],
        vec![
            (
                "group",
                framed(BeatNode::sequence("Group", vec![id("crop")]), 3),
            ),
            ("crop", crop),
            ("source", extended_source),
        ],
    );
    let original_plan = RenderPlan::compile(&before).unwrap();
    let grown_plan = RenderPlan::compile(&grown).unwrap();
    let source_index = index(
        "video",
        clock(),
        &[
            -3003, -2002, -1001, 0, 1001, 2002, 3003, 4004, 5005, 6006, 7007, 8008, 9009, 10_010,
            11_011,
        ],
        12_012,
    );
    for frame in [9, 0, 4, 5, 1, 8, 2, 7, 3, 6] {
        let old = original_plan.picture(ProjectFrame(frame)).unwrap();
        let new = grown_plan.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(ticks(&new.picture), ticks(&old.picture));
        assert_eq!(
            new.picture.select_source_frame(&source_index).unwrap(),
            old.picture.select_source_frame(&source_index).unwrap()
        );
        assert_eq!(
            new.framing
                .iter()
                .filter_map(|layer| layer.pose)
                .collect::<Vec<_>>(),
            old.framing
                .iter()
                .filter_map(|layer| layer.pose)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            new.framing[0].local_position,
            old.framing[0]
                .local_position
                .checked_add(ExactRatio::integer(3))
                .unwrap()
        );
        assert_eq!(new.framing[0].duration, duration(15));
        assert_eq!(old.framing[0].duration, duration(10));
        assert_eq!(new.framing[2], old.framing[1]);
    }
    // Exposed handles hold the camera endpoints; the body still moves.
    let extended_plan = RenderPlan::compile(&extended).unwrap();
    for frame in [0, 1, 2] {
        assert_eq!(
            extended_plan.picture(ProjectFrame(frame)).unwrap().framing[0]
                .pose
                .unwrap(),
            FramingPose::identity()
        );
    }
    for frame in [13, 14] {
        assert_eq!(
            extended_plan.picture(ProjectFrame(frame)).unwrap().framing[0]
                .pose
                .unwrap()
                .scale,
            ExactRatio::integer(2)
        );
    }
    let transaction = apply(
        &grown,
        &CommandRequest {
            project_id: grown.project_id().clone(),
            expected_revision: grown.revision_id().clone(),
            new_revision: revision("split-retained-clock"),
            command: Command::Split {
                node: id("crop"),
                at: duration(4),
                identities: SplitIdentities {
                    nodes: (0..16)
                        .map(|n| id(&format!("retained-split-{n}")))
                        .collect(),
                },
            },
        },
    )
    .unwrap();
    let divided = transaction.forward.apply(&grown).unwrap();
    assert_eq!(transaction.inverse.apply(&divided).unwrap(), grown);
    let divided = ProjectDocument::from_json(&divided.to_json().unwrap()).unwrap();
    let divided_plan = RenderPlan::compile(&divided).unwrap();
    assert_eq!(
        divided
            .nodes()
            .values()
            .filter(|node| node.framing.as_ref() == Some(&retained))
            .count(),
        2
    );
    for frame in 0..10 {
        let old = grown_plan.picture(ProjectFrame(frame)).unwrap();
        let new = divided_plan.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(new.picture, old.picture);
        assert_eq!(
            new.framing
                .iter()
                .filter_map(|layer| layer.pose)
                .collect::<Vec<_>>(),
            old.framing
                .iter()
                .filter_map(|layer| layer.pose)
                .collect::<Vec<_>>()
        );
        assert_eq!(new.framing[0].local_position, old.framing[0].local_position);
        assert_eq!(new.framing[0].duration, old.framing[0].duration);
    }
}
