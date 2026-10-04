use super::*;

fn wrapping(selector: SemanticSelector, plays: u32) -> SemanticInstruction {
    SemanticInstruction::Repeat {
        selector,
        plays: NonZeroU32::new(plays).unwrap(),
    }
}

fn wrapping_intent(update: &ProjectUpdate, selector: SemanticSelector, plays: u32) {
    let edit = snapshot(update).edit.as_ref().unwrap();
    assert_eq!(
        edit.operation,
        RepeatableEdit::Repeat {
            selector,
            plays: NonZeroU32::new(plays).unwrap(),
        }
    );
    assert!(!edit.uses_register());
    assert_eq!(edit.register, None);
}

#[test]
fn repeat_wrap_retains_motion_count_and_plays_then_visual_override_becomes_saved_intent() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("wrap-repeat-dot.deadpan");
    let (harness, opened) = setup(&path);
    let opened = command(
        &harness.service,
        ProjectRequest::CaptureEditSlice(capture(
            opened.workspace.as_ref().unwrap(),
            100,
            Some('k'),
            1,
            3,
        )),
    );
    let bank = ProjectStore::open(&path, AccessMode::ReadOnly)
        .unwrap()
        .registers()
        .unwrap();
    let first = execute(
        &harness.service,
        apply(
            &opened,
            1,
            context(&opened, 28, None),
            wrapping(motion(7), 3),
            None,
        ),
    );
    wrapping_intent(&first, motion(7), 3);
    assert!(Arc::ptr_eq(
        copied(opened.registers.as_ref().unwrap(), 'k'),
        copied(first.registers.as_ref().unwrap(), 'k'),
    ));
    assert_eq!(
        first.workspace.as_ref().unwrap().plan.duration().frames(),
        34
    );
    let second = execute(
        &harness.service,
        repeat(&first, 2, context(&first, 3, None), 'z'),
    );
    wrapping_intent(&second, motion(7), 3);
    assert_eq!(
        second.workspace.as_ref().unwrap().plan.duration().frames(),
        48
    );
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)
            .unwrap()
            .registers()
            .unwrap(),
        bank
    );
    let version = snapshot(&second).version;
    for (request, instruction, expected_version) in [
        (3, wrapping(motion(7), 4), version),
        (4, wrapping(motion(6), 3), version),
        (5, wrapping(motion(7), 3), version - 1),
        (6, super::instruction(motion(7), 'z'), version),
    ] {
        refused(
            &harness.service,
            &path,
            apply(
                &second,
                request,
                context(&second, 26, None),
                instruction,
                Some(expected_version),
            ),
            &second,
        );
    }
    let mut visual = context(&second, 26, None);
    visual.visual_selection = Some(SemanticVisualSelection::Time {
        anchor: ProjectFrame(29),
        head: ProjectFrame(26),
        extending: true,
    });
    let third = execute(&harness.service, repeat(&second, 7, visual, 'a'));
    wrapping_intent(&third, SemanticSelector::VisualSelection, 3);
    assert_eq!(
        third.workspace.as_ref().unwrap().plan.duration().frames(),
        54
    );
    assert!(matches!(
        &third.saved_macro.as_ref().unwrap().outcome,
        crate::project::macros::Outcome::Applied {
            visual_selection: None,
            cursor: ProjectFrame(26),
            ..
        }
    ));
    let selected = third
        .saved_macro
        .as_ref()
        .unwrap()
        .committed()
        .unwrap()
        .selected_node
        .clone();
    let absent = SemanticContext {
        selected_child: selected,
        ..context(&third, 26, None)
    };
    refused(
        &harness.service,
        &path,
        repeat(&third, 8, absent.clone(), 'a'),
        &third,
    );
    let empty = SemanticContext {
        visual_selection: Some(SemanticVisualSelection::Time {
            anchor: ProjectFrame(26),
            head: ProjectFrame(26),
            extending: false,
        }),
        ..absent
    };
    refused(
        &harness.service,
        &path,
        repeat(&third, 9, empty, 'a'),
        &third,
    );
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: third
                .workspace
                .as_ref()
                .unwrap()
                .document
                .revision_id()
                .clone(),
        },
    );
    same_document(
        &undone.workspace.as_ref().unwrap().document,
        &second.workspace.as_ref().unwrap().document,
    );
    wrapping_intent(&undone, SemanticSelector::VisualSelection, 3);
    let redone = command(
        &harness.service,
        ProjectRequest::Redo {
            expected_revision: undone
                .workspace
                .as_ref()
                .unwrap()
                .document
                .revision_id()
                .clone(),
        },
    );
    same_document(
        &redone.workspace.as_ref().unwrap().document,
        &third.workspace.as_ref().unwrap().document,
    );
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)
            .unwrap()
            .registers()
            .unwrap(),
        bank
    );
}

