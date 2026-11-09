//! Split, ripple deletion and Repeat wraps inside one explicit Repeat play.

use super::*;

/// root = [rep, tail]; rep plays body = [a, b, c] three times. At 30000/1001
/// a one-frame shift is not a whole number of samples, so a moved owner must
/// keep its retained entry rather than re-round on the new grid.
fn played() -> ProjectDocument {
    tree(
        &["rep", "tail"],
        vec![
            ("rep", repeat("body")),
            (
                "body",
                BeatNode::sequence("Body", vec![id("a"), id("b"), id("c")]),
            ),
            ("a", source(1)),
            ("b", source(1)),
            ("c", source(1)),
            ("tail", source(2)),
        ],
    )
}

fn repeat(child: &str) -> BeatNode {
    let mut repeat = BeatNode::sequence("Repeat", vec![]);
    repeat.kind = NodeKind::Repeat {
        child: id(child),
        iterations: IterationOrder::new(revision("plays"), 3).unwrap(),
        gap: None,
        escalation: None,
    };
    repeat
}

fn second_play(node: &str) -> InstancePath {
    InstancePath {
        node: id(node),
        repeats: vec![RepeatInstance {
            node: id("rep"),
            iteration: IterationId {
                allocation: revision("plays"),
                ordinal: 1,
            },
        }],
    }
}

fn occurrence(
    document: &ProjectDocument,
    name: &str,
    instance: InstancePath,
    edit: OccurrenceEdit,
) -> (CommandRequest, ScopedNodeTarget, InstancePath) {
    let needs = document
        .occurrence_structure_requirements(&instance, &edit)
        .unwrap();
    let identities = OccurrenceIdentities {
        nodes: (0..needs.nodes)
            .map(|index| id(&format!("{name}-copy-{index}")))
            .collect(),
        marks: vec![],
    };
    let (target, presentation) = document
        .occurrence_structure_selection(&instance, &edit, &identities)
        .unwrap();
    (
        request(
            document,
            name,
            Command::EditOccurrence {
                instance,
                edit,
                identities,
            },
        ),
        target,
        presentation,
    )
}

fn timing(name: &str) -> AudioTimingId {
    AudioTimingId {
        allocation: revision(name),
        ordinal: 0,
    }
}

fn play_root(document: &ProjectDocument, ordinal: u32) -> NodeId {
    let NodeKind::Repeat { child, .. } = &document.nodes()[&id("rep")].kind else {
        panic!("Repeat")
    };
    document
        .overrides()
        .get(&id("rep"))
        .and_then(|plays| {
            plays.get(&IterationId {
                allocation: revision("plays"),
                ordinal,
            })
        })
        .unwrap_or(child)
        .clone()
}

fn kids(document: &ProjectDocument, node: &NodeId) -> Vec<NodeId> {
    document.children(node).cloned().collect()
}

#[test]
fn deleting_in_one_play_shortens_only_that_play_and_keeps_later_clocks() {
    deadpan_core::with_reference_timing_representation(|| {
        let before = played();
        let (request, target, presentation) = occurrence(
            &before,
            "delete",
            second_play("b"),
            OccurrenceEdit::DeleteRipple {
                timing: timing("delete"),
            },
        );
        let after = edit(&before, request);
        assert_eq!(after.duration().unwrap(), duration(10));
        // The shared definition still plays [a, b, c] in plays 1 and 3.
        assert_eq!(kids(&after, &id("body")), vec![id("a"), id("b"), id("c")]);
        assert_eq!(play_root(&after, 0), id("body"));
        assert_eq!(play_root(&after, 2), id("body"));
        let owned = play_root(&after, 1);
        assert_ne!(owned, id("body"));
        let remaining = kids(&after, &owned);
        assert_eq!(remaining.len(), 2);
        // The selection follows the deleted beat's next sibling, c's copy.
        assert_eq!(target.node, remaining[1]);
        assert_eq!(presentation.node, remaining[1]);
        assert!(target.matches_instance(&after, &presentation).unwrap());
        // The tail started at frame 9: B(9) = 14414 on the old grid. Moving
        // to frame 8 must keep that entry, not re-round to B(8) = 12813.
        assert_eq!(
            reference_at_anchor(&after, &id("tail")),
            ExactRatio::integer(14414)
        );
        // The shared `a` started play 3 at frame 6, B(6) = 9610. That play now
        // starts at frame 5 and keeps the old entry; play 1 is unmoved.
        let third = InstancePath {
            node: id("a"),
            repeats: vec![RepeatInstance {
                node: id("rep"),
                iteration: IterationId {
                    allocation: revision("plays"),
                    ordinal: 2,
                },
            }],
        };
        assert_eq!(anchored(&after, &third), ExactRatio::integer(9610));
    })
}

