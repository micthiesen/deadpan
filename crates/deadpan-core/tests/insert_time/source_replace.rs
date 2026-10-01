use super::*;

fn range(a: i64, b: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(a), ProjectFrame(b)).unwrap()
}

fn replace(before: &ProjectDocument, parent: &str, a: i64, b: i64, frames: i64) -> CommandRequest {
    let NodeKind::Source { source } = source(frames).kind else {
        unreachable!()
    };
    request(
        before,
        "replace",
        Command::ReplaceSource {
            parent: id(parent),
            range: range(a, b),
            source,
            id: id("replacement"),
            label: "Replacement".into(),
            identities: pool("replace", 12),
            timing: AudioTimingId {
                allocation: revision("replace"),
                ordinal: 0,
            },
        },
    )
}

fn sequence<'a>(document: &'a ProjectDocument, owner: &str) -> &'a [NodeId] {
    let NodeKind::Sequence { children } = &document.nodes()[&id(owner)].kind else {
        panic!()
    };
    children
}

#[test]
fn same_child_replacement_splits_twice_and_undoes_once() {
    for frames in [1, 2, 4] {
        let original = tree(&["whole"], vec![("whole", source(6))]);
        let query = original
            .source_replacement(&id("root"), range(2, 4))
            .unwrap();
        assert_eq!(
            (query.start_index, query.end_index, query.required_ids),
            (0, 1, 5)
        );
        let mut request = replace(&original, "root", 2, 4, frames);
        let Command::ReplaceSource { identities, .. } = &mut request.command else {
            panic!()
        };
        *identities = pool("replace", query.required_ids);
        let after = edit(&original, request);
        assert_eq!(after.duration().unwrap(), duration(4 + frames));
        assert_eq!(sequence(&after, "root").len(), 3);
        assert_eq!(sequence(&after, "root")[1], id("replacement"));
        assert_eq!(
            after.node_duration(&sequence(&after, "root")[0]).unwrap(),
            duration(2)
        );
        assert_eq!(
            after.node_duration(&sequence(&after, "root")[2]).unwrap(),
            duration(2)
        );
        let bindings = after.audio_bindings().bindings();
        assert!(!bindings.contains_key(&id("replacement")));
        assert_eq!(
            bindings
                .values()
                .filter(|binding| !binding.reanchors.is_empty())
                .count(),
            1
        );
    }
}

#[test]
fn whole_replacement_preserves_endpoint_empty_groups_and_removes_interior_groups() {
    let original = tree(
        &["left", "a", "inside", "b", "right"],
        vec![
            ("left", BeatNode::sequence("Left", vec![])),
            ("a", source(2)),
            ("inside", BeatNode::sequence("Inside", vec![])),
            ("b", source(2)),
            ("right", BeatNode::sequence("Right", vec![])),
        ],
    );
    let preflight = original
        .source_replacement(&id("root"), range(0, 4))
        .unwrap();
    assert_eq!(preflight.required_ids, 0);
    let after = edit(&original, replace(&original, "root", 0, 4, 1));
    assert_eq!(
        sequence(&after, "root"),
        [id("left"), id("replacement"), id("right")]
    );
    assert!(!after.nodes().contains_key(&id("inside")));
    assert_eq!(after.nodes().len(), 4);
    assert!(after.audio_bindings().is_empty());
}

#[test]
fn nested_replacement_removes_whole_composites_and_reanchors_outer_suffix_once() {
    let original = tree(
        &["prefix", "group", "tail"],
        vec![
            ("prefix", source(1)),
            (
                "group",
                BeatNode::sequence("Group", vec![id("a"), id("composite"), id("b")]),
            ),
            ("a", source(3)),
            (
                "composite",
                BeatNode::sequence("Middle", vec![id("middle")]),
            ),
            ("middle", BeatNode::hold("Held", recipe(2))),
            ("b", source(3)),
            ("tail", source(2)),
        ],
    );
    assert_eq!(
        original
            .source_replacement(&id("group"), range(2, 8))
            .unwrap()
            .required_ids,
        6
    );
    let after = edit(&original, replace(&original, "group", 2, 8, 4));
    assert_eq!(sequence(&after, "root"), sequence(&original, "root"));
    assert_eq!(after.duration().unwrap(), duration(9));
    assert!(!after.nodes().contains_key(&id("composite")));
    assert!(!after.nodes().contains_key(&id("middle")));
    assert_eq!(
        after.audio_bindings().bindings()[&id("tail")]
            .reanchors
            .len(),
        1
    );
    assert!(
        after.audio_bindings().bindings()[&id("prefix")]
            .reanchors
            .is_empty()
    );
}

