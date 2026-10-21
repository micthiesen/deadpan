//! Whole-owner gain steps keep nested invocation authority and durable history.
use super::*;
use deadpan_core::AudioChange;

fn step(millidecibels: i32) -> SemanticInstruction {
    SemanticInstruction::SetAudio {
        change: AudioChange::Step { millidecibels },
    }
}

fn trim(update: &ProjectUpdate, frame: i64) -> i32 {
    let workspace = update.workspace.as_ref().unwrap();
    let picture = workspace.plan.picture(ProjectFrame(frame)).unwrap();
    workspace.document.nodes()[&picture.instance.node]
        .audio_treatments
        .trim()
        .millidecibels()
}

#[test]
fn native_scoped_gain_step_and_dot_resolve_new_nested_play_with_one_undo_each() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened, root) = setup_source(&scratch.path().join("Documents"));
    let before = opened.workspace.as_ref().unwrap().clone();
    let path = before.path.clone();
    let target = capture_scope(&opened, &root, 365);
    let old_rows = rows(&path);
    let old_bank = stored_bank(&path);
    let first = send(&service, apply(&opened, &target, 1, step(3000)));
    let commit = receipt(&first).committed().unwrap();
    let mapped = commit.scoped.as_ref().unwrap();
    assert!(commit.preserve_cursor);
    assert_eq!(commit.cursor, Some(ProjectFrame(365)));
    assert_eq!(mapped.before, target);
    assert_ne!(mapped.target.node, target.target.node);
    assert_eq!(mapped.target.repeats.len(), 2);
    assert!(
        mapped
            .target
            .matches_instance(
                &first.workspace.as_ref().unwrap().document,
                mapped.presentation.as_ref().unwrap()
            )
            .unwrap()
    );
    assert_eq!(
        rows(&path),
        (old_rows.0 + 1, old_rows.1 + 1, old_rows.2 + 1)
    );
    assert_eq!(stored_bank(&path), old_bank);
    for (id, node) in before.document.nodes() {
        assert_eq!(
            &first.workspace.as_ref().unwrap().document.nodes()[id],
            node
        );
    }
    for frame in 0..720 {
        assert_eq!(
            trim(&first, frame),
            if (360..480).contains(&frame) { 3000 } else { 0 }
        );
        assert_eq!(
            before.plan.picture(ProjectFrame(frame)).unwrap().picture,
            first
                .workspace
                .as_ref()
                .unwrap()
                .plan
                .picture(ProjectFrame(frame))
                .unwrap()
                .picture
        );
    }
    let semantic = first.semantic.as_ref().unwrap();
    assert_eq!(
        semantic.edit.as_ref().unwrap().operation,
        RepeatableEdit::Parameter(step(3000))
    );
    let next = capture_scope(&first, &root, 5);
    let mut operation = apply(&first, &next, 2, step(3000));
    let Operation::Apply { repeat_version, .. } = &mut operation else {
        unreachable!()
    };
    *repeat_version = Some(semantic.version);
    let second = send(&service, operation);
    for frame in 0..720 {
        assert_eq!(
            trim(&second, frame),
            if (0..120).contains(&frame) || (360..480).contains(&frame) {
                3000
            } else {
                0
            }
        );
    }
    assert_eq!(
        receipt(&second).committed().unwrap().cursor,
        Some(ProjectFrame(5))
    );
    let undo_second = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: second
                .workspace
                .as_ref()
                .unwrap()
                .document
                .revision_id()
                .clone(),
        },
    );
    assert_same_content(
        &undo_second.workspace.as_ref().unwrap().document,
        &first.workspace.as_ref().unwrap().document,
    );
    let undo_first = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: undo_second
                .workspace
                .as_ref()
                .unwrap()
                .document
                .revision_id()
                .clone(),
        },
    );
    assert_same_content(
        &undo_first.workspace.as_ref().unwrap().document,
        &before.document,
    );
    let redone = command(
        &service,
        ProjectRequest::Redo {
            expected_revision: undo_first
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
        &first.workspace.as_ref().unwrap().document,
    );
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(path));
    assert_eq!(
        reopened.workspace.as_ref().unwrap().document,
        redone.workspace.as_ref().unwrap().document
    );
}

