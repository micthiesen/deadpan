use super::*;

#[path = "delete_range/nested_partitions.rs"]
mod nested_partitions;

fn range(a: i64, b: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(a), ProjectFrame(b)).unwrap()
}

fn deletion(before: &ProjectDocument, parent: &str, a: i64, b: i64, name: &str) -> CommandRequest {
    request(
        before,
        name,
        Command::DeleteRange {
            parent: id(parent),
            range: range(a, b),
            identities: pool(name, 12),
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    )
}

fn sequence<'a>(document: &'a ProjectDocument, parent: &str) -> &'a [NodeId] {
    let NodeKind::Sequence { children } = &document.nodes()[&id(parent)].kind else {
        panic!()
    };
    children
}

#[test]
fn every_single_source_range_keeps_exact_fragments_and_one_inverse() {
    for length in 1..=8 {
        let before = tree(&["whole"], vec![("whole", source(length))]);
        for start in 0..length {
            for end in start + 1..=length {
                let query = before
                    .range_deletion(&id("root"), range(start, end))
                    .unwrap();
                let expected_ids = match (start > 0, end < length) {
                    (true, true) => 5,
                    (false, false) => 0,
                    _ => 3,
                };
                assert_eq!(query.required_ids, expected_ids);
                let mut request = deletion(&before, "root", start, end, "deleted");
                let Command::DeleteRange { identities, .. } = &mut request.command else {
                    panic!()
                };
                *identities = pool("deleted", query.required_ids);
                let after = edit(&before, request);
                assert_eq!(after.duration().unwrap(), duration(length - (end - start)));
                let expected: Vec<_> = [(0, start), (end, length)]
                    .into_iter()
                    .filter(|(a, b)| a < b)
                    .collect();
                assert_eq!(children(&after).len(), expected.len());
                for (child, (a, b)) in children(&after).iter().zip(expected) {
                    let NodeKind::Retime {
                        child: physical,
                        mapping,
                        duration: length,
                        purpose,
                        ..
                    } = &after.nodes()[child].kind
                    else {
                        panic!()
                    };
                    assert_eq!(*mapping, range(a, b));
                    assert_eq!(*length, duration(b - a));
                    assert_eq!(*purpose, RetimePurpose::Partition);
                    assert_eq!(after.nodes()[physical], before.nodes()[&id("whole")]);
                }
                if end == length {
                    assert!(
                        after
                            .audio_bindings()
                            .bindings()
                            .values()
                            .all(|b| b.reanchors.is_empty())
                    );
                }
            }
        }
    }
}

#[test]
fn nested_range_keeps_outer_nodes_and_reanchors_every_surviving_suffix_once() {
    // Asserts the authored reference representation (every reanchor step
    // and complete timing tables). tests/timing_representation.rs proves the
    // compact storage resolves and renders identically.
    deadpan_core::with_reference_timing_representation(|| {
        let before = tree(
            &["prefix", "group", "tail"],
            vec![
                ("prefix", source(1)),
                (
                    "group",
                    BeatNode::sequence("Group", vec![id("a"), id("middle"), id("b")]),
                ),
                ("a", source(3)),
                ("middle", BeatNode::sequence("Middle", vec![id("held")])),
                ("held", BeatNode::hold("Held", recipe(2))),
                ("b", source(3)),
                ("tail", source(2)),
            ],
        );
        let query = before.range_deletion(&id("group"), range(2, 8)).unwrap();
        assert_eq!(
            (query.start_index, query.end_index, query.required_ids),
            (0, 3, 6)
        );
        let after = edit(&before, deletion(&before, "group", 2, 8, "deleted"));
        assert_eq!(after.duration().unwrap(), duration(5));
        assert_eq!(children(&after), children(&before));
        assert_eq!(sequence(&after, "group").len(), 2);
        assert!(!after.nodes().contains_key(&id("middle")));
        assert!(!after.nodes().contains_key(&id("held")));
        assert_eq!(after.nodes()[&id("prefix")], before.nodes()[&id("prefix")]);
        assert_eq!(
            after.audio_bindings().bindings()[&id("prefix")]
                .reanchors
                .len(),
            0
        );
        assert_eq!(
            after.audio_bindings().bindings()[&id("tail")]
                .reanchors
                .len(),
            1
        );
        let right = owner(&after, &sequence(&after, "group")[1]);
        assert_eq!(after.audio_bindings().bindings()[&right].reanchors.len(), 1);
    })
}

