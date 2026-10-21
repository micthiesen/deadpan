//! Explicit mute state follows current nested invocation authority, never a toggle.
use super::*;
use deadpan_core::{AudioChange, AudioTreatments};

fn mute(muted: bool) -> SemanticInstruction {
    SemanticInstruction::SetAudio {
        change: AudioChange::Mute { muted },
    }
}

fn treatments(update: &ProjectUpdate, frame: i64) -> &AudioTreatments {
    let workspace = update.workspace.as_ref().unwrap();
    let picture = workspace.plan.picture(ProjectFrame(frame)).unwrap();
    &workspace.document.nodes()[&picture.instance.node].audio_treatments
}

fn muted(update: &ProjectUpdate, frame: i64) -> bool {
    treatments(update, frame)
        .clip_gain()
        .is_some_and(deadpan_core::ClipGain::muted)
}

#[test]
fn native_scoped_mute_boolean_dot_keeps_nested_treatments_pictures_and_durable_history() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened, root) = setup_source(&scratch.path().join("Documents"));
    let initial = capture_scope(&opened, &root, 365);
    let mut recipe = treatments(&opened, 365).clone();
    for change in [
        AudioChange::Trim {
            gain: deadpan_core::GainDb::new(-6000).unwrap(),
        },
        AudioChange::Saturation {
            drive: Some(deadpan_core::GainDb::new(6000).unwrap()),
        },
    ] {
        recipe = change.apply(&recipe).unwrap();
    }
    let workspace = opened.workspace.as_ref().unwrap();
    let prepared = command(
        &service,
        ProjectRequest::Edit {
            expected_session: workspace.session,
            expected_revision: workspace.document.revision_id().clone(),
            cursor: initial.cursor,
            scope: initial.scope.clone(),
            edit: ProjectEdit::Scoped {
                target: Box::new(initial),
                edit: deadpan_core::ScopedNodeEdit::SetAudioTreatments {
                    treatments: recipe.clone(),
                },
            },
        },
    );
    assert!(prepared.error.is_none(), "{:?}", prepared.error);
    let before = prepared.workspace.as_ref().unwrap().clone();
    let path = before.path.clone();
    let target = capture_scope(&prepared, &root, 365);
    let old_rows = rows(&path);
    let first = send(&service, apply(&prepared, &target, 1, mute(true)));
    let commit = receipt(&first).committed().unwrap();
    assert_eq!(commit.cursor, Some(ProjectFrame(365)));
    assert_eq!(commit.scoped.as_ref().unwrap().before, target);
    assert_eq!(
        treatments(&first, 365),
        &AudioChange::Mute { muted: true }.apply(&recipe).unwrap()
    );
    assert_eq!(
        rows(&path),
        (old_rows.0 + 1, old_rows.1 + 1, old_rows.2 + 1)
    );
    assert_eq!(
        first
            .semantic
            .as_ref()
            .unwrap()
            .edit
            .as_ref()
            .unwrap()
            .operation,
        RepeatableEdit::Parameter(mute(true))
    );
    let next = capture_scope(&first, &root, 5);
    let mut operation = apply(&first, &next, 2, mute(true));
    let Operation::Apply { repeat_version, .. } = &mut operation else {
        unreachable!()
    };
    *repeat_version = Some(first.semantic.as_ref().unwrap().version);
    let second = send(&service, operation);
    for frame in 0..720 {
        assert_eq!(
            muted(&second, frame),
            (0..120).contains(&frame) || (360..480).contains(&frame)
        );
        assert_eq!(
            before.plan.picture(ProjectFrame(frame)).unwrap().picture,
            second
                .workspace
                .as_ref()
                .unwrap()
                .plan
                .picture(ProjectFrame(frame))
                .unwrap()
                .picture
        );
    }
    let same_target = capture_scope(&second, &root, 365);
    let stable_rows = rows(&path);
    let noop = command(
        &service,
        ProjectRequest::Macro(apply(&second, &same_target, 3, mute(true))),
    );
    assert!(noop.macros.as_ref().unwrap().result.is_err());
    assert_eq!(
        noop.workspace.as_ref().unwrap().document,
        second.workspace.as_ref().unwrap().document
    );
    assert_eq!(rows(&path), stable_rows);
    let unmuted = send(&service, apply(&second, &same_target, 4, mute(false)));
    assert_eq!(treatments(&unmuted, 365), &recipe);
    assert!(muted(&unmuted, 5));
    let undo = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: unmuted
                .workspace
                .as_ref()
                .unwrap()
                .document
                .revision_id()
                .clone(),
        },
    );
    assert_same_content(
        &undo.workspace.as_ref().unwrap().document,
        &second.workspace.as_ref().unwrap().document,
    );
    let redo = command(
        &service,
        ProjectRequest::Redo {
            expected_revision: undo
                .workspace
                .as_ref()
                .unwrap()
                .document
                .revision_id()
                .clone(),
        },
    );
    assert_same_content(
        &redo.workspace.as_ref().unwrap().document,
        &unmuted.workspace.as_ref().unwrap().document,
    );
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(path));
    assert_eq!(
        reopened.workspace.as_ref().unwrap().document,
        redo.workspace.as_ref().unwrap().document
    );
}

