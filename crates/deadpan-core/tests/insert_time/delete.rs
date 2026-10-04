use super::*;

fn deletion(document: &ProjectDocument, target: &str, name: &str) -> CommandRequest {
    request(
        document,
        name,
        Command::DeleteRipple {
            node: id(target),
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    )
}

#[test]
fn nested_delete_keeps_every_surviving_suffix_clock_and_one_inverse() {
    let original = tree(
        &["prefix", "group", "tail"],
        vec![
            ("prefix", source(1)),
            (
                "group",
                BeatNode::sequence("Group", vec![id("a"), id("removed"), id("b")]),
            ),
            ("a", source(2)),
            ("removed", BeatNode::sequence("Removed", vec![id("inside")])),
            ("inside", source(1)),
            ("b", source(3)),
            ("tail", source(4)),
        ],
    );
    let after = edit(&original, deletion(&original, "removed", "deleted"));
    assert_eq!(after.duration().unwrap(), duration(10));
    assert_eq!(children(&after), children(&original));
    assert!(!after.nodes().contains_key(&id("removed")));
    assert!(!after.nodes().contains_key(&id("inside")));
    for name in ["prefix", "a", "b", "tail"] {
        assert_eq!(after.nodes()[&id(name)], original.nodes()[&id(name)]);
        assert_eq!(
            after.audio_bindings().bindings()[&id(name)].reanchors.len(),
            usize::from(matches!(name, "b" | "tail")),
            "{name}"
        );
    }
}

#[test]
fn zero_and_terminal_deletion_do_not_capture_unmoved_sampling() {
    let original = tree(
        &["empty", "a", "tail"],
        vec![
            ("empty", BeatNode::sequence("Empty", vec![])),
            ("a", source(2)),
            ("tail", source(1)),
        ],
    );
    let empty_removed = edit(&original, deletion(&original, "empty", "no-time"));
    assert_eq!(empty_removed.duration().unwrap(), duration(3));
    assert_eq!(empty_removed.audio_bindings(), original.audio_bindings());
    let tail_removed = edit(&empty_removed, deletion(&empty_removed, "tail", "end"));
    assert_eq!(tail_removed.audio_bindings(), original.audio_bindings());
    let all_removed = edit(&tail_removed, deletion(&tail_removed, "a", "all"));
    assert_eq!(all_removed.duration().unwrap(), FrameDuration::ZERO);
    assert_eq!(all_removed.nodes().len(), 1);
    assert!(all_removed.audio_bindings().is_empty());
}

#[test]
fn deletion_retains_previous_lattice_and_appends_one_resume() {
    let original = tree(&["a", "b"], vec![("a", source(2)), ("b", source(4))]);
    let inserted = edit(&original, insertion(&original, "pause", 2, 1));
    let after = edit(&inserted, deletion(&inserted, "a", "delete"));
    let old = &inserted.audio_bindings().bindings()[&id("b")];
    let new = &after.audio_bindings().bindings()[&id("b")];
    assert_eq!(new.lattice, old.lattice);
    assert_eq!(new.reanchors.len(), old.reanchors.len() + 1);
    assert_eq!(&new.reanchors[..old.reanchors.len()], old.reanchors);
}

#[test]
fn deletion_moves_content_marks_but_keeps_sequence_marks_and_loss_policy() {
    let original = tree(&["a", "b"], vec![("a", source(1)), ("b", source(4))]);
    let mark = |owner: &str, coordinate, loss_policy| Mark {
        owner: id(owner),
        label: "Boundary".into(),
        boundary: BoundaryAnchor {
            coordinate,
            bias: InsertionBias::Right,
        },
        loss_policy,
        state: MarkState::Bound,
        fragments: vec![],
    };
    let mut wire = json!(original);
    wire["marks"] = json!(BTreeMap::from([
        (
            "content",
            mark(
                "root",
                Anchor::Local {
                    node: id("root"),
                    position: ExactRatio::integer(2)
                },
                AnchorLossPolicy::KeepUnresolved
            )
        ),
        (
            "pinned",
            mark(
                "root",
                Anchor::Sequence {
                    frame: ProjectFrame(2)
                },
                AnchorLossPolicy::KeepUnresolved
            )
        ),
        (
            "owned",
            mark(
                "a",
                Anchor::Local {
                    node: id("a"),
                    position: ExactRatio::ZERO
                },
                AnchorLossPolicy::DeleteOwned
            )
        ),
        (
            "lost",
            mark(
                "root",
                Anchor::Local {
                    node: id("a"),
                    position: ExactRatio::ZERO
                },
                AnchorLossPolicy::KeepUnresolved
            )
        ),
    ]));
    let original = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let after = edit(&original, deletion(&original, "a", "deleted"));
    assert_eq!(
        after.marks()[&MarkId::new("content").unwrap()]
            .boundary
            .coordinate,
        Anchor::Local {
            node: id("root"),
            position: ExactRatio::integer(1)
        }
    );
    assert_eq!(
        after.marks()[&MarkId::new("pinned").unwrap()],
        original.marks()[&MarkId::new("pinned").unwrap()]
    );
    assert!(!after.marks().contains_key(&MarkId::new("owned").unwrap()));
    assert!(matches!(
        after.marks()[&MarkId::new("lost").unwrap()].state,
        MarkState::Unresolved { .. }
    ));
}

#[test]
fn deletion_rejects_root_missing_target_and_wrong_allocation_atomically() {
    let original = tree(&["a", "b"], vec![("a", source(1)), ("b", source(4))]);
    for target in ["root", "missing"] {
        assert!(apply(&original, &deletion(&original, target, "deleted")).is_err());
    }
    let mut bad = deletion(&original, "a", "deleted");
    let Command::DeleteRipple { timing, .. } = &mut bad.command else {
        panic!()
    };
    timing.allocation = revision("wrong");
    assert!(apply(&original, &bad).is_err());
    assert_eq!(original.revision_id(), &revision("initial"));
}

#[test]
fn whole_composites_can_be_deleted_but_their_clocked_children_cannot() {
    for kind in [
        NodeKind::Repeat {
            child: id("group"),
            iterations: IterationOrder::new(revision("plays"), 2).unwrap(),
            gap: None,
            escalation: None,
        },
        NodeKind::Retime {
            child: id("group"),
            duration: duration(2),
            mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(2)).unwrap(),
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Edit,
        },
    ] {
        let original = tree(
            &["container", "tail"],
            vec![
                ("container", BeatNode { kind, ..source(1) }),
                (
                    "group",
                    BeatNode::sequence("Group", vec![id("leaf"), id("empty")]),
                ),
                ("leaf", source(2)),
                ("empty", BeatNode::sequence("Empty", vec![])),
                ("tail", source(4)),
            ],
        );
        for target in ["leaf", "empty", "group"] {
            assert!(apply(&original, &deletion(&original, target, "deleted")).is_err());
        }
        let after = edit(&original, deletion(&original, "container", "deleted"));
        assert_eq!(children(&after), [id("tail")]);
        assert_eq!(after.nodes().len(), 2);
        assert_eq!(after.duration().unwrap(), duration(4));
    }
}
