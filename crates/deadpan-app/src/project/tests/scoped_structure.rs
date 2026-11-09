//! `s`, `dd`, `rr`, `:repeat N` and `:repeat gap=` on a beat in one explicit
//! Repeat play, through the semantic Apply the native keys and commands use.

use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;
use crate::project::macros::{Id, Operation};
use crate::project::scoped::Target;
use deadpan_core::{
    InstancePath, IterationId, PauseLength, RepeatEditBranch, RepeatEditStep, RepeatInstance,
    ScopedNodeTarget, SemanticContext, SemanticInstruction, SemanticScopedContext,
    SemanticSelector,
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

static REQUEST: AtomicU64 = AtomicU64::new(1);

/// The semantic context the native app captures for `target`.
fn scoped_context(target: &Target) -> SemanticContext {
    SemanticContext {
        original: None,
        hold_duration: None,
        shared_hold_duration: None,
        mixed_hold_duration: None,
        selected_hold_duration: None,
        scoped: target
            .presentation
            .clone()
            .filter(|_| target.has_single_play_authority())
            .map(|presentation| SemanticScopedContext {
                root: target.root.clone(),
                target: target.target.clone(),
                presentation,
            }),
        parent: node("root"),
        cursor: target.cursor,
        selected_child: Some(target.root.clone()),
        visual_selection: None,
    }
}

fn apply(target: &Target, instruction: SemanticInstruction) -> Operation {
    Operation::Apply {
        id: Id {
            session: target.session,
            project: target.project.clone(),
            revision: target.revision.clone(),
            // These projects never write a register.
            bank_version: 0,
            request: REQUEST.fetch_add(1, Ordering::Relaxed),
            transcript_guard: None,
        },
        instruction: Box::new(instruction),
        repeat_version: None,
        scope: target.scope.clone(),
        context: scoped_context(target),
    }
}

fn structure(
    service: &ProjectService,
    target: &Target,
    instruction: SemanticInstruction,
) -> ProjectUpdate {
    command(service, ProjectRequest::Macro(apply(target, instruction)))
}

/// The refusal of a one-play edit, or `None` when it saved.
fn refusal(update: &ProjectUpdate) -> Option<String> {
    update
        .error
        .clone()
        .or_else(|| update.macros.as_ref()?.result.as_ref().err().cloned())
}

fn saved(update: &ProjectUpdate) {
    assert_eq!(refusal(update), None);
    assert!(update.macros.as_ref().unwrap().result.is_ok());
}

fn count(plays: u32) -> NonZeroU32 {
    NonZeroU32::new(plays).unwrap()
}

fn wrap(plays: u32) -> SemanticInstruction {
    SemanticInstruction::Repeat {
        selector: SemanticSelector::SelectedBeat,
        plays: count(plays),
        escalation: None,
    }
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
        SemanticInstruction::DeleteBeat,
    );
    saved(&update);
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
        SemanticInstruction::Split,
    );
    saved(&update);
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
    let update = structure(&service, &next, wrap(3));
    saved(&update);
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
        SemanticInstruction::DeleteBeat,
    );
    saved(&update);
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
    let refused = structure(&service, &all, SemanticInstruction::DeleteBeat);
    assert!(
        refusal(&refused)
            .as_deref()
            .is_some_and(|error| error.contains("This play")),
        "{:?}",
        refusal(&refused)
    );
    let whole = structure(
        &service,
        &target(&opened, "body", &[("rep", 1)], 25),
        SemanticInstruction::DeleteBeat,
    );
    assert!(
        refusal(&whole)
            .as_deref()
            .is_some_and(|error| error.contains("whole contents")),
        "{:?}",
        refusal(&whole)
    );
    // Neither refusal saved a revision: the captured head still admits an edit.
    let update = structure(
        &service,
        &target(&opened, "y", &[("rep", 2)], 45),
        SemanticInstruction::DeleteBeat,
    );
    saved(&update);
}

