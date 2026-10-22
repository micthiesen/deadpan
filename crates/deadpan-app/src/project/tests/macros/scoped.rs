//! Real qualified pictures through one-play semantic service receipts.

mod audio_setters;
mod duration;
mod fixed_duration;
mod gain;
mod mute;
mod pauses;
mod this_play_fixed_duration;

use super::*;
use crate::project::scoped::Target;
use crate::project::semantic::RepeatableEdit;
use deadpan_core::{
    ExactRatio, RepeatEditBranch, RepeatEditStep, ScopedNodeTarget, SemanticZoomCenter,
    framing_zoom::{ZoomCurve, ZoomKind},
};

fn setup_source(documents: &Path) -> (ProjectService, ProjectUpdate, NodeId) {
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(documents.to_path_buf()).unwrap()),
    )
    .unwrap();
    service
        .submit(ProjectRequest::CreateFromSource {
            ownership: OriginalOwnership::Managed,
            path: fixture("cfr-bframes.mp4"),
        })
        .unwrap();
    let initialized = wait(&service, |update| {
        update.import.as_ref().is_some_and(|status| {
            matches!(status.stage, ImportStage::Complete | ImportStage::Failed)
        })
    });
    assert!(initialized.error.is_none(), "{:?}", initialized.error);
    assert_eq!(
        initialized.import.as_ref().unwrap().stage,
        ImportStage::Complete
    );
    let opened = command(
        &service,
        ProjectRequest::Open(initialized.workspace.as_ref().unwrap().path.clone()),
    );
    let workspace = opened.workspace.as_ref().unwrap();
    let Some(SingleSourceState::Ready { node, .. }) = &workspace.single_source else {
        panic!()
    };
    assert_eq!(workspace.plan.duration().frames(), 120);
    let inner = edited(
        &service,
        workspace,
        ProjectEdit::WrapRepeat {
            node: node.clone(),
            plays: 2,
        },
    );
    let inner_node = inner
        .committed
        .as_ref()
        .unwrap()
        .selected_node
        .clone()
        .unwrap();
    let outer = edited(
        &service,
        inner.workspace.as_ref().unwrap(),
        ProjectEdit::WrapRepeat {
            node: inner_node,
            plays: 3,
        },
    );
    let root = outer
        .committed
        .as_ref()
        .unwrap()
        .selected_node
        .clone()
        .unwrap();
    assert_eq!(
        outer.workspace.as_ref().unwrap().plan.duration().frames(),
        720
    );
    (service, outer, root)
}

fn capture_scope(update: &ProjectUpdate, root: &NodeId, frame: i64) -> Target {
    let workspace = update.workspace.as_ref().unwrap();
    let sample = workspace.plan.picture(ProjectFrame(frame)).unwrap();
    let layer = sample
        .framing
        .iter()
        .find(|layer| !layer.escalation && layer.instance.node == sample.instance.node)
        .unwrap();
    assert_eq!(layer.duration.frames(), 120);
    assert_eq!(layer.instance.repeats.len(), 2);
    let target = Target {
        session: workspace.session,
        navigation: 0,
        project: workspace.document.project_id().clone(),
        revision: workspace.document.revision_id().clone(),
        scope: SequenceScope::default(),
        root: root.clone(),
        target: ScopedNodeTarget {
            node: layer.instance.node.clone(),
            repeats: layer
                .instance
                .repeats
                .iter()
                .map(|actual| RepeatEditStep {
                    repeat: actual.node.clone(),
                    branch: RepeatEditBranch::Play {
                        iteration: actual.iteration.clone(),
                    },
                })
                .collect(),
        },
        presentation: Some(layer.instance.clone()),
        cursor: ProjectFrame(frame),
        also: vec![],
    };
    target.semantic_framing(workspace).unwrap();
    target
}

fn context_for(update: &ProjectUpdate, target: &Target) -> SemanticContext {
    SemanticContext {
        original: None,
        hold_duration: None,
        shared_hold_duration: None,
        selected_hold_duration: None,
        mixed_hold_duration: None,
        scoped: Some(
            target
                .semantic_framing(update.workspace.as_ref().unwrap())
                .unwrap(),
        ),
        parent: update.workspace.as_ref().unwrap().document.root().clone(),
        cursor: target.cursor,
        selected_child: Some(target.root.clone()),
        visual_selection: None,
    }
}

fn smash(scale: i64) -> SemanticInstruction {
    SemanticInstruction::Zoom {
        kind: ZoomKind::Smash {
            scale: ExactRatio::integer(scale),
        },
        center: SemanticZoomCenter::OriginalCenter,
    }
}

fn apply(
    update: &ProjectUpdate,
    target: &Target,
    request: u64,
    instruction: SemanticInstruction,
) -> Operation {
    Operation::Apply {
        id: id(update, request),
        instruction,
        repeat_version: None,
        scope: target.scope.clone(),
        context: context_for(update, target),
    }
}

