use super::*;

fn fixture() -> ProjectDocument {
    tree(
        &["prefix", "outer", "repeat"],
        vec![
            ("prefix", source(1)),
            (
                "outer",
                BeatNode::sequence("Outer", vec![id("inner"), id("outer-tail")]),
            ),
            ("inner", BeatNode::sequence("Inner", vec![id("a"), id("b")])),
            ("a", source(2)),
            ("b", source(2)),
            ("outer-tail", source(1)),
            (
                "repeat",
                BeatNode {
                    audio_treatments: Default::default(),
                    label: "Compact suffix".into(),
                    framing: None,
                    audio_editorial_edges: Default::default(),
                    audio_edges: Default::default(),
                    kind: NodeKind::Repeat {
                        child: id("repeated"),
                        iterations: IterationOrder::new(revision("plays"), 1_000_000_000).unwrap(),
                        gap: Some(recipe(1)),
                        escalation: None,
                    },
                    cutaways: Vec::new(),
                    captions: Vec::new(),
                },
            ),
            ("repeated", source(1)),
        ],
    )
}

fn sequence<'a>(document: &'a ProjectDocument, key: &str) -> &'a [NodeId] {
    let NodeKind::Sequence { children } = &document.nodes()[&id(key)].kind else {
        panic!()
    };
    children
}

#[test]
fn preflight_finds_live_sequence_owner_and_exact_split_identity_count() {
    let before = fixture();
    for (at, parent, slot, split) in [
        (0, "root", 0, None),
        (1, "root", 1, None),
        (2, "inner", 0, Some(("a", 1))),
        (3, "inner", 1, None),
        (5, "outer", 1, None),
        (6, "root", 2, None),
    ] {
        let target = before.insert_time_target(ProjectFrame(at)).unwrap();
        assert_eq!(target.parent, id(parent));
        assert_eq!(target.index, slot);
        assert_eq!(
            target.split,
            split.map(|(node, at)| InsertTimeSplit {
                target: id(node),
                at: duration(at),
                required_ids: 3,
            })
        );
    }
    assert!(before.insert_time_target(ProjectFrame(-1)).is_err());
    assert!(
        before
            .insert_time_target(ProjectFrame(before.duration().unwrap().frames() + 1))
            .is_err()
    );
    // An interior Repeat is still outside this admission, even at a play seam.
    assert!(before.insert_time_target(ProjectFrame(7)).is_err());
}

#[test]
fn nested_pause_keeps_group_ownership_and_reanchors_every_ancestor_suffix_once() {
    let before = fixture();
    let after = edit(&before, insertion(&before, "nested", 2, 1));
    assert_eq!(children(&after), children(&before));
    assert_eq!(sequence(&after, "outer"), sequence(&before, "outer"));
    assert_eq!(sequence(&after, "inner")[1], id("pause-nested"));
    assert_eq!(after.node_duration(&id("inner")).unwrap(), duration(5));
    assert_eq!(after.node_duration(&id("outer")).unwrap(), duration(6));
    assert_eq!(after.nodes().len(), before.nodes().len() + 4);
    let right = owner(&after, &sequence(&after, "inner")[2]);
    for key in [&right, &id("b"), &id("outer-tail"), &id("repeated")] {
        assert_eq!(
            after.audio_bindings().bindings()[key].reanchors.len(),
            1,
            "{key}"
        );
    }
    assert!(
        after.audio_bindings().bindings()[&id("prefix")]
            .reanchors
            .is_empty()
    );
    assert!(
        after.audio_bindings().bindings()[&id("a")]
            .reanchors
            .is_empty()
    );
    assert_eq!(
        after.audio_bindings().gap_bindings()[&id("repeat")]
            .reanchors
            .len(),
        1
    );
    assert_eq!(after.nodes()[&id("repeat")], before.nodes()[&id("repeat")]);
    assert_eq!(after.audio_bindings().timings().len(), 2);
    // A second pause at the new inner seam consumes no split identities.
    let mut again = insertion(&after, "again", 3, 1);
    let Command::InsertTime { identities, .. } = &mut again.command else {
        panic!()
    };
    identities.nodes.clear();
    let again = edit(&after, again);
    assert_eq!(sequence(&again, "inner")[2], id("pause-again"));
    assert_eq!(again.audio_bindings().bindings()[&right].reanchors.len(), 2);
    assert_eq!(again.nodes().len(), after.nodes().len() + 1);
}

#[test]
fn nested_seam_keeps_empty_children_and_boundary_marks_in_their_own_clocks() {
    let original = fixture();
    let mut wire = serde_json::to_value(&original).unwrap();
    wire["nodes"]["empty"] = serde_json::to_value(BeatNode::sequence("Empty", vec![])).unwrap();
    wire["nodes"]["inner"]["kind"]["children"] = json!(["a", "empty", "b"]);
    wire["marks"] = serde_json::to_value(BTreeMap::from([
        ("left", mark(3, InsertionBias::Left, false)),
        ("right", mark(3, InsertionBias::Right, false)),
        ("outer-tail", mark(5, InsertionBias::Right, false)),
        ("pin", mark(5, InsertionBias::Right, true)),
    ]))
    .unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let after = edit(&before, insertion(&before, "nested-seam", 3, 1));
    assert_eq!(
        sequence(&after, "inner"),
        &[id("a"), id("pause-nested-seam"), id("empty"), id("b")]
    );
    assert_eq!(after.nodes().len(), before.nodes().len() + 1);
    for (key, expected) in [("left", 3), ("right", 4), ("outer-tail", 6), ("pin", 5)] {
        assert_eq!(
            mark_position(&after, key),
            ExactRatio::integer(expected),
            "{key}"
        );
    }
}

#[test]
fn nested_insertion_failures_do_not_publish_intermediate_copies_or_clocks() {
    let before = fixture();
    let bytes = before.to_json().unwrap();
    for mode in ["pool", "timing", "collision"] {
        let mut request = insertion(&before, mode, 2, 1);
        let Command::InsertTime {
            identities,
            timing,
            id: hold,
            ..
        } = &mut request.command
        else {
            panic!()
        };
        match mode {
            "pool" => identities.nodes.truncate(2),
            "timing" => timing.ordinal = u32::MAX,
            "collision" => *hold = id("inner"),
            _ => unreachable!(),
        }
        assert!(apply(&before, &request).is_err());
        assert_eq!(before.to_json().unwrap(), bytes);
    }
    let retimed = tree(
        &["retime"],
        vec![
            (
                "retime",
                BeatNode {
                    audio_treatments: Default::default(),
                    label: "Retime".into(),
                    framing: None,
                    audio_editorial_edges: Default::default(),
                    audio_edges: Default::default(),
                    kind: NodeKind::Retime {
                        child: id("group"),
                        duration: duration(2),
                        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(3)).unwrap(),
                        pitch: PitchPolicy::Preserve,
                        purpose: RetimePurpose::Edit,
                    },
                    cutaways: Vec::new(),
                    captions: Vec::new(),
                },
            ),
            ("group", BeatNode::sequence("Group", vec![id("a")])),
            ("a", source(3)),
        ],
    );
    assert!(retimed.insert_time_target(ProjectFrame(1)).is_err());
    assert!(apply(&retimed, &insertion(&retimed, "unsupported", 1, 1)).is_err());
}