#[test]
fn native_scoped_mute_program_keeps_boolean_intent_and_refuses_stale_or_invalid_authority_atomically()
 {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened, root) = setup_source(&scratch.path().join("Documents"));
    let saved = send(&service, save(&opened, 1, 'm', program(vec![mute(true)])));
    let target = capture_scope(&saved, &root, 605);
    let path = saved.workspace.as_ref().unwrap().path.clone();
    let bank = stored_bank(&path);
    let run = |update: &ProjectUpdate, target: &Target, request| Operation::Run {
        id: id(update, request),
        register: 'm',
        count: 1,
        scope: target.scope.clone(),
        context: context_for(update, target),
    };
    let first = send(&service, run(&saved, &target, 2));
    assert!(muted(&first, 605));
    let same = capture_scope(&first, &root, 605);
    let stable_rows = rows(&path);
    let noop = command(&service, ProjectRequest::Macro(run(&first, &same, 3)));
    assert!(
        noop.macros.as_ref().unwrap().result.is_err(),
        "a saved true must not toggle the already-muted target"
    );
    assert_eq!(rows(&path), stable_rows);
    for case in 0..3 {
        let fresh = capture_scope(&first, &root, 5);
        let mut operation = apply(&first, &fresh, 4 + case, mute(true));
        let Operation::Apply { context, id, .. } = &mut operation else {
            unreachable!()
        };
        match case {
            0 => {
                context.scoped.as_mut().unwrap().target.repeats[0].branch =
                    RepeatEditBranch::Default
            }
            1 => {
                context.scoped.as_mut().unwrap().presentation.repeats[0].iteration =
                    target.presentation.as_ref().unwrap().repeats[0]
                        .iteration
                        .clone()
            }
            2 => id.session += 1,
            _ => unreachable!(),
        }
        let refused = command(&service, ProjectRequest::Macro(operation));
        assert!(refused.macros.as_ref().unwrap().result.is_err());
        assert_eq!(
            refused.workspace.as_ref().unwrap().document,
            first.workspace.as_ref().unwrap().document
        );
        assert_eq!(rows(&path), stable_rows);
        assert_eq!(stored_bank(&path), bank);
    }
    let stale = command(
        &service,
        ProjectRequest::Macro(apply(&saved, &target, 8, mute(false))),
    );
    assert!(stale.macros.as_ref().unwrap().result.is_err());
    assert_eq!(rows(&path), stable_rows);
    let invalid = send(
        &service,
        save(
            &first,
            9,
            'b',
            program(vec![
                mute(true),
                SemanticInstruction::SetAudio {
                    change: AudioChange::WetSend {
                        effect: deadpan_core::TailEffect::Reverb,
                        level: Some(deadpan_core::GainDb::UNITY),
                    },
                },
            ]),
        ),
    );
    let fresh = capture_scope(&invalid, &root, 5);
    let before = rows(&path);
    let failed = command(
        &service,
        ProjectRequest::Macro(Operation::Run {
            id: id(&invalid, 10),
            register: 'b',
            count: 1,
            scope: fresh.scope.clone(),
            context: context_for(&invalid, &fresh),
        }),
    );
    assert!(failed.macros.as_ref().unwrap().result.is_err());
    assert_eq!(rows(&path), before);
    assert_eq!(
        failed.workspace.as_ref().unwrap().document,
        first.workspace.as_ref().unwrap().document
    );
    command(&service, ProjectRequest::Close);
}
