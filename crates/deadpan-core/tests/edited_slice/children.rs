use super::*;

fn capture(document: &ProjectDocument, parent: &str, first: &str, last: &str) -> CapturedEditSlice {
    CapturedEditSlice::capture_selection(
        document,
        &id(parent),
        &SliceCaptureSelection::Children {
            first: id(first),
            last: id(last),
        },
        timing("forest-capture"),
    )
    .unwrap()
}

fn roots(slice: &CapturedEditSlice) -> Vec<String> {
    serde_json::to_value(slice).unwrap()["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|part| part["root"].as_str().unwrap().to_owned())
        .collect()
}

fn siblings<'a>(document: &'a ProjectDocument, parent: &str) -> &'a [NodeId] {
    let NodeKind::Sequence { children } = &document.nodes()[&id(parent)].kind else {
        panic!("expected Sequence")
    };
    children
}

fn endpoint_tree() -> ProjectDocument {
    tree(
        &["prefix", "group", "suffix"],
        vec![
            ("prefix", hold(3)),
            (
                "group",
                BeatNode::sequence(
                    "Parent",
                    [
                        "outside-left",
                        "first",
                        "body",
                        "middle",
                        "tail",
                        "last",
                        "outside-right",
                    ]
                    .map(id)
                    .to_vec(),
                ),
            ),
            ("outside-left", BeatNode::sequence("Excluded left", vec![])),
            ("first", BeatNode::sequence("First empty", vec![])),
            ("body", hold(4)),
            ("middle", BeatNode::sequence("Interior empty", vec![])),
            ("tail", hold(2)),
            ("last", BeatNode::sequence("Last empty", vec![id("inner")])),
            ("inner", BeatNode::sequence("Nested empty", vec![])),
            (
                "outside-right",
                BeatNode::sequence("Excluded right", vec![]),
            ),
            ("suffix", hold(5)),
        ],
    )
}

#[test]
fn identity_span_includes_empty_endpoints_and_nested_structure_at_exact_slots() {
    let before = endpoint_tree();
    let snapshot = before.to_json().unwrap();
    assert_eq!(
        before
            .sequence_children(&id("group"), &id("first"), &id("last"))
            .unwrap(),
        SequenceChildrenPlan {
            first: 1,
            end: 6,
            range: range(3, 9)
        }
    );
    let forest = capture(&before, "group", "first", "last");
    assert_eq!(roots(&forest), ["first", "body", "middle", "tail", "last"]);
    assert_eq!(forest.range(), range(3, 9));
    assert_eq!(forest.duration(), duration(6));
    assert_eq!(forest.identity_requirements().unwrap().nodes, 7);
    let wire = serde_json::to_value(&forest).unwrap();
    assert_eq!(
        wire["selection"],
        json!({"type":"children", "first":"first", "last":"last"})
    );
    assert_eq!(
        wire["parts"],
        json!([
            {"root":"first", "mapping":range(0,0), "source_start":3},
            {"root":"body", "mapping":range(0,4), "source_start":3},
            {"root":"middle", "mapping":range(0,0), "source_start":7},
            {"root":"tail", "mapping":range(0,2), "source_start":7},
            {"root":"last", "mapping":range(0,0), "source_start":9},
        ])
    );
    assert!(wire["nodes"].get("inner").is_some());
    for excluded in [
        "root",
        "group",
        "prefix",
        "suffix",
        "outside-left",
        "outside-right",
    ] {
        assert!(wire["nodes"].get(excluded).is_none());
    }
    forest.validate_capture(&before).unwrap();
    assert_eq!(
        CapturedEditSlice::from_json(&forest.to_json().unwrap()).unwrap(),
        forest
    );
    let ranged =
        CapturedEditSlice::capture(&before, &id("group"), range(3, 9), timing("forest-capture"))
            .unwrap();
    assert_eq!(roots(&ranged), ["body", "middle", "tail"]);
    assert!(
        serde_json::to_value(&ranged)
            .unwrap()
            .get("selection")
            .is_none()
    );
    assert_eq!(before.to_json().unwrap(), snapshot);
}