/// root = [rep, z]; rep plays body = [xr, y] three times and xr plays x twice
/// (10 frames each), so each rep play is 30 frames.
fn open_counted(path: &Path) -> (ProjectService, Arc<Workspace>) {
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
        for (child, id, plays) in [("x", "xr", 2), ("body", "rep", 3)] {
            seed_command(
                &mut store,
                Command::WrapRepeat {
                    node: node(child),
                    id: node(id),
                    plays,
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

fn plays_of(document: &ProjectDocument, repeat: &NodeId) -> Vec<IterationId> {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[repeat].kind else {
        panic!("Repeat")
    };
    (0..iterations.len())
        .map(|index| iterations.at(index).unwrap())
        .collect()
}

#[test]
fn setting_plays_in_one_play_recounts_that_repeat_only_and_undoes() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened) = open_counted(&scratch.path().join("count.deadpan"));
    let before = (*opened.document).clone();
    assert_eq!(before.duration().unwrap().frames(), 100);
    let shared = plays_of(&before, &node("xr"));
    // Play 2 of rep runs from 30 to 60; xr fills its first 20 frames.
    let chosen = target(&opened, "xr", &[("rep", 1)], 35);
    let same = structure(
        &service,
        &chosen,
        SemanticInstruction::SetRepeatPlays { plays: count(2) },
    );
    assert!(
        refusal(&same)
            .as_deref()
            .is_some_and(|error| error.contains("already plays 2 times")),
        "{:?}",
        refusal(&same)
    );
    let update = structure(
        &service,
        &chosen,
        SemanticInstruction::SetRepeatPlays { plays: count(3) },
    );
    saved(&update);
    let document = &update.workspace.as_ref().unwrap().document;
    assert_eq!(document.duration().unwrap().frames(), 110);
    // Plays 1 and 3 keep the shared definition with two plays of x.
    assert_eq!(plays_of(document, &node("xr")), shared);
    assert_eq!(
        children(document, &node("body")),
        vec![node("xr"), node("y")]
    );
    let overrides = &document.overrides()[&node("rep")];
    assert_eq!(overrides.len(), 1);
    let copy = owned(document, &node("rep"), &play("rep", 1));
    let counted = children(document, &copy)[0].clone();
    let plays = plays_of(document, &counted);
    assert_eq!(plays.len(), 3);
    assert_eq!(plays[..2], shared[..]);
    // The inspector continues on the recounted Repeat at its start.
    let committed = update.committed.as_ref().unwrap();
    let receipt = committed.scoped.as_ref().unwrap();
    assert_eq!(receipt.target.node, counted);
    assert_eq!(committed.cursor, Some(ProjectFrame(30)));
    // The same play shrinks to one play of x.
    let mut next = target(&update.workspace.clone().unwrap(), "xr", &[("rep", 1)], 35);
    next.target = receipt.target.clone();
    next.presentation = receipt.presentation.clone();
    let shrunk = structure(
        &service,
        &next,
        SemanticInstruction::SetRepeatPlays { plays: count(1) },
    );
    saved(&shrunk);
    let shrunk_document = &shrunk.workspace.as_ref().unwrap().document;
    assert_eq!(shrunk_document.duration().unwrap().frames(), 90);
    assert_eq!(plays_of(shrunk_document, &counted), shared[..1]);
    assert_eq!(plays_of(shrunk_document, &node("xr")), shared);
    let restored = undo(&service, shrunk_document);
    assert_eq!(restored.nodes(), document.nodes());
    let restored = undo(&service, &restored);
    assert_eq!(restored.nodes(), before.nodes());
    assert!(restored.overrides().is_empty());
    assert_eq!(restored.duration().unwrap().frames(), 100);
}

#[test]
fn setting_gaps_in_one_play_changes_that_repeat_only_and_undoes() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened) = open_counted(&scratch.path().join("gaps.deadpan"));
    let before = (*opened.document).clone();
    let shared = plays_of(&before, &node("xr"));
    let chosen = target(&opened, "xr", &[("rep", 1)], 35);
    // `:repeat 3 gap=5f gap-step=-2f`: three plays of x with a 5f gap and
    // a 3f Hold after the second play, so xr grows from 20 to 38 frames.
    let gaps = || SemanticInstruction::SetRepeat {
        plays: Some(count(3)),
        gaps: Some(vec![
            PauseLength::Frames { frames: count(5) },
            PauseLength::Frames { frames: count(3) },
        ]),
        escalation: None,
        speed: None,
    };
    let update = structure(&service, &chosen, gaps());
    saved(&update);
    let document = &update.workspace.as_ref().unwrap().document;
    assert_eq!(document.duration().unwrap().frames(), 118);
    // Plays 1 and 3 keep the shared, gapless two-play definition.
    assert_eq!(plays_of(document, &node("xr")), shared);
    let NodeKind::Repeat { gap, .. } = &document.nodes()[&node("xr")].kind else {
        panic!("Repeat")
    };
    assert!(gap.is_none());
    assert!(document.gap_overrides().get(&node("xr")).is_none());
    let copy = owned(document, &node("rep"), &play("rep", 1));
    let changed = children(document, &copy)[0].clone();
    let NodeKind::Repeat { gap, .. } = &document.nodes()[&changed].kind else {
        panic!("Repeat")
    };
    assert_eq!(
        gap.as_ref().unwrap().duration,
        FrameDuration::new(5).unwrap()
    );
    let plays = plays_of(document, &changed);
    assert_eq!(plays.len(), 3);
    assert_eq!(plays[..2], shared[..]);
    // The later gap is a fresh independent Hold after the second play.
    let branches = &document.gap_overrides()[&changed];
    assert_eq!(branches.len(), 1);
    let (after, hold) = branches.iter().next().unwrap();
    assert_eq!(after, &shared[1]);
    assert!(!before.nodes().contains_key(hold));
    assert!(matches!(
        &document.nodes()[hold].kind,
        NodeKind::Hold { recipe } if recipe.duration == FrameDuration::new(3).unwrap()
    ));
    let committed = update.committed.as_ref().unwrap();
    let receipt = committed.scoped.as_ref().unwrap();
    assert_eq!(receipt.target.node, changed);
    assert_eq!(committed.cursor, Some(ProjectFrame(30)));
    // Asking again for the same plays and gaps saves nothing.
    let mut next = target(&update.workspace.clone().unwrap(), "xr", &[("rep", 1)], 35);
    next.target = receipt.target.clone();
    next.presentation = receipt.presentation.clone();
    let same = structure(&service, &next, gaps());
    assert!(
        refusal(&same)
            .as_deref()
            .is_some_and(|error| error.contains("already has these plays and gaps")),
        "{:?}",
        refusal(&same)
    );
    // A plain beat has no gaps to set in this play.
    let plain = structure(
        &service,
        &target(&update.workspace.clone().unwrap(), "y", &[("rep", 0)], 25),
        gaps(),
    );
    assert!(
        refusal(&plain)
            .as_deref()
            .is_some_and(|error| error.contains("wrap a beat first")),
        "{:?}",
        refusal(&plain)
    );
    let restored = undo(&service, document);
    assert_eq!(restored.nodes(), before.nodes());
    assert!(restored.overrides().is_empty());
    assert_eq!(restored.duration().unwrap().frames(), 100);
}

