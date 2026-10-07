//! `ib` and `ab`: identical picture time, different attachment selection.
use super::*;
use crate::{
    Anchor, AnchorLossPolicy, BoundaryAnchor, Caption, CaptionPlacement, ExactRatio, InsertionBias,
    SliceAttachments,
};
use SemanticTextObject::{AroundBeat, InnerBeat};

fn object(kind: SemanticTextObject) -> SemanticSelector {
    SemanticSelector::TextObject { object: kind }
}
fn yank(kind: SemanticTextObject, register: char) -> SemanticInstruction {
    SemanticInstruction::Yank {
        selector: object(kind),
        register: name(register),
    }
}
fn cut(kind: SemanticTextObject) -> SemanticInstruction {
    SemanticInstruction::Cut {
        selector: object(kind),
        register: name('a'),
    }
}
fn repeat(kind: SemanticTextObject, plays: u32) -> SemanticInstruction {
    SemanticInstruction::Repeat {
        selector: object(kind),
        plays: NonZeroU32::new(plays).unwrap(),
        escalation: None,
    }
}
fn caption(text: &str, start: i64, end: i64) -> Caption {
    Caption {
        range: range(start, end),
        text: text.into(),
        placement: CaptionPlacement::Bottom,
        reveal: None,
    }
}
fn apply_command(document: &ProjectDocument, id: &str, command: Command) -> ProjectDocument {
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(id),
        command,
    };
    let transaction = crate::apply(document, &request).unwrap();
    transaction.forward.apply(document).unwrap()
}
/// `lead` (2 f), `beat` (4 f, a caption over local 1..3 and a mark at local 2)
/// and `tail` (3 f) under the root.
fn attached() -> ProjectDocument {
    let mut beat = hold(4);
    beat.captions = vec![caption("hello", 1, 3)];
    let document = tree(
        &["lead", "beat", "tail"],
        vec![("lead", hold(2)), ("beat", beat), ("tail", hold(3))],
    );
    apply_command(
        &document,
        "marked",
        Command::SetMark {
            id: crate::MarkId::new("native-mark-m").unwrap(),
            owner: node("beat"),
            label: "m".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: node("beat"),
                    position: ExactRatio::integer(2),
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::DeleteOwned,
        },
    )
}
fn copied(plan: &SemanticPlan, register: char) -> &CapturedEditSlice {
    let RegisterValue::Edited { slice } = plan.register_writes[&name(register)].as_ref() else {
        panic!("Edited register expected")
    };
    slice
}
fn captions(document: &ProjectDocument) -> Vec<(NodeId, Vec<Caption>)> {
    document
        .nodes()
        .iter()
        .filter(|(_, beat)| !beat.captions.is_empty())
        .map(|(id, beat)| (id.clone(), beat.captions.clone()))
        .collect()
}

#[test]
fn beat_objects_resolve_the_selected_or_right_hand_beat_with_one_picture_range() {
    let document = attached();
    for kind in [InnerBeat, AroundBeat] {
        for (cursor, expected) in [
            (0, "lead"),
            (2, "beat"),
            (5, "beat"),
            (6, "tail"),
            (9, "tail"),
        ] {
            let object = document
                .resolve_group_object(&context("root", cursor), kind)
                .unwrap();
            assert_eq!(object.group, node(expected), "{kind:?} at {cursor}");
        }
        // The explicit selection wins over the cursor.
        let selected = SemanticContext {
            selected_child: Some(node("tail")),
            ..context("root", 3)
        };
        let object = document.resolve_group_object(&selected, kind).unwrap();
        assert_eq!(object.group, node("tail"));
        let target = document
            .resolve_object_selection(&node("root"), &object)
            .unwrap();
        assert_eq!(target.range, range(6, 9));
        assert_eq!(target.attachments.is_owned(), kind == AroundBeat);
    }
    // A Sequence of only empty children has no beat to guess.
    let empty = tree(
        &["only"],
        vec![("only", BeatNode::sequence("Only", vec![]))],
    );
    assert!(
        empty
            .resolve_group_object(&context("root", 0), InnerBeat)
            .is_err()
    );
    // Selecting the empty child explicitly is still an exact beat object.
    let explicit = SemanticContext {
        selected_child: Some(node("only")),
        ..context("root", 0)
    };
    assert_eq!(
        empty
            .resolve_group_object(&explicit, AroundBeat)
            .unwrap()
            .group,
        node("only")
    );
}

