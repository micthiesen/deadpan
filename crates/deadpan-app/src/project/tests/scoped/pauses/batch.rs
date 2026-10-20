//! Selected counterparts insert together, including mixed ancestor choices.
use super::*;

#[test]
fn native_selected_play_pauses_capture_each_counterpart_and_commit_one_history_step() {
    let (_scratch, service, mut before, root) = mixed::source_project();
    let path = before.path.clone();
    for (defaults, varied, cuts) in [
        ([false, false], 0, vec![124, 604]),
        ([false, false], 1, vec![484, 604]),
        ([true, false], 1, vec![4, 124, 244, 364, 484, 604]),
        ([false, true], 0, vec![4, 124, 484, 604]),
    ] {
        for black in [false, true] {
            let mut target = mixed::capture(&before, &root, 604, defaults);
            let selected_repeat = &target.target.repeats[varied].repeat;
            let NodeKind::Repeat { iterations, .. } =
                &before.document.nodes()[selected_repeat].kind
            else {
                panic!()
            };
            let other_iteration = iterations.at(0).unwrap();
            target.also.push(
                crate::project::scoped::retarget(
                    &before.document,
                    &target.target,
                    varied,
                    &other_iteration,
                )
                .unwrap(),
            );
            let rows = counts(&path);
            let result = command(&service, pause(target.clone(), black, false));
            assert!(result.error.is_none(), "{defaults:?}: {:?}", result.error);
            let commit = result.committed.unwrap();
            let receipt = commit.scoped.unwrap();
            let after = result.workspace.unwrap();
            let new_at =
                604 + i64::try_from(cuts.iter().filter(|cut| **cut < 604).count()).unwrap() * 3;
            assert_eq!(receipt.before, target);
            assert_eq!(commit.cursor, Some(ProjectFrame(new_at)));
            assert_eq!(
                after.plan.duration().frames(),
                720 + i64::try_from(cuts.len()).unwrap() * 3
            );
            assert_eq!(counts(&path), (rows.0 + 1, rows.1 + 1));
            assert_eq!(
                after.plan.picture(ProjectFrame(new_at)).unwrap().instance,
                receipt.presentation.clone().unwrap()
            );
            let counterpart = &receipt.also.as_ref().unwrap()[0];
            assert!(matches!(
                after.document.nodes()[&counterpart.node].kind,
                NodeKind::Hold { .. }
            ));
            let index = before
                .sources
                .values()
                .find_map(|source| source.video_index.as_ref())
                .unwrap();
            for (position, cut) in cuts.iter().enumerate() {
                let at = *cut + i64::try_from(position).unwrap() * 3;
                for offset in 0..3 {
                    let picture = after.plan.picture(ProjectFrame(at + offset)).unwrap();
                    if black {
                        assert!(matches!(picture.picture, deadpan_plan::Picture::Background));
                    } else {
                        assert_eq!(
                            picture.picture.select_source_frame(index).unwrap(),
                            before
                                .plan
                                .picture(ProjectFrame(cut - 1))
                                .unwrap()
                                .picture
                                .select_source_frame(index)
                                .unwrap()
                        );
                    }
                }
            }
            for frame in 0..720 {
                let shifted = frame
                    + i64::try_from(cuts.iter().filter(|cut| **cut <= frame).count()).unwrap() * 3;
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
                        .unwrap()
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

#[test]
fn native_selected_play_pause_rejects_overlapping_heterogeneous_and_ai_sets_without_writes() {
    let (_scratch, service, before, root) = mixed::source_project();
    let primary = mixed::capture(&before, &root, 4, [false, false]);
    let rows = counts(&before.path);
    for case in 0..3 {
        let mut target = primary.clone();
        target.also.push(match case {
            0 => primary.target.clone(),
            1 => mixed::capture(&before, &root, 124, [true, false]).target,
            _ => mixed::capture(&before, &root, 244, [false, false]).target,
        });
        let result = command(&service, pause(target, false, case == 2));
        assert!(result.error.is_some(), "case {case}");
        assert!(result.committed.is_none());
        assert_eq!(*result.workspace.unwrap().document, *before.document);
        assert_eq!(counts(&before.path), rows);
    }
}

#[test]
fn native_selected_play_pause_retains_each_owned_sources_independent_framing() {
    use deadpan_core::{ExactRatio, Framing, FramingPose};
    let (_scratch, service, mut before, root) = mixed::source_project();
    for (frame, scale) in [(124, 2), (604, 3)] {
        let target = mixed::capture(&before, &root, frame, [false, false]);
        let result = command(
            &service,
            request(
                &target,
                ScopedNodeEdit::SetFraming {
                    framing: Some(
                        Framing::static_pose(FramingPose {
                            scale: ExactRatio::integer(scale),
                            ..FramingPose::identity()
                        })
                        .unwrap(),
                    ),
                },
            ),
        );
        assert!(result.error.is_none(), "{:?}", result.error);
        before = result.workspace.unwrap();
    }
    let mut target = mixed::capture(&before, &root, 604, [false, false]);
    let NodeKind::Repeat { iterations, .. } = &before.document.nodes()[&root].kind else {
        panic!()
    };
    target.also.push(
        crate::project::scoped::retarget(
            &before.document,
            &target.target,
            0,
            &iterations.at(0).unwrap(),
        )
        .unwrap(),
    );
    let result = command(&service, pause(target, false, false));
    assert!(result.error.is_none(), "{:?}", result.error);
    let commit = result.committed.unwrap().scoped.unwrap();
    let after = result.workspace.unwrap();
    let NodeKind::Hold { recipe: primary } = &after.document.nodes()[&commit.target.node].kind
    else {
        panic!()
    };
    let NodeKind::Hold { recipe: other } =
        &after.document.nodes()[&commit.also.unwrap()[0].node].kind
    else {
        panic!()
    };
    assert!(primary.picture_context.is_some());
    assert!(other.picture_context.is_some());
    assert_ne!(
        primary.picture_context, other.picture_context,
        "each selected play must capture its own crop, rather than copy the primary freeze"
    );
    assert_eq!(
        primary.video, other.video,
        "both plays freeze the same Original frame with different owned crops"
    );
}