/// Each play's own contents as child durations, with the shared definition
/// for plays that have none. Fresh node names differ between runs; the
/// structure and timing must not.
fn shape(document: &ProjectDocument) -> Vec<Vec<i64>> {
    let durations = document.durations().unwrap();
    plays_of(document, &node("rep"))
        .iter()
        .map(|play| {
            let body = document
                .overrides()
                .get(&node("rep"))
                .and_then(|plays| plays.get(play))
                .cloned()
                .unwrap_or_else(|| node("body"));
            children(document, &body)
                .iter()
                .map(|child| durations[child].frames())
                .collect()
        })
        .collect()
}

fn continued(update: &ProjectUpdate, before: &Target) -> Target {
    let receipt = update.committed.as_ref().unwrap().scoped.as_ref().unwrap();
    let workspace = update.workspace.as_ref().unwrap();
    Target {
        revision: workspace.document.revision_id().clone(),
        target: receipt.target.clone(),
        presentation: receipt.presentation.clone(),
        cursor: update.committed.as_ref().unwrap().cursor.unwrap(),
        ..before.clone()
    }
}

#[test]
fn recorded_split_and_delete_replay_in_another_play_as_the_native_edits_did() {
    let scratch = tempfile::tempdir().unwrap();
    // The native keys: `s` then `dd` inside play 2.
    let (native, opened) = open(&scratch.path().join("native.deadpan"), false);
    let first = target(&opened, "x", &[("rep", 1)], 25);
    let split = structure(&native, &first, SemanticInstruction::Split);
    saved(&split);
    let deleted = structure(
        &native,
        &continued(&split, &first),
        SemanticInstruction::DeleteBeat,
    );
    saved(&deleted);
    let native_document = &deleted.workspace.as_ref().unwrap().document;
    assert_eq!(
        shape(native_document),
        vec![vec![10, 10], vec![5, 10], vec![10, 10]]
    );
    // Recording them saves only intent, which a run resolves in play 2 to the
    // same document and, in play 3, to the same edit there.
    let (service, opened) = open(&scratch.path().join("recorded.deadpan"), false);
    let program = Arc::new(
        deadpan_core::SemanticProgram::new(vec![
            SemanticInstruction::Split,
            SemanticInstruction::DeleteBeat,
        ])
        .unwrap(),
    );
    let save = command(
        &service,
        ProjectRequest::Macro(Operation::Save {
            id: Id {
                session: opened.session,
                project: opened.document.project_id().clone(),
                revision: opened.document.revision_id().clone(),
                bank_version: 0,
                request: REQUEST.fetch_add(1, Ordering::Relaxed),
                transcript_guard: None,
            },
            register: 'q',
            program,
        }),
    );
    saved(&save);
    let bank = save.registers.as_ref().unwrap().version;
    let run = |update: &ProjectUpdate, target: &Target| {
        command(
            &service,
            ProjectRequest::Macro(Operation::Run {
                id: Id {
                    session: target.session,
                    project: target.project.clone(),
                    revision: update
                        .workspace
                        .as_ref()
                        .unwrap()
                        .document
                        .revision_id()
                        .clone(),
                    bank_version: bank,
                    request: REQUEST.fetch_add(1, Ordering::Relaxed),
                    transcript_guard: None,
                },
                register: 'q',
                count: 1,
                scope: target.scope.clone(),
                context: scoped_context(target),
            }),
        )
    };
    let workspace = save.workspace.as_ref().unwrap();
    let replayed = run(&save, &target(workspace, "x", &[("rep", 1)], 25));
    saved(&replayed);
    let replayed_document = &replayed.workspace.as_ref().unwrap().document;
    assert_eq!(shape(replayed_document), shape(native_document));
    assert_eq!(
        replayed_document.nodes().len(),
        native_document.nodes().len()
    );
    // The inspector continues as natively, on y's copy at the deleted start.
    let receipt = replayed.committed.as_ref().unwrap();
    assert_eq!(receipt.cursor, deleted.committed.as_ref().unwrap().cursor);
    assert_eq!(receipt.cursor, Some(ProjectFrame(25)));
    // Play 3 starts at 35 after play 2 closed up by five frames.
    let workspace = replayed.workspace.as_ref().unwrap();
    let again = run(&replayed, &target(workspace, "x", &[("rep", 2)], 40));
    saved(&again);
    assert_eq!(
        shape(&again.workspace.as_ref().unwrap().document),
        vec![vec![10, 10], vec![5, 10], vec![5, 10]]
    );
    // Without an explicit play the program refuses and saves nothing.
    let workspace = again.workspace.as_ref().unwrap();
    let mut shared = target(workspace, "x", &[("rep", 0)], 5);
    shared.target.repeats[0].branch = RepeatEditBranch::Default;
    let refused = run(&again, &shared);
    assert!(
        refusal(&refused)
            .as_deref()
            .is_some_and(|error| error.contains("This play")),
        "{:?}",
        refusal(&refused)
    );
    assert_eq!(
        refused.workspace.as_ref().unwrap().document,
        again.workspace.as_ref().unwrap().document
    );
}

