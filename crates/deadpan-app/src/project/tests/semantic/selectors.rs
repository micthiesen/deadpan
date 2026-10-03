use super::*;
use crate::project::macros::{Id, Operation as MacroOperation};
use deadpan_core::{
    RegisterName, SemanticContext, SemanticInstruction, SemanticMotion, SemanticProgram,
    SemanticSelector, SemanticVisualSelection,
};
use std::num::NonZeroU32;

mod repeats;

fn context(update: &ProjectUpdate, cursor: i64, selected: Option<&str>) -> SemanticContext {
    SemanticContext {
        parent: update.workspace.as_ref().unwrap().document.root().clone(),
        cursor: ProjectFrame(cursor),
        selected_child: selected.map(node),
        visual_selection: None,
    }
}

fn id(update: &ProjectUpdate, request: u64) -> Id {
    let workspace = update.workspace.as_ref().unwrap();
    Id {
        session: workspace.session,
        project: workspace.document.project_id().clone(),
        revision: workspace.document.revision_id().clone(),
        bank_version: update.registers.as_ref().unwrap().version,
        request,
    }
}

fn apply(
    update: &ProjectUpdate,
    request: u64,
    context: SemanticContext,
    instruction: SemanticInstruction,
    repeat_version: Option<u64>,
) -> MacroOperation {
    MacroOperation::Apply {
        id: id(update, request),
        instruction,
        repeat_version,
        context,
        scope: SequenceScope::default(),
    }
}

fn motion(count: u32) -> SemanticSelector {
    SemanticSelector::Motion {
        motion: SemanticMotion::Frames {
            forward: true,
            count: NonZeroU32::new(count).unwrap(),
        },
    }
}

fn instruction(selector: SemanticSelector, register: char) -> SemanticInstruction {
    SemanticInstruction::Cut {
        selector,
        register: RegisterName::new(register).unwrap(),
    }
}

fn repeat(
    update: &ProjectUpdate,
    ticket: u64,
    context: SemanticContext,
    register: char,
) -> MacroOperation {
    let snapshot = snapshot(update);
    let instruction = snapshot
        .edit
        .as_ref()
        .unwrap()
        .instruction(&context, RegisterName::new(register).unwrap());
    apply(update, ticket, context, instruction, Some(snapshot.version))
}

fn execute(service: &ProjectService, operation: MacroOperation) -> ProjectUpdate {
    let update = command(service, ProjectRequest::Macro(operation));
    assert!(
        update.macros.as_ref().unwrap().result.is_ok(),
        "{:?}",
        update.macros
    );
    update
}

fn refused(
    service: &ProjectService,
    path: &Path,
    operation: MacroOperation,
    previous: &ProjectUpdate,
) -> ProjectUpdate {
    let document = saved(path);
    let rows = counts(path);
    let bank = ProjectStore::open(path, AccessMode::ReadOnly)
        .unwrap()
        .registers()
        .unwrap();
    let update = command(service, ProjectRequest::Macro(operation));
    assert!(update.macros.as_ref().unwrap().result.is_err());
    assert_eq!(snapshot(&update), snapshot(previous));
    assert_eq!(saved(path), document);
    assert_eq!(counts(path), rows);
    assert_eq!(
        ProjectStore::open(path, AccessMode::ReadOnly)
            .unwrap()
            .registers()
            .unwrap(),
        bank
    );
    update
}

fn same_document(actual: &ProjectDocument, expected: &ProjectDocument) {
    let mut expected = serde_json::to_value(expected).unwrap();
    expected["revision_id"] = serde_json::to_value(actual.revision_id()).unwrap();
    assert_eq!(serde_json::to_value(actual).unwrap(), expected);
}

