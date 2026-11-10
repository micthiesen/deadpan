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

fn gap_edit(
    name: &str,
    plays: u32,
    gap: Option<i64>,
    branches: &[(u32, &str, i64)],
) -> OccurrenceEdit {
    // Play IDs after the count: surviving inner plays, then this revision's.
    let order = IterationOrder::new(revision("inner-plays"), 2)
        .unwrap()
        .resized(plays, revision(name))
        .unwrap();
    OccurrenceEdit::SetRepeatGaps {
        plays,
        gap: gap.map(recipe),
        branches: branches
            .iter()
            .map(|(after, node, frames)| RepeatGapHold {
                after: order.at(*after).unwrap(),
                id: id(node),
                hold: recipe(*frames),
            })
            .collect(),
        timing: timing(name),
    }
}

#[test]
fn setting_gaps_in_one_play_adds_fresh_holds_there_and_keeps_later_clocks() {
    deadpan_core::with_reference_timing_representation(|| {
        let before = nested();
        let shared = plays_of(&before, &id("inner"));
        // Three plays of b with a 2f default gap and a 1f Hold after play 2:
        // inner grows from 2 to 6 frames, so the tail moves from 12 to 16.
        let (request, target, presentation) = occurrence(
            &before,
            "gaps",
            second_play("inner"),
            gap_edit("gaps", 3, Some(2), &[(1, "gap-hold", 1)]),
        );
        let after = edit(&before, request);
        assert_eq!(after.duration().unwrap(), duration(18));
        // The shared definition keeps two gapless plays in plays 1 and 3.
        let NodeKind::Repeat { gap, .. } = &after.nodes()[&id("inner")].kind else {
            panic!("Repeat")
        };
        assert_eq!(gap, &None);
        assert!(after.gap_overrides().get(&id("inner")).is_none());
        assert_eq!(plays_of(&after, &id("inner")), shared);
        assert_eq!(play_root(&after, 0), id("body"));
        assert_eq!(play_root(&after, 2), id("body"));
        let owned = kids(&after, &play_root(&after, 1));
        let inner = owned[1].clone();
        assert_eq!(target.node, inner);
        assert_eq!(presentation.node, inner);
        assert!(target.matches_instance(&after, &presentation).unwrap());
        let NodeKind::Repeat { gap, child, .. } = &after.nodes()[&inner].kind else {
            panic!("Repeat")
        };
        assert_eq!(gap, &Some(recipe(2)));
        let child = child.clone();
        let counted = plays_of(&after, &inner);
        assert_eq!(counted[..2], shared[..]);
        assert_eq!(counted[2].allocation, revision("gaps"));
        // The named gap is a fresh independent Hold after the second play.
        let branches: Vec<_> = after.gap_overrides()[&inner]
            .iter()
            .map(|(after_play, root)| (after_play.clone(), root.clone()))
            .collect();
        assert_eq!(branches, vec![(shared[1].clone(), id("gap-hold"))]);
        assert!(matches!(
            &after.nodes()[&id("gap-hold")].kind,
            NodeKind::Hold { recipe: hold } if *hold == recipe(1)
        ));
        // At 30000/1001 each moved owner keeps its old entry: the tail at
        // 12 (B = 19219), c at 7 (B = 11211), play 3 at 8 (B = 12813) and
        // b's second play in this occurrence, moved from 6 to 8 (B = 9610).
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
        let mut second_b = in_play(&child, 1);
        second_b.repeats.push(RepeatInstance {
            node: inner.clone(),
            iteration: shared[1].clone(),
        });
        assert_eq!(anchored(&after, &second_b), ExactRatio::integer(9610));

        // A gap-only change keeps the count and replaces the gap set: the
        // named Hold ends and the default shortens to 1f.
        let (request, ..) = occurrence(
            &after,
            "shorter",
            InstancePath {
                node: inner.clone(),
                repeats: second_play("inner").repeats,
            },
            OccurrenceEdit::SetRepeatGaps {
                plays: 3,
                gap: Some(recipe(1)),
                branches: vec![],
                timing: timing("shorter"),
            },
        );
        let shorter = edit(&after, request);
        assert_eq!(shorter.duration().unwrap(), duration(17));
        assert!(!shorter.nodes().contains_key(&id("gap-hold")));
        assert!(shorter.gap_overrides().get(&inner).is_none());
        assert_eq!(plays_of(&shorter, &inner), counted);
        assert_eq!(plays_of(&shorter, &id("inner")), shared);
    })
}