#[test]
fn equal_endpoint_is_one_whole_child_without_changing_child_wire() {
    let before = endpoint_tree();
    for (name, slot, span) in [("body", 2, range(3, 7)), ("last", 5, range(9, 9))] {
        assert_eq!(
            before
                .sequence_children(&id("group"), &id(name), &id(name))
                .unwrap(),
            SequenceChildrenPlan {
                first: slot,
                end: slot + 1,
                range: span
            }
        );
        let forest = capture(&before, "group", name, name);
        let child = CapturedEditSlice::capture_selection(
            &before,
            &id("group"),
            &SliceCaptureSelection::Child { node: id(name) },
            timing("forest-capture"),
        )
        .unwrap();
        let mut wire = serde_json::to_value(forest).unwrap();
        wire["selection"] = json!({"type":"child", "node":name});
        assert_eq!(wire, serde_json::to_value(&child).unwrap());
        assert_eq!(
            CapturedEditSlice::from_json(&wire.to_string()).unwrap(),
            child
        );
    }
}

#[test]
fn missing_reversed_nonchild_and_nonordinary_ancestry_refuse_without_mutation() {
    let before = endpoint_tree();
    let snapshot = before.clone();
    for (first, last) in [
        ("last", "first"),
        ("missing", "last"),
        ("first", "missing"),
        ("inner", "last"),
        ("first", "inner"),
        ("group", "last"),
        ("prefix", "last"),
    ] {
        let error = before
            .sequence_children(&id("group"), &id(first), &id(last))
            .unwrap_err();
        assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
        assert!(
            CapturedEditSlice::capture_selection(
                &before,
                &id("group"),
                &SliceCaptureSelection::Children {
                    first: id(first),
                    last: id(last)
                },
                timing("invalid")
            )
            .is_err()
        );
    }
    for parent in ["missing", "body", "inner"] {
        assert!(
            before
                .sequence_children(&id(parent), &id("first"), &id("last"))
                .is_err()
        );
    }
    assert_eq!(before, snapshot);

    for kind in [
        NodeKind::Repeat {
            child: id("group"),
            iterations: IterationOrder::new(revision("plays"), 2).unwrap(),
            gap: None,
            escalation: None,
        },
        NodeKind::Retime {
            child: id("group"),
            duration: duration(4),
            mapping: range(0, 2),
            pitch: PitchPolicy::Preserve,
            purpose: RetimePurpose::Edit,
        },
    ] {
        let mut owner = BeatNode::sequence("Owner", vec![]);
        owner.kind = kind;
        let nested = tree(
            &["owner"],
            vec![
                ("owner", owner),
                (
                    "group",
                    BeatNode::sequence("Nested", vec![id("a"), id("b")]),
                ),
                ("a", hold(1)),
                ("b", hold(1)),
            ],
        );
        assert_eq!(
            nested
                .sequence_children(&id("group"), &id("a"), &id("b"))
                .unwrap_err()
                .code,
            EditErrorCode::InvalidCommand
        );
        assert!(
            CapturedEditSlice::capture_selection(
                &nested,
                &id("group"),
                &SliceCaptureSelection::Children {
                    first: id("a"),
                    last: id("b")
                },
                timing("invalid")
            )
            .is_err()
        );
    }
}

