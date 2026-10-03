use super::*;
use SemanticInstruction::{BeginSelection, ClearSelection, FinishSelection, MoveScope};

fn visual(anchor: i64, head: i64, extending: bool) -> SemanticVisualSelection {
    SemanticVisualSelection::Time {
        anchor: ProjectFrame(anchor),
        head: ProjectFrame(head),
        extending,
    }
}
fn with_visual(cursor: i64, anchor: i64, head: i64, extending: bool) -> SemanticContext {
    SemanticContext {
        selected_child: Some(node("held")),
        visual_selection: Some(visual(anchor, head, extending)),
        ..context("root", cursor)
    }
}
fn yank(register: char) -> SemanticInstruction {
    SemanticInstruction::YankSelection {
        register: name(register),
    }
}
fn cut_range(register: char) -> SemanticInstruction {
    SemanticInstruction::CutSelection {
        register: name(register),
    }
}
fn replace(register: char) -> SemanticInstruction {
    SemanticInstruction::ReplaceSelection {
        register: name(register),
    }
}
fn paste(register: char) -> SemanticInstruction {
    SemanticInstruction::Paste {
        register: name(register),
        before: false,
    }
}
fn beats(forward: bool, count: u32) -> SemanticInstruction {
    SemanticInstruction::MoveBeats {
        forward,
        count: NonZeroU32::new(count).unwrap(),
    }
}
fn copied(planned: &SemanticPlan, register: char) -> &CapturedEditSlice {
    let RegisterValue::Edited { slice } = planned.register_writes[&name(register)].as_ref() else {
        panic!()
    };
    slice
}
fn copied_bank(document: &ProjectDocument) -> BTreeMap<RegisterName, Arc<RegisterValue>> {
    BTreeMap::from([(
        name('a'),
        Arc::new(RegisterValue::Edited {
            slice: Arc::new(
                CapturedEditSlice::capture(
                    document,
                    document.root(),
                    range(1, 3),
                    AudioTimingId {
                        allocation: revision("historical-copy"),
                        ordinal: 0,
                    },
                )
                .unwrap(),
            ),
        }),
    )])
}