fn assert_same_content(
    actual: &deadpan_core::ProjectDocument,
    expected: &deadpan_core::ProjectDocument,
) {
    let mut actual: serde_json::Value = serde_json::from_str(&actual.to_json().unwrap()).unwrap();
    let mut expected: serde_json::Value =
        serde_json::from_str(&expected.to_json().unwrap()).unwrap();
    actual.as_object_mut().unwrap().remove("revision_id");
    expected.as_object_mut().unwrap().remove("revision_id");
    assert_eq!(actual, expected);
}

fn stored_bank(path: &Path) -> deadpan_store::registers::RegisterBank {
    ProjectStore::open(path, AccessMode::ReadOnly)
        .unwrap()
        .registers()
        .unwrap()
}

#[test]
fn native_scoped_macro_maps_nested_source_receipt_keeps_other_plays_and_survives_history_reopen() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened, root) = setup_source(&scratch.path().join("Documents"));
    let creep = SemanticInstruction::Zoom {
        kind: ZoomKind::Creep {
            from: None,
            to: ExactRatio::integer(3),
            curve: ZoomCurve::Linear,
        },
        center: SemanticZoomCenter::OriginalCenter,
    };
    let saved_inner = send(&service, save(&opened, 1, 'z', program(vec![creep])));
    let saved = send(
        &service,
        save(
            &saved_inner,
            2,
            'a',
            program(vec![
                smash(2),
                SemanticInstruction::Call {
                    register: RegisterName::new('z').unwrap(),
                    count: NonZeroU32::new(1).unwrap(),
                },
            ]),
        ),
    );
    let before = saved.workspace.as_ref().unwrap().clone();
    let path = before.path.clone();
    let target = capture_scope(&saved, &root, 360);
    let bank = stored_bank(&path);
    let initial_rows = rows(&path);
    let executed = send(
        &service,
        Operation::Run {
            id: id(&saved, 3),
            register: 'a',
            count: 1,
            scope: target.scope.clone(),
            context: context_for(&saved, &target),
        },
    );
    let committed = receipt(&executed).committed().unwrap();
    let scoped = committed.scoped.as_ref().unwrap();
    let after = executed.workspace.as_ref().unwrap();
    assert!(committed.preserve_cursor);
    assert_eq!(committed.selected_node.as_ref(), Some(&root));
    assert_eq!(committed.cursor, Some(target.cursor));
    assert_eq!(committed.scope, target.scope);
    assert_eq!(scoped.before, target);
    assert_eq!(&scoped.revision, after.document.revision_id());
    assert_eq!(scoped.revision, committed.revision);
    assert_ne!(scoped.target.node, target.target.node);
    assert_eq!(scoped.target.repeats[0].repeat, root);
    assert_ne!(
        scoped.target.repeats[1].repeat,
        target.target.repeats[1].repeat
    );
    assert!(
        scoped
            .target
            .matches_instance(&after.document, scoped.presentation.as_ref().unwrap())
            .unwrap()
    );
    let continued = Target {
        revision: scoped.revision.clone(),
        target: scoped.target.clone(),
        presentation: scoped.presentation.clone(),
        ..scoped.before.clone()
    };
    continued.semantic_framing(after).unwrap();
    for (node, value) in before.document.nodes() {
        assert_eq!(
            &after.document.nodes()[node],
            value,
            "shared template {node}"
        );
    }
    assert_eq!(before.plan.duration(), after.plan.duration());
    for frame in 0..720 {
        let before_picture = before.plan.picture(ProjectFrame(frame)).unwrap();
        let after_picture = after.plan.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(
            before_picture.picture, after_picture.picture,
            "provider frame {frame}"
        );
        assert_eq!(before_picture.local_position, after_picture.local_position);
        assert_eq!(
            before_picture.picture_context,
            after_picture.picture_context
        );
        assert_eq!(before_picture.gap_after, after_picture.gap_after);
        assert_eq!(before_picture.captions, after_picture.captions);
        assert_eq!(before_picture.framing.len(), after_picture.framing.len());
        for (before_layer, after_layer) in before_picture.framing.iter().zip(&after_picture.framing)
        {
            assert_eq!(before_layer.local_position, after_layer.local_position);
            assert_eq!(before_layer.duration, after_layer.duration);
            assert_eq!(
                before_layer.effective_duration,
                after_layer.effective_duration
            );
            assert_eq!(before_layer.escalation, after_layer.escalation);
            if !(360..480).contains(&frame) {
                assert_eq!(
                    before_layer.pose, after_layer.pose,
                    "unselected frame {frame}"
                );
            }
        }
        if (360..480).contains(&frame) {
            let pose = after_picture
                .framing
                .iter()
                .find(|layer| layer.instance.node == scoped.target.node)
                .unwrap()
                .pose
                .unwrap();
            assert!(pose.scale.compare_integer(2).is_gt());
            assert!(pose.scale.compare_integer(3).is_lt());
        }
    }
    assert_eq!(stored_bank(&path), bank);
    assert_eq!(
        rows(&path),
        (initial_rows.0 + 1, initial_rows.1 + 1, initial_rows.2 + 2)
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
    assert_eq!(stored_bank(&path), bank);
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
    let reopened = command(&service, ProjectRequest::Open(path));
    assert_eq!(
        *reopened.workspace.as_ref().unwrap().document,
        *redone.workspace.as_ref().unwrap().document
    );
}