#[test]
fn dot_repeats_a_one_play_edit_in_the_play_chosen_now() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened) = open(&scratch.path().join("dot.deadpan"), false);
    let wrapped = structure(&service, &target(&opened, "y", &[("rep", 1)], 35), wrap(2));
    saved(&wrapped);
    let semantic = wrapped.semantic.as_ref().unwrap();
    assert_eq!(
        semantic.edit.as_ref().unwrap().operation,
        crate::project::semantic::RepeatableEdit::Repeat {
            selector: SemanticSelector::SelectedBeat,
            plays: count(2),
            escalation: None,
        }
    );
    // Dot in play 3 (now 40..60 after play 2 grew by 10) wraps y there too.
    let workspace = wrapped.workspace.as_ref().unwrap();
    let mut operation = apply(&target(workspace, "y", &[("rep", 2)], 55), wrap(2));
    let Operation::Apply { repeat_version, .. } = &mut operation else {
        unreachable!()
    };
    *repeat_version = Some(semantic.version);
    let dotted = command(&service, ProjectRequest::Macro(operation));
    saved(&dotted);
    let document = &dotted.workspace.as_ref().unwrap().document;
    assert_eq!(document.duration().unwrap().frames(), 90);
    let durations = document.durations().unwrap();
    for ordinal in [1, 2] {
        let copy = owned(document, &node("rep"), &play("rep", ordinal));
        let wrapper = children(document, &copy)[1].clone();
        assert!(matches!(
            document.nodes()[&wrapper].kind,
            NodeKind::Repeat { .. }
        ));
        assert_eq!(durations[&wrapper].frames(), 20);
    }
    assert!(
        document
            .overrides()
            .get(&node("rep"))
            .unwrap()
            .get(&play("rep", 0))
            .is_none()
    );
}