#[test]
fn yank_takes_attachments_only_with_ab_and_both_copies_share_their_picture_time() {
    let document = attached();
    let bank = BTreeMap::new();
    let planned = plan(
        &document,
        context("root", 3),
        vec![yank(InnerBeat, 'i'), yank(AroundBeat, 'o')],
        &bank,
    )
    .unwrap();
    // Copy-only programs leave the timeline untouched.
    assert_eq!(planned.document, document);
    let (inner, around) = (copied(&planned, 'i'), copied(&planned, 'o'));
    assert_eq!(inner.range(), range(2, 6));
    assert_eq!(inner.range(), around.range());
    assert_eq!(inner.attachments(), SliceAttachments::Excluded);
    assert_eq!(around.attachments(), SliceAttachments::Owned);
    assert_eq!(inner.identity_requirements().unwrap().marks, 0);
    assert_eq!(around.identity_requirements().unwrap().marks, 1);
    // Both are exact recaptures of the same immutable revision.
    inner.validate_capture(&document).unwrap();
    around.validate_capture(&document).unwrap();
    let inner_json = inner.to_json().unwrap();
    assert!(inner_json.contains("\"attachments\":\"excluded\""));
    assert!(!inner_json.contains("hello"));
    assert!(!around.to_json().unwrap().contains("\"attachments\""));
    assert_eq!(CapturedEditSlice::from_json(&inner_json).unwrap(), *inner);
    // A forged "excluded" copy that still carries a caption is refused.
    let forged = around.to_json().unwrap().replacen(
        "\"source_duration\"",
        "\"attachments\":\"excluded\",\"source_duration\"",
        1,
    );
    assert!(CapturedEditSlice::from_json(&forged).is_err());
    assert!(inner.outline()[0].contains("without captions"));

    // Pasting each after the tail: only the ab copy brings its caption and mark.
    for (register, carries) in [('i', false), ('o', true)] {
        let mut bank = BTreeMap::new();
        bank.insert(
            name(register),
            planned.register_writes[&name(register)].clone(),
        );
        let pasted = plan(
            &document,
            SemanticContext {
                selected_child: Some(node("tail")),
                ..context("root", 9)
            },
            vec![SemanticInstruction::Paste {
                register: name(register),
                before: false,
            }],
            &bank,
        )
        .unwrap();
        assert_eq!(pasted.document.duration().unwrap().frames(), 13);
        assert_eq!(
            captions(&pasted.document).len(),
            if carries { 2 } else { 1 }
        );
        assert_eq!(pasted.document.marks().len(), if carries { 2 } else { 1 });
        if carries {
            // The copy keeps the caption at the same beat-local frames.
            assert!(
                captions(&pasted.document)
                    .iter()
                    .all(|(_, value)| value == &vec![caption("hello", 1, 3)])
            );
        }
    }
}

#[test]
fn cutting_either_beat_object_removes_the_same_time_and_owned_attachments() {
    let document = attached();
    let bank = BTreeMap::new();
    let inner = plan(&document, context("root", 3), vec![cut(InnerBeat)], &bank).unwrap();
    let around = plan(&document, context("root", 3), vec![cut(AroundBeat)], &bank).unwrap();
    // Ripple deletion removes the host and therefore everything it owns; the
    // objects differ only in what the register keeps.
    assert_eq!(inner.document.nodes(), around.document.nodes());
    assert_eq!(inner.document.marks(), around.document.marks());
    assert!(captions(&inner.document).is_empty());
    assert_eq!(inner.document.duration().unwrap().frames(), 5);
    assert_eq!(
        copied(&inner, 'a').attachments(),
        SliceAttachments::Excluded
    );
    assert_eq!(copied(&around, 'a').attachments(), SliceAttachments::Owned);
    assert_eq!(inner.context.cursor, ProjectFrame(2));
    assert_eq!(inner.context.selected_child, Some(node("tail")));
}

