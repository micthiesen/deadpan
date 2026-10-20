//! Existing pauses keep their exact occurrence across semantic timing edits.

use super::*;
use deadpan_core::PauseLength;

fn length(frames: u32) -> SemanticInstruction {
    SemanticInstruction::SetHoldDuration {
        length: PauseLength::Frames {
            frames: NonZeroU32::new(frames).unwrap(),
        },
    }
}

fn pause(frames: u32) -> SemanticInstruction {
    SemanticInstruction::InsertPause {
        length: PauseLength::Frames {
            frames: NonZeroU32::new(frames).unwrap(),
        },
        black: false,
    }
}

fn continued(update: &ProjectUpdate, interior: i64) -> Target {
    let committed = receipt(update).committed().unwrap();
    let scoped = committed.scoped.as_ref().unwrap();
    Target {
        revision: scoped.revision.clone(),
        target: scoped.target.clone(),
        presentation: scoped.presentation.clone(),
        cursor: ProjectFrame(committed.cursor.unwrap().0 + interior),
        ..scoped.before.clone()
    }
}

#[test]
fn native_scoped_hold_duration_from_interior_retains_exact_start_history_and_macro_continuation() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened, root) = setup_source(&scratch.path().join("Documents"));
    let initial = capture_scope(&opened, &root, 365);
    let inserted = send(&service, apply(&opened, &initial, 1, pause(9)));
    let before = inserted.workspace.as_ref().unwrap().clone();
    let target = continued(&inserted, 8);
    let path = before.path.clone();
    let old_rows = rows(&path);
    let bank = stored_bank(&path);
    let shortened = send(&service, apply(&inserted, &target, 2, length(3)));
    let commit = receipt(&shortened).committed().unwrap();
    assert_eq!(commit.cursor, Some(ProjectFrame(365)));
    assert_eq!(commit.selected_node.as_ref(), Some(&root));
    assert_eq!(commit.scoped.as_ref().unwrap().before, target);
    let after = shortened.workspace.as_ref().unwrap().clone();
    assert_eq!(after.plan.duration().frames(), 723);
    let selected = continued(&shortened, 0);
    assert_eq!(
        selected.target, target.target,
        "already isolated Hold identity survives"
    );
    assert!(
        matches!(&after.document.nodes()[&selected.target.node].kind,
        NodeKind::Hold { recipe } if recipe.duration.frames() == 3)
    );
    for frame in 0..729 {
        if (365..374).contains(&frame) {
            continue;
        }
        assert_eq!(
            before.plan.picture(ProjectFrame(frame)).unwrap().picture,
            after
                .plan
                .picture(ProjectFrame(frame - if frame >= 374 { 6 } else { 0 }))
                .unwrap()
                .picture,
            "retained picture {frame}"
        );
    }
    assert_eq!(stored_bank(&path), bank);
    assert_eq!(
        rows(&path),
        (old_rows.0 + 1, old_rows.1 + 1, old_rows.2 + 1)
    );
    let undone = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: commit.revision.clone(),
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
    let mut current = selected;
    current.revision = reopened
        .workspace
        .as_ref()
        .unwrap()
        .document
        .revision_id()
        .clone();
    current.session = reopened.workspace.as_ref().unwrap().session;
    current.cursor = ProjectFrame(367);
    let body = program(vec![length(7), smash(2), pause(2)]);
    let saved = send(&service, save(&reopened, 3, 'd', body));
    let executed = send(
        &service,
        Operation::Run {
            id: id(&saved, 4),
            register: 'd',
            count: 1,
            scope: current.scope.clone(),
            context: context_for(&saved, &current),
        },
    );
    let final_target = continued(&executed, 0);
    let final_workspace = executed.workspace.as_ref().unwrap();
    assert_eq!(final_target.cursor, ProjectFrame(365));
    assert_eq!(final_workspace.plan.duration().frames(), 729);
    assert_ne!(final_target.target.node, current.target.node);
    let resized = &final_workspace.document.nodes()[&current.target.node];
    assert!(matches!(&resized.kind, NodeKind::Hold { recipe } if recipe.duration.frames() == 7));
    assert!(
        resized.framing.is_some(),
        "Zoom continued on the resized Hold"
    );
    assert!(
        matches!(&final_workspace.document.nodes()[&final_target.target.node].kind,
        NodeKind::Hold { recipe } if recipe.duration.frames() == 2)
    );
    command(&service, ProjectRequest::Close);
}

#[test]
fn native_scoped_hold_duration_noop_late_call_and_durable_retry_preserve_atomic_history() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened, root) = setup_source(&scratch.path().join("Documents"));
    let inserted = send(
        &service,
        apply(&opened, &capture_scope(&opened, &root, 365), 1, pause(9)),
    );
    let target = continued(&inserted, 8);
    let path = inserted.workspace.as_ref().unwrap().path.clone();
    let stable_rows = rows(&path);
    let stable_bank = stored_bank(&path);
    let refused = command(
        &service,
        ProjectRequest::Macro(apply(&inserted, &target, 2, length(9))),
    );
    assert!(refused.macros.as_ref().unwrap().result.is_err());
    assert_eq!(rows(&path), stable_rows);
    assert_eq!(stored_bank(&path), stable_bank);
    let saved_inner = send(
        &service,
        save(&inserted, 3, 'x', program(vec![SemanticInstruction::Split])),
    );
    let saved = send(
        &service,
        save(
            &saved_inner,
            4,
            'd',
            program(vec![
                length(4),
                SemanticInstruction::Call {
                    register: RegisterName::new('x').unwrap(),
                    count: NonZeroU32::new(1).unwrap(),
                },
            ]),
        ),
    );
    let bank = stored_bank(&path);
    let refused = command(
        &service,
        ProjectRequest::Macro(Operation::Run {
            id: id(&saved, 5),
            register: 'd',
            count: 1,
            scope: target.scope.clone(),
            context: context_for(&saved, &target),
        }),
    );
    assert!(refused.macros.as_ref().unwrap().result.is_err());
    assert_eq!(
        refused.workspace.as_ref().unwrap().document,
        inserted.workspace.as_ref().unwrap().document
    );
    assert_eq!(rows(&path), stable_rows);
    assert_eq!(stored_bank(&path), bank);
    service
        .shared
        .workspace_refresh_failure
        .store(true, Ordering::Release);
    let operation = apply(&saved, &target, 6, length(4));
    let durable = send(&service, operation.clone());
    let committed = receipt(&durable).committed().unwrap();
    assert_eq!(committed.cursor, Some(ProjectFrame(365)));
    assert_eq!(committed.scoped.as_ref().unwrap().before, target);
    let Outcome::Applied { refresh_error, .. } = &receipt(&durable).outcome else {
        panic!()
    };
    assert!(refresh_error.as_ref().unwrap().contains("Reopen"));
    assert_eq!(
        durable.workspace.as_ref().unwrap().document,
        inserted.workspace.as_ref().unwrap().document
    );
    let retried = send(&service, operation);
    assert_eq!(receipt(&retried).committed(), Some(committed));
    assert_eq!(
        rows(&path),
        (stable_rows.0 + 1, stable_rows.1 + 1, stable_rows.2 + 1)
    );
    assert_eq!(stored_bank(&path), bank);
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(path));
    assert_eq!(
        reopened
            .workspace
            .as_ref()
            .unwrap()
            .plan
            .duration()
            .frames(),
        724
    );
    command(&service, ProjectRequest::Close);
}