#[test]
fn visual_presence_overrides_every_saved_kind_without_erasing_empty_or_absent_distinction() {
    let mut context = SemanticContext {
        parent: node("root"),
        cursor: ProjectFrame(9),
        selected_child: None,
        visual_selection: None,
    };
    for operation in [
        RepeatableCut::Frames(FrameCut::new(7).unwrap()),
        RepeatableCut::Selector(motion(7)),
        RepeatableCut::Selector(SemanticSelector::SelectedBeat),
        RepeatableCut::Selector(SemanticSelector::VisualSelection),
    ] {
        let last = LastEdit {
            operation: RepeatableEdit::Cut(operation.clone()),
            register: Some('a'),
        };
        context.visual_selection = None;
        assert_eq!(
            last.instruction(&context, RegisterName::unnamed()),
            operation.instruction(RegisterName::unnamed())
        );
        for selection in [
            SemanticVisualSelection {
                anchor: ProjectFrame(12),
                head: ProjectFrame(9),
                extending: true,
            },
            SemanticVisualSelection {
                anchor: ProjectFrame(2),
                head: ProjectFrame(6),
                extending: false,
            },
            SemanticVisualSelection {
                anchor: ProjectFrame(9),
                head: ProjectFrame(9),
                extending: true,
            },
        ] {
            context.visual_selection = Some(selection);
            let effective = last.instruction(&context, RegisterName::new('z').unwrap());
            assert_eq!(
                effective,
                instruction(SemanticSelector::VisualSelection, 'z')
            );
            assert_eq!(
                LastEdit::from_instruction(&effective),
                Some(LastEdit {
                    operation: RepeatableEdit::Cut(RepeatableCut::Selector(
                        SemanticSelector::VisualSelection
                    )),
                    register: Some('z'),
                })
            );
        }
    }
}

#[test]
fn motion_repeat_retains_requested_count_and_register_override_with_full_history_inverse() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("typed-repeat.deadpan");
    let (harness, opened) = setup(&path);
    let first = execute(
        &harness.service,
        apply(
            &opened,
            1,
            context(&opened, 28, None),
            instruction(motion(7), 'a'),
            None,
        ),
    );
    assert_eq!(
        snapshot(&first).edit,
        Some(LastEdit {
            operation: RepeatableEdit::Cut(RepeatableCut::Selector(motion(7))),
            register: Some('a')
        })
    );
    assert_eq!(
        copied(first.registers.as_ref().unwrap(), 'a')
            .slice()
            .duration()
            .frames(),
        2
    );
    let second = execute(
        &harness.service,
        repeat(&first, 2, context(&first, 3, None), '"'),
    );
    assert_eq!(
        snapshot(&second).edit,
        Some(LastEdit {
            operation: RepeatableEdit::Cut(RepeatableCut::Selector(motion(7))),
            register: None
        })
    );
    let bank = second.registers.as_ref().unwrap();
    assert_eq!(
        copied(bank, '"').slice().range(),
        FrameRange::new(ProjectFrame(3), ProjectFrame(10)).unwrap()
    );
    assert_eq!(copied(bank, 'a').slice().duration().frames(), 2);
    let after = second.workspace.as_ref().unwrap().document.clone();
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: after.revision_id().clone(),
        },
    );
    same_document(
        &undone.workspace.as_ref().unwrap().document,
        &first.workspace.as_ref().unwrap().document,
    );
    assert_eq!(snapshot(&undone).edit, snapshot(&second).edit);
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
    same_document(&redone.workspace.as_ref().unwrap().document, &after);
    assert_eq!(
        copied(redone.registers.as_ref().unwrap(), '"').slice(),
        copied(bank, '"').slice()
    );
}

#[test]
fn beat_motion_repeat_resolves_the_new_selected_child_instead_of_reusing_a_length() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("beat-repeat.deadpan");
    let (harness, opened) = setup(&path);
    let selector = SemanticSelector::Motion {
        motion: SemanticMotion::Beats {
            forward: true,
            count: NonZeroU32::new(1).unwrap(),
        },
    };
    let first = execute(
        &harness.service,
        apply(
            &opened,
            1,
            context(&opened, 3, Some("a")),
            instruction(selector, 'a'),
            None,
        ),
    );
    assert_eq!(
        copied(first.registers.as_ref().unwrap(), 'a')
            .slice()
            .range(),
        FrameRange::new(ProjectFrame(3), ProjectFrame(10)).unwrap()
    );
    let second = execute(
        &harness.service,
        repeat(&first, 2, context(&first, 4, Some("b")), 'b'),
    );
    assert_eq!(
        copied(second.registers.as_ref().unwrap(), 'b')
            .slice()
            .range(),
        FrameRange::new(ProjectFrame(4), ProjectFrame(13)).unwrap()
    );
    assert_eq!(
        snapshot(&second).edit.as_ref().unwrap().operation,
        RepeatableEdit::Cut(RepeatableCut::Selector(selector))
    );
    assert_eq!(
        second.workspace.as_ref().unwrap().plan.duration().frames(),
        14
    );
}