#[test]
fn setting_the_same_gaps_in_one_play_refuses_without_an_edit() {
    let before = nested();
    let same = OccurrenceEdit::SetRepeatGaps {
        plays: 2,
        gap: None,
        branches: vec![],
        timing: timing("same"),
    };
    let error = before
        .occurrence_structure_requirements(&second_play("inner"), &same)
        .unwrap_err();
    assert!(
        error.message.contains("already has these plays and gaps"),
        "{error}"
    );
    let request = request(
        &before,
        "same",
        Command::EditOccurrence {
            instance: second_play("inner"),
            edit: same,
            identities: OccurrenceIdentities {
                nodes: (0..8).map(|n| id(&format!("same-{n}"))).collect(),
                marks: vec![],
            },
        },
    );
    let error = apply(&before, &request).unwrap_err();
    assert!(
        error.message.contains("already has these plays and gaps"),
        "{error}"
    );
    // A gap after the last play is dormant and cannot be named.
    let (request, ..) = occurrence(
        &before,
        "last",
        second_play("inner"),
        gap_edit("last", 3, Some(1), &[(2, "late", 1)]),
    );
    let error = apply(&before, &request).unwrap_err();
    assert!(error.message.contains("not the last"), "{error}");
}

fn slice_pool(name: &str, slice: &CapturedEditSlice) -> SlicePasteIdentities {
    let needs = slice.identity_requirements().unwrap();
    SlicePasteIdentities {
        authored: OccurrenceIdentities {
            nodes: (0..needs.nodes)
                .map(|index| id(&format!("{name}-node-{index}")))
                .collect(),
            marks: (0..needs.marks)
                .map(|index| MarkId::new(format!("{name}-mark-{index}")).unwrap())
                .collect(),
        },
        aliases: (0..needs.aliases)
            .map(|index| id(&format!("{name}-alias-{index}")))
            .collect(),
    }
}

fn in_ordinal(node: &str, ordinal: u32) -> InstancePath {
    in_play(&id(node), ordinal)
}

#[test]
fn copying_in_one_play_captures_that_occurrence_without_an_edit() {
    let before = played();
    // b plays at frames 1, 4 and 7; play 2 holds it over [4, 5).
    let slice =
        CapturedEditSlice::capture_occurrence(&before, &second_play("b"), timing("copy")).unwrap();
    assert_eq!(
        slice.range(),
        FrameRange::new(ProjectFrame(4), ProjectFrame(5)).unwrap()
    );
    assert_eq!(slice.parent(), &id("body"));
    assert_eq!(
        slice.selection(),
        &SliceCaptureSelection::Child { node: id("b") }
    );
    assert_eq!(slice.plays(), second_play("b").repeats.as_slice());
    assert_eq!(slice.duration(), duration(1));
    // The payload names its occurrence and recaptures exactly from it.
    slice.validate_capture(&before).unwrap();
    let wire = slice.to_json().unwrap();
    assert_eq!(CapturedEditSlice::from_json(&wire).unwrap(), slice);
    // Each play is a distinct capture of the same shared definition.
    let third = CapturedEditSlice::capture_occurrence(&before, &in_ordinal("b", 2), timing("copy"))
        .unwrap();
    assert_ne!(third, slice);
    assert_eq!(
        third.range(),
        FrameRange::new(ProjectFrame(7), ProjectFrame(8)).unwrap()
    );
    // The whole play contents have no group to copy from.
    let whole =
        CapturedEditSlice::capture_occurrence(&before, &second_play("body"), timing("copy"))
            .unwrap_err();
    assert!(whole.message.contains("whole contents"), "{whole}");
}