#[test]
fn all_empty_forest_pastes_at_exact_slots_with_fresh_ids_and_no_clock() {
    let before = tree(
        &["outside", "a", "b", "c"],
        vec![
            ("outside", BeatNode::sequence("Outside", vec![])),
            ("a", BeatNode::sequence("A", vec![])),
            ("b", BeatNode::sequence("B", vec![id("inner")])),
            ("inner", BeatNode::sequence("Inner", vec![])),
            ("c", BeatNode::sequence("C", vec![])),
        ],
    );
    let slice = capture(&before, "root", "a", "c");
    assert_eq!(slice.range(), range(0, 0));
    assert_eq!(roots(&slice), ["a", "b", "c"]);
    assert_eq!(
        slice.identity_requirements().unwrap(),
        SliceIdentityRequirements {
            nodes: 5,
            marks: 0,
            aliases: 0,
            timings: 0
        }
    );
    assert_eq!(
        serde_json::to_value(&slice).unwrap()["audio_bindings"],
        json!(AudioBindingState::default())
    );
    slice.validate_capture(&before).unwrap();
    for slot in [0, 2, 4] {
        let name = format!("empty-{slot}");
        let mut command = paste(&before, &slice, &name, slot);
        let Command::SpliceSlice { timing, .. } = &mut command.command else {
            unreachable!()
        };
        timing.ordinal = u32::MAX;
        let tx = apply(&before, &command).unwrap();
        assert_eq!(tx.duration_delta, 0);
        assert!(tx.forward.audio_bindings.is_none());
        let after = edit(&before, &command);
        let mut expected = siblings(&before, "root").to_vec();
        expected.insert(slot, id(&format!("{name}-node-0")));
        assert_eq!(siblings(&after, "root"), expected);
        assert_eq!(
            siblings(&after, &format!("{name}-node-0")),
            &[
                id(&format!("{name}-node-1")),
                id(&format!("{name}-node-2")),
                id(&format!("{name}-node-3"))
            ]
        );
        assert_eq!(
            siblings(&after, &format!("{name}-node-2")),
            &[id(&format!("{name}-node-4"))]
        );
        assert_eq!(after.duration().unwrap(), FrameDuration::ZERO);
        assert_eq!(after.audio_bindings(), before.audio_bindings());
        assert_eq!(after.sounds(), before.sounds());
        for (node, value) in before
            .nodes()
            .iter()
            .filter(|(node, _)| *node != before.root())
        {
            assert_eq!(&after.nodes()[node], value);
        }
    }
    let positive = tree(&["hold"], vec![("hold", hold(4))]);
    assert!(
        positive
            .slice_splice_interior(&id("root"), &id("hold"), duration(1), &slice)
            .is_err()
    );
    assert!(
        positive
            .slice_replacement(&id("root"), range(1, 2), &slice)
            .is_err()
    );
}

#[test]
fn forest_preserves_owned_effects_and_marks_but_excludes_the_unselected_parent() {
    let mut parent = BeatNode::sequence("Parent effects", vec![id("a"), id("b"), id("c")]);
    parent.framing = Some(
        Framing::static_pose(
            FramingPose::new(
                ExactRatio::new(1, 2).unwrap(),
                ExactRatio::new(1, 2).unwrap(),
                ExactRatio::integer(2),
            )
            .unwrap(),
        )
        .unwrap(),
    );
    parent.audio_treatments = AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::new(-3000).unwrap(), false, vec![], vec![]).unwrap(),
    );
    let mut a = BeatNode::sequence("Owned empty", vec![]);
    a.framing = parent.framing.clone();
    a.audio_treatments = parent.audio_treatments.clone();
    let mut dormant = mark("b", 99, InsertionBias::Right);
    dormant.state = MarkState::Unresolved {
        reason: MarkLossReason::OutOfRange,
    };
    let before = marked(
        &tree(
            &["group", "outside"],
            vec![
                ("group", parent.clone()),
                ("a", a.clone()),
                ("b", hold(4)),
                ("c", BeatNode::sequence("Last empty", vec![])),
                ("outside", hold(2)),
            ],
        ),
        vec![
            ("root-mark", mark("root", 0, InsertionBias::Right)),
            ("parent-mark", mark("group", 2, InsertionBias::Right)),
            ("a-mark", mark("a", 0, InsertionBias::Left)),
            ("b-mark", mark("b", 2, InsertionBias::Right)),
            ("c-mark", mark("c", 0, InsertionBias::Right)),
            ("dormant", dormant),
            ("outside-mark", mark("outside", 0, InsertionBias::Left)),
        ],
    );
    let forest = capture(&before, "group", "a", "c");
    let whole = CapturedEditSlice::capture_selection(
        &before,
        &id("root"),
        &SliceCaptureSelection::Child { node: id("group") },
        timing("forest-capture"),
    )
    .unwrap();
    let wire = serde_json::to_value(&forest).unwrap();
    assert!(wire["nodes"].get("group").is_none());
    assert_eq!(wire["nodes"]["a"], json!(a));
    assert_eq!(
        wire["marks"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["a-mark", "b-mark", "c-mark", "dormant"]
    );
    let whole_wire = serde_json::to_value(whole).unwrap();
    assert_eq!(whole_wire["nodes"]["group"], json!(parent));
    assert!(whole_wire["marks"].get("parent-mark").is_some());
    forest.validate_capture(&before).unwrap();
    let empty = tree(&[], vec![]);
    let pasted = edit(&empty, &paste(&empty, &forest, "owned", 0));
    assert_eq!(pasted.nodes()[&id("owned-node-0")].framing, None);
    assert_eq!(
        pasted.nodes()[&id("owned-node-0")].audio_treatments,
        AudioTreatments::default()
    );
    assert_eq!(pasted.nodes()[&id("owned-node-1")], a);
    let marks = copied_marks(&pasted, "owned");
    assert_eq!(marks.len(), 4);
    assert!(
        marks
            .iter()
            .any(|mark| mark.owner == id("owned-node-1")
                && mark.boundary.bias == InsertionBias::Left)
    );
    assert!(
        marks
            .iter()
            .any(|mark| mark.owner == id("owned-node-3")
                && mark.boundary.bias == InsertionBias::Right)
    );
    assert!(marks.iter().any(|mark| matches!(
        mark.state,
        MarkState::Unresolved {
            reason: MarkLossReason::OutOfRange
        }
    )));
}