#[test]
fn legacy_cut_attempts_validate_explicit_selection_and_keep_their_own_receipts() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("legacy-selector-cut.deadpan");
    let (harness, opened) = setup(&path);
    let workspace = opened.workspace.as_ref().unwrap();
    let rows = counts(&path);
    let bank = opened.registers.as_ref().unwrap();
    let mut child = capture(workspace, 1, Some('a'), 0, 10);
    child.selection = SliceCaptureSelection::Child { node: node("a") };
    let requests = [
        (
            child,
            RepeatableCut::Selector(SemanticSelector::VisualSelection),
            None,
        ),
        (
            capture(workspace, 2, Some('a'), 2, 4),
            RepeatableCut::Selector(SemanticSelector::SelectedBeat),
            None,
        ),
        (
            capture(workspace, 3, Some('a'), 2, 4),
            RepeatableCut::Selector(motion(2)),
            None,
        ),
        (
            capture(workspace, 4, Some('a'), 2, 2),
            RepeatableCut::Selector(SemanticSelector::VisualSelection),
            None,
        ),
        (
            capture(workspace, 5, Some('a'), 2, 4),
            RepeatableCut::Selector(SemanticSelector::VisualSelection),
            Some(snapshot(&opened).version),
        ),
    ];
    for (capture, operation, repeat_version) in requests {
        let update = command(
            &harness.service,
            ProjectRequest::CutFrames {
                capture,
                attempt: CutAttempt {
                    operation,
                    repeat_version,
                },
            },
        );
        assert!(update.cut_slice.as_ref().unwrap().result.is_err());
        assert_eq!(snapshot(&update), snapshot(&opened));
        assert_eq!(counts(&path), rows);
        assert_eq!(saved(&path), *workspace.document);
        assert!(Arc::ptr_eq(update.registers.as_ref().unwrap(), bank));
    }
    let visual = cut(
        &harness.service,
        ProjectRequest::CutFrames {
            capture: capture(workspace, 6, Some('v'), 4, 7),
            attempt: CutAttempt {
                operation: RepeatableCut::Selector(SemanticSelector::VisualSelection),
                repeat_version: None,
            },
        },
    );
    assert_eq!(
        snapshot(&visual).edit,
        Some(LastEdit {
            operation: RepeatableEdit::Cut(RepeatableCut::Selector(
                SemanticSelector::VisualSelection
            )),
            register: Some('v')
        })
    );
    assert!(visual.saved_cut.is_some() && visual.saved_macro.is_none());
    let mut child = capture(visual.workspace.as_ref().unwrap(), 7, Some('b'), 0, 1);
    child.selection = SliceCaptureSelection::Child { node: node("b") };
    let selected = cut(
        &harness.service,
        ProjectRequest::CutFrames {
            capture: child,
            attempt: CutAttempt {
                operation: RepeatableCut::Selector(SemanticSelector::SelectedBeat),
                repeat_version: None,
            },
        },
    );
    assert_eq!(
        snapshot(&selected).edit,
        Some(LastEdit {
            operation: RepeatableEdit::Cut(RepeatableCut::Selector(SemanticSelector::SelectedBeat)),
            register: Some('b')
        })
    );
    assert_eq!(
        selected.saved_cut.as_ref().unwrap().copied.child_label(),
        Some("b")
    );
    assert!(selected.saved_macro.is_none());
}