#[test]
fn pasting_a_play_copy_in_another_play_changes_only_that_occurrence_and_keeps_later_clocks() {
    deadpan_core::with_reference_timing_representation(|| {
        let before = played();
        let slice =
            CapturedEditSlice::capture_occurrence(&before, &second_play("b"), timing("copy"))
                .unwrap();
        let paste = OccurrenceEdit::PasteSlice {
            before: false,
            identities: slice_pool("paste", &slice),
            slice: Box::new(slice.clone()),
            timing: timing("paste"),
            at: None,
            splits: SplitIdentities::default(),
        };
        // `p` on c in play 3 inserts the copy after it, at frame 9.
        let (request, target, presentation) =
            occurrence(&before, "paste", in_ordinal("c", 2), paste);
        let after = edit(&before, request);
        assert_eq!(after.duration().unwrap(), duration(12));
        // Plays 1 and 2 keep the shared definition.
        assert_eq!(kids(&after, &id("body")), vec![id("a"), id("b"), id("c")]);
        assert_eq!(play_root(&after, 0), id("body"));
        assert_eq!(play_root(&after, 1), id("body"));
        let owned = kids(&after, &play_root(&after, 2));
        assert_eq!(owned.len(), 4);
        assert_eq!(owned[3], id("paste-node-0"));
        assert_eq!(target.node, id("paste-node-0"));
        assert_eq!(presentation.node, id("paste-node-0"));
        assert!(target.matches_instance(&after, &presentation).unwrap());
        // As at the root, the copy arrives as one group holding the beat.
        let pasted = kids(&after, &id("paste-node-0"));
        assert_eq!(pasted.len(), 1);
        assert_eq!(
            after.nodes()[&pasted[0]].kind,
            before.nodes()[&id("b")].kind
        );
        // The tail started at frame 9, B(9) = 14414. It now starts at frame
        // 10 and keeps that entry instead of re-rounding to B(10) = 16016.
        assert_eq!(
            reference_at_anchor(&after, &id("tail")),
            ExactRatio::integer(14414)
        );
        // Copying the pasted beat back out of play 3 captures exactly it.
        let copied = CapturedEditSlice::capture_occurrence(
            &after,
            &presentation,
            AudioTimingId {
                allocation: revision("again"),
                ordinal: 0,
            },
        )
        .unwrap();
        assert_eq!(
            copied.range(),
            FrameRange::new(ProjectFrame(9), ProjectFrame(10)).unwrap()
        );
        assert_eq!(copied.parent(), &play_root(&after, 2));
    })
}

#[test]
fn pasting_an_original_moment_before_a_beat_in_one_play_keeps_later_plays_clocks() {
    deadpan_core::with_reference_timing_representation(|| {
        let before = played();
        let NodeKind::Source { source } = source(2).kind else {
            panic!("Source")
        };
        let paste = OccurrenceEdit::PasteSource {
            before: true,
            source: Box::new(source.clone()),
            id: id("moment"),
            label: "Media [0..2)".into(),
            timing: timing("moment"),
            at: None,
            splits: SplitIdentities::default(),
        };
        // `P` on a in play 1 inserts two frames at frame 0.
        let (request, target, _) = occurrence(&before, "moment", in_ordinal("a", 0), paste);
        let after = edit(&before, request);
        assert_eq!(after.duration().unwrap(), duration(13));
        assert_eq!(target.node, id("moment"));
        let owned = kids(&after, &play_root(&after, 0));
        assert_eq!(owned[0], id("moment"));
        assert_eq!(owned.len(), 4);
        assert_eq!(kids(&after, &id("body")), vec![id("a"), id("b"), id("c")]);
        assert_eq!(after.nodes()[&id("moment")].label, "Media [0..2)");
        // Shared a started play 2 at frame 3, B(3) = 4805, and now at 5.
        assert_eq!(
            anchored(&after, &in_ordinal("a", 1)),
            ExactRatio::integer(4805)
        );
        assert_eq!(
            reference_at_anchor(&after, &id("tail")),
            ExactRatio::integer(14414)
        );
    })
}