#[test]
fn full_range_preserves_endpoint_empty_groups_and_retires_internal_empty_groups() {
    let before = tree(
        &["left", "a", "inside", "b", "right"],
        vec![
            ("left", BeatNode::sequence("Left", vec![])),
            ("a", source(2)),
            ("inside", BeatNode::sequence("Inside", vec![])),
            ("b", source(2)),
            ("right", BeatNode::sequence("Right", vec![])),
        ],
    );
    let after = edit(&before, deletion(&before, "root", 0, 4, "deleted"));
    assert_eq!(children(&after), [id("left"), id("right")]);
    assert_eq!(after.nodes().len(), 3);
    assert_eq!(after.duration().unwrap(), FrameDuration::ZERO);
    assert!(after.audio_bindings().is_empty());
}

#[test]
fn invalid_selection_identities_and_timing_fail_without_mutation() {
    let before = tree(&["whole"], vec![("whole", source(6))]);
    let initial = before.to_json().unwrap();
    for (parent, start, end) in [
        ("root", 1, 1),
        ("root", 0, 7),
        ("root", -1, 4),
        ("missing", 1, 3),
        ("whole", 1, 3),
    ] {
        assert!(apply(&before, &deletion(&before, parent, start, end, "deleted")).is_err());
    }
    for nodes in [vec![], vec![id("whole")], vec![id("fresh"); 5]] {
        let mut request = deletion(&before, "root", 1, 3, "deleted");
        let Command::DeleteRange { identities, .. } = &mut request.command else {
            panic!()
        };
        identities.nodes = nodes;
        assert!(apply(&before, &request).is_err());
    }
    for timing in [
        AudioTimingId {
            allocation: revision("wrong"),
            ordinal: 0,
        },
        AudioTimingId {
            allocation: revision("deleted"),
            ordinal: u32::MAX,
        },
    ] {
        let mut request = deletion(&before, "root", 1, 3, "deleted");
        let Command::DeleteRange { timing: target, .. } = &mut request.command else {
            panic!()
        };
        *target = timing;
        assert!(apply(&before, &request).is_err());
    }
    assert_eq!(before.to_json().unwrap(), initial);
}

#[test]
fn complete_composites_are_admitted_but_partial_or_clocked_ancestry_is_rejected() {
    for kind in [
        NodeKind::Repeat {
            child: id("group"),
            iterations: IterationOrder::new(revision("plays"), 2).unwrap(),
            gap: None,
            escalation: None,
        },
        NodeKind::Retime {
            child: id("group"),
            duration: duration(6),
            mapping: range(0, 6),
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Edit,
        },
    ] {
        let before = tree(
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
                        cutaways: Vec::new(),
                        captions: Vec::new(),
                    },
                ),
                ("group", BeatNode::sequence("Group", vec![id("whole")])),
                ("whole", source(6)),
            ],
        );
        for parent in ["root", "group"] {
            assert!(apply(&before, &deletion(&before, parent, 1, 4, "deleted")).is_err());
        }
        let total = before.duration().unwrap().frames();
        let after = edit(&before, deletion(&before, "root", 0, total, "deleted"));
        assert_eq!(after.nodes().len(), 1);
        assert!(after.audio_bindings().is_empty());
    }
}

#[test]
fn deletion_preflight_reserves_only_its_endpoint_nodes_at_the_temporary_limit() {
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
    assert_eq!(
        before
            .range_deletion(&id("root"), range(1, 3))
            .unwrap()
            .required_ids,
        5
    );
    assert_eq!(
        before
            .source_replacement(&id("root"), range(1, 3))
            .unwrap_err()
            .code,
        EditErrorCode::LimitExceeded
    );
    // This query proves only the temporary node budget. The huge fixture may
    // independently exceed audio-binding work when applied; admission does not
    // waive that separate runtime bound.
    let mut terminal = deletion(&before, "root", 0, 6, "terminal");
    let Command::DeleteRange {
        timing, identities, ..
    } = &mut terminal.command
    else {
        panic!()
    };
    timing.ordinal = u32::MAX;
    identities.nodes.clear();
    // Aligned terminal deletion needs neither split copies nor shifted clocks,
    // and must remain available even on this large document.
    let after = edit(&before, terminal);
    assert_eq!(after.duration().unwrap(), FrameDuration::ZERO);
    assert!(after.audio_bindings().is_empty());
    wire["nodes"]["extra"] = json!(BeatNode::sequence("", vec![]));
    wire["nodes"]["root"]["kind"]["children"]
        .as_array_mut()
        .unwrap()
        .push(json!("extra"));
    let over = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_eq!(
        over.range_deletion(&id("root"), range(1, 3))
            .unwrap_err()
            .code,
        EditErrorCode::LimitExceeded
    );
}

