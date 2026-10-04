use super::*;

fn frames(forward: bool, count: u32) -> SemanticMotion {
    SemanticMotion::Frames {
        forward,
        count: NonZeroU32::new(count).unwrap(),
    }
}
fn beats(forward: bool, count: u32) -> SemanticMotion {
    SemanticMotion::Beats {
        forward,
        count: NonZeroU32::new(count).unwrap(),
    }
}
fn navigate(motion: SemanticMotion) -> SemanticInstruction {
    match motion {
        SemanticMotion::Frames { forward, count } => {
            SemanticInstruction::MoveFrames { forward, count }
        }
        SemanticMotion::Beats { forward, count } => {
            SemanticInstruction::MoveBeats { forward, count }
        }
        SemanticMotion::Scope { end } => SemanticInstruction::MoveScope { end },
        SemanticMotion::Words {
            forward,
            count,
            end,
        } => SemanticInstruction::MoveWords {
            forward,
            count,
            end,
        },
        SemanticMotion::Sentences { forward, count } => {
            SemanticInstruction::MoveSentences { forward, count }
        }
        SemanticMotion::Pauses { forward, count } => {
            SemanticInstruction::MovePauses { forward, count }
        }
        SemanticMotion::Shots { forward, count } => {
            SemanticInstruction::MoveShots { forward, count }
        }
    }
}
fn yank(selector: SemanticSelector, register: char) -> SemanticInstruction {
    SemanticInstruction::Yank {
        selector,
        register: name(register),
    }
}
fn cut_selector(selector: SemanticSelector, register: char) -> SemanticInstruction {
    SemanticInstruction::Cut {
        selector,
        register: name(register),
    }
}
fn motion_selector(motion: SemanticMotion) -> SemanticSelector {
    SemanticSelector::Motion { motion }
}
fn copied(planned: &SemanticPlan, register: char) -> &CapturedEditSlice {
    let RegisterValue::Edited { slice } = planned.register_writes[&name(register)].as_ref() else {
        panic!()
    };
    slice
}
fn assert_inverse(document: &ProjectDocument, planned: &SemanticPlan) {
    let replayed = crate::replay_compound::<EditError>(
        document,
        planned.request.as_ref().unwrap(),
        |_| Ok(()),
    )
    .unwrap();
    let request = planned.request.as_ref().unwrap();
    let Command::Compound { transaction } = &request.command else {
        panic!("semantic requests use Compound")
    };
    let mut expected = planned.document.clone();
    if transaction.steps().iter().all(|step| step.edit().is_none()) {
        // Generic Compound replay reserves its supplied outer revision. The
        // semantic/store bank-only path deliberately keeps the authored head.
        assert_eq!(planned.document, *document);
        expected.revision_id = request.new_revision.clone();
    }
    assert_eq!(replayed.document, expected);
    assert_eq!(replayed.register_writes, planned.register_writes);
    assert_eq!(
        replayed.edit.inverse.apply(&replayed.document).unwrap(),
        *document
    );
}

