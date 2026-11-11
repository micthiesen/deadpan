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