#[test]
fn one_clock_deletions_use_the_supplied_maximum_timing_ordinal() {
    for (before, start, end) in [
        (tree(&["whole"], vec![("whole", source(6))]), 2, 6),
        (
            tree(&["a", "b"], vec![("a", source(1)), ("b", source(5))]),
            0,
            1,
        ),
    ] {
        let mut request = deletion(&before, "root", start, end, "deleted");
        let Command::DeleteRange { timing, .. } = &mut request.command else {
            panic!()
        };
        timing.ordinal = u32::MAX;
        let after = edit(&before, request);
        assert!(!after.audio_bindings().is_empty());
        assert!(
            after
                .audio_bindings()
                .timings()
                .keys()
                .all(|id| id.ordinal == u32::MAX)
        );
    }
}

#[test]
fn deleted_marks_follow_loss_policy_and_surviving_marks_keep_their_content() {
    let original = tree(&["whole"], vec![("whole", source(6))]);
    let mark = |position, loss_policy| Mark {
        owner: id("root"),
        label: "Point".into(),
        boundary: BoundaryAnchor {
            coordinate: Anchor::Local {
                node: id("root"),
                position: ExactRatio::integer(position),
            },
            bias: InsertionBias::Right,
        },
        loss_policy,
        state: MarkState::Bound,
        fragments: vec![],
    };
    let mut pinned = mark(5, AnchorLossPolicy::KeepUnresolved);
    pinned.boundary.coordinate = Anchor::Sequence {
        frame: ProjectFrame(3),
    };
    let mut wire = json!(original);
    wire["marks"] = json!(BTreeMap::from([
        ("prefix", mark(1, AnchorLossPolicy::KeepUnresolved)),
        ("suffix", mark(5, AnchorLossPolicy::KeepUnresolved)),
        ("lost", mark(3, AnchorLossPolicy::KeepUnresolved)),
        ("removed", mark(3, AnchorLossPolicy::DeleteOwned)),
        ("pinned", pinned),
    ]));
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let after = edit(&before, deletion(&before, "root", 2, 4, "deleted"));
    for (name, position) in [("prefix", 1), ("suffix", 3)] {
        let mark = &after.marks()[&MarkId::new(name).unwrap()];
        assert_eq!(mark.state, MarkState::Bound);
        assert_eq!(
            mark.boundary.coordinate,
            Anchor::Local {
                node: id("root"),
                position: ExactRatio::integer(position)
            }
        );
    }
    assert!(matches!(
        after.marks()[&MarkId::new("lost").unwrap()].state,
        MarkState::Unresolved { .. }
    ));
    assert!(!after.marks().contains_key(&MarkId::new("removed").unwrap()));
    assert_eq!(
        after.marks()[&MarkId::new("pinned").unwrap()],
        before.marks()[&MarkId::new("pinned").unwrap()]
    );
}

#[test]
fn later_deletion_refines_treated_fragments_without_recapturing_framing_or_lattice() {
    let mut leaf = source(10);
    leaf.framing = Some(
        Framing::creep(
            FramingPose::new(
                ExactRatio::new(1, 2).unwrap(),
                ExactRatio::new(1, 2).unwrap(),
                ExactRatio::ONE,
            )
            .unwrap(),
            FramingPose::new(
                ExactRatio::new(1, 2).unwrap(),
                ExactRatio::new(1, 2).unwrap(),
                ExactRatio::integer(2),
            )
            .unwrap(),
            FramingCurve::Linear,
        )
        .unwrap(),
    );
    let original = tree(&["whole"], vec![("whole", leaf)]);
    let first = edit(&original, deletion(&original, "root", 1, 3, "first"));
    let treated = children(&first)[1].clone();
    let mut wire = json!(first);
    wire["nodes"][treated.as_str()]["label"] = json!("Treated fragment");
    wire["nodes"][treated.as_str()]["audio_treatments"] =
        json!(AudioTreatments::from_clip_gain(ClipGain::default()));
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let query = before.range_deletion(&id("root"), range(3, 5)).unwrap();
    assert_eq!(query.required_ids, 7);
    let mut request = deletion(&before, "root", 3, 5, "second");
    let Command::DeleteRange { identities, .. } = &mut request.command else {
        panic!()
    };
    *identities = pool("second", query.required_ids);
    let after = edit(&before, request);
    assert_eq!(after.duration().unwrap(), duration(6));
    assert_eq!(after.nodes()[&treated], before.nodes()[&treated]);
    let retained: Vec<_> = after
        .nodes()
        .values()
        .filter(|node| node.label == "Treated fragment" && !node.audio_treatments.is_empty())
        .collect();
    assert_eq!(retained.len(), 2);
    for node in retained {
        assert_eq!(node.framing, before.nodes()[&treated].framing);
        assert_eq!(
            node.audio_treatments,
            before.nodes()[&treated].audio_treatments
        );
    }
    let old = &before.audio_bindings().bindings()[&owner(&before, &treated)].lattice;
    for binding in after.audio_bindings().bindings().values() {
        assert_eq!(&binding.lattice, old);
    }
}