#[test]
fn selected_beat_repeat_removes_adjacent_empty_children_independently_of_cursor() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("empty-repeat.deadpan");
    let mut store = seed_holds(&path, &["a", "b"]);
    for (index, name) in [(1, "empty-one"), (2, "empty-two")] {
        seed_command(
            &mut store,
            Command::Insert {
                parent: node("root"),
                index,
                subtree: Subtree {
                    root: node(name),
                    nodes: BTreeMap::from([(node(name), BeatNode::sequence(name, vec![]))]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
            name,
        );
    }
    drop(store);
    let harness = Harness::new();
    let opened = command(&harness.service, ProjectRequest::Open(path.clone()));
    let first = execute(
        &harness.service,
        apply(
            &opened,
            1,
            context(&opened, 3, Some("empty-one")),
            instruction(SemanticSelector::SelectedBeat, 'a'),
            None,
        ),
    );
    assert_eq!(
        first
            .saved_macro
            .as_ref()
            .unwrap()
            .committed()
            .unwrap()
            .selected_node,
        Some(node("empty-two"))
    );
    let second = execute(
        &harness.service,
        repeat(&first, 2, context(&first, 17, Some("empty-two")), 'b'),
    );
    assert_eq!(
        second.workspace.as_ref().unwrap().plan.duration().frames(),
        20
    );
    assert!(
        !second
            .workspace
            .as_ref()
            .unwrap()
            .document
            .nodes()
            .contains_key(&node("empty-two"))
    );
    let copy = copied(second.registers.as_ref().unwrap(), 'b');
    assert_eq!(copy.child_label(), Some("empty-two"));
    assert_eq!(
        copy.slice().selection(),
        &SliceCaptureSelection::Child {
            node: node("empty-two")
        }
    );
    assert_eq!(copy.slice().duration().frames(), 0);
    assert_eq!(
        snapshot(&second).edit.as_ref().unwrap().operation,
        RepeatableEdit::Cut(RepeatableCut::Selector(SemanticSelector::SelectedBeat))
    );
    refused(
        &harness.service,
        &path,
        repeat(&second, 3, context(&second, 10, None), 'b'),
        &second,
    );
}

#[test]
fn successful_visual_override_becomes_intent_and_missing_or_empty_visual_never_falls_back() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("visual-repeat.deadpan");
    let (harness, opened) = setup(&path);
    let first = execute(
        &harness.service,
        apply(
            &opened,
            1,
            context(&opened, 1, None),
            instruction(motion(2), 'a'),
            None,
        ),
    );
    let mut visual = context(&first, 17, Some("c"));
    visual.visual_selection = Some(SemanticVisualSelection {
        anchor: ProjectFrame(7),
        head: ProjectFrame(4),
        extending: false,
    });
    // A caller cannot bypass a current Visual selector by submitting the old motion.
    refused(
        &harness.service,
        &path,
        apply(
            &first,
            9,
            visual.clone(),
            instruction(motion(2), 'b'),
            Some(snapshot(&first).version),
        ),
        &first,
    );
    let second = execute(&harness.service, repeat(&first, 2, visual, 'b'));
    assert_eq!(
        copied(second.registers.as_ref().unwrap(), 'b')
            .slice()
            .range(),
        FrameRange::new(ProjectFrame(4), ProjectFrame(7)).unwrap()
    );
    assert_eq!(
        snapshot(&second).edit.as_ref().unwrap().operation,
        RepeatableEdit::Cut(RepeatableCut::Selector(SemanticSelector::VisualSelection))
    );
    refused(
        &harness.service,
        &path,
        repeat(&second, 3, context(&second, 4, Some("c")), 'b'),
        &second,
    );
    let mut empty = context(&second, 4, Some("c"));
    empty.visual_selection = Some(SemanticVisualSelection {
        anchor: ProjectFrame(4),
        head: ProjectFrame(4),
        extending: true,
    });
    refused(
        &harness.service,
        &path,
        repeat(&second, 4, empty, 'b'),
        &second,
    );
}

#[test]
fn stale_or_altered_repeat_intent_is_atomic_and_bank_only_saves_preserve_candidate() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("repeat-version.deadpan");
    let (harness, opened) = setup(&path);
    let first_operation = apply(
        &opened,
        1,
        context(&opened, 0, None),
        instruction(motion(2), 'a'),
        None,
    );
    let first = execute(&harness.service, first_operation);
    let version = snapshot(&first).version;
    for (ticket, instruction, repeat_version) in [
        (2, instruction(motion(3), 'a'), Some(version)),
        (
            3,
            instruction(SemanticSelector::SelectedBeat, 'a'),
            Some(version),
        ),
        (4, instruction(motion(2), 'a'), Some(version - 1)),
        (
            5,
            SemanticInstruction::Yank {
                selector: motion(2),
                register: RegisterName::new('a').unwrap(),
            },
            Some(version),
        ),
    ] {
        refused(
            &harness.service,
            &path,
            apply(
                &first,
                ticket,
                context(&first, 4, Some("c")),
                instruction,
                repeat_version,
            ),
            &first,
        );
    }
    let stale_bank = repeat(&first, 6, context(&first, 4, None), 'a');
    let saved = execute(
        &harness.service,
        MacroOperation::Save {
            id: id(&first, 7),
            register: 'm',
            program: Arc::new(SemanticProgram::new(vec![instruction(motion(1), 'a')]).unwrap()),
        },
    );
    assert_eq!(snapshot(&saved), snapshot(&first));
    refused(&harness.service, &path, stale_bank, &saved);
    let yanked = execute(
        &harness.service,
        apply(
            &saved,
            8,
            context(&saved, 3, None),
            SemanticInstruction::Yank {
                selector: motion(2),
                register: RegisterName::new('b').unwrap(),
            },
            None,
        ),
    );
    assert_eq!(snapshot(&yanked), snapshot(&first));
    let run = execute(
        &harness.service,
        MacroOperation::Run {
            id: id(&yanked, 9),
            register: 'm',
            count: 1,
            scope: SequenceScope::default(),
            context: context(&yanked, 4, None),
        },
    );
    assert!(snapshot(&run).edit.is_none());
}

