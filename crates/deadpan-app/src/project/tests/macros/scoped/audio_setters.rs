//! Literal audio setters keep current occurrence authority and independent recipes.
use super::*;
use deadpan_core::{AudioChange, AudioTreatments, ClipGain, GainDb, GainRange};

fn db(value: i32) -> GainDb {
    GainDb::new(value).unwrap()
}
fn instruction(change: AudioChange) -> SemanticInstruction {
    SemanticInstruction::SetAudio { change }
}
fn changes() -> Vec<AudioChange> {
    let eq =
        |bass| deadpan_core::EqualizerRecipe::new([db(bass), db(0), db(0)], None, None).unwrap();
    vec![
        AudioChange::Trim { gain: db(-6000) },
        AudioChange::Saturation {
            drive: Some(db(6000)),
        },
        AudioChange::Balance {
            balance: Some(deadpan_core::StereoBalance::new(35).unwrap()),
        },
        AudioChange::Equalizer {
            recipe: Some(eq(3000)),
        },
        AudioChange::Trim { gain: db(-9000) },
        AudioChange::Saturation {
            drive: Some(db(9000)),
        },
        AudioChange::Balance {
            balance: Some(deadpan_core::StereoBalance::new(-35).unwrap()),
        },
        AudioChange::Equalizer {
            recipe: Some(eq(-3000)),
        },
        AudioChange::Saturation { drive: None },
        AudioChange::Balance { balance: None },
        AudioChange::Equalizer { recipe: None },
    ]
}
fn recipe(update: &ProjectUpdate, frame: i64) -> &AudioTreatments {
    let workspace = update.workspace.as_ref().unwrap();
    let picture = workspace.plan.picture(ProjectFrame(frame)).unwrap();
    &workspace.document.nodes()[&picture.instance.node].audio_treatments
}
fn seeded(service: &ProjectService, opened: &ProjectUpdate, root: &NodeId) -> ProjectUpdate {
    let target = capture_scope(opened, root, 365);
    let range = GainRange::new(ExactRatio::integer(1), ExactRatio::integer(100)).unwrap();
    let envelope = deadpan_core::GainEnvelope::new(
        deadpan_core::GainClock::OwnerOutput,
        range,
        db(-3000),
        vec![
            deadpan_core::GainSegment::new(
                range.end(),
                db(3000),
                deadpan_core::GainCurve::Smoothstep,
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let treatments = AudioTreatments::from_clip_gain(
        ClipGain::new(
            db(-12000),
            true,
            vec![envelope],
            vec![GainRange::new(ExactRatio::integer(2), ExactRatio::integer(3)).unwrap()],
        )
        .unwrap(),
    );
    let treatments = AudioChange::WetSend {
        effect: deadpan_core::TailEffect::Reverb,
        level: Some(db(-6000)),
    }
    .apply(&treatments)
    .unwrap();
    let treatments = AudioChange::TailSend {
        effect: deadpan_core::TailEffect::Delay,
        level: Some(db(-9000)),
    }
    .apply(&treatments)
    .unwrap();
    let workspace = opened.workspace.as_ref().unwrap();
    let update = command(
        service,
        ProjectRequest::Edit {
            expected_session: workspace.session,
            expected_revision: workspace.document.revision_id().clone(),
            cursor: target.cursor,
            scope: target.scope.clone(),
            edit: ProjectEdit::Scoped {
                target: Box::new(target),
                edit: deadpan_core::ScopedNodeEdit::SetAudioTreatments { treatments },
            },
        },
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    update
}

#[test]
fn native_scoped_audio_setters_dot_replaces_new_peer_preserves_curves_sends_and_exact_history() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened, root) = setup_source(&scratch.path().join("Documents"));
    let mut current = seeded(&service, &opened, &root);
    let before = current.workspace.as_ref().unwrap().clone();
    let path = before.path.clone();
    let bank = stored_bank(&path);
    let mut expected_selected = recipe(&current, 365).clone();
    let mut expected_peer = recipe(&current, 5).clone();
    let mut serial = 1;
    for change in changes() {
        let target = capture_scope(&current, &root, 365);
        let operation = instruction(change);
        let previous = current.workspace.as_ref().unwrap().document.clone();
        current = send(
            &service,
            apply(&current, &target, serial, operation.clone()),
        );
        serial += 1;
        expected_selected = change.apply(&expected_selected).unwrap();
        assert_eq!(recipe(&current, 365), &expected_selected);
        assert_eq!(recipe(&current, 5), &expected_peer);
        assert_eq!(
            receipt(&current).committed().unwrap().cursor,
            Some(ProjectFrame(365))
        );
        assert_eq!(
            current
                .semantic
                .as_ref()
                .unwrap()
                .edit
                .as_ref()
                .unwrap()
                .operation,
            RepeatableEdit::Parameter(operation.clone())
        );
        let target = capture_scope(&current, &root, 5);
        let mut dot = apply(&current, &target, serial, operation);
        serial += 1;
        let Operation::Apply { repeat_version, .. } = &mut dot else {
            unreachable!()
        };
        *repeat_version = Some(current.semantic.as_ref().unwrap().version);
        current = send(&service, dot);
        expected_peer = change.apply(&expected_peer).unwrap();
        assert_eq!(recipe(&current, 5), &expected_peer);
        assert_eq!(recipe(&current, 365), &expected_selected);
        let stable_rows = rows(&path);
        let target = capture_scope(&current, &root, 5);
        let noop = command(
            &service,
            ProjectRequest::Macro(apply(&current, &target, serial, instruction(change))),
        );
        serial += 1;
        assert!(noop.macros.as_ref().unwrap().result.is_err());
        assert_eq!(rows(&path), stable_rows);
        assert_eq!(
            noop.workspace.as_ref().unwrap().document,
            current.workspace.as_ref().unwrap().document
        );
        assert_ne!(current.workspace.as_ref().unwrap().document, previous);
    }
    assert_eq!(stored_bank(&path), bank);
    for frame in 0..720 {
        let expected = if (360..480).contains(&frame) {
            &expected_selected
        } else if (0..120).contains(&frame) {
            &expected_peer
        } else {
            recipe(&opened, frame)
        };
        assert_eq!(recipe(&current, frame), expected);
        assert_eq!(
            current
                .workspace
                .as_ref()
                .unwrap()
                .plan
                .picture(ProjectFrame(frame))
                .unwrap()
                .picture,
            before.plan.picture(ProjectFrame(frame)).unwrap().picture
        );
    }
    let after = current.workspace.as_ref().unwrap().document.clone();
    let undo = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: after.revision_id().clone(),
        },
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
    assert_same_content(&redo.workspace.as_ref().unwrap().document, &after);
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(path));
    assert_eq!(
        reopened.workspace.as_ref().unwrap().document,
        redo.workspace.as_ref().unwrap().document
    );
}