#[test]
fn empty_child_refuses_without_cursor_fallback_and_legacy_wrap_retains_queue_receipt() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("empty-repeat-dot.deadpan");
    let mut store = seed_holds(&path, &["a"]);
    seed_command(
        &mut store,
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: deadpan_core::Subtree {
                root: node("empty"),
                nodes: std::collections::BTreeMap::from([(
                    node("empty"),
                    deadpan_core::BeatNode::sequence("Empty", vec![]),
                )]),
                overrides: std::collections::BTreeMap::new(),
                gap_overrides: std::collections::BTreeMap::new(),
            },
        },
        "seed-empty-repeat",
    );
    drop(store);
    let harness = Harness::new();
    let opened = command(&harness.service, ProjectRequest::Open(path.clone()));
    let before = opened.workspace.as_ref().unwrap();
    let initial_rows = counts(&path);
    let initial_bank = ProjectStore::open(&path, AccessMode::ReadOnly)
        .unwrap()
        .registers()
        .unwrap();
    let rejected = command(
        &harness.service,
        edit_request_in(
            before,
            SequenceScope::default(),
            ProjectFrame(9),
            ProjectEdit::WrapRepeat {
                node: node("empty"),
                plays: 3,
            },
        ),
    );
    assert!(rejected.error.is_some());
    assert!(rejected.committed.is_none());
    assert_eq!(counts(&path), initial_rows);
    assert_eq!(saved(&path), *before.document);
    assert_eq!(snapshot(&rejected), snapshot(&opened));
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)
            .unwrap()
            .registers()
            .unwrap(),
        initial_bank
    );
    refused(
        &harness.service,
        &path,
        apply(
            &opened,
            100,
            context(&opened, 9, Some("empty")),
            wrapping(SemanticSelector::SelectedBeat, 3),
            None,
        ),
        &opened,
    );
    let wrapped = edited(
        &harness.service,
        before,
        ProjectEdit::WrapRepeat {
            node: node("a"),
            plays: 3,
        },
    );
    wrapping_intent(&wrapped, SemanticSelector::SelectedBeat, 3);
    let receipt = wrapped.committed.as_ref().unwrap();
    assert_eq!(receipt.cursor, None);
    assert!(!receipt.preserve_cursor);
    let wrapper = receipt.selected_node.as_ref().unwrap();
    assert!(
        matches!(&wrapped.workspace.as_ref().unwrap().document.nodes()[wrapper].kind,
        NodeKind::Repeat { child, iterations, gap: None, .. } if child == &node("a") && iterations.len() == 3)
    );
    assert_eq!(
        wrapped.workspace.as_ref().unwrap().plan.duration().frames(),
        30
    );
    let next_context = SemanticContext {
        selected_child: Some(wrapper.clone()),
        ..context(&wrapped, 9, None)
    };
    let again = execute(&harness.service, repeat(&wrapped, 1, next_context, 'z'));
    let outer = again
        .saved_macro
        .as_ref()
        .unwrap()
        .committed()
        .unwrap()
        .selected_node
        .as_ref()
        .unwrap();
    assert!(
        matches!(&again.workspace.as_ref().unwrap().document.nodes()[outer].kind,
        NodeKind::Repeat { child, iterations, .. } if child == wrapper && iterations.len() == 3)
    );
    assert_eq!(
        again.workspace.as_ref().unwrap().plan.duration().frames(),
        90
    );
    let set = edited(
        &harness.service,
        again.workspace.as_ref().unwrap(),
        ProjectEdit::Repeat {
            node: outer.clone(),
            plays: 5,
        },
    );
    assert!(snapshot(&set).edit.is_none());
    assert_eq!(
        set.committed.as_ref().unwrap().selected_node.as_ref(),
        Some(outer)
    );
    assert!(
        matches!(&set.workspace.as_ref().unwrap().document.nodes()[outer].kind,
        NodeKind::Repeat { child, iterations, .. } if child == wrapper && iterations.len() == 5)
    );
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(path));
    assert_eq!(
        reopened.workspace.as_ref().unwrap().document,
        set.workspace.as_ref().unwrap().document
    );
}