/// The retained reference sample at an occurrence's current start.
fn anchored(document: &ProjectDocument, instance: &InstancePath) -> ExactRatio {
    let resolved = document
        .audio_bindings()
        .resolve(&instance.node, instance, MAX_AUDIO_BINDING_ENTRIES)
        .unwrap();
    let start = resolved
        .lattice
        .sample_boundary(resolved.lattice.local_support.start)
        .unwrap();
    ExactRatio::integer(start)
        .checked_add(
            resolved
                .resume
                .unwrap()
                .reference_local_delta
                .checked_div(resolved.lattice.local_frames_per_sample().unwrap())
                .unwrap(),
        )
        .unwrap()
}

#[test]
fn wrapping_in_one_play_repeats_only_that_beat_there() {
    deadpan_core::with_reference_timing_representation(|| {
        let before = played();
        let (request, target, _) = occurrence(
            &before,
            "wrap",
            second_play("b"),
            OccurrenceEdit::RepeatChild {
                id: id("wrap-repeat"),
                plays: 3,
                timing: timing("wrap"),
            },
        );
        let after = edit(&before, request);
        assert_eq!(after.duration().unwrap(), duration(13));
        assert_eq!(target.node, id("wrap-repeat"));
        assert_eq!(kids(&after, &id("body")), vec![id("a"), id("b"), id("c")]);
        let owned = kids(&after, &play_root(&after, 1));
        assert_eq!(owned[1], id("wrap-repeat"));
        assert!(matches!(
            after.nodes()[&id("wrap-repeat")].kind,
            NodeKind::Repeat { .. }
        ));
        assert_eq!(
            reference_at_anchor(&after, &id("tail")),
            ExactRatio::integer(14414)
        );
    })
}

#[test]
fn splitting_in_one_play_selects_the_right_fragment_there() {
    let before = tree(
        &["rep", "tail"],
        vec![
            ("rep", repeat("long")),
            ("long", source(4)),
            ("tail", source(2)),
        ],
    );
    let edit_value = OccurrenceEdit::Split {
        at: duration(1),
        identities: pool("cut", 4),
    };
    let (request, target, _) = occurrence(&before, "split", second_play("long"), edit_value);
    let after = edit(&before, request);
    assert_eq!(after.duration(), before.duration());
    assert_eq!(play_root_of(&after, "long"), id("long"));
    let owned = after.overrides()[&id("rep")]
        .get(&IterationId {
            allocation: revision("plays"),
            ordinal: 1,
        })
        .unwrap()
        .clone();
    // The play's single beat gains a group holding both fragments.
    let fragments = kids(&after, &owned);
    assert_eq!(fragments.len(), 2);
    assert_eq!(target.node, fragments[1]);
    assert_eq!(target.node, id("cut-1"));
}

fn play_root_of(document: &ProjectDocument, child: &str) -> NodeId {
    let NodeKind::Repeat { child: shared, .. } = &document.nodes()[&id("rep")].kind else {
        panic!("Repeat")
    };
    assert_eq!(shared, &id(child));
    shared.clone()
}

#[test]
fn one_play_edits_refuse_the_whole_play_default_paths_and_sounds_readably() {
    let before = played();
    let delete = OccurrenceEdit::DeleteRipple {
        timing: timing("delete"),
    };
    // The play's whole contents have no group to ripple within.
    let whole = before
        .occurrence_structure_requirements(&second_play("body"), &delete)
        .unwrap_err();
    assert!(whole.message.contains("whole contents"), "{whole}");
    // Without an explicit play there is no occurrence to edit.
    let shared = InstancePath {
        node: id("b"),
        repeats: vec![],
    };
    assert!(
        before
            .occurrence_structure_requirements(&shared, &delete)
            .is_err()
    );
    assert!(
        before
            .occurrence_structure_requirements(
                &second_play("b"),
                &OccurrenceEdit::Rename { label: "x".into() }
            )
            .is_err()
    );
}

