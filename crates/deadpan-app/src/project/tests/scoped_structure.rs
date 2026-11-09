//! `s`, `dd` and `rr` on a beat in one explicit Repeat play.

use super::*;
use crate::project::ScopedStructure;
use crate::project::scoped::Target;
use deadpan_core::{
    InstancePath, IterationId, RepeatEditBranch, RepeatEditStep, RepeatInstance, ScopedNodeTarget,
};

/// root = [rep, z]; rep plays body = [x, y] (10 frames each) three times.
/// With `nested`, root = [outer, z]; outer plays inner twice, inner plays body
/// twice.
fn open(path: &Path, nested: bool) -> (ProjectService, Arc<Workspace>) {
    {
        let mut store = seed_holds(path, &["x", "y", "z"]);
        seed_command(
            &mut store,
            Command::Group {
                parent: node("root"),
                start: 0,
                end: 2,
                id: node("body"),
                label: "Body".into(),
            },
            "group",
        );
        let wraps: &[(&str, &str, u32)] = if nested {
            &[("body", "inner", 2), ("inner", "outer", 2)]
        } else {
            &[("body", "rep", 3)]
        };
        for (child, id, plays) in wraps {
            seed_command(
                &mut store,
                Command::WrapRepeat {
                    node: node(child),
                    id: node(id),
                    plays: *plays,
                    gap: None,
                    anchor_policy: Default::default(),
                },
                &format!("{id}-plays"),
            );
        }
    }
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let workspace = command(&service, ProjectRequest::Open(path.to_path_buf()))
        .workspace
        .unwrap();
    (service, workspace)
}

fn play(repeat: &str, ordinal: u32) -> IterationId {
    IterationId {
        allocation: RevisionId::new(format!("{repeat}-plays")).unwrap(),
        ordinal,
    }
}

/// `node` reached through the named plays, outermost first.
fn target(workspace: &Workspace, node_name: &str, plays: &[(&str, u32)], cursor: i64) -> Target {
    Target {
        session: workspace.session,
        navigation: 0,
        project: workspace.document.project_id().clone(),
        revision: workspace.document.revision_id().clone(),
        scope: SequenceScope::default(),
        root: node(plays[0].0),
        target: ScopedNodeTarget {
            node: node(node_name),
            repeats: plays
                .iter()
                .map(|(repeat, ordinal)| RepeatEditStep {
                    repeat: node(repeat),
                    branch: RepeatEditBranch::Play {
                        iteration: play(repeat, *ordinal),
                    },
                })
                .collect(),
        },
        presentation: Some(InstancePath {
            node: node(node_name),
            repeats: plays
                .iter()
                .map(|(repeat, ordinal)| RepeatInstance {
                    node: node(repeat),
                    iteration: play(repeat, *ordinal),
                })
                .collect(),
        }),
        cursor: ProjectFrame(cursor),
        also: Vec::new(),
    }
}

fn structure(service: &ProjectService, target: &Target, edit: ScopedStructure) -> ProjectUpdate {
    command(
        service,
        ProjectRequest::Edit {
            expected_session: target.session,
            expected_revision: target.revision.clone(),
            scope: target.scope.clone(),
            cursor: target.cursor,
            edit: ProjectEdit::ScopedStructure {
                target: Box::new(target.clone()),
                edit,
            },
        },
    )
}

fn children(document: &ProjectDocument, node: &NodeId) -> Vec<NodeId> {
    document.children(node).cloned().collect()
}

fn owned(document: &ProjectDocument, repeat: &NodeId, iteration: &IterationId) -> NodeId {
    document.overrides()[repeat].get(iteration).unwrap().clone()
}

fn undo(service: &ProjectService, document: &ProjectDocument) -> ProjectDocument {
    let update = command(
        service,
        ProjectRequest::Undo {
            expected_revision: document.revision_id().clone(),
        },
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    (*update.workspace.unwrap().document).clone()
}

#[test]
fn deleting_a_beat_in_one_play_closes_up_that_play_only_and_undoes() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened) = open(&scratch.path().join("delete.deadpan"), false);
    let before = (*opened.document).clone();
    // Play 2 runs from 20 to 40.
    let update = structure(
        &service,
        &target(&opened, "x", &[("rep", 1)], 25),
        ScopedStructure::Delete,
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    let document = &update.workspace.as_ref().unwrap().document;
    assert_eq!(document.duration().unwrap().frames(), 60);
    assert_eq!(
        children(document, &node("body")),
        vec![node("x"), node("y")]
    );
    let overrides = &document.overrides()[&node("rep")];
    assert_eq!(overrides.len(), 1);
    let copy = owned(document, &node("rep"), &play("rep", 1));
    let remaining = children(document, &copy);
    assert_eq!(remaining.len(), 1);
    assert_ne!(remaining[0], node("y"));
    // The inspector continues on y's copy in the same play, at the old start.
    let committed = update.committed.as_ref().unwrap();
    let receipt = committed.scoped.as_ref().unwrap();
    assert_eq!(receipt.target.node, remaining[0]);
    assert_eq!(
        receipt.target.repeats[0].branch,
        RepeatEditBranch::Play {
            iteration: play("rep", 1)
        }
    );
    assert_eq!(committed.cursor, Some(ProjectFrame(20)));
    let restored = undo(&service, document);
    assert_eq!(restored.nodes(), before.nodes());
    assert!(restored.overrides().is_empty());
    assert_eq!(restored.duration().unwrap().frames(), 70);
}

