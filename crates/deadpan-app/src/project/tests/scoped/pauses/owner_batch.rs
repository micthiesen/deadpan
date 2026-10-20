//! Owner-relative pauses may resolve different implicit inner Repeat plays.
use super::*;
use deadpan_core::{IterationOrder, PlayOverride, PlayOverrides};
use std::collections::BTreeMap;

fn setup() -> (tempfile::TempDir, ProjectService, Arc<Workspace>) {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("owner-batch.deadpan");
    let document = ProjectDocument::new_automatic(
        ProjectId::new("owner-batch").unwrap(),
        RevisionId::new("initial").unwrap(),
        node("root"),
    )
    .unwrap();
    let mut store = ProjectStore::create(&path, &document).unwrap();
    let repeated = |name: &str, child: &str| {
        let mut beat = BeatNode::sequence(name, Vec::new());
        beat.kind = NodeKind::Repeat {
            child: node(child),
            iterations: IterationOrder::new(RevisionId::new(name).unwrap(), 2).unwrap(),
            gap: None,
            escalation: None,
        };
        beat
    };
    seed_command(
        &mut store,
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("outer"),
                nodes: BTreeMap::from([
                    (node("outer"), repeated("outer", "inner")),
                    (node("inner"), repeated("inner", "a")),
                    (node("a"), BeatNode::hold("a", hold(4))),
                    (node("owned-inner"), repeated("owned-inner", "owned-a")),
                    (node("owned-a"), BeatNode::hold("owned-a", hold(2))),
                ]),
                overrides: BTreeMap::from([(
                    node("outer"),
                    PlayOverrides::try_from(vec![PlayOverride {
                        iteration: iteration("outer", 1),
                        root: node("owned-inner"),
                    }])
                    .unwrap(),
                )]),
                gap_overrides: BTreeMap::new(),
            },
        },
        "fixture",
    );
    drop(store);
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let workspace = command(&service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    assert_eq!(workspace.plan.duration().frames(), 12);
    (scratch, service, workspace)
}

fn capture(workspace: &Workspace) -> Target {
    let NodeKind::Repeat { iterations, .. } = &workspace.document.nodes()[&node("outer")].kind
    else {
        panic!("fixture outer Repeat")
    };
    let first = iterations.at(0).unwrap();
    let second = iterations.at(1).unwrap();
    let primary = ScopedNodeTarget {
        node: node("inner"),
        repeats: vec![RepeatEditStep {
            repeat: node("outer"),
            branch: RepeatEditBranch::Play {
                iteration: first.clone(),
            },
        }],
    };
    let other =
        crate::project::scoped::retarget(&workspace.document, &primary, 0, &second).unwrap();
    Target {
        session: workspace.session,
        navigation: 0,
        project: workspace.document.project_id().clone(),
        revision: workspace.document.revision_id().clone(),
        scope: SequenceScope::default(),
        root: node("outer"),
        target: primary,
        presentation: Some(InstancePath {
            node: node("inner"),
            repeats: vec![RepeatInstance {
                node: node("outer"),
                iteration: first,
            }],
        }),
        cursor: ProjectFrame(3),
        also: vec![other],
    }
}

fn inner_play(document: &ProjectDocument, target: &ScopedNodeTarget) -> u32 {
    assert_eq!(target.repeats.len(), 2);
    let step = &target.repeats[1];
    let RepeatEditBranch::Play { iteration } = &step.branch else {
        panic!("the actual cut captures a concrete inner play")
    };
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&step.repeat].kind else {
        panic!("retained inner Repeat")
    };
    iterations.position(iteration).unwrap()
}

fn assert_compound(path: &Path) {
    let json: String = rusqlite::Connection::open(path.join("project.sqlite"))
        .unwrap()
        .query_row(
            "SELECT request FROM history ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let request: CommandRequest = serde_json::from_str(&json).unwrap();
    let Command::Compound { transaction } = request.command.base_command() else {
        panic!("selected owner pauses persist as one Compound")
    };
    assert_eq!(transaction.steps().len(), 2);
    assert!(transaction.steps().iter().all(|step| matches!(
        step.edit().unwrap().command.as_command(),
        Command::InsertOccurrenceTime { .. }
    )));
}

#[test]
fn native_owner_pause_batch_retains_different_inferred_inner_plays_for_consecutive_edits() {
    let (_scratch, service, before) = setup();
    let path = before.path.clone();
    let rows = counts(&path);
    let target = capture(&before);
    let first = command(&service, pause(target.clone(), true, false));
    assert!(first.error.is_none(), "{:?}", first.error);
    let first_commit = first.committed.unwrap();
    let first_receipt = first_commit.scoped.unwrap();
    let after_first = first.workspace.unwrap();
    assert_eq!(first_receipt.before, target);
    assert_eq!(first_commit.cursor, Some(ProjectFrame(3)));
    assert_eq!(after_first.plan.duration().frames(), 18);
    assert_eq!(counts(&path), (rows.0 + 1, rows.1 + 1));
    assert_compound(&path);
    let [other] = first_receipt.also.as_ref().unwrap().as_slice() else {
        panic!("one further selected Hold")
    };
    assert_eq!(inner_play(&after_first.document, &first_receipt.target), 0);
    assert_eq!(inner_play(&after_first.document, other), 1);
    assert_eq!(
        after_first.plan.picture(ProjectFrame(3)).unwrap().instance,
        first_receipt.presentation.clone().unwrap()
    );
    assert_eq!(
        after_first
            .plan
            .picture(ProjectFrame(14))
            .unwrap()
            .instance
            .node,
        other.node
    );

    let mut next = continued(&after_first, &first_receipt);
    next.cursor = first_commit.cursor.unwrap();
    next.also = first_receipt.also.clone().unwrap();
    next.validate(&after_first).unwrap();
    let second = command(&service, pause(next.clone(), true, false));
    assert!(second.error.is_none(), "{:?}", second.error);
    let second_commit = second.committed.unwrap();
    let second_receipt = second_commit.scoped.unwrap();
    let after_second = second.workspace.unwrap();
    assert_eq!(second_receipt.before, next);
    assert_eq!(second_commit.cursor, Some(ProjectFrame(3)));
    assert_eq!(after_second.plan.duration().frames(), 24);
    assert_eq!(counts(&path), (rows.0 + 2, rows.1 + 2));
    assert_compound(&path);
    assert_eq!(
        inner_play(&after_second.document, &second_receipt.target),
        0
    );
    assert_eq!(
        inner_play(
            &after_second.document,
            &second_receipt.also.as_ref().unwrap()[0]
        ),
        1
    );
    assert_eq!(
        after_second
            .plan
            .picture(ProjectFrame(17))
            .unwrap()
            .instance
            .node,
        second_receipt.also.as_ref().unwrap()[0].node
    );
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    assert_eq!(*reopened.document, *after_second.document);
    let undo_second = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: reopened.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_same_content(&undo_second.document, &after_first.document);
    let undo_first = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: undo_second.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_same_content(&undo_first.document, &before.document);
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    assert_same_content(&reopened.document, &before.document);
}