#[test]
fn native_scoped_apply_keeps_zoom_only_dot_noop_and_atomic_failures_and_durable_refresh_receipt() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened, root) = setup_source(&scratch.path().join("Documents"));
    let target = capture_scope(&opened, &root, 360);
    let applied = send(&service, apply(&opened, &target, 1, smash(2)));
    let path = applied.workspace.as_ref().unwrap().path.clone();
    let snapshot = applied.semantic.as_ref().unwrap();
    assert_eq!(
        snapshot.edit.as_ref().unwrap().operation,
        RepeatableEdit::Parameter(smash(2))
    );
    assert_eq!(snapshot.edit.as_ref().unwrap().register, None);
    let rows_after = rows(&path);
    let selected = capture_scope(&applied, &root, 360);
    let same = send(&service, apply(&applied, &selected, 2, smash(2)));
    assert!(receipt(&same).committed().is_none());
    assert_eq!(rows(&path), rows_after);
    assert_eq!(same.semantic.as_ref().unwrap().version, snapshot.version);
    assert_eq!(
        same.workspace.as_ref().unwrap().document,
        applied.workspace.as_ref().unwrap().document
    );
    let saved_bad_inner = send(
        &service,
        save(&same, 3, 'c', program(vec![SemanticInstruction::Split])),
    );
    let saved_bad = send(
        &service,
        save(
            &saved_bad_inner,
            4,
            'b',
            program(vec![
                smash(4),
                SemanticInstruction::Call {
                    register: RegisterName::new('c').unwrap(),
                    count: NonZeroU32::new(1).unwrap(),
                },
            ]),
        ),
    );
    let unchanged = saved_bad.workspace.as_ref().unwrap().document.clone();
    let bank = stored_bank(&path);
    let stable_rows = rows(&path);
    for case in 0..3 {
        let mut operation = apply(&saved_bad, &selected, 10 + case, smash(5));
        let Operation::Apply { context, .. } = &mut operation else {
            unreachable!()
        };
        let scoped = context.scoped.as_mut().unwrap();
        match case {
            0 => scoped.target.repeats[0].branch = RepeatEditBranch::Default,
            1 => {}
            2 => scoped.root = scoped.target.node.clone(),
            _ => unreachable!(),
        }
        if case == 1 {
            // A real different play, not the captured occurrence.
            let NodeKind::Repeat { iterations, .. } = &unchanged.nodes()[&root].kind else {
                panic!()
            };
            scoped.presentation.repeats[0].iteration = iterations.at(0).unwrap();
        }
        let refused = command(&service, ProjectRequest::Macro(operation));
        assert!(refused.macros.as_ref().unwrap().result.is_err());
        assert_eq!(rows(&path), stable_rows);
        assert_eq!(stored_bank(&path), bank);
        assert_eq!(refused.workspace.as_ref().unwrap().document, unchanged);
    }
    let failed = command(
        &service,
        ProjectRequest::Macro(Operation::Run {
            id: id(&saved_bad, 20),
            register: 'b',
            count: 1,
            scope: selected.scope.clone(),
            context: context_for(&saved_bad, &selected),
        }),
    );
    assert!(
        failed
            .macros
            .as_ref()
            .unwrap()
            .result
            .as_ref()
            .unwrap_err()
            .contains("only Zoom")
    );
    assert_eq!(rows(&path), stable_rows);
    assert_eq!(stored_bank(&path), bank);
    assert_eq!(failed.workspace.as_ref().unwrap().document, unchanged);
    assert_eq!(
        failed.workspace.as_ref().unwrap().document.marks(),
        unchanged.marks()
    );
    service
        .shared
        .workspace_refresh_failure
        .store(true, Ordering::Release);
    let submitted = apply(&saved_bad, &selected, 21, smash(6));
    let durable = send(&service, submitted.clone());
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
    assert_eq!(committed.scoped.as_ref().unwrap().before, selected);
    assert_ne!(&committed.revision, unchanged.revision_id());
    assert_eq!(durable.workspace.as_ref().unwrap().document, unchanged);
    assert_eq!(
        rows(&path),
        (stable_rows.0 + 1, stable_rows.1 + 1, stable_rows.2 + 1)
    );
    assert_eq!(stored_bank(&path), bank);
    let retry = send(&service, submitted);
    assert_eq!(receipt(&retry).committed(), Some(committed.as_ref()));
    assert_eq!(
        rows(&path),
        (stable_rows.0 + 1, stable_rows.1 + 1, stable_rows.2 + 1)
    );
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(path));
    assert_eq!(
        reopened.workspace.as_ref().unwrap().document.revision_id(),
        &committed.revision
    );
}