#[test]
fn rib_keeps_attachments_on_the_first_play_while_rab_repeats_them() {
    let document = attached();
    let bank = BTreeMap::new();
    let around = plan(
        &document,
        context("root", 3),
        vec![repeat(AroundBeat, 3)],
        &bank,
    )
    .unwrap();
    let inner = plan(
        &document,
        context("root", 3),
        vec![repeat(InnerBeat, 3)],
        &bank,
    )
    .unwrap();
    for planned in [&around, &inner] {
        assert_eq!(planned.document.duration().unwrap().frames(), 2 + 12 + 3);
    }
    let repeat_node = node("repeat-0");
    assert_eq!(around.context.selected_child, Some(repeat_node.clone()));
    assert_eq!(inner.context.selected_child, Some(repeat_node.clone()));
    // rab: one shared definition shows the caption in every play.
    assert_eq!(
        captions(&around.document),
        vec![(node("beat"), vec![caption("hello", 1, 3)])]
    );
    assert!(around.document.overrides().is_empty());
    // rib: the shared definition (plays 2 and 3) has no caption; play 1 owns
    // an isolated copy that keeps it at the same local frames.
    assert!(inner.document.nodes()[&node("beat")].captions.is_empty());
    let NodeKind::Repeat { iterations, .. } = &inner.document.nodes()[&repeat_node].kind else {
        panic!("Repeat expected")
    };
    let overrides = &inner.document.overrides()[&repeat_node];
    assert_eq!(overrides.len(), 1);
    let first = overrides.get(&iterations.at(0).unwrap()).unwrap();
    assert_eq!(
        inner.document.nodes()[first].captions,
        vec![caption("hello", 1, 3)]
    );
    assert!(overrides.get(&iterations.at(1).unwrap()).is_none());
    // The mark moves with the caption: same identity and label, bound to the
    // first play's copy at the same local frame, with no duplicate.
    assert_eq!(inner.document.marks().len(), 1);
    let mark = &inner.document.marks()[&crate::MarkId::new("native-mark-m").unwrap()];
    assert_eq!(&mark.owner, first);
    assert_eq!(
        mark.boundary.coordinate,
        Anchor::Local {
            node: first.clone(),
            position: ExactRatio::integer(2)
        }
    );
    // One Compound: Repeat wrap plus the atomic first-play edit, one Undo.
    let Command::Compound { transaction } = &inner.request.as_ref().unwrap().command else {
        panic!("Compound expected")
    };
    assert_eq!(transaction.steps().len(), 2);
    let replay =
        crate::replay_compound::<EditError>(&document, inner.request.as_ref().unwrap(), |_| Ok(()))
            .unwrap();
    assert_eq!(replay.document, inner.document);
    assert_eq!(
        replay.edit.inverse.apply(&replay.document).unwrap(),
        document
    );
}

#[test]
fn rib_without_attachments_matches_rab_and_dot_keeps_the_object_kind() {
    let document = tree(
        &["lead", "beat"],
        vec![("lead", hold(2)), ("beat", hold(4))],
    );
    let bank = BTreeMap::new();
    let inner = plan(
        &document,
        context("root", 3),
        vec![repeat(InnerBeat, 2)],
        &bank,
    )
    .unwrap();
    let around = plan(
        &document,
        context("root", 3),
        vec![repeat(AroundBeat, 2)],
        &bank,
    )
    .unwrap();
    assert_eq!(inner.document, around.document);
    let Command::Compound { transaction } = &inner.request.as_ref().unwrap().command else {
        panic!("Compound expected")
    };
    assert_eq!(transaction.steps().len(), 1);
    // The macro wire keeps the beat object kinds distinct.
    let wire = serde_json::to_string(&program(vec![
        yank(InnerBeat, 'a'),
        SemanticInstruction::SelectObject { object: AroundBeat },
    ]))
    .unwrap();
    assert!(wire.contains("inner_beat") && wire.contains("around_beat"));
    assert_eq!(
        serde_json::from_str::<SemanticProgram>(&wire).unwrap(),
        program(vec![
            yank(InnerBeat, 'a'),
            SemanticInstruction::SelectObject { object: AroundBeat },
        ])
    );
    assert!(
        serde_json::from_str::<SemanticProgram>(
            r#"{"instructions":[{"type":"select_object","object":{"type":"inner_beat","node":"x"}}]}"#
        )
        .is_err()
    );
}

