//! Real qualified Original pictures and durable mixed-scope continuations.
use super::*;

pub(super) fn source_project() -> (tempfile::TempDir, ProjectService, Arc<Workspace>, NodeId) {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
    )
    .unwrap();
    service
        .submit(ProjectRequest::CreateFromSource {
            ownership: OriginalOwnership::Managed,
            path: fixture("cfr-bframes.mp4"),
        })
        .unwrap();
    let opened = wait(&service, |update| {
        update.import.as_ref().is_some_and(|status| {
            matches!(status.stage, ImportStage::Complete | ImportStage::Failed)
        })
    });
    assert!(opened.error.is_none(), "{:?}", opened.error);
    let source = opened.committed.unwrap().selected_node.unwrap();
    let workspace = opened.workspace.unwrap();
    assert_eq!(workspace.plan.duration().frames(), 120);
    let inner = edited(
        &service,
        &workspace,
        ProjectEdit::WrapRepeat {
            node: source,
            plays: 2,
        },
    );
    let inner_id = inner.committed.unwrap().selected_node.unwrap();
    let outer = edited(
        &service,
        &inner.workspace.unwrap(),
        ProjectEdit::WrapRepeat {
            node: inner_id,
            plays: 3,
        },
    );
    let root = outer.committed.unwrap().selected_node.unwrap();
    let before = outer.workspace.unwrap();
    assert_eq!(before.plan.duration().frames(), 720);
    (scratch, service, before, root)
}

pub(super) fn capture(
    workspace: &Workspace,
    root: &NodeId,
    frame: i64,
    defaults: [bool; 2],
) -> Target {
    let instance = workspace
        .plan
        .picture(ProjectFrame(frame))
        .unwrap()
        .instance;
    assert_eq!(instance.repeats.len(), 2);
    Target {
        session: workspace.session,
        navigation: 0,
        project: workspace.document.project_id().clone(),
        revision: workspace.document.revision_id().clone(),
        scope: SequenceScope::default(),
        root: root.clone(),
        target: ScopedNodeTarget {
            node: instance.node.clone(),
            repeats: instance
                .repeats
                .iter()
                .zip(defaults)
                .map(|(step, shared)| RepeatEditStep {
                    repeat: step.node.clone(),
                    branch: if shared {
                        RepeatEditBranch::Default
                    } else {
                        RepeatEditBranch::Play {
                            iteration: step.iteration.clone(),
                        }
                    },
                })
                .collect(),
        },
        presentation: Some(instance),
        cursor: ProjectFrame(frame),
        also: vec![],
    }
}

#[test]
fn native_mixed_pause_preserves_each_level_and_qualified_picture_with_one_history_step() {
    let (_scratch, service, mut before, root) = source_project();
    let path = before.path.clone();
    for defaults in [[true, false], [false, true]] {
        for outer in [0, 2] {
            for inner in [0, 1] {
                let old_at = outer * 240 + inner * 120 + 4;
                let new_at = if defaults[0] {
                    outer * 243 + inner * 120 + 4
                } else {
                    outer * 240 + inner * 123 + 4
                };
                let cuts: Vec<i64> = if defaults[0] {
                    (0..3).map(|play| play * 240 + inner * 120 + 4).collect()
                } else {
                    (0..2).map(|play| outer * 240 + play * 120 + 4).collect()
                };
                let target = capture(&before, &root, old_at, defaults);
                let rows = counts(&path);
                let result = command(&service, pause(target.clone(), false, false));
                assert!(
                    result.error.is_none(),
                    "{defaults:?} outer{outer} inner{inner}: {:?}",
                    result.error
                );
                let commit = result.committed.unwrap();
                let receipt = commit.scoped.unwrap();
                let after = result.workspace.unwrap();
                assert_eq!(receipt.before, target);
                assert_eq!(commit.cursor, Some(ProjectFrame(new_at)));
                assert_eq!(
                    after.plan.duration().frames(),
                    720 + i64::try_from(cuts.len()).unwrap() * 3
                );
                assert_eq!(counts(&path), (rows.0 + 1, rows.1 + 1));
                assert_eq!(receipt.target.repeats.len(), 2);
                for (step, shared) in receipt.target.repeats.iter().zip(defaults) {
                    assert_eq!(matches!(step.branch, RepeatEditBranch::Default), shared);
                }
                let presentation = receipt.presentation.as_ref().unwrap();
                assert!(
                    receipt
                        .target
                        .matches_instance(&after.document, presentation)
                        .unwrap()
                );
                assert_eq!(
                    after.plan.picture(ProjectFrame(new_at)).unwrap().instance,
                    *presentation
                );
                let index = before
                    .sources
                    .values()
                    .find_map(|source| source.video_index.as_ref())
                    .unwrap();
                let frozen = before
                    .plan
                    .picture(ProjectFrame(old_at - 1))
                    .unwrap()
                    .picture
                    .select_source_frame(index)
                    .unwrap();
                for offset in 0..3 {
                    assert_eq!(
                        after
                            .plan
                            .picture(ProjectFrame(new_at + offset))
                            .unwrap()
                            .picture
                            .select_source_frame(index)
                            .unwrap(),
                        frozen
                    );
                }
                // Independent authored old cuts determine the translation. No
                // changed plan is used to construct expected source positions.
                for frame in 0..720 {
                    let shifted = frame
                        + i64::try_from(cuts.iter().filter(|cut| **cut <= frame).count()).unwrap()
                            * 3;
                    assert_eq!(
                        after
                            .plan
                            .picture(ProjectFrame(shifted))
                            .unwrap()
                            .picture
                            .select_source_frame(index)
                            .unwrap(),
                        before
                            .plan
                            .picture(ProjectFrame(frame))
                            .unwrap()
                            .picture
                            .select_source_frame(index)
                            .unwrap(),
                        "{defaults:?} outer{outer} inner{inner} old frame{frame}"
                    );
                }
                command(&service, ProjectRequest::Close);
                let reopened = command(&service, ProjectRequest::Open(path.clone()))
                    .workspace
                    .unwrap();
                assert_eq!(*reopened.document, *after.document);
                let undone = command(
                    &service,
                    ProjectRequest::Undo {
                        expected_revision: reopened.document.revision_id().clone(),
                    },
                )
                .workspace
                .unwrap();
                assert_same_content(&undone.document, &before.document);
                let redone = command(
                    &service,
                    ProjectRequest::Redo {
                        expected_revision: undone.document.revision_id().clone(),
                    },
                )
                .workspace
                .unwrap();
                assert_same_content(&redone.document, &after.document);
                before = command(
                    &service,
                    ProjectRequest::Undo {
                        expected_revision: redone.document.revision_id().clone(),
                    },
                )
                .workspace
                .unwrap();
            }
        }
    }
}