#[test]
fn children_wire_requires_exact_endpoints_distinct_whole_parts_and_contiguous_time() {
    let before = tree(
        &["a", "b", "c"],
        vec![
            ("a", hold(3)),
            ("b", BeatNode::sequence("Interior empty", vec![])),
            ("c", hold(2)),
        ],
    );
    let slice = capture(&before, "root", "a", "c");
    let original = serde_json::to_value(&slice).unwrap();
    let command = paste(&before, &slice, "wire", 0);
    assert_eq!(
        serde_json::from_str::<CommandRequest>(&serde_json::to_string(&command).unwrap()).unwrap(),
        command
    );
    let mut corruptions = Vec::new();
    for selection in [
        json!(null),
        json!({"type":"children","first":"a"}),
        json!({"type":"children","first":"a","last":"c","extra":true}),
        json!({"type":"children","first":"b","last":"c"}),
        json!({"type":"children","first":"a","last":"b"}),
        json!({"type":"children","first":"a","last":"a"}),
    ] {
        let mut wire = original.clone();
        wire["selection"] = selection;
        corruptions.push(wire);
    }
    let mut crop = original.clone();
    crop["parts"][0]["mapping"] = json!(range(1, 3));
    crop["range"] = json!(range(1, 5));
    corruptions.push(crop);
    let mut crop = original.clone();
    crop["parts"][2]["mapping"] = json!(range(0, 1));
    crop["range"] = json!(range(0, 4));
    corruptions.push(crop);
    let mut duplicate = original.clone();
    duplicate["parts"]
        .as_array_mut()
        .unwrap()
        .insert(2, original["parts"][1].clone());
    corruptions.push(duplicate);
    let mut gap = original.clone();
    gap["parts"][2]["source_start"] = json!(4);
    corruptions.push(gap);
    let mut empty = original.clone();
    empty["parts"] = json!([]);
    corruptions.push(empty);
    for (index, wire) in corruptions.into_iter().enumerate() {
        assert!(
            CapturedEditSlice::from_json(&wire.to_string()).is_err(),
            "accepted corruption {index}"
        );
        assert!(
            serde_json::from_value::<CapturedEditSlice>(wire.clone()).is_err(),
            "accepted buffered corruption {index}"
        );
        let mut request = serde_json::to_value(&command).unwrap();
        request["command"]["slice"] = wire;
        assert!(
            serde_json::from_value::<CommandRequest>(request).is_err(),
            "accepted command corruption {index}"
        );
    }
    let wire = slice.to_json().unwrap();
    let duplicate = wire.replacen("\"first\":\"a\"", "\"first\":\"a\",\"first\":\"a\"", 1);
    assert_ne!(duplicate, wire);
    assert!(CapturedEditSlice::from_json(&duplicate).is_err());
    assert!(serde_json::from_str::<CapturedEditSlice>(&duplicate).is_err());
}