#[test]
fn one_play_pastes_refuse_whole_contents_and_root_beats_readably() {
    let before = played();
    let slice =
        CapturedEditSlice::capture_occurrence(&before, &second_play("b"), timing("copy")).unwrap();
    let paste = OccurrenceEdit::PasteSlice {
        before: false,
        identities: slice_pool("paste", &slice),
        slice: Box::new(slice),
        timing: timing("paste"),
        at: None,
        splits: SplitIdentities::default(),
    };
    let whole = before
        .occurrence_structure_requirements(&second_play("body"), &paste)
        .unwrap_err();
    assert!(whole.message.contains("whole contents"), "{whole}");
    let root = InstancePath {
        node: id("tail"),
        repeats: vec![],
    };
    assert!(
        before
            .occurrence_structure_requirements(&root, &paste)
            .is_err()
    );
}

/// root = [rep, tail]; rep plays body = [a, b] (three frames each) three
/// times, at frames 0, 6 and 12; tail starts at 18, B(18) = 28829.
fn long_plays() -> ProjectDocument {
    tree(
        &["rep", "tail"],
        vec![
            ("rep", repeat("body")),
            ("body", BeatNode::sequence("Body", vec![id("a"), id("b")])),
            ("a", source(3)),
            ("b", source(3)),
            ("tail", source(2)),
        ],
    )
}

/// Place slice in one play: the plan sizes the Split pool before isolation,
/// then one `EditOccurrence` carries the explicit destination.
fn placed(
    document: &ProjectDocument,
    name: &str,
    instance: InstancePath,
    at: OccurrencePlacement,
    content: impl Fn(SplitIdentities) -> OccurrenceEdit,
) -> (
    CommandRequest,
    ScopedNodeTarget,
    InstancePath,
    OccurrencePlacementPlan,
) {
    let edit = content(SplitIdentities::default());
    let slice = match &edit {
        OccurrenceEdit::PasteSlice { slice, .. } => Some(slice.as_ref().clone()),
        _ => None,
    };
    let plan = document
        .occurrence_placement(&instance, at, slice.as_ref())
        .unwrap();
    let edit = content(pool(&format!("{name}-split"), plan.required_ids));
    let (request, target, presentation) = occurrence(document, name, instance, edit);
    (request, target, presentation, plan)
}

fn moment(frames: i64) -> Box<SourceNode> {
    let NodeKind::Source { source } = source(frames).kind else {
        panic!("Source")
    };
    Box::new(source)
}

#[test]
fn place_slice_inside_a_beat_of_one_play_splits_there_only_and_keeps_later_clocks() {
    deadpan_core::with_reference_timing_representation(|| {
        let before = long_plays();
        // Copy a as it plays in play 1, then place it one frame inside b in
        // play 2 (b plays there over [9, 12)).
        let first = InstancePath {
            node: id("a"),
            repeats: second_play("a")
                .repeats
                .into_iter()
                .map(|mut step| {
                    step.iteration.ordinal = 0;
                    step
                })
                .collect(),
        };
        let slice = CapturedEditSlice::capture_occurrence(&before, &first, timing("copy")).unwrap();
        let at = OccurrencePlacement::Interior {
            index: 1,
            at: duration(1),
        };
        let (request, target, presentation, plan) =
            placed(&before, "place", second_play("b"), at, |splits| {
                OccurrenceEdit::PasteSlice {
                    before: false,
                    identities: slice_pool("place", &slice),
                    slice: Box::new(slice.clone()),
                    timing: timing("place"),
                    at: Some(at),
                    splits,
                }
            });
        assert_eq!(plan.parent, id("body"));
        assert_eq!(plan.origin, ProjectFrame(6));
        assert_eq!(plan.boundary, ProjectFrame(10));
        assert_eq!(plan.removed, None);
        assert!(plan.required_ids > 0);
        let after = edit(&before, request);
        assert_eq!(after.duration().unwrap(), duration(23));
        // Plays 1 and 3 keep the shared definition; play 2 owns its group.
        assert_eq!(kids(&after, &id("body")), vec![id("a"), id("b")]);
        assert_eq!(play_root(&after, 0), id("body"));
        assert_eq!(play_root(&after, 2), id("body"));
        let owned = kids(&after, &play_root(&after, 1));
        assert_eq!(owned.len(), 4, "a, b's left part, the copy, b's right part");
        assert_eq!(owned[2], id("place-node-0"));
        assert_eq!(target.node, id("place-node-0"));
        assert!(target.matches_instance(&after, &presentation).unwrap());
        assert_eq!(
            after.occurrence_structure_interval(&presentation).unwrap(),
            (ProjectFrame(10), ProjectFrame(13))
        );
        // Play 3 still starts at B(12) = 19219, now at frame 15, and the tail
        // keeps B(18) = 28829 at frame 21 instead of re-rounding to 33634.
        assert_eq!(
            anchored(&after, &in_ordinal("a", 2)),
            ExactRatio::integer(19219)
        );
        assert_eq!(
            reference_at_anchor(&after, &id("tail")),
            ExactRatio::integer(28829)
        );
    })
}