#[test]
fn splitting_and_repeating_in_one_play_leave_the_shared_definition_intact() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened) = open(&scratch.path().join("split.deadpan"), false);
    let update = structure(
        &service,
        &target(&opened, "x", &[("rep", 1)], 25),
        ScopedStructure::Split,
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    let split = update.workspace.unwrap();
    let document = &split.document;
    assert_eq!(document.duration().unwrap().frames(), 70);
    assert_eq!(
        children(document, &node("body")),
        vec![node("x"), node("y")]
    );
    let copy = owned(document, &node("rep"), &play("rep", 1));
    let pieces = children(document, &copy);
    assert_eq!(pieces.len(), 3);
    let receipt = update.committed.unwrap().scoped.unwrap();
    assert_eq!(receipt.target.node, pieces[1]);
    assert_eq!(document.durations().unwrap()[&pieces[1]].frames(), 5);

    // Repeat the right fragment three times in that same play.
    let mut next = target(&split, "x", &[("rep", 1)], 25);
    next.target = receipt.target.clone();
    next.presentation = receipt.presentation.clone();
    let update = structure(&service, &next, ScopedStructure::Repeat { plays: 3 });
    assert!(update.error.is_none(), "{:?}", update.error);
    let document = &update.workspace.as_ref().unwrap().document;
    assert_eq!(document.duration().unwrap().frames(), 80);
    let pieces = children(document, &copy);
    let receipt = update.committed.as_ref().unwrap().scoped.as_ref().unwrap();
    assert_eq!(receipt.target.node, pieces[1]);
    assert!(matches!(
        document.nodes()[&pieces[1]].kind,
        NodeKind::Repeat { .. }
    ));
    assert_eq!(
        children(document, &node("body")),
        vec![node("x"), node("y")]
    );
    assert_eq!(document.overrides()[&node("rep")].len(), 1);
    let restored = undo(&service, document);
    assert_eq!(restored.duration().unwrap().frames(), 70);
    assert_eq!(restored.nodes(), split.document.nodes());
}

#[test]
fn nested_deletion_copies_only_the_chosen_outer_and_inner_plays() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened) = open(&scratch.path().join("nested.deadpan"), true);
    // outer play 2 starts at 40; its inner play 2 runs from 60 to 80.
    let update = structure(
        &service,
        &target(&opened, "x", &[("outer", 1), ("inner", 1)], 65),
        ScopedStructure::Delete,
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    let document = &update.workspace.as_ref().unwrap().document;
    assert_eq!(document.duration().unwrap().frames(), 80);
    // The shared inner Repeat and its body are untouched.
    assert!(document.overrides().get(&node("inner")).is_none());
    assert_eq!(
        children(document, &node("body")),
        vec![node("x"), node("y")]
    );
    let inner_copy = owned(document, &node("outer"), &play("outer", 1));
    let body_copy = owned(document, &inner_copy, &play("inner", 1));
    assert_eq!(children(document, &body_copy).len(), 1);
    let receipt = update.committed.as_ref().unwrap().scoped.as_ref().unwrap();
    assert_eq!(receipt.target.repeats[1].repeat, inner_copy);
    assert_eq!(receipt.target.node, children(document, &body_copy)[0]);
}

#[test]
fn shared_choices_and_whole_play_contents_refuse_without_editing() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened) = open(&scratch.path().join("refuse.deadpan"), false);
    let mut all = target(&opened, "x", &[("rep", 1)], 25);
    all.target.repeats[0].branch = RepeatEditBranch::Default;
    all.presentation = None;
    let refused = structure(&service, &all, ScopedStructure::Delete);
    assert!(
        refused
            .error
            .as_deref()
            .is_some_and(|error| error.contains("This play")),
        "{:?}",
        refused.error
    );
    let whole = structure(
        &service,
        &target(&opened, "body", &[("rep", 1)], 25),
        ScopedStructure::Delete,
    );
    assert!(
        whole
            .error
            .as_deref()
            .is_some_and(|error| error.contains("whole contents")),
        "{:?}",
        whole.error
    );
    // Neither refusal saved a revision: the captured head still admits an edit.
    let update = structure(
        &service,
        &target(&opened, "y", &[("rep", 2)], 45),
        ScopedStructure::Delete,
    );
    assert!(update.error.is_none(), "{:?}", update.error);
}