#[test]
fn historical_recapture_detects_missing_or_reordered_interior_empty_children() {
    let before = tree(
        &["a", "b", "c", "d"],
        vec![
            ("a", BeatNode::sequence("Same", vec![])),
            ("b", BeatNode::sequence("Same", vec![])),
            ("c", BeatNode::sequence("Same", vec![])),
            ("d", BeatNode::sequence("Same", vec![])),
        ],
    );
    let slice = capture(&before, "root", "a", "d");
    let original = serde_json::to_value(&slice).unwrap();
    let mut missing = original.clone();
    missing["parts"].as_array_mut().unwrap().remove(1);
    missing["nodes"].as_object_mut().unwrap().remove("b");
    let mut reordered = original;
    reordered["parts"].as_array_mut().unwrap().swap(1, 2);
    for wire in [missing, reordered] {
        // Self-contained structure and zero frame bounds cannot prove which
        // interior siblings existed. Historical admission must recapture them.
        let forged = CapturedEditSlice::from_json(&wire.to_string()).unwrap();
        assert_eq!(forged.range(), slice.range());
        assert_eq!(forged.selection(), slice.selection());
        let error = forged.validate_capture(&before).unwrap_err();
        assert_eq!(error.code, EditErrorCode::InvalidCommand);
        assert!(
            error
                .message
                .contains("differs from its captured selection")
        );
    }
    slice.validate_capture(&before).unwrap();
}

#[test]
fn positive_forest_repeated_pastes_are_independent_and_exactly_reversible() {
    let before = endpoint_tree();
    let slice = capture(&before, "group", "first", "last");
    let empty = tree(&[], vec![]);
    let once = edit(&empty, &paste(&empty, &slice, "one", 0));
    let twice = edit(&once, &paste(&once, &slice, "two", 1));
    assert_eq!(twice.duration().unwrap(), duration(12));
    assert_eq!(
        siblings(&twice, "root"),
        &[id("one-node-0"), id("two-node-0")]
    );
    let expected = [
        "First empty",
        "Hold",
        "Interior empty",
        "Hold",
        "Last empty",
    ];
    for name in ["one", "two"] {
        let children = siblings(&twice, &format!("{name}-node-0"));
        assert_eq!(children.len(), 5);
        assert_eq!(
            children
                .iter()
                .map(|node| twice.nodes()[node].label.as_str())
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            twice.node_duration(&children[0]).unwrap(),
            FrameDuration::ZERO
        );
        assert_eq!(
            twice.node_duration(&children[4]).unwrap(),
            FrameDuration::ZERO
        );
    }
    assert!(
        once.nodes()
            .keys()
            .filter(|node| *node != once.root())
            .all(|node| node.as_str().starts_with("one-"))
    );
    let snapshot = twice.clone();
    for mode in 0..3 {
        let mut bad = paste(&twice, &slice, "bad", 1);
        let Command::SpliceSlice { identities, .. } = &mut bad.command else {
            unreachable!()
        };
        match mode {
            0 => {
                identities.authored.nodes.pop();
            }
            1 => identities.authored.nodes[1] = identities.authored.nodes[0].clone(),
            _ => identities.authored.nodes[1] = id("one-node-1"),
        }
        assert!(apply(&twice, &bad).is_err());
        assert_eq!(twice, snapshot);
    }
    slice.validate_capture(&before).unwrap();
}
