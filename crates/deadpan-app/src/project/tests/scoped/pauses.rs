use super::*;

fn simple(workspace: &Workspace, play_index: u32, at: i64) -> Target {
    Target {
        session: workspace.session,
        project: workspace.document.project_id().clone(),
        revision: workspace.document.revision_id().clone(),
        scope: SequenceScope::default(),
        root: node("repeat"),
        target: ScopedNodeTarget {
            node: node("a"),
            repeats: vec![RepeatEditStep {
                repeat: node("repeat"),
                branch: play("plays", play_index),
            }],
        },
        presentation: Some(InstancePath {
            node: node("a"),
            repeats: vec![RepeatInstance {
                node: node("repeat"),
                iteration: iteration("plays", play_index),
            }],
        }),
        cursor: ProjectFrame(at),
        also: Vec::new(),
    }
}

fn pause(target: Target, black: bool, ai: bool) -> ProjectRequest {
    ProjectRequest::Edit {
        expected_session: target.session,
        expected_revision: target.revision.clone(),
        scope: target.scope.clone(),
        cursor: target.cursor,
        edit: ProjectEdit::ScopedPause {
            target: Box::new(target),
            duration: FrameDuration::new(3).unwrap(),
            black,
            ai,
        },
    }
}

fn setup() -> (tempfile::TempDir, PathBuf, ProjectService, Arc<Workspace>) {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("pauses.deadpan");
    let mut store = seed_holds(&path, &["a", "b"]);
    seed_command(
        &mut store,
        Command::WrapRepeat {
            node: node("a"),
            id: node("repeat"),
            plays: 3,
            gap: None,
            anchor_policy: Default::default(),
        },
        "plays",
    );
    drop(store);
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let workspace = command(&service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    (scratch, path, service, workspace)
}

#[test]
fn scoped_pause_keeps_selected_play_edges_receipts_and_durable_history() {
    let (_scratch, path, service, mut before) = setup();
    for play_index in 0..3 {
        for local in [0, 4, 10] {
            for (black, ai) in [(false, false), (true, false), (false, true)] {
                let at = i64::from(play_index) * 10 + local;
                let target = simple(&before, play_index, at);
                let rows = counts(&path);
                let update = command(&service, pause(target.clone(), black, ai));
                assert!(update.error.is_none(), "{:?}", update.error);
                let commit = update.committed.unwrap();
                let receipt = commit.scoped.unwrap();
                let after = update.workspace.unwrap();
                assert_eq!(receipt.before, target);
                assert_eq!(commit.cursor, Some(ProjectFrame(at)));
                assert_eq!(commit.selected_node, Some(node("repeat")));
                assert_eq!(after.plan.duration().frames(), 43);
                assert_eq!(
                    after.plan.picture(ProjectFrame(at)).unwrap().instance,
                    receipt.presentation.clone().unwrap()
                );
                assert_eq!(receipt.target.repeats[0].branch, play("plays", play_index));
                assert_eq!(
                    after.document.nodes()[&node("a")],
                    before.document.nodes()[&node("a")]
                );
                assert_eq!(counts(&path), (rows.0 + 1, rows.1 + 1));
                continued(&after, &receipt).validate(&after).unwrap();
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

#[test]
fn scoped_pause_rejects_broader_missing_stale_and_foreign_captures_without_writes() {
    let (_scratch, path, service, before) = setup();
    let valid = simple(&before, 1, 14);
    let original_counts = counts(&path);
    for case in 0..7 {
        let mut target = valid.clone();
        match case {
            0 => target.target.repeats[0].branch = RepeatEditBranch::Default,
            1 => target.presentation = None,
            2 => target.cursor = ProjectFrame(4),
            3 => target.session += 1,
            4 => target.revision = RevisionId::new("stale").unwrap(),
            5 => target.also.push(simple(&before, 2, 24).target),
            _ => target.presentation.as_mut().unwrap().repeats[0].iteration = iteration("plays", 0),
        }
        let update = command(&service, pause(target, false, false));
        assert!(update.error.is_some(), "case {case}");
        assert!(update.committed.is_none());
        assert_eq!(*update.workspace.unwrap().document, *before.document);
        assert_eq!(counts(&path), original_counts);
    }
}