#[test]
fn replacement_keeps_pinned_marks_valid_against_final_extent() {
    let mut wire = json!(tree(&["whole"], vec![("whole", source(6))]));
    wire["marks"] = json!(BTreeMap::from([
        ("pinned", mark(5, InsertionBias::Right, true)),
        ("suffix", mark(5, InsertionBias::Right, false)),
    ]));
    let original = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let after = edit(&original, replace(&original, "root", 1, 5, 5));
    assert_eq!(mark_position(&after, "pinned"), ExactRatio::integer(5));
    assert_eq!(mark_position(&after, "suffix"), ExactRatio::integer(6));
}

#[test]
fn replacement_admission_and_identity_failures_are_atomic() {
    let original = tree(
        &["group", "tail"],
        vec![
            ("group", BeatNode::sequence("Group", vec![id("a")])),
            ("a", source(6)),
            ("tail", source(2)),
        ],
    );
    for (parent, a, b) in [
        ("root", 1, 5),
        ("group", 0, 7),
        ("group", 3, 3),
        ("a", 1, 3),
        ("missing", 0, 1),
    ] {
        assert!(
            original
                .source_replacement(&id(parent), range(a, b))
                .is_err()
        );
        assert!(apply(&original, &replace(&original, parent, a, b, 2)).is_err());
    }
    for mode in 0..5 {
        let mut request = replace(&original, "group", 1, 5, 2);
        let Command::ReplaceSource {
            identities,
            id,
            timing,
            source,
            ..
        } = &mut request.command
        else {
            panic!()
        };
        match mode {
            0 => identities.nodes.truncate(4),
            1 => *id = super::id("a"),
            2 => identities.nodes[0] = id.clone(),
            3 => timing.ordinal = u32::MAX,
            4 => source.duration = FrameDuration::ZERO,
            _ => unreachable!(),
        }
        assert!(apply(&original, &request).is_err(), "mode {mode}");
        assert_eq!(original.revision_id(), &revision("initial"));
    }
}

#[test]
fn second_replacement_refines_existing_fragments_with_exact_identity_budget() {
    let original = tree(&["whole"], vec![("whole", source(10))]);
    let first = edit(&original, replace(&original, "root", 1, 3, 1));
    let query = first.source_replacement(&id("root"), range(4, 6)).unwrap();
    assert_eq!(query.required_ids, 4);
    let mut request = replace(&first, "root", 4, 6, 2);
    request.new_revision = revision("second");
    let Command::ReplaceSource {
        id,
        identities,
        timing,
        ..
    } = &mut request.command
    else {
        panic!()
    };
    *id = super::id("second");
    *identities = pool("second", query.required_ids);
    timing.allocation = revision("second");
    let after = edit(&first, request);
    assert_eq!(after.duration().unwrap(), first.duration().unwrap());
    assert_eq!(sequence(&after, "root").len(), 5);
}