#[test]
fn native_scoped_gain_macro_noop_stale_authority_and_failed_late_step_are_atomic() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened, root) = setup_source(&scratch.path().join("Documents"));
    let saved = send(
        &service,
        save(&opened, 1, 'g', program(vec![step(3000), step(3000)])),
    );
    let target = capture_scope(&saved, &root, 605);
    let path = saved.workspace.as_ref().unwrap().path.clone();
    let stable_rows = rows(&path);
    let stable_bank = stored_bank(&path);
    let noop = command(
        &service,
        ProjectRequest::Macro(apply(&saved, &target, 2, step(0))),
    );
    assert!(noop.macros.as_ref().unwrap().result.is_err());
    assert_eq!(
        noop.workspace.as_ref().unwrap().document,
        saved.workspace.as_ref().unwrap().document
    );
    assert_eq!(rows(&path), stable_rows);
    assert_eq!(stored_bank(&path), stable_bank);
    for case in 0..3 {
        let mut operation = apply(&saved, &target, 3 + case, step(3000));
        let Operation::Apply { context, id, .. } = &mut operation else {
            unreachable!()
        };
        match case {
            0 => {
                context.scoped.as_mut().unwrap().target.repeats[0].branch =
                    RepeatEditBranch::Default
            }
            1 => {
                context.scoped.as_mut().unwrap().presentation.repeats[1].iteration = {
                    let inner = &context.scoped.as_ref().unwrap().target.repeats[1].repeat;
                    let NodeKind::Repeat { iterations, .. } =
                        &saved.workspace.as_ref().unwrap().document.nodes()[inner].kind
                    else {
                        panic!()
                    };
                    iterations.at(0).unwrap()
                }
            }
            2 => id.session += 1,
            _ => unreachable!(),
        }
        let refused = command(&service, ProjectRequest::Macro(operation));
        assert!(refused.macros.as_ref().unwrap().result.is_err());
        assert_eq!(rows(&path), stable_rows);
        assert_eq!(stored_bank(&path), stable_bank);
        assert_eq!(
            refused.workspace.as_ref().unwrap().document,
            saved.workspace.as_ref().unwrap().document
        );
    }
    let run = Operation::Run {
        id: id(&saved, 7),
        register: 'g',
        count: 1,
        scope: target.scope.clone(),
        context: context_for(&saved, &target),
    };
    let executed = send(&service, run);
    assert_eq!(
        rows(&path),
        (stable_rows.0 + 1, stable_rows.1 + 1, stable_rows.2 + 2)
    );
    for frame in 0..720 {
        assert_eq!(
            trim(&executed, frame),
            if (600..720).contains(&frame) { 6000 } else { 0 }
        );
    }
    let stale = command(
        &service,
        ProjectRequest::Macro(apply(&saved, &target, 8, step(3000))),
    );
    assert!(stale.macros.as_ref().unwrap().result.is_err());
    assert_eq!(
        stale.workspace.as_ref().unwrap().document,
        executed.workspace.as_ref().unwrap().document
    );
    let failed_program = send(
        &service,
        save(&executed, 9, 'b', program(vec![step(3000), step(30000)])),
    );
    let failed_target = capture_scope(&failed_program, &root, 5);
    let before_failed = rows(&path);
    let failed = command(
        &service,
        ProjectRequest::Macro(Operation::Run {
            id: id(&failed_program, 10),
            register: 'b',
            count: 1,
            scope: failed_target.scope.clone(),
            context: context_for(&failed_program, &failed_target),
        }),
    );
    assert!(failed.macros.as_ref().unwrap().result.is_err());
    assert_eq!(rows(&path), before_failed);
    assert_eq!(
        failed.workspace.as_ref().unwrap().document,
        failed_program.workspace.as_ref().unwrap().document
    );
    let undone = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: failed
                .workspace
                .as_ref()
                .unwrap()
                .document
                .revision_id()
                .clone(),
        },
    );
    assert_same_content(
        &undone.workspace.as_ref().unwrap().document,
        &saved.workspace.as_ref().unwrap().document,
    );
    command(&service, ProjectRequest::Close);
}