/// root = [rep, tail]; rep plays body = [a, inner, c] three times, and inner
/// plays b twice. Plays start at frames 0, 4 and 8; the tail at 12.
fn nested() -> ProjectDocument {
    let mut inner = BeatNode::sequence("Inner", vec![]);
    inner.kind = NodeKind::Repeat {
        child: id("b"),
        iterations: IterationOrder::new(revision("inner-plays"), 2).unwrap(),
        gap: None,
        escalation: None,
    };
    tree(
        &["rep", "tail"],
        vec![
            ("rep", repeat("body")),
            (
                "body",
                BeatNode::sequence("Body", vec![id("a"), id("inner"), id("c")]),
            ),
            ("a", source(1)),
            ("inner", inner),
            ("b", source(1)),
            ("c", source(1)),
            ("tail", source(2)),
        ],
    )
}

fn plays_of(document: &ProjectDocument, node: &NodeId) -> Vec<IterationId> {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[node].kind else {
        panic!("Repeat")
    };
    (0..iterations.len())
        .map(|index| iterations.at(index).unwrap())
        .collect()
}

fn in_play(node: &NodeId, ordinal: u32) -> InstancePath {
    InstancePath {
        node: node.clone(),
        repeats: vec![RepeatInstance {
            node: id("rep"),
            iteration: IterationId {
                allocation: revision("plays"),
                ordinal,
            },
        }],
    }
}

#[test]
fn setting_plays_in_one_play_changes_only_that_occurrence_and_keeps_later_clocks() {
    deadpan_core::with_reference_timing_representation(|| {
        let before = nested();
        let shared = plays_of(&before, &id("inner"));
        for (plays, total) in [(3, 15), (1, 13)] {
            let name = format!("set-{plays}");
            let (request, target, presentation) = occurrence(
                &before,
                &name,
                second_play("inner"),
                OccurrenceEdit::SetRepeatPlays {
                    plays,
                    timing: timing(&name),
                },
            );
            let after = edit(&before, request);
            assert_eq!(after.duration().unwrap(), duration(total));
            // The shared definition still plays b twice in plays 1 and 3.
            assert_eq!(plays_of(&after, &id("inner")), shared);
            assert_eq!(play_root(&after, 0), id("body"));
            assert_eq!(play_root(&after, 2), id("body"));
            let owned = kids(&after, &play_root(&after, 1));
            assert_eq!(target.node, owned[1]);
            assert_eq!(presentation.node, owned[1]);
            assert!(target.matches_instance(&after, &presentation).unwrap());
            let counted = plays_of(&after, &owned[1]);
            assert_eq!(counted.len(), plays as usize);
            // Surviving plays keep their stable identities; growth takes
            // this revision's allocation.
            assert_eq!(
                counted[..counted.len().min(2)],
                shared[..counted.len().min(2)]
            );
            if plays == 3 {
                assert_eq!(counted[2].allocation, revision(&name));
            }
            // At 30000/1001 a one-frame shift is not whole samples. The tail
            // started at frame 12 (B = 19219), c at 7 in play 2 (B = 11211)
            // and play 3 at 8 (B = 12813); each keeps its old entry.
            assert_eq!(
                reference_at_anchor(&after, &id("tail")),
                ExactRatio::integer(19219)
            );
            assert_eq!(
                anchored(&after, &in_play(&owned[2], 1)),
                ExactRatio::integer(11211)
            );
            assert_eq!(
                anchored(&after, &in_play(&id("a"), 2)),
                ExactRatio::integer(12813)
            );
        }
    })
}

#[test]
fn setting_the_same_plays_in_one_play_refuses_without_an_edit() {
    let before = nested();
    let edit_value = OccurrenceEdit::SetRepeatPlays {
        plays: 2,
        timing: timing("same"),
    };
    let error = before
        .occurrence_structure_requirements(&second_play("inner"), &edit_value)
        .unwrap_err();
    assert!(error.message.contains("already plays 2 times"), "{error}");
    let request = request(
        &before,
        "same",
        Command::EditOccurrence {
            instance: second_play("inner"),
            edit: edit_value,
            identities: OccurrenceIdentities {
                nodes: (0..8).map(|n| id(&format!("same-{n}"))).collect(),
                marks: vec![],
            },
        },
    );
    let error = apply(&before, &request).unwrap_err();
    assert!(error.message.contains("already plays 2 times"), "{error}");
    // A beat that is not a Repeat has no count to set.
    let error = before
        .occurrence_structure_requirements(
            &second_play("a"),
            &OccurrenceEdit::SetRepeatPlays {
                plays: 3,
                timing: timing("beat"),
            },
        )
        .unwrap_err();
    assert!(error.message.contains("selected Repeat"), "{error}");
}