#[test]
fn replacement_retains_source_framing_and_treated_fragment_clock_ownership() {
    let pose = |scale| {
        FramingPose::new(
            ExactRatio::new(1, 2).unwrap(),
            ExactRatio::new(1, 2).unwrap(),
            ExactRatio::integer(scale),
        )
        .unwrap()
    };
    let framing = Framing::creep(pose(1), pose(2), FramingCurve::Linear).unwrap();
    let mut leaf = source(6);
    leaf.framing = Some(framing.clone());
    let original = tree(&["whole"], vec![("whole", leaf)]);
    let replaced = edit(&original, replace(&original, "root", 2, 4, 1));
    for child in [
        sequence(&replaced, "root")[0].clone(),
        sequence(&replaced, "root")[2].clone(),
    ] {
        assert_eq!(
            replaced.nodes()[&owner(&replaced, &child)],
            original.nodes()[&id("whole")]
        );
        assert!(replaced.nodes()[&child].framing.is_none());
    }
    let split = split(&original, "first-cut", id("whole"), 1);
    let treated = sequence(&split, "root")[1].clone();
    let mut wire = json!(&split);
    wire["nodes"][treated.as_str()]["label"] = json!("Treated fragment");
    wire["nodes"][treated.as_str()]["framing"] = json!(framing);
    let treatments = AudioTreatments::from_clip_gain(ClipGain::default());
    wire["nodes"][treated.as_str()]["audio_treatments"] = json!(&treatments);
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_eq!(
        before
            .source_replacement(&id("root"), range(2, 4))
            .unwrap()
            .required_ids,
        7
    );
    let after = edit(&before, replace(&before, "root", 2, 4, 1));
    assert_eq!(after.nodes()[&treated], before.nodes()[&treated]);
    let retained: Vec<_> = after
        .nodes()
        .values()
        .filter(|node| node.label == "Treated fragment" && !node.audio_treatments.is_empty())
        .collect();
    assert_eq!(retained.len(), 2);
    for node in retained {
        assert_eq!(node.framing, before.nodes()[&treated].framing);
        assert_eq!(node.audio_treatments, treatments);
        let NodeKind::Retime {
            mapping, duration, ..
        } = node.kind
        else {
            panic!()
        };
        assert_eq!(mapping, range(1, 6));
        assert_eq!(duration, super::duration(5));
    }
}

#[test]
fn replacement_rejects_repeat_and_retime_ancestors_without_descending() {
    for kind in [
        NodeKind::Repeat {
            child: id("group"),
            iterations: IterationOrder::new(revision("plays"), 2).unwrap(),
            gap: None,
        },
        NodeKind::Retime {
            child: id("group"),
            duration: duration(6),
            mapping: range(0, 6),
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Edit,
        },
    ] {
        let original = tree(
            &["container"],
            vec![
                (
                    "container",
                    BeatNode {
                        label: "Container".into(),
                        framing: None,
                        audio_treatments: Default::default(),
                        audio_editorial_edges: Default::default(),
                        audio_edges: Default::default(),
                        kind,
                    },
                ),
                ("group", BeatNode::sequence("Group", vec![id("leaf")])),
                ("leaf", source(6)),
            ],
        );
        for parent in ["group", "root"] {
            assert!(
                original
                    .source_replacement(&id(parent), range(1, 4))
                    .is_err()
            );
            assert!(apply(&original, &replace(&original, parent, 1, 4, 2)).is_err());
        }
        // A complete composite is an ordinary direct-child selection at root.
        let total = original.duration().unwrap().frames();
        let after = edit(&original, replace(&original, "root", 0, total, 2));
        assert_eq!(after.nodes().len(), 2);
        assert_eq!(after.duration().unwrap(), duration(2));
    }
}

#[test]
fn replacement_checks_combined_temporary_node_budget_before_any_split() {
    let original = tree(&["whole"], vec![("whole", source(6))]);
    let mut wire = json!(original);
    for index in 2..MAX_DOCUMENT_NODES - 5 {
        let name = format!("empty-{index}");
        wire["nodes"][&name] = json!(BeatNode::sequence("", vec![]));
        wire["nodes"]["root"]["kind"]["children"]
            .as_array_mut()
            .unwrap()
            .push(json!(name));
    }
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_eq!(before.nodes().len(), MAX_DOCUMENT_NODES - 5);
    // Five Split IDs and one Source must coexist before the removed middle
    // context is pruned. Its eventual smaller size cannot waive this bound.
    assert_eq!(
        before
            .source_replacement(&id("root"), range(1, 3))
            .unwrap_err()
            .code,
        EditErrorCode::LimitExceeded
    );
    assert_eq!(
        apply(&before, &replace(&before, "root", 1, 3, 1))
            .unwrap_err()
            .code,
        EditErrorCode::LimitExceeded
    );
    assert_eq!(before.revision_id(), &revision("initial"));
}