/// root = [rep, z]; rep plays body = [x, y] three times with a shared 4-frame
/// gap: 20 + 4 + 20 + 4 + 20, then z.
fn open_gapped(path: &Path) -> (ProjectService, Arc<Workspace>) {
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
        seed_command(
            &mut store,
            Command::WrapRepeat {
                node: node("body"),
                id: node("rep"),
                plays: 3,
                gap: Some(hold(4)),
                anchor_policy: Default::default(),
            },
            "rep-plays",
        );
    }
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let workspace = command(&service, ProjectRequest::Open(path.to_path_buf()))
        .workspace
        .unwrap();
    (service, workspace)
}

#[test]
fn a_shared_gap_length_in_one_play_becomes_its_own_pause_in_one_undo() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened) = open_gapped(&scratch.path().join("play-gap.deadpan"));
    let before = (*opened.document).clone();
    assert_eq!(before.duration().unwrap().frames(), 78);
    // The shared gap row addresses play 2's contents, which start at 24.
    let update = structure(
        &service,
        &target(&opened, "body", &[("rep", 1)], 24),
        SemanticInstruction::SetPlayGap {
            change: deadpan_core::PlayGapChange::Length {
                length: PauseLength::Frames { frames: count(10) },
            },
        },
    );
    saved(&update);
    let document = &update.workspace.as_ref().unwrap().document;
    assert_eq!(document.duration().unwrap().frames(), 84);
    let gap = document.gap_overrides()[&node("rep")]
        .get(&play("rep", 1))
        .unwrap()
        .clone();
    let NodeKind::Hold { recipe } = &document.nodes()[&gap].kind else {
        panic!("this play's gap is its own pause");
    };
    assert_eq!(recipe.duration.frames(), 10);
    // Plays 1 and 3 keep the shared 4-frame gap and the shared body.
    assert_eq!(document.gap_overrides()[&node("rep")].len(), 1);
    assert_eq!(document.nodes()[&node("rep")], before.nodes()[&node("rep")]);
    assert!(document.overrides().is_empty());
    // The inspector continues on the new gap row at its first frame.
    let committed = update.committed.as_ref().unwrap();
    let receipt = committed.scoped.as_ref().unwrap();
    assert_eq!(receipt.target.node, gap);
    assert_eq!(
        receipt.target.repeats,
        vec![RepeatEditStep {
            repeat: node("rep"),
            branch: RepeatEditBranch::Play {
                iteration: play("rep", 1)
            },
        }]
    );
    assert_eq!(committed.cursor, Some(ProjectFrame(44)));
    // One Undo restores the shared gap exactly.
    let restored = undo(&service, document);
    assert_eq!(restored.nodes(), before.nodes());
    assert_eq!(restored.gap_overrides(), before.gap_overrides());
    assert_eq!(restored.audio_bindings(), before.audio_bindings());
    assert_eq!(restored.duration().unwrap().frames(), 78);
}

#[test]
fn shared_gap_edits_refuse_the_last_play_and_unchanged_values_without_saving() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened) = open_gapped(&scratch.path().join("play-gap-refusals.deadpan"));
    let last = structure(
        &service,
        &target(&opened, "body", &[("rep", 2)], 48),
        SemanticInstruction::SetPlayGap {
            change: deadpan_core::PlayGapChange::Length {
                length: PauseLength::Frames { frames: count(10) },
            },
        },
    );
    assert!(refusal(&last).unwrap().contains("last play has no gap"));
    let silent = structure(
        &service,
        &target(&opened, "body", &[("rep", 0)], 0),
        SemanticInstruction::SetPlayGap {
            change: deadpan_core::PlayGapChange::Audio {
                audio: deadpan_core::HoldAudio::Silence,
            },
        },
    );
    assert!(refusal(&silent).unwrap().contains("already has that sound"));
    assert!(last.committed.is_none() && silent.committed.is_none());
}
