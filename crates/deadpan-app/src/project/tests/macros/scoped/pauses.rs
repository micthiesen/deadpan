//! Pauses retain semantic intent and exact native scope through durable receipts.

use super::*;
use deadpan_core::{HoldVideo, PauseLength};

fn pause(frames: u32, black: bool) -> SemanticInstruction {
    SemanticInstruction::InsertPause {
        length: PauseLength::Frames {
            frames: NonZeroU32::new(frames).unwrap(),
        },
        black,
    }
}

fn continued(update: &ProjectUpdate) -> Target {
    let committed = receipt(update).committed().unwrap();
    let scoped = committed.scoped.as_ref().unwrap();
    let target = Target {
        revision: scoped.revision.clone(),
        target: scoped.target.clone(),
        presentation: scoped.presentation.clone(),
        ..scoped.before.clone()
    };
    target
        .semantic_framing(update.workspace.as_ref().unwrap())
        .unwrap();
    target
}

#[test]
fn native_scoped_pause_macro_selects_actual_hold_keeps_clocks_and_reopens_one_undo() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened, root) = setup_source(&scratch.path().join("Documents"));
    let instructions = vec![pause(3, false), smash(2), pause(2, true)];
    let body = program(instructions.clone());
    let saved = send(&service, save(&opened, 1, 'p', body.clone()));
    // The saved operation is intent only, with no recording-time addresses.
    assert_eq!(body.instructions(), instructions);
    let before = saved.workspace.as_ref().unwrap().clone();
    let path = before.path.clone();
    let bank = stored_bank(&path);
    let initial_rows = rows(&path);
    let target = capture_scope(&saved, &root, 365);
    let executed = send(
        &service,
        Operation::Run {
            id: id(&saved, 2),
            register: 'p',
            count: 1,
            scope: target.scope.clone(),
            context: context_for(&saved, &target),
        },
    );
    let committed = receipt(&executed).committed().unwrap();
    let after = executed.workspace.as_ref().unwrap().clone();
    assert!(committed.preserve_cursor);
    assert_eq!(committed.cursor, Some(ProjectFrame(365)));
    assert_eq!(committed.selected_node.as_ref(), Some(&root));
    assert_eq!(committed.scoped.as_ref().unwrap().before, target);
    assert_eq!(after.plan.duration().frames(), 725);
    let selected = continued(&executed);
    let NodeKind::Hold { recipe } = &after.document.nodes()[&selected.target.node].kind else {
        panic!("the exact inserted black pause must remain selected")
    };
    assert_eq!(recipe.duration.frames(), 2);
    assert_eq!(recipe.video, HoldVideo::Background);
    assert_eq!(recipe.picture_context, None);
    let freezes: Vec<_> = after
        .document
        .nodes()
        .values()
        .filter(
            |node| matches!(&node.kind, NodeKind::Hold { recipe } if recipe.duration.frames() == 3),
        )
        .collect();
    assert_eq!(freezes.len(), 1);
    assert!(
        freezes[0].framing.is_some(),
        "Zoom must target the inserted freeze"
    );
    for (node, value) in before.document.nodes() {
        assert_eq!(
            &after.document.nodes()[node],
            value,
            "shared template {node}"
        );
    }
    for frame in 0..720 {
        let shifted = frame + if frame >= 365 { 5 } else { 0 };
        assert_eq!(
            before.plan.picture(ProjectFrame(frame)).unwrap().picture,
            after.plan.picture(ProjectFrame(shifted)).unwrap().picture,
            "original picture {frame}"
        );
    }
    assert_eq!(stored_bank(&path), bank);
    assert_eq!(
        rows(&path),
        (initial_rows.0 + 1, initial_rows.1 + 1, initial_rows.2 + 3)
    );
    let undone = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: committed.revision.clone(),
        },
    );
    assert_same_content(
        &undone.workspace.as_ref().unwrap().document,
        &before.document,
    );
    let redone = command(
        &service,
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
    assert_same_content(
        &redone.workspace.as_ref().unwrap().document,
        &after.document,
    );
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(path.clone()));
    assert_eq!(
        reopened.workspace.as_ref().unwrap().document,
        redone.workspace.as_ref().unwrap().document
    );
    assert_eq!(stored_bank(&path), bank);
    command(&service, ProjectRequest::Close);
}