#[test]
fn typed_cut_refresh_failure_keeps_proof_and_exact_retry_does_not_reprove_after_undo() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("repeat-refresh.deadpan");
    let (harness, opened) = setup(&path);
    fail_refresh(&harness.service);
    let operation = apply(
        &opened,
        1,
        context(&opened, 4, None),
        instruction(motion(3), 'a'),
        None,
    );
    let first = execute(&harness.service, operation.clone());
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
    assert_eq!(
        snapshot(&first).edit.as_ref().unwrap().operation,
        RepeatableEdit::Cut(RepeatableCut::Selector(motion(3)))
    );
    assert!(
        snapshot(&first)
            .edit_for(first.workspace.as_ref().unwrap())
            .is_err()
    );
    refused(
        &harness.service,
        &path,
        repeat(&first, 2, context(&first, 9, None), 'a'),
        &first,
    );
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: receipt.committed().unwrap().revision.clone(),
        },
    );
    assert!(undone.error.is_none(), "{:?}", undone.error);
    let changed_candidate = cut(
        &harness.service,
        frame_cut(
            undone.workspace.as_ref().unwrap(),
            10,
            Some('z'),
            0,
            1,
            None,
        ),
    );
    intent(&changed_candidate, 1, Some('z'));
    let rows = counts(&path);
    let duplicate = execute(&harness.service, operation.clone());
    assert_eq!(snapshot(&duplicate), snapshot(&changed_candidate));
    assert_eq!(counts(&path), rows);
    assert_eq!(
        duplicate.saved_macro.as_ref().unwrap().committed(),
        receipt.committed()
    );
    let mut changed = operation;
    let MacroOperation::Apply { repeat_version, .. } = &mut changed else {
        unreachable!()
    };
    *repeat_version = Some(snapshot(&changed_candidate).version);
    refused(&harness.service, &path, changed, &changed_candidate);
}