#[test]
fn motion_selectors_match_navigation_and_preserve_yank_context_in_nested_scopes() {
    let document = tree(
        &["prefix", "group", "suffix"],
        vec![
            ("prefix", hold(5)),
            (
                "group",
                BeatNode::sequence(
                    "Group",
                    vec![node("a"), node("empty"), node("b"), node("c")],
                ),
            ),
            ("a", hold(3)),
            ("empty", BeatNode::sequence("Empty", vec![])),
            ("b", hold(4)),
            ("c", hold(5)),
            ("suffix", hold(2)),
        ],
    );
    for (cursor, selected, motion, destination) in [
        (10, Some("b"), frames(true, 2), 12),
        (10, Some("b"), frames(false, 3), 7),
        (10, Some("empty"), frames(true, u32::MAX), 17),
        (10, Some("empty"), frames(false, u32::MAX), 5),
        (10, Some("b"), beats(true, 1), 12),
        (10, Some("b"), beats(false, 1), 8),
        (10, Some("b"), beats(true, u32::MAX), 12),
        (10, Some("b"), beats(false, u32::MAX), 5),
        (16, Some("c"), beats(true, 1), 12),
        (10, Some("empty"), beats(true, 1), 8),
        (8, None, beats(true, 1), 12),
        (10, None, beats(false, 1), 8),
        (10, Some("empty"), SemanticMotion::Scope { end: false }, 5),
        (10, Some("empty"), SemanticMotion::Scope { end: true }, 17),
    ] {
        let entry = SemanticContext {
            selected_child: selected.map(node),
            visual_selection: Some(SemanticVisualSelection::Time {
                anchor: ProjectFrame(6),
                head: ProjectFrame(cursor),
                extending: true,
            }),
            ..context("group", cursor)
        };
        let moved = plan(
            &document,
            entry.clone(),
            vec![navigate(motion)],
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(moved.context.cursor, ProjectFrame(destination));
        assert_eq!(
            match moved.context.visual_selection.as_ref().unwrap() {
                SemanticVisualSelection::Time { head, .. } => *head,
                _ => panic!("expected Time Visual"),
            },
            ProjectFrame(destination)
        );
        assert!(moved.request.is_none());
        let selector = motion_selector(motion);
        let copied_plan = plan(
            &document,
            entry.clone(),
            vec![yank(selector, 'a')],
            &BTreeMap::new(),
        )
        .unwrap();
        let expected = range(cursor.min(destination), cursor.max(destination));
        assert_eq!(copied_plan.context, entry);
        assert_eq!(copied_plan.document, document);
        assert_eq!(
            copied_plan.trace[0].resolved_selection,
            Some(SliceCaptureSelection::Range { range: expected })
        );
        assert_eq!(copied_plan.trace[0].captured_child_label, None);
        assert_eq!(copied(&copied_plan, 'a').range(), expected);
        copied(&copied_plan, 'a')
            .validate_capture(&document)
            .unwrap();
        assert_inverse(&document, &copied_plan);

        let cut_plan = plan(
            &document,
            entry,
            vec![cut_selector(selector, 'a')],
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(cut_plan.trace[0].resolved_range, Some(expected));
        assert_eq!(cut_plan.context.cursor, expected.start());
        assert_eq!(cut_plan.context.visual_selection, None);
        assert_eq!(
            cut_plan.document.duration().unwrap().frames(),
            19 - (expected.end().0 - expected.start().0)
        );
        assert_eq!(cut_plan.trace[0].instruction, cut_selector(selector, 'a'));
        assert_inverse(&document, &cut_plan);
    }
}

#[test]
fn selected_child_cut_uses_exact_identity_and_zero_split_pool_even_for_empty_children() {
    let document = tree(
        &["left", "first", "second", "right"],
        vec![
            ("left", hold(3)),
            ("first", BeatNode::sequence("First empty", vec![])),
            ("second", BeatNode::sequence("Second empty", vec![])),
            ("right", hold(4)),
        ],
    );
    for (child, expected_range, next_child, label) in [
        ("left", range(0, 3), "first", "Held"),
        ("first", range(3, 3), "second", "First empty"),
        ("right", range(3, 7), "second", "Held"),
    ] {
        let entry = SemanticContext {
            selected_child: Some(node(child)),
            visual_selection: Some(SemanticVisualSelection::Time {
                anchor: ProjectFrame(1),
                head: ProjectFrame(6),
                extending: false,
            }),
            ..context("root", 6)
        };
        let selected = SemanticSelector::SelectedBeat;
        let yanked = plan(
            &document,
            entry.clone(),
            vec![yank(selected, 'a')],
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(yanked.context, entry);
        assert_eq!(yanked.trace[0].captured_child_label.as_deref(), Some(label));
        let planned = plan_semantic(
            &document,
            &entry,
            &program(vec![cut_selector(selected, 'a')]),
            SemanticRegisterBank {
                entries: &BTreeMap::new(),
                version: 7,
            },
            revision("outer"),
            |request| {
                assert_eq!(
                    request,
                    SemanticAllocationRequest::Cut {
                        step_index: 0,
                        required_split_ids: 0
                    }
                );
                allocate(request)
            },
            no_original,
        )
        .unwrap();
        assert_eq!(planned.context.cursor, expected_range.start());
        assert_eq!(planned.context.selected_child, Some(node(next_child)));
        assert_eq!(planned.context.visual_selection, None);
        assert!(!planned.document.nodes().contains_key(&node(child)));
        assert_eq!(
            planned.trace[0].captured_child_label.as_deref(),
            Some(label)
        );
        assert_eq!(
            planned.trace[0].resolved_selection,
            Some(SliceCaptureSelection::Child { node: node(child) })
        );
        assert_eq!(copied(&planned, 'a').range(), expected_range);
        copied(&planned, 'a').validate_capture(&document).unwrap();
        let Command::Compound { transaction } = &planned.request.as_ref().unwrap().command else {
            panic!()
        };
        let ResolvedStep::Cut { delete, .. } = &transaction.steps()[0] else {
            panic!()
        };
        assert!(
            matches!(delete.command.as_command(), Command::DeleteRipple { node: selected, .. } if selected == &node(child))
        );
        assert_inverse(&document, &planned);
    }
    let empty = tree(
        &["one", "two"],
        vec![
            ("one", BeatNode::sequence("One", vec![])),
            ("two", BeatNode::sequence("Two", vec![])),
        ],
    );
    let planned = plan(
        &empty,
        SemanticContext {
            selected_child: Some(node("one")),
            ..context("root", 0)
        },
        vec![
            cut_selector(SemanticSelector::SelectedBeat, 'a'),
            cut_selector(SemanticSelector::SelectedBeat, 'b'),
        ],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(planned.context.selected_child, None);
    assert_eq!(planned.document.nodes().len(), 1);
    assert_eq!(
        planned.trace[1].captured_child_label.as_deref(),
        Some("Two")
    );
    assert_inverse(&empty, &planned);
}

#[test]
fn empty_motion_and_absent_selectors_refuse_before_allocating_without_child_fallback() {
    let document = tree(
        &["left", "empty", "right"],
        vec![
            ("left", hold(3)),
            ("empty", BeatNode::sequence("Empty", vec![])),
            ("right", hold(4)),
        ],
    );
    for (cursor, child, visual, selector) in [
        (2, None, None, SemanticSelector::SelectedBeat),
        (2, Some("left"), None, SemanticSelector::VisualSelection),
        (
            3,
            Some("empty"),
            Some(SemanticVisualSelection::Time {
                anchor: ProjectFrame(3),
                head: ProjectFrame(3),
                extending: false,
            }),
            SemanticSelector::VisualSelection,
        ),
        (
            7,
            Some("right"),
            None,
            motion_selector(frames(true, u32::MAX)),
        ),
        (
            0,
            Some("left"),
            None,
            motion_selector(frames(false, u32::MAX)),
        ),
        (3, Some("empty"), None, motion_selector(beats(true, 1))),
        (3, Some("right"), None, motion_selector(beats(false, 1))),
        (
            7,
            Some("right"),
            None,
            motion_selector(SemanticMotion::Scope { end: true }),
        ),
    ] {
        for cut in [false, true] {
            let instruction = if cut {
                cut_selector(selector, 'a')
            } else {
                yank(selector, 'a')
            };
            let error = plan_semantic(
                &document,
                &SemanticContext {
                    selected_child: child.map(node),
                    visual_selection: visual.clone(),
                    ..context("root", cursor)
                },
                &program(vec![instruction]),
                SemanticRegisterBank {
                    entries: &BTreeMap::new(),
                    version: 0,
                },
                revision("outer"),
                |_| panic!("empty or absent selector must fail before allocation"),
                no_original,
            )
            .unwrap_err();
            assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
        }
    }
    let empty = tree(&[], vec![]);
    for motion in [
        frames(true, 1),
        beats(false, 1),
        SemanticMotion::Scope { end: true },
    ] {
        let entry = context("root", 0);
        let moved = plan(
            &empty,
            entry.clone(),
            vec![navigate(motion)],
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(moved.context, entry);
        let error = plan(
            &empty,
            entry,
            vec![yank(motion_selector(motion), 'a')],
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
    }
}

#[test]
fn explicit_visual_yank_finishes_but_motion_yank_preserves_oriented_selection() {
    let document = fixture(10);
    let entry = SemanticContext {
        selected_child: Some(node("held")),
        visual_selection: Some(SemanticVisualSelection::Time {
            anchor: ProjectFrame(8),
            head: ProjectFrame(3),
            extending: true,
        }),
        ..context("root", 3)
    };
    let planned = plan(
        &document,
        entry.clone(),
        vec![
            yank(motion_selector(frames(true, 2)), 'a'),
            yank(SemanticSelector::VisualSelection, 'b'),
        ],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(planned.trace[0].after, entry);
    let mut finished = entry;
    let Some(SemanticVisualSelection::Time { extending, .. }) = &mut finished.visual_selection
    else {
        panic!("expected Time Visual")
    };
    *extending = false;
    assert_eq!(planned.context, finished);
    assert_eq!(copied(&planned, 'a').range(), range(3, 5));
    assert_eq!(copied(&planned, 'b').range(), range(3, 8));
    assert_eq!(planned.document, document);
}

#[test]
fn typed_selectors_resolve_each_counted_staged_document_and_freeze_called_body() {
    let document = fixture(5);
    let bank = BTreeMap::from([(
        name('a'),
        macro_value(vec![
            yank(motion_selector(frames(true, 1)), 'b'),
            cut_selector(motion_selector(frames(true, 1)), 'a'),
        ]),
    )]);
    let original_bank = bank.clone();
    let planned = plan(&document, context("root", 0), vec![call('a', 2)], &bank).unwrap();
    assert_eq!(planned.document.duration().unwrap().frames(), 3);
    assert_eq!(copied(&planned, 'a').revision_id(), &revision("leaf-1"));
    assert_eq!(copied(&planned, 'b').revision_id(), &revision("leaf-1"));
    assert_eq!(planned.trace[3].before_scope, range(0, 4));
    assert_eq!(
        planned.trace[4].resolved_selection,
        Some(SliceCaptureSelection::Range { range: range(0, 1) })
    );
    let Command::Compound { transaction } = &planned.request.as_ref().unwrap().command else {
        panic!()
    };
    assert_eq!(transaction.steps().len(), 4);
    assert_inverse(&document, &planned);
    let error = plan(
        &document,
        context("root", 0),
        vec![call('a', 2), call('a', 1)],
        &bank,
    )
    .unwrap_err();
    assert!(error.message.contains("copied content"));
    assert_eq!(bank, original_bank);
    assert_eq!(document, fixture(5));
}

#[test]
fn typed_selectors_enforce_step_limits_and_exact_child_cut_allocations() {
    let document = fixture(4);
    let entry = SemanticContext {
        selected_child: Some(node("held")),
        ..context("root", 2)
    };
    for instruction in [
        yank(SemanticSelector::SelectedBeat, 'b'),
        cut_selector(SemanticSelector::SelectedBeat, 'b'),
    ] {
        let bank = BTreeMap::from([(name('a'), macro_value(vec![instruction]))]);
        let error = plan_semantic(
            &document,
            &entry,
            &program(vec![call('a', 1025)]),
            SemanticRegisterBank {
                entries: &bank,
                version: 0,
            },
            revision("outer"),
            |_| panic!("count must fail before allocation"),
            no_original,
        )
        .unwrap_err();
        assert_eq!(error.code, EditErrorCode::LimitExceeded);
        assert!(error.message.contains("resolved editing steps"));
    }
    for reused_revision in [false, true] {
        let error = plan_semantic(
            &document,
            &entry,
            &program(vec![cut_selector(SemanticSelector::SelectedBeat, 'a')]),
            SemanticRegisterBank {
                entries: &BTreeMap::new(),
                version: 0,
            },
            revision("outer"),
            |request| {
                let mut allocation = allocate(request)?;
                let SemanticAllocation::Cut {
                    new_revision,
                    capture_revision,
                    split_identities,
                } = &mut allocation
                else {
                    panic!()
                };
                if reused_revision {
                    *capture_revision = new_revision.clone();
                } else {
                    split_identities.nodes.push(node("unused"));
                }
                Ok(allocation)
            },
            no_original,
        )
        .unwrap_err();
        assert_eq!(
            error.code,
            if reused_revision {
                EditErrorCode::IdentityConflict
            } else {
                EditErrorCode::InvalidCommand
            }
        );
    }
}

#[test]
fn typed_selector_wire_is_strict_and_retains_requested_counts() {
    let wire = r#"{"instructions":[{"type":"yank","selector":{"type":"selected_beat"},"register":"a"},{"type":"cut","selector":{"type":"visual_selection"},"register":"b"},{"type":"yank","selector":{"type":"motion","motion":{"type":"frames","forward":false,"count":4294967295}},"register":"c"},{"type":"cut","selector":{"type":"motion","motion":{"type":"beats","forward":true,"count":3}},"register":"d"},{"type":"yank","selector":{"type":"motion","motion":{"type":"scope","end":true}},"register":"e"}]}"#;
    let decoded: SemanticProgram = serde_json::from_str(wire).unwrap();
    assert_eq!(
        decoded.instructions()[2],
        yank(motion_selector(frames(false, u32::MAX)), 'c')
    );
    assert_eq!(
        serde_json::to_value(&decoded).unwrap(),
        serde_json::from_str::<serde_json::Value>(wire).unwrap()
    );
    for selector in [
        r#"{"type":"selected_beat","unexpected":true}"#,
        r#"{"type":"visual_selection","unexpected":true}"#,
        r#"{"type":"motion","motion":{"type":"frames","forward":true,"count":0}}"#,
        r#"{"type":"motion","motion":{"type":"beats","forward":true,"count":1,"unexpected":true}}"#,
        r#"{"type":"motion","motion":{"type":"scope","end":true,"count":1}}"#,
        r#"{"type":"motion","motion":{"type":"scope","end":false},"unexpected":true}"#,
        r#"{"type":"selected_beat","type":"visual_selection"}"#,
    ] {
        let wire = format!(
            r#"{{"instructions":[{{"type":"yank","selector":{selector},"register":"a"}}]}}"#
        );
        assert!(
            serde_json::from_str::<SemanticProgram>(&wire).is_err(),
            "{wire}"
        );
    }
    assert!(serde_json::from_str::<SemanticProgram>(r#"{"instructions":[{"type":"cut","selector":{"type":"selected_beat"},"register":"a","unexpected":true}]}"#).is_err());
}