#[test]
fn native_scoped_pause_dot_rebinds_play_and_retains_durable_refresh_failure_receipt() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened, root) = setup_source(&scratch.path().join("Documents"));
    let instruction = pause(4, false);
    let target = capture_scope(&opened, &root, 365);
    let applied = send(&service, apply(&opened, &target, 1, instruction.clone()));
    let path = applied.workspace.as_ref().unwrap().path.clone();
    let edit = applied.semantic.as_ref().unwrap();
    assert_eq!(
        edit.edit.as_ref().unwrap().operation,
        RepeatableEdit::Parameter(instruction.clone())
    );
    let first = continued(&applied);
    assert!(matches!(
        applied.workspace.as_ref().unwrap().document.nodes()[&first.target.node].kind,
        NodeKind::Hold { .. }
    ));
    let before = applied.workspace.as_ref().unwrap().document.clone();
    let old_rows = rows(&path);
    let bank = stored_bank(&path);
    let other = capture_scope(&applied, &root, 10);
    service
        .shared
        .workspace_refresh_failure
        .store(true, Ordering::Release);
    let mut operation = apply(&applied, &other, 2, instruction);
    let Operation::Apply { repeat_version, .. } = &mut operation else {
        unreachable!()
    };
    *repeat_version = Some(edit.version);
    let durable = send(&service, operation.clone());
    let Outcome::Applied {
        committed,
        refresh_error,
        ..
    } = &receipt(&durable).outcome
    else {
        panic!()
    };
    assert!(refresh_error.as_ref().unwrap().contains("Reopen"));
    let committed = committed.as_ref().unwrap();
    let scoped = committed.scoped.as_ref().unwrap();
    assert_eq!(scoped.before, other);
    assert_ne!(scoped.target.node, first.target.node);
    assert_eq!(committed.cursor, Some(ProjectFrame(10)));
    assert_eq!(committed.selected_node.as_ref(), Some(&root));
    assert_eq!(durable.workspace.as_ref().unwrap().document, before);
    assert_eq!(
        rows(&path),
        (old_rows.0 + 1, old_rows.1 + 1, old_rows.2 + 1)
    );
    assert_eq!(stored_bank(&path), bank);
    let retried = send(&service, operation);
    assert_eq!(receipt(&retried).committed(), Some(committed.as_ref()));
    assert_eq!(
        rows(&path),
        (old_rows.0 + 1, old_rows.1 + 1, old_rows.2 + 1)
    );
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(path.clone()));
    let workspace = reopened.workspace.as_ref().unwrap();
    assert_eq!(workspace.document.revision_id(), &committed.revision);
    assert_eq!(workspace.plan.duration().frames(), 728);
    assert!(matches!(
        workspace.document.nodes()[&scoped.target.node].kind,
        NodeKind::Hold { .. }
    ));
    assert!(matches!(
        workspace.document.nodes()[&first.target.node].kind,
        NodeKind::Hold { .. }
    ));
    assert_eq!(stored_bank(&path), bank);
    command(&service, ProjectRequest::Close);
}

#[test]
fn native_scoped_pause_macro_late_unsupported_call_is_atomic() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened, root) = setup_source(&scratch.path().join("Documents"));
    let saved_inner = send(
        &service,
        save(&opened, 1, 'x', program(vec![SemanticInstruction::Split])),
    );
    let saved = send(
        &service,
        save(
            &saved_inner,
            2,
            'p',
            program(vec![
                pause(3, true),
                SemanticInstruction::Call {
                    register: RegisterName::new('x').unwrap(),
                    count: NonZeroU32::new(1).unwrap(),
                },
            ]),
        ),
    );
    let target = capture_scope(&saved, &root, 365);
    let before = saved.workspace.as_ref().unwrap().document.clone();
    let path = saved.workspace.as_ref().unwrap().path.clone();
    let old_rows = rows(&path);
    let bank = stored_bank(&path);
    let refused = command(
        &service,
        ProjectRequest::Macro(Operation::Run {
            id: id(&saved, 3),
            register: 'p',
            count: 1,
            scope: target.scope.clone(),
            context: context_for(&saved, &target),
        }),
    );
    assert!(refused.macros.as_ref().unwrap().result.is_err());
    assert_eq!(refused.workspace.as_ref().unwrap().document, before);
    assert_eq!(rows(&path), old_rows);
    assert_eq!(stored_bank(&path), bank);
    command(&service, ProjectRequest::Close);
}
