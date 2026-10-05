//! Synthetic indexed-picture and framing witnesses, not decoded/GPU output.
use super::*;

fn fixture() -> ProjectDocument {
    let repeated = BeatNode {
        label: "repeat".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: id("repeated-source"),
            iterations: IterationOrder::new(revision("original-plays"), 2).unwrap(),
            gap: Some(freeze_recipe(1, 4004)),
            escalation: None,
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    };
    let before = document(
        &[
            "lead",
            "empty-left",
            "notes",
            "empty-right",
            "scope",
            "repeat",
            "tail",
        ],
        vec![
            ("lead", creep(source(2, -2002, 0), 3)),
            ("empty-left", BeatNode::sequence("", vec![])),
            (
                "notes",
                creep(BeatNode::sequence("", vec![id("nested-notes")]), 5),
            ),
            ("nested-notes", BeatNode::sequence("", vec![])),
            ("empty-right", BeatNode::sequence("", vec![])),
            (
                "scope",
                creep(BeatNode::sequence("", vec![id("voice"), id("hold")]), 2),
            ),
            ("voice", creep(source(3, 1001, 4004), 4)),
            ("hold", creep(freeze(2, 4004), 3)),
            ("repeat", creep(repeated, 3)),
            ("repeated-source", creep(source(2, 6006, 8008), 2)),
            ("tail", creep(source(2, 12012, 14014), 2)),
        ],
    );
    let mut wire = serde_json::to_value(&before).unwrap();
    wire["nodes"]["root"] =
        serde_json::to_value(creep(before.nodes()[before.root()].clone(), 2)).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn source_index() -> SourceFrameIndex {
    SourceFrameIndex::new(
        asset(),
        clock(),
        (-2..=14)
            .enumerate()
            .map(|(ordinal, position)| IndexedSourceFrame {
                identity: SourceFrameId(u64::try_from(ordinal).unwrap()),
                pts: position * 1001,
                reported_duration: None,
                keyframe: ordinal == 0,
                seek_from: Some(SourceFrameId(0)),
                decode_timestamp: None,
            })
            .collect(),
        15_015,
        TerminalProvenance::Explicit,
    )
    .unwrap()
}

fn expected_picture(frame: i64) -> (&'static str, ExactRatio, i64, i64, ExactRatio, u64) {
    // Provider, local center, provider length, final scale, source PTS, ordinal.
    match frame {
        0..=1 => (
            "lead",
            ratio(i128::from(2 * frame + 1), 2),
            2,
            3,
            ratio(i128::from(-3003 + 2002 * frame), 2),
            u64::try_from(frame).unwrap(),
        ),
        2..=4 => (
            "voice",
            ratio(i128::from(2 * frame - 3), 2),
            3,
            4,
            ratio(i128::from(2002 * frame - 1001), 2),
            u64::try_from(frame + 1).unwrap(),
        ),
        5..=6 => (
            "hold",
            ratio(i128::from(2 * frame - 9), 2),
            2,
            3,
            ExactRatio::integer(4004),
            6,
        ),
        7..=8 | 10..=11 => {
            let local = if frame < 9 { frame - 7 } else { frame - 10 };
            (
                "repeated-source",
                ratio(i128::from(2 * local + 1), 2),
                2,
                2,
                ratio(i128::from(13013 + 2002 * local), 2),
                u64::try_from(8 + local).unwrap(),
            )
        }
        9 => ("repeat", ratio(1, 2), 1, 3, ExactRatio::integer(4004), 6),
        12..=13 => (
            "tail",
            ratio(i128::from(2 * frame - 23), 2),
            2,
            2,
            ratio(i128::from(25025 + 2002 * (frame - 12)), 2),
            u64::try_from(frame + 2).unwrap(),
        ),
        _ => unreachable!(),
    }
}

#[test]
fn empty_child_slots_preserve_every_indexed_picture_and_complete_framing_clock() {
    let before = fixture();
    let selection = SliceCaptureSelection::Child { node: id("notes") };
    let copied = CapturedEditSlice::capture_selection(
        &before,
        before.root(),
        &selection,
        AudioTimingId {
            allocation: revision("empty-picture-capture"),
            ordinal: u32::MAX,
        },
    )
    .unwrap();
    assert_eq!(copied.selection(), &selection);
    assert_eq!(copied.range(), range(2, 2));
    let required = copied.identity_requirements().unwrap();
    assert_eq!(required.timings, 0);
    let index = source_index();
    let original_plan = RenderPlan::compile(&before).unwrap();
    assert_eq!(original_plan.duration(), duration(14));
    let NodeKind::Sequence { children } = &before.nodes()[before.root()].kind else {
        unreachable!()
    };
    for slot in 0..=children.len() {
        let name = format!("empty-picture-slot-{slot}");
        let fresh: Vec<_> = (0..required.nodes)
            .map(|n| id(&format!("{name}-{n}")))
            .collect();
        let wrapper = fresh[0].clone();
        let transaction = apply(
            &before,
            &CommandRequest {
                project_id: before.project_id().clone(),
                expected_revision: before.revision_id().clone(),
                new_revision: revision(&name),
                command: Command::SpliceSlice {
                    parent: before.root().clone(),
                    index: slot,
                    slice: copied.clone(),
                    identities: SlicePasteIdentities {
                        authored: OccurrenceIdentities {
                            nodes: fresh,
                            marks: vec![],
                        },
                        aliases: (0..required.aliases)
                            .map(|n| id(&format!("{name}-alias-{n}")))
                            .collect(),
                    },
                    timing: AudioTimingId {
                        allocation: revision(&name),
                        ordinal: u32::MAX,
                    },
                },
            },
        )
        .unwrap();
        let after = transaction.forward.apply(&before).unwrap();
        let NodeKind::Sequence { children: inserted } = &after.nodes()[after.root()].kind else {
            unreachable!()
        };
        let mut expected_children = children.clone();
        expected_children.insert(slot, wrapper.clone());
        assert_eq!(*inserted, expected_children);
        let NodeKind::Sequence { children: owned } = &after.nodes()[&wrapper].kind else {
            unreachable!()
        };
        assert_eq!(owned.len(), 1);
        assert_eq!(after.nodes()[&owned[0]].label, "notes");
        assert_eq!(
            after.nodes()[&owned[0]].framing,
            before.nodes()[&id("notes")].framing
        );
        // All four slots around the three empty siblings share frame 2;
        // retaining the explicit slot is what keeps each insertion distinct.
        if (1..=4).contains(&slot) {
            let before_frames: i64 = inserted[..slot]
                .iter()
                .map(|child| after.node_duration(child).unwrap().frames())
                .sum();
            assert_eq!(before_frames, 2);
        }
        assert_eq!(after.node_duration(&wrapper).unwrap(), duration(0));
        let plan = RenderPlan::compile(&after).unwrap();
        assert_eq!(plan.duration(), original_plan.duration());
        for frame in (0..14).rev() {
            let actual = plan.picture(ProjectFrame(frame)).unwrap();
            let old = original_plan.picture(ProjectFrame(frame)).unwrap();
            assert_eq!(actual.picture, old.picture);
            assert_eq!(actual.picture_context, old.picture_context);
            assert_eq!(actual.instance, old.instance);
            assert_eq!(actual.gap_after, old.gap_after);
            assert_eq!(actual.local_position, old.local_position);
            assert_eq!(actual.framing, old.framing);
            let (provider, local, length, scale, pts, ordinal) = expected_picture(frame);
            assert_eq!(actual.instance.node, id(provider));
            assert_eq!(actual.local_position, local);
            assert_eq!(picture_ticks(&actual), pts);
            let selected = actual.picture.select_source_frame(&index).unwrap();
            assert_eq!(selected.identity, SourceFrameId(ordinal));
            assert_eq!(selected.pts, (i64::try_from(ordinal).unwrap() - 2) * 1001);
            if frame != 9 {
                let layer = &actual.framing[0];
                assert_eq!(layer.local_position, local);
                assert_eq!(layer.duration, duration(length));
                assert_eq!(
                    layer.pose.unwrap().scale,
                    linear_scale_at(local, length, scale)
                );
            }
            for (owner, start, end_scale) in [("scope", 2, 2), ("repeat", 7, 3)] {
                if (start..start + 5).contains(&frame) {
                    let layer = actual
                        .framing
                        .iter()
                        .find(|layer| layer.instance.node == id(owner))
                        .unwrap();
                    let local = ratio(i128::from(2 * (frame - start) + 1), 2);
                    assert_eq!(layer.local_position, local);
                    assert_eq!(layer.duration, duration(5));
                    assert_eq!(
                        layer.pose.unwrap().scale,
                        linear_scale_at(local, 5, end_scale)
                    );
                }
            }
            let root = actual.framing.last().unwrap();
            let root_local = ratio(i128::from(2 * frame + 1), 2);
            assert_eq!(root.local_position, root_local);
            assert_eq!(root.duration, duration(14));
            assert_eq!(root.pose.unwrap().scale, linear_scale_at(root_local, 14, 2));
        }
        assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
    }
}