#[test]
fn repeat_refresh_failure_retains_proof_and_retry_does_not_replace_a_newer_cut() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("repeat-refresh-proof.deadpan");
    let (harness, opened) = setup(&path);
    fail_refresh(&harness.service);
    let operation = apply(
        &opened,
        1,
        context(&opened, 19, Some("a")),
        wrapping(SemanticSelector::SelectedBeat, 2),
        None,
    );
    let first = execute(&harness.service, operation.clone());
    wrapping_intent(&first, SemanticSelector::SelectedBeat, 2);
    let receipt = first.saved_macro.as_ref().unwrap();
    assert!(matches!(
        &receipt.outcome,
        crate::project::macros::Outcome::Applied {
            refresh_error: Some(_),
            ..
        }
    ));
    assert_eq!(
        snapshot(&first).head.as_ref(),
        Some(&receipt.committed().unwrap().revision)
    );
    assert!(
        snapshot(&first)
            .edit_for(first.workspace.as_ref().unwrap())
            .is_err()
    );
    refused(
        &harness.service,
        &path,
        repeat(&first, 2, context(&first, 10, Some("b")), 'z'),
        &first,
    );
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: receipt.committed().unwrap().revision.clone(),
        },
    );
    same_document(
        &undone.workspace.as_ref().unwrap().document,
        &opened.workspace.as_ref().unwrap().document,
    );
    let changed = cut(
        &harness.service,
        frame_cut(undone.workspace.as_ref().unwrap(), 4, Some('a'), 0, 1, None),
    );
    let before_retry = counts(&path);
    let duplicate = execute(&harness.service, operation.clone());
    assert_eq!(counts(&path), before_retry);
    assert_eq!(snapshot(&duplicate), snapshot(&changed));
    assert_eq!(
        duplicate.saved_macro.as_ref().unwrap().committed(),
        receipt.committed()
    );
    let mut altered = operation;
    let MacroOperation::Apply { instruction, .. } = &mut altered else {
        unreachable!()
    };
    *instruction = wrapping(SemanticSelector::SelectedBeat, 3);
    refused(&harness.service, &path, altered, &changed);
}

#[test]
fn macro_bank_save_preserves_repeat_but_stales_captured_bank_and_named_run_clears_proof() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("repeat-bank-proof.deadpan");
    let (harness, opened) = setup(&path);
    let first = execute(
        &harness.service,
        apply(
            &opened,
            1,
            context(&opened, 0, Some("a")),
            wrapping(SemanticSelector::SelectedBeat, 2),
            None,
        ),
    );
    let stale = repeat(&first, 2, context(&first, 20, Some("b")), 'z');
    let saved = execute(
        &harness.service,
        MacroOperation::Save {
            id: id(&first, 3),
            register: 'q',
            program: Arc::new(
                SemanticProgram::new(vec![
                    wrapping(SemanticSelector::SelectedBeat, 2),
                    SemanticInstruction::Yank {
                        selector: SemanticSelector::SelectedBeat,
                        register: RegisterName::new('a').unwrap(),
                    },
                ])
                .unwrap(),
            ),
        },
    );
    assert_eq!(snapshot(&saved), snapshot(&first));
    refused(&harness.service, &path, stale, &saved);
    let before_run = counts(&path);
    let ran = execute(
        &harness.service,
        MacroOperation::Run {
            id: id(&saved, 4),
            register: 'q',
            count: 2,
            scope: SequenceScope::default(),
            context: context(&saved, 20, Some("b")),
        },
    );
    assert!(snapshot(&ran).edit.is_none());
    assert_eq!(
        ran.registers.as_ref().unwrap().version,
        saved.registers.as_ref().unwrap().version + 1
    );
    let copied = copied(ran.registers.as_ref().unwrap(), 'a');
    assert_eq!(copied.slice().duration().frames(), 40);
    assert!(copied.child_label().is_some());
    let store = ProjectStore::open(&path, AccessMode::ReadOnly).unwrap();
    let historical = store
        .capture_snapshot_at(copied.slice().revision_id())
        .unwrap();
    copied.slice().validate_capture(&historical).unwrap();
    assert_ne!(
        historical.revision_id(),
        ran.workspace.as_ref().unwrap().document.revision_id()
    );
    assert_ne!(
        historical.revision_id(),
        saved.workspace.as_ref().unwrap().document.revision_id()
    );
    drop(store);
    assert_eq!(counts(&path), (before_run.0 + 1, before_run.1 + 1));
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: ran
                .workspace
                .as_ref()
                .unwrap()
                .document
                .revision_id()
                .clone(),
        },
    );
    same_document(
        &undone.workspace.as_ref().unwrap().document,
        &saved.workspace.as_ref().unwrap().document,
    );
}