#[test]
fn reverse_selection_yank_finishes_without_moving_and_remains_bank_only() {
    let document = fixture(10);
    let planned = plan(
        &document,
        context("root", 8),
        vec![BeginSelection, motion(false, 5), yank('a'), motion(true, 6)],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(planned.context.cursor, ProjectFrame(9));
    assert_eq!(planned.context.visual_selection, Some(visual(8, 3, false)));
    assert_eq!(planned.document, document);
    assert_eq!(
        planned.trace[1].after.visual_selection,
        Some(visual(8, 3, true))
    );
    assert_eq!(
        planned.trace[2].before.cursor,
        planned.trace[2].after.cursor
    );
    assert_eq!(
        planned.trace[2].before.selected_child,
        planned.trace[2].after.selected_child
    );
    assert_eq!(planned.trace[2].resolved_range, Some(range(3, 8)));
    assert_eq!(planned.trace[2].captured_child_label, None);
    assert_eq!(
        copied(&planned, 'a').selection(),
        &SliceCaptureSelection::Range { range: range(3, 8) }
    );
    assert_eq!(copied(&planned, 'a').revision_id(), document.revision_id());
    assert_eq!(
        planned.register_writes[&name('a')],
        planned.register_writes[&name('"')]
    );
    let replayed =
        crate::replay_compound::<EditError>(&document, planned.request.as_ref().unwrap(), |_| {
            Ok(())
        })
        .unwrap();
    assert_eq!(replayed.register_writes, planned.register_writes);
    assert_eq!(
        replayed.edit.inverse.apply(&replayed.document).unwrap(),
        document
    );
}

#[test]
fn empty_finished_and_absent_selections_have_distinct_behavior() {
    let document = fixture(5);
    let planned = plan(
        &document,
        context("root", 2),
        vec![
            ClearSelection,
            BeginSelection,
            FinishSelection,
            motion(true, 1),
        ],
        &BTreeMap::new(),
    )
    .unwrap();
    assert!(planned.request.is_none());
    assert_eq!(planned.context.visual_selection, Some(visual(2, 2, false)));
    assert_eq!(planned.context.cursor, ProjectFrame(3));
    for entry in [context("root", 2), with_visual(2, 2, 2, true)] {
        for instruction in [yank('a'), cut_range('a'), replace('a')] {
            let error = plan(
                &document,
                entry.clone(),
                vec![instruction],
                &copied_bank(&document),
            )
            .unwrap_err();
            assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
        }
    }
    assert_eq!(
        plan(
            &document,
            context("root", 2),
            vec![FinishSelection],
            &BTreeMap::new()
        )
        .unwrap_err()
        .code,
        EditErrorCode::SelectionUnavailable
    );
    let restarted = plan(
        &document,
        with_visual(4, 3, 1, false),
        vec![BeginSelection],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(restarted.context.visual_selection, Some(visual(4, 4, true)));
    let cleared = plan(
        &document,
        restarted.context,
        vec![ClearSelection],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(cleared.context.visual_selection, None);
}

#[test]
fn invalid_visual_context_is_rejected_before_resolution_or_allocation() {
    let document = fixture(5);
    for entry in [
        with_visual(2, -1, 2, true),
        with_visual(2, 6, 2, true),
        with_visual(2, 1, 6, false),
        with_visual(2, 1, 3, true),
        with_visual(-1, 1, 3, false),
    ] {
        let error = plan_semantic(
            &document,
            &entry,
            &program(vec![ClearSelection]),
            SemanticRegisterBank {
                entries: &BTreeMap::new(),
                version: 0,
            },
            revision("outer"),
            |_| panic!("invalid context must fail before allocation"),
            no_original,
        )
        .unwrap_err();
        assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
    }
    let valid = plan(
        &document,
        with_visual(5, 4, 1, false),
        vec![yank('a')],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(valid.context.cursor, ProjectFrame(5));
    assert_eq!(valid.context.visual_selection, Some(visual(4, 1, false)));
}

#[test]
fn beat_and_scope_motions_match_explicit_empty_siblings_and_boundary_fallback() {
    let document = tree(
        &["first", "a", "middle", "b", "last"],
        vec![
            ("first", BeatNode::sequence("First", vec![])),
            ("a", hold(2)),
            ("middle", BeatNode::sequence("Middle", vec![])),
            ("b", hold(3)),
            ("last", BeatNode::sequence("Last", vec![])),
        ],
    );
    for (selection, cursor, instruction, expected, at) in [
        (Some("middle"), 4, beats(true, 1), "b", 2),
        (Some("first"), 4, beats(false, u32::MAX), "first", 0),
        (Some("a"), 1, beats(false, 1), "first", 0),
        (None, 2, beats(false, 1), "middle", 2),
        (Some("a"), 1, beats(true, u32::MAX), "last", 5),
        (Some("last"), 5, MoveScope { end: false }, "a", 0),
        (Some("a"), 1, MoveScope { end: true }, "last", 5),
    ] {
        let entry = SemanticContext {
            selected_child: selection.map(node),
            visual_selection: Some(visual(cursor, cursor, true)),
            ..context("root", cursor)
        };
        let planned = plan(&document, entry, vec![instruction], &BTreeMap::new()).unwrap();
        assert_eq!(planned.context.selected_child, Some(node(expected)));
        assert_eq!(planned.context.cursor, ProjectFrame(at));
        assert_eq!(
            planned.context.visual_selection,
            Some(visual(cursor, at, true))
        );
        assert!(planned.request.is_none());
    }
    let empty = tree(&[], vec![]);
    let planned = plan(
        &empty,
        context("root", 0),
        vec![BeginSelection, beats(true, 1), MoveScope { end: true }],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(planned.context.selected_child, None);
    assert_eq!(planned.context.visual_selection, Some(visual(0, 0, true)));
    let all_empty = tree(
        &["first", "last"],
        vec![
            ("first", BeatNode::sequence("First", vec![])),
            ("last", BeatNode::sequence("Last", vec![])),
        ],
    );
    let planned = plan(
        &all_empty,
        context("root", 0),
        vec![MoveScope { end: false }, beats(false, 1)],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(planned.trace[0].after.selected_child, Some(node("last")));
    assert_eq!(planned.context.selected_child, Some(node("first")));
}

#[test]
fn staged_visual_yank_replace_cut_and_paste_keep_composites_and_one_exact_inverse() {
    let mut repeat = hold(2);
    repeat.kind = NodeKind::Repeat {
        child: node("repeated"),
        iterations: crate::IterationOrder::new(revision("plays"), 3).unwrap(),
        gap: None,
    };
    let document = tree(
        &["prefix", "group", "suffix"],
        vec![
            ("prefix", hold(3)),
            (
                "group",
                BeatNode::sequence("Group", vec![node("a"), node("repeat"), node("tail")]),
            ),
            ("a", hold(6)),
            ("repeat", repeat),
            ("repeated", hold(2)),
            ("tail", hold(6)),
            ("suffix", hold(4)),
        ],
    );
    let planned = plan(
        &document,
        context("group", 15),
        vec![
            BeginSelection,
            motion(false, 8),
            yank('a'),
            MoveScope { end: true },
            BeginSelection,
            motion(false, 3),
            replace('a'),
            BeginSelection,
            motion(true, 8),
            cut_range('b'),
            paste('b'),
        ],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(planned.document.duration().unwrap().frames(), 30);
    assert_eq!(planned.context.cursor, ProjectFrame(18));
    assert_eq!(planned.context.selected_child, Some(node("paste-3-0")));
    assert_eq!(planned.context.visual_selection, None);
    assert_eq!(planned.trace[6].removed_range, Some(range(18, 21)));
    assert_eq!(planned.trace[6].resolved_range, Some(range(18, 26)));
    assert_eq!(
        planned.trace[6].after.selected_child,
        Some(node("paste-1-0"))
    );
    assert_eq!(planned.trace[9].resolved_range, Some(range(18, 26)));
    assert_eq!(copied(&planned, 'a').range(), range(7, 15));
    assert_eq!(copied(&planned, 'b').revision_id(), &revision("leaf-1"));
    for sibling in ["prefix", "suffix"] {
        assert_eq!(
            planned.document.nodes()[&node(sibling)],
            document.nodes()[&node(sibling)]
        );
    }
    assert_eq!(
        planned
            .document
            .nodes()
            .values()
            .filter(|node| matches!(node.kind, NodeKind::Repeat { .. }))
            .count(),
        2
    );
    let request = planned.request.as_ref().unwrap();
    let Command::Compound { transaction } = &request.command else {
        panic!()
    };
    assert_eq!(transaction.steps().len(), 4);
    let replayed = crate::replay_compound::<EditError>(&document, request, |_| Ok(())).unwrap();
    assert_eq!(replayed.document, planned.document);
    assert_eq!(replayed.register_writes, planned.register_writes);
    assert_eq!(
        replayed.edit.inverse.apply(&replayed.document).unwrap(),
        document
    );
}

#[test]
fn replacements_use_exact_disjoint_pools_and_seam_pastes_require_zero_splits() {
    let document = fixture(10);
    let bank = copied_bank(&document);
    for failure in 0..5 {
        let error = plan_semantic(
            &document,
            &with_visual(9, 3, 7, false),
            &program(vec![replace('a')]),
            SemanticRegisterBank {
                entries: &bank,
                version: 0,
            },
            revision("outer"),
            |request| {
                let mut allocation = allocate(request)?;
                let SemanticAllocation::PasteEdited {
                    identities,
                    split_identities,
                    ..
                } = &mut allocation
                else {
                    panic!()
                };
                assert!(split_identities.nodes.len() >= 2);
                match failure {
                    0 => {
                        split_identities.nodes.pop();
                    }
                    1 => split_identities.nodes.push(node("extra")),
                    2 => split_identities.nodes[0] = identities.authored.nodes[0].clone(),
                    3 => split_identities.nodes[0] = node("held"),
                    4 => split_identities.nodes[0] = split_identities.nodes[1].clone(),
                    _ => unreachable!(),
                }
                Ok(allocation)
            },
            no_original,
        )
        .unwrap_err();
        assert_eq!(
            error.code,
            if failure <= 1 {
                EditErrorCode::InvalidCommand
            } else {
                EditErrorCode::IdentityConflict
            }
        );
    }
    let error = plan_semantic(
        &document,
        &with_visual(9, 3, 7, false),
        &program(vec![paste('a')]),
        SemanticRegisterBank {
            entries: &bank,
            version: 0,
        },
        revision("outer"),
        |request| {
            let mut allocation = allocate(request)?;
            let SemanticAllocation::PasteEdited {
                split_identities, ..
            } = &mut allocation
            else {
                panic!()
            };
            split_identities.nodes.push(node("extra"));
            Ok(allocation)
        },
        no_original,
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::InvalidCommand);
    let inserted = plan(
        &document,
        with_visual(9, 3, 7, false),
        vec![paste('a')],
        &bank,
    )
    .unwrap();
    assert_eq!(inserted.document.duration().unwrap().frames(), 12);
    assert_eq!(inserted.context.cursor, ProjectFrame(10));
    assert_eq!(inserted.context.visual_selection, None);
    let cut = plan(
        &document,
        with_visual(4, 3, 7, false),
        vec![super::cut(1, 'b')],
        &bank,
    )
    .unwrap();
    assert_eq!(cut.context.visual_selection, None);
    assert_eq!(cut.document.duration().unwrap().frames(), 9);
}

#[test]
fn replacement_refuses_empty_copies_and_wrong_register_types_without_fallback() {
    let document = tree(
        &["empty", "held"],
        vec![
            ("empty", BeatNode::sequence("Empty", vec![])),
            ("held", hold(5)),
        ],
    );
    let mut entry = with_visual(4, 1, 3, false);
    entry.selected_child = Some(node("empty"));
    let error = plan(
        &document,
        entry.clone(),
        vec![
            SemanticInstruction::YankBeat {
                register: name('a'),
            },
            replace('a'),
        ],
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert!(error.message.contains("empty copied structure"));
    for bank in [
        BTreeMap::new(),
        BTreeMap::from([(name('a'), macro_value(vec![ClearSelection]))]),
    ] {
        let original = bank.clone();
        assert_eq!(
            plan(&document, entry.clone(), vec![replace('a')], &bank)
                .unwrap_err()
                .code,
            EditErrorCode::InvalidCommand
        );
        assert_eq!(bank, original);
    }
    assert_eq!(document.duration().unwrap().frames(), 5);
    assert_eq!(document.nodes().len(), 3);
}

#[test]
fn counted_range_body_freezes_self_overwrite_and_late_failure_is_atomic() {
    let document = fixture(4);
    let bank = BTreeMap::from([(
        name('a'),
        macro_value(vec![
            BeginSelection,
            motion(true, 1),
            yank('a'),
            cut_range('b'),
        ]),
    )]);
    let original_bank = bank.clone();
    let planned = plan(&document, context("root", 0), vec![call('a', 2)], &bank).unwrap();
    assert_eq!(planned.document.duration().unwrap().frames(), 2);
    assert_eq!(copied(&planned, 'a').revision_id(), &revision("leaf-1"));
    assert_eq!(copied(&planned, 'b').revision_id(), &revision("leaf-1"));
    assert_eq!(planned.context.visual_selection, None);
    let error = plan(
        &document,
        context("root", 0),
        vec![call('a', 2), call('a', 1)],
        &bank,
    )
    .unwrap_err();
    assert!(error.message.contains("copied content"));
    assert_eq!(document, fixture(4));
    assert_eq!(bank, original_bank);
}

#[test]
fn range_steps_and_selection_only_instructions_obey_separate_limits() {
    let document = fixture(5);
    for instruction in [yank('b'), cut_range('b'), replace('b')] {
        let bank = BTreeMap::from([(name('a'), macro_value(vec![instruction]))]);
        let error = plan_semantic(
            &document,
            &with_visual(0, 0, 1, false),
            &program(vec![call('a', 1025)]),
            SemanticRegisterBank {
                entries: &bank,
                version: 0,
            },
            revision("outer"),
            |_| panic!("resolved-step limit must precede allocation"),
            no_original,
        )
        .unwrap_err();
        assert_eq!(error.code, EditErrorCode::LimitExceeded);
        assert!(error.message.contains("resolved editing steps"));
    }
    let bank = BTreeMap::from([(name('a'), macro_value(vec![ClearSelection]))]);
    let planned = plan(&document, context("root", 0), vec![call('a', 4095)], &bank).unwrap();
    assert_eq!(planned.trace.len(), MAX_SEMANTIC_INSTRUCTION_FUEL);
    assert!(planned.request.is_none());
    assert_eq!(
        plan(&document, context("root", 0), vec![call('a', 4096)], &bank)
            .unwrap_err()
            .code,
        EditErrorCode::LimitExceeded
    );
}

#[test]
fn original_replacement_resolves_the_current_document_and_retains_exact_source() {
    let (document, value, source) = super::content::original_fixture();
    let bank = BTreeMap::from([(name('a'), value.clone())]);
    let mut called = false;
    let planned = plan_semantic(
        &document,
        &with_visual(4, 3, 1, false),
        &program(vec![
            replace('a'),
            BeginSelection,
            motion(true, 2),
            yank('b'),
        ]),
        SemanticRegisterBank {
            entries: &bank,
            version: 0,
        },
        revision("outer"),
        allocate,
        |staged, selected| {
            assert_eq!(staged, &document);
            assert_eq!(selected, value.as_ref());
            called = true;
            Ok(source.clone())
        },
    )
    .unwrap();
    assert!(called);
    assert_eq!(planned.document.duration().unwrap().frames(), 32);
    assert_eq!(planned.trace[0].removed_range, Some(range(1, 3)));
    assert_eq!(planned.trace[0].resolved_range, Some(range(1, 31)));
    assert_eq!(planned.trace[0].after.cursor, ProjectFrame(1));
    assert_eq!(
        planned.trace[0].after.selected_child,
        Some(node("original-0"))
    );
    let NodeKind::Source { source: inserted } = &planned.document.nodes()[&node("original-0")].kind
    else {
        panic!()
    };
    assert_eq!(inserted, &source);
    assert_eq!(copied(&planned, 'b').revision_id(), &revision("leaf-0"));
    let replayed =
        crate::replay_compound::<EditError>(&document, planned.request.as_ref().unwrap(), |_| {
            Ok(())
        })
        .unwrap();
    assert_eq!(
        replayed.edit.inverse.apply(&replayed.document).unwrap(),
        document
    );
}

#[test]
fn visual_wire_is_closed_and_keeps_oriented_context_explicit() {
    let body = program(vec![
        BeginSelection,
        FinishSelection,
        ClearSelection,
        yank('a'),
        cut_range('b'),
        replace('c'),
        beats(false, 2),
        MoveScope { end: true },
    ]);
    assert_eq!(
        serde_json::from_str::<SemanticProgram>(&serde_json::to_string(&body).unwrap()).unwrap(),
        body
    );
    for json in [
        r#"{"type":"begin_selection","anchor":1}"#,
        r#"{"type":"finish_selection","range":[0,1]}"#,
        r#"{"type":"clear_selection","unexpected":true}"#,
        r#"{"type":"replace_selection","register":"a","before":true}"#,
        r#"{"type":"move_beats","forward":true,"count":0}"#,
        r#"{"type":"move_scope","end":true,"count":1}"#,
    ] {
        assert!(
            serde_json::from_str::<SemanticProgram>(&format!("{{\"instructions\":[{json}]}}"))
                .is_err(),
            "{json}"
        );
    }
    let selection = visual(8, 3, false);
    assert_eq!(
        serde_json::from_str::<SemanticVisualSelection>(
            &serde_json::to_string(&selection).unwrap()
        )
        .unwrap(),
        selection
    );
    assert!(
        serde_json::from_str::<SemanticVisualSelection>(
            r#"{"anchor":1,"head":2,"extending":true,"role":"audio"}"#
        )
        .is_err()
    );
}
