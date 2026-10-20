//! Sequence-relative cuts retain actual Hold owners across differing layouts.
use super::*;
use deadpan_core::{IterationOrder, PlayOverride, PlayOverrides};
use std::collections::BTreeMap;

fn setup() -> (tempfile::TempDir, ProjectService, Arc<Workspace>) {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("sequence-batch.deadpan");
    let document = ProjectDocument::new_automatic(
        ProjectId::new("sequence-batch").unwrap(),
        RevisionId::new("initial").unwrap(),
        node("root"),
    )
    .unwrap();
    let mut store = ProjectStore::create(&path, &document).unwrap();
    let mut repeated = BeatNode::sequence("repeat", Vec::new());
    repeated.kind = NodeKind::Repeat {
        child: node("body"),
        iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 2).unwrap(),
        gap: None,
        escalation: None,
    };
    seed_command(
        &mut store,
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("repeat"),
                nodes: BTreeMap::from([
                    (node("repeat"), repeated),
                    (
                        node("body"),
                        BeatNode::sequence("body", vec![node("a"), node("b")]),
                    ),
                    (node("a"), BeatNode::hold("a", hold(4))),
                    (node("b"), BeatNode::hold("b", hold(4))),
                    (
                        node("owned-body"),
                        BeatNode::sequence("owned-body", vec![node("owned-a"), node("owned-b")]),
                    ),
                    (node("owned-a"), BeatNode::hold("owned-a", hold(2))),
                    (node("owned-b"), BeatNode::hold("owned-b", hold(6))),
                ]),
                overrides: BTreeMap::from([(
                    node("repeat"),
                    PlayOverrides::try_from(vec![PlayOverride {
                        iteration: iteration("plays", 1),
                        root: node("owned-body"),
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
    assert_eq!(workspace.plan.duration().frames(), 16);
    (scratch, service, workspace)
}

fn capture(workspace: &Workspace) -> Target {
    let NodeKind::Repeat { iterations, .. } = &workspace.document.nodes()[&node("repeat")].kind
    else {
        panic!("fixture Repeat")
    };
    let first = iterations.at(0).unwrap();
    let second = iterations.at(1).unwrap();
    let primary = ScopedNodeTarget {
        node: node("body"),
        repeats: vec![RepeatEditStep {
            repeat: node("repeat"),
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
        root: node("repeat"),
        target: primary,
        presentation: Some(InstancePath {
            node: node("body"),
            repeats: vec![RepeatInstance {
                node: node("repeat"),
                iteration: first,
            }],
        }),
        cursor: ProjectFrame(3),
        also: vec![other],
    }
}

fn slot(document: &ProjectDocument, child: &NodeId) -> usize {
    document
        .nodes()
        .keys()
        .find_map(|parent| document.children(parent).position(|entry| entry == child))
        .expect("retained Hold has a Sequence parent")
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
        panic!("selected plays must persist as one Compound")
    };
    assert_eq!(transaction.steps().len(), 2);
    assert!(transaction.steps().iter().all(|step| matches!(
        step.edit().unwrap().command.as_command(),
        Command::InsertOccurrenceTime { .. }
    )));
}

#[test]
fn native_sequence_pause_batch_keeps_different_child_slots_and_repeats_as_one_undo() {
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
    assert_eq!(after_first.plan.duration().frames(), 22);
    assert_eq!(counts(&path), (rows.0 + 1, rows.1 + 1));
    assert_compound(&path);
    let primary = &first_receipt.target;
    let [other] = first_receipt.also.as_ref().unwrap().as_slice() else {
        panic!("one further selected Hold")
    };
    assert_eq!(slot(&after_first.document, &primary.node), 1);
    assert_eq!(slot(&after_first.document, &other.node), 2);
    let RepeatEditBranch::Play {
        iteration: selected,
    } = &other.repeats[0].branch
    else {
        panic!("retained selected play")
    };
    assert_ne!(
        crate::project::scoped::retarget(&after_first.document, primary, 0, selected,).ok(),
        Some(other.clone()),
        "corresponding child slots do not identify the actual inserted Holds"
    );
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
    assert_eq!(after_second.plan.duration().frames(), 28);
    assert_eq!(counts(&path), (rows.0 + 2, rows.1 + 2));
    assert_compound(&path);
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

#[test]
fn native_sequence_pause_batch_value_receipts_keep_the_actual_hold_set() {
    let (_scratch, service, before) = setup();
    let paused = command(&service, pause(capture(&before), true, false));
    assert!(paused.error.is_none(), "{:?}", paused.error);
    let committed = paused.committed.unwrap();
    let receipt = committed.scoped.unwrap();
    let mut workspace = paused.workspace.unwrap();
    let holds = std::iter::once(receipt.target.clone())
        .chain(receipt.also.clone().unwrap())
        .collect::<Vec<_>>();
    let mut target = continued(&workspace, &receipt);
    target.cursor = committed.cursor.unwrap();
    target.also = receipt.also.unwrap();
    for trim in [-3_000, -6_000] {
        let updated = command(
            &service,
            request(
                &target,
                ScopedNodeEdit::SetAudioTreatments {
                    treatments: gain(trim),
                },
            ),
        );
        assert!(updated.error.is_none(), "{:?}", updated.error);
        let receipt = updated.committed.unwrap().scoped.unwrap();
        workspace = updated.workspace.unwrap();
        assert_eq!(receipt.target, holds[0]);
        assert_eq!(receipt.also.as_ref().unwrap(), &holds[1..]);
        for hold in &holds {
            assert_eq!(
                workspace.document.nodes()[&hold.node].audio_treatments,
                gain(trim)
            );
        }
        assert_eq!(
            workspace.document.nodes()[&node("owned-b")].audio_treatments,
            AudioTreatments::default()
        );
        target = continued(&workspace, &receipt);
        target.also = receipt.also.unwrap();
    }
}