#[test]
fn place_slice_replaces_a_range_of_one_play_with_an_original_moment() {
    deadpan_core::with_reference_timing_representation(|| {
        let before = long_plays();
        // Replace [2, 4) of play 2's group, across the a/b seam, by one frame.
        let at = OccurrencePlacement::Replace {
            start: duration(2),
            end: duration(4),
        };
        let (request, target, presentation, plan) =
            placed(&before, "swap", second_play("a"), at, |splits| {
                OccurrenceEdit::PasteSource {
                    before: false,
                    source: moment(1),
                    id: id("swap-moment"),
                    label: "Media [0..1)".into(),
                    timing: timing("swap"),
                    at: Some(at),
                    splits,
                }
            });
        assert_eq!(
            plan.removed,
            Some(FrameRange::new(ProjectFrame(8), ProjectFrame(10)).unwrap())
        );
        let after = edit(&before, request);
        assert_eq!(after.duration().unwrap(), duration(19));
        assert_eq!(kids(&after, &id("body")), vec![id("a"), id("b")]);
        assert_eq!(play_root(&after, 0), id("body"));
        assert_eq!(play_root(&after, 2), id("body"));
        let owned = kids(&after, &play_root(&after, 1));
        assert_eq!(owned.len(), 3, "a's left part, the moment, b's right part");
        assert_eq!(owned[1], id("swap-moment"));
        assert_eq!(target.node, id("swap-moment"));
        assert_eq!(
            after.occurrence_structure_interval(&presentation).unwrap(),
            (ProjectFrame(8), ProjectFrame(9))
        );
        assert_eq!(
            anchored(&after, &in_ordinal("a", 2)),
            ExactRatio::integer(19219)
        );
        assert_eq!(
            reference_at_anchor(&after, &id("tail")),
            ExactRatio::integer(28829)
        );
    })
}

#[test]
fn place_slice_inserts_an_original_moment_inside_and_at_seams_of_one_play() {
    deadpan_core::with_reference_timing_representation(|| {
        let before = long_plays();
        for (name, at, boundary) in [
            (
                "inside",
                OccurrencePlacement::Interior {
                    index: 0,
                    at: duration(2),
                },
                8,
            ),
            ("seam", OccurrencePlacement::Seam { index: 2 }, 12),
        ] {
            let (request, target, presentation, plan) =
                placed(&before, name, second_play("b"), at, |splits| {
                    OccurrenceEdit::PasteSource {
                        before: false,
                        source: moment(2),
                        id: id(&format!("{name}-moment")),
                        label: "Media [0..2)".into(),
                        timing: timing(name),
                        at: Some(at),
                        splits,
                    }
                });
            assert_eq!(plan.boundary, ProjectFrame(boundary));
            let after = edit(&before, request);
            assert_eq!(after.duration().unwrap(), duration(22));
            assert_eq!(kids(&after, &id("body")), vec![id("a"), id("b")]);
            assert_eq!(play_root(&after, 2), id("body"));
            assert_eq!(target.node, id(&format!("{name}-moment")));
            assert_eq!(
                after.occurrence_structure_interval(&presentation).unwrap(),
                (ProjectFrame(boundary), ProjectFrame(boundary + 2))
            );
            assert_eq!(
                anchored(&after, &in_ordinal("a", 2)),
                ExactRatio::integer(19219)
            );
            assert_eq!(
                reference_at_anchor(&after, &id("tail")),
                ExactRatio::integer(28829)
            );
        }
    })
}