#[test]
fn visual_beat_objects_finish_and_replace_their_exact_beat() {
    let document = attached();
    let bank = BTreeMap::new();
    let selected = plan(
        &document,
        context("root", 3),
        vec![SemanticInstruction::SelectObject { object: InnerBeat }],
        &bank,
    )
    .unwrap();
    assert_eq!(selected.context.cursor, ProjectFrame(6));
    let Some(SemanticVisualSelection::Object {
        selection,
        extending,
    }) = &selected.context.visual_selection
    else {
        panic!("Object Visual expected")
    };
    assert!(*extending);
    assert_eq!(selection.group, node("beat"));
    // Visual yank of the ib object leaves attachments behind; a later Visual
    // cut of the ab object removes the beat in one step.
    let planned = plan(
        &document,
        context("root", 3),
        vec![
            SemanticInstruction::SelectObject { object: InnerBeat },
            SemanticInstruction::YankSelection {
                register: name('b'),
            },
            SemanticInstruction::SelectObject { object: AroundBeat },
            SemanticInstruction::CutSelection {
                register: name('c'),
            },
        ],
        &bank,
    )
    .unwrap();
    assert_eq!(
        copied(&planned, 'b').attachments(),
        SliceAttachments::Excluded
    );
    assert_eq!(copied(&planned, 'c').attachments(), SliceAttachments::Owned);
    assert_eq!(planned.document.duration().unwrap().frames(), 5);
}

#[test]
fn rib_moves_mark_only_hosts_and_keeps_every_logical_fragment() {
    let mut document = attached();
    document
        .nodes
        .get_mut(&node("beat"))
        .unwrap()
        .captions
        .clear();
    let id = crate::MarkId::new("native-mark-m").unwrap();
    let outside = crate::MarkFragment {
        owner: node("tail"),
        coordinate: Anchor::Local {
            node: node("tail"),
            position: ExactRatio::ONE,
        },
        state: crate::MarkState::Bound,
    };
    let unresolved = crate::MarkFragment {
        owner: node("beat"),
        coordinate: Anchor::Local {
            node: node("beat"),
            position: ExactRatio::integer(3),
        },
        state: crate::MarkState::Unresolved {
            reason: crate::MarkLossReason::OutsideMapping,
        },
    };
    let mark = document.marks.get_mut(&id).unwrap();
    mark.loss_policy = AnchorLossPolicy::KeepUnresolved;
    mark.fragments = vec![outside.clone(), unresolved.clone()];
    document.validate().unwrap();
    let result = plan(
        &document,
        context("root", 3),
        vec![repeat(InnerBeat, 3)],
        &BTreeMap::new(),
    )
    .unwrap();
    let repeat = node("repeat-0");
    let NodeKind::Repeat { iterations, .. } = &result.document.nodes()[&repeat].kind else {
        panic!()
    };
    let first = result.document.overrides()[&repeat]
        .get(&iterations.at(0).unwrap())
        .unwrap();
    assert_eq!(result.document.marks().len(), 1);
    let mark = &result.document.marks()[&id];
    assert_eq!(mark.owner, *first);
    assert_eq!(
        mark.boundary.coordinate,
        Anchor::Local {
            node: first.clone(),
            position: ExactRatio::integer(2)
        }
    );
    assert_eq!(mark.fragments[0], outside);
    assert_eq!(mark.fragments[1].owner, *first);
    // Unresolved coordinates keep the last known host; moving ownership must
    // never make an unresolved mark resolve as an incidental effect.
    assert_eq!(mark.fragments[1].coordinate, unresolved.coordinate);
    assert_eq!(mark.fragments[1].state, unresolved.state);
    let edit = crate::apply(&document, result.request.as_ref().unwrap()).unwrap();
    assert_eq!(edit.inverse.apply(&result.document).unwrap(), document);
}