#[test]
fn native_scoped_audio_setters_counted_program_and_late_refusals_are_atomic() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened, root) = setup_source(&scratch.path().join("Documents"));
    let sequence = changes();
    let body = sequence
        .iter()
        .copied()
        .enumerate()
        .filter(|(index, _)| ![5, 6, 7].contains(index))
        .map(|(_, change)| instruction(change))
        .collect::<Vec<_>>();
    let saved = send(&service, save(&opened, 1, 't', program(body.clone())));
    let target = capture_scope(&saved, &root, 605);
    let path = saved.workspace.as_ref().unwrap().path.clone();
    let old_rows = rows(&path);
    let executed = send(
        &service,
        Operation::Run {
            id: id(&saved, 2),
            register: 't',
            count: 3,
            scope: target.scope.clone(),
            context: context_for(&saved, &target),
        },
    );
    let mut expected = AudioTreatments::default();
    for _ in 0..3 {
        for instruction in &body {
            let SemanticInstruction::SetAudio { change } = instruction else {
                unreachable!()
            };
            expected = change.apply(&expected).unwrap();
        }
    }
    assert_eq!(recipe(&executed, 605), &expected);
    assert_eq!(
        rows(&path),
        (old_rows.0 + 1, old_rows.1 + 1, old_rows.2 + 24)
    );
    assert_eq!(recipe(&executed, 5), &AudioTreatments::default());
    let stale = command(
        &service,
        ProjectRequest::Macro(apply(&saved, &target, 3, instruction(sequence[0]))),
    );
    assert!(stale.macros.as_ref().unwrap().result.is_err());
    let stable_rows = rows(&path);
    for (case, change) in sequence.iter().take(3).copied().enumerate() {
        let fresh = capture_scope(&executed, &root, 5);
        let mut operation = apply(
            &executed,
            &fresh,
            4 + u64::try_from(case).unwrap(),
            instruction(change),
        );
        let Operation::Apply { context, id, .. } = &mut operation else {
            unreachable!()
        };
        match case {
            0 => {
                context.scoped.as_mut().unwrap().target.repeats[0].branch =
                    RepeatEditBranch::Default
            }
            1 => id.session += 1,
            2 => {
                context.visual_selection = Some(deadpan_core::SemanticVisualSelection::Time {
                    anchor: ProjectFrame(5),
                    head: ProjectFrame(6),
                    extending: false,
                })
            }
            _ => unreachable!(),
        }
        let refused = command(&service, ProjectRequest::Macro(operation));
        assert!(refused.macros.as_ref().unwrap().result.is_err());
        assert_eq!(rows(&path), stable_rows);
        assert_eq!(
            refused.workspace.as_ref().unwrap().document,
            executed.workspace.as_ref().unwrap().document
        );
    }
    let invalid = send(
        &service,
        save(
            &executed,
            7,
            'b',
            program(vec![
                instruction(sequence[0]),
                instruction(AudioChange::WetSend {
                    effect: deadpan_core::TailEffect::Reverb,
                    level: Some(db(-6000)),
                }),
            ]),
        ),
    );
    let target = capture_scope(&invalid, &root, 5);
    let stable_rows = rows(&path);
    let stable_bank = stored_bank(&path);
    let failed = command(
        &service,
        ProjectRequest::Macro(Operation::Run {
            id: id(&invalid, 8),
            register: 'b',
            count: 1,
            scope: target.scope.clone(),
            context: context_for(&invalid, &target),
        }),
    );
    assert!(failed.macros.as_ref().unwrap().result.is_err());
    assert_eq!(rows(&path), stable_rows);
    assert_eq!(stored_bank(&path), stable_bank);
    assert_eq!(
        failed.workspace.as_ref().unwrap().document,
        executed.workspace.as_ref().unwrap().document
    );
    let undo = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: executed
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
        &saved.workspace.as_ref().unwrap().document,
    );
    command(&service, ProjectRequest::Close);
}