#[test]
fn place_slice_in_one_play_refuses_speed_changes_and_bad_destinations_readably() {
    let mut before = long_plays();
    // body = [a, slow]; slow plays b's three frames over six.
    let mut nodes: Vec<(&str, BeatNode)> = vec![
        ("rep", repeat("body")),
        (
            "body",
            BeatNode::sequence("Body", vec![id("a"), id("slow")]),
        ),
        ("a", source(3)),
        ("inner", BeatNode::sequence("Inner", vec![id("b")])),
        ("b", source(3)),
        ("tail", source(2)),
    ];
    let mut slow = BeatNode::sequence("Slow", vec![]);
    slow.kind = NodeKind::Retime {
        child: id("inner"),
        duration: duration(6),
        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(3)).unwrap(),
        pitch: PitchPolicy::FollowSpeed,
        purpose: RetimePurpose::Edit,
    };
    nodes.push(("slow", slow));
    let retimed = tree(&["rep", "tail"], nodes);
    let error = retimed
        .occurrence_placement(
            &second_play("b"),
            OccurrencePlacement::Seam { index: 0 },
            None,
        )
        .unwrap_err();
    assert!(error.message.contains("speed change"), "{error}");
    // Destinations outside the group, empty ranges and seams with Split
    // names refuse before any edit.
    assert!(
        before
            .occurrence_placement(
                &second_play("a"),
                OccurrencePlacement::Seam { index: 3 },
                None
            )
            .is_err()
    );
    assert!(
        before
            .occurrence_placement(
                &second_play("a"),
                OccurrencePlacement::Interior {
                    index: 0,
                    at: duration(3)
                },
                None
            )
            .is_err()
    );
    assert!(
        before
            .occurrence_placement(
                &second_play("a"),
                OccurrencePlacement::Replace {
                    start: duration(2),
                    end: duration(2)
                },
                None
            )
            .is_err()
    );
    // An explicit destination never also pastes before the beat.
    let both = OccurrenceEdit::PasteSource {
        before: true,
        source: moment(1),
        id: id("both"),
        label: "Media".into(),
        timing: timing("both"),
        at: Some(OccurrencePlacement::Seam { index: 0 }),
        splits: SplitIdentities::default(),
    };
    let needs = before
        .occurrence_structure_requirements(&second_play("a"), &both)
        .unwrap();
    let request = request(
        &before,
        "both",
        Command::EditOccurrence {
            instance: second_play("a"),
            edit: both,
            identities: OccurrenceIdentities {
                nodes: (0..needs.nodes)
                    .map(|index| id(&format!("both-copy-{index}")))
                    .collect(),
                marks: vec![],
            },
        },
    );
    let error = apply(&before, &request).unwrap_err();
    assert!(error.message.contains("before the beat"), "{error}");
    before = long_plays();
    assert_eq!(before.duration().unwrap(), duration(20));
}

#[test]
fn place_slice_replaces_a_range_of_one_play_with_an_edited_copy() {
    deadpan_core::with_reference_timing_representation(|| {
        let before = long_plays();
        let slice =
            CapturedEditSlice::capture_occurrence(&before, &in_ordinal("b", 2), timing("copy"))
                .unwrap();
        // Replace play 2's [1, 5) with play 3's copy of b (three frames).
        let at = OccurrencePlacement::Replace {
            start: duration(1),
            end: duration(5),
        };
        let (request, target, presentation, _) =
            placed(&before, "over", second_play("b"), at, |splits| {
                OccurrenceEdit::PasteSlice {
                    before: false,
                    identities: slice_pool("over", &slice),
                    slice: Box::new(slice.clone()),
                    timing: timing("over"),
                    at: Some(at),
                    splits,
                }
            });
        let after = edit(&before, request);
        assert_eq!(after.duration().unwrap(), duration(19));
        assert_eq!(play_root(&after, 0), id("body"));
        assert_eq!(play_root(&after, 2), id("body"));
        assert_eq!(target.node, id("over-node-0"));
        assert_eq!(
            after.occurrence_structure_interval(&presentation).unwrap(),
            (ProjectFrame(7), ProjectFrame(10))
        );
        assert_eq!(
            anchored(&after, &in_ordinal("a", 2)),
            ExactRatio::integer(19219)
        );
        assert_eq!(
            reference_at_anchor(&after, &id("tail")),
            ExactRatio::integer(28829)
        );
    })
}