#[test]
fn rib_moves_nested_default_override_and_gap_attachments_together() {
    let mut default = hold(4);
    default.captions = vec![caption("default", 1, 3)];
    let mut override_beat = hold(5);
    override_beat.captions = vec![caption("override", 2, 4)];
    let mut gap = hold(2);
    gap.captions = vec![caption("gap", 0, 1)];
    let iterations = crate::IterationOrder::new(revision("inner-plays"), 3).unwrap();
    let mut inner = BeatNode::sequence("Nested", vec![]);
    inner.kind = NodeKind::Repeat {
        child: node("default"),
        iterations: iterations.clone(),
        gap: None,
        escalation: None,
    };
    let mut document = tree(&["nested"], vec![("nested", inner), ("default", default)]);
    document.nodes.insert(node("override"), override_beat);
    document.nodes.insert(node("gap"), gap);
    document
        .overrides
        .entry(node("nested"))
        .or_default()
        .insert(iterations.at(1).unwrap(), node("override"));
    document
        .gap_overrides
        .entry(node("nested"))
        .or_default()
        .insert(iterations.at(0).unwrap(), node("gap"));
    document.validate().unwrap();
    let result = plan(
        &document,
        context("root", 0),
        vec![repeat(InnerBeat, 2)],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(
        result.document.duration().unwrap().frames(),
        document.duration().unwrap().frames() * 2
    );
    for old in ["default", "override", "gap"] {
        assert!(result.document.nodes()[&node(old)].captions.is_empty());
    }
    let copied = captions(&result.document);
    assert_eq!(copied.len(), 3);
    assert_eq!(
        copied
            .iter()
            .flat_map(|(_, captions)| captions.iter().map(|caption| caption.text.as_str()))
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["default", "override", "gap"])
    );
    let request = result.request.as_ref().unwrap();
    let wire = serde_json::to_vec(request).unwrap();
    let restored: CommandRequest = serde_json::from_slice(&wire).unwrap();
    let applied = crate::apply(&document, &restored).unwrap();
    assert_eq!(applied.forward.apply(&document).unwrap(), result.document);
    assert_eq!(applied.inverse.apply(&result.document).unwrap(), document);
}

#[test]
fn first_play_attachment_command_requires_exact_fresh_nodes_and_keeps_mark_identity() {
    let document = attached();
    let wrapped = plan(
        &document,
        context("root", 3),
        vec![repeat(AroundBeat, 3)],
        &BTreeMap::new(),
    )
    .unwrap()
    .document;
    let repeat = node("repeat-0");
    assert_eq!(wrapped.first_play_attachment_nodes(&repeat).unwrap(), 1);
    for identities in [
        crate::OccurrenceIdentities {
            nodes: vec![],
            marks: vec![],
        },
        crate::OccurrenceIdentities {
            nodes: vec![node("beat")],
            marks: vec![],
        },
        crate::OccurrenceIdentities {
            nodes: vec![node("fresh")],
            marks: vec![crate::MarkId::new("unused").unwrap()],
        },
    ] {
        let request = CommandRequest {
            project_id: wrapped.project_id().clone(),
            expected_revision: wrapped.revision_id().clone(),
            new_revision: revision("invalid"),
            command: Command::KeepFirstPlayAttachments {
                node: repeat.clone(),
                identities,
            },
        };
        assert!(crate::apply(&wrapped, &request).is_err());
    }
    let result = apply_command(
        &wrapped,
        "valid",
        Command::KeepFirstPlayAttachments {
            node: repeat.clone(),
            identities: crate::OccurrenceIdentities {
                nodes: vec![node("first")],
                marks: vec![],
            },
        },
    );
    assert_eq!(
        result.marks().keys().collect::<Vec<_>>(),
        wrapped.marks().keys().collect::<Vec<_>>()
    );
    assert!(result.first_play_attachment_nodes(&repeat).is_err());
}
