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

fn kids(document: &ProjectDocument, node: &NodeId) -> Vec<NodeId> {
    document.children(node).cloned().collect()
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

fn moment(frames: i64) -> Box<SourceNode> {
    let NodeKind::Source { source } = source(frames).kind else {
        panic!("Source")
    };
    Box::new(source)
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
