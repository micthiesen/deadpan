//! `:splice` (Place slice) with its destination in one explicit Repeat play.

use super::*;
use crate::project::splice::{
    Destination, Occurrence, Operation as SpliceOperation, Proposal, ProposalId, Source,
};

fn copied(update: &ProjectUpdate) -> Arc<crate::project::slice::Captured> {
    match update.registers.as_ref().unwrap().entries.get(&'"') {
        Some(crate::project::registers::Value::Edited(captured)) => captured.clone(),
        other => panic!("an edited copy: {other:?}"),
    }
}

/// Copy x as it plays in play 2 (20..30), then return the register copy.
fn copy_x(service: &ProjectService, opened: &Workspace) -> Arc<crate::project::slice::Captured> {
    let update = command(
        service,
        ProjectRequest::Macro(apply(
            &target(opened, "x", &[("rep", 1)], 25),
            SemanticInstruction::Yank {
                selector: SemanticSelector::SelectedBeat,
                register: deadpan_core::RegisterName::unnamed(),
            },
        )),
    );
    saved(&update);
    copied(&update)
}

/// A Place slice proposal inside play 3 (40..60) of the beat `beat`.
fn proposal(
    workspace: &Workspace,
    copy: &Arc<crate::project::slice::Captured>,
    beat: &str,
    change: u64,
    destination: Destination,
) -> Proposal {
    let target = target(workspace, beat, &[("rep", 2)], 45);
    Proposal {
        id: ProposalId {
            session: workspace.session,
            project: workspace.document.project_id().clone(),
            base_revision: workspace.document.revision_id().clone(),
            draft: 1,
            change,
        },
        operation: SpliceOperation::Copy,
        source: Source::Edited {
            copied: copy.clone(),
            range: copy.slice().range(),
        },
        scope: SequenceScope::default(),
        parent: node("body"),
        destination,
        occurrence: Some(Occurrence {
            instance: target.presentation.clone().unwrap(),
            target,
            origin: ProjectFrame(40),
        }),
    }
}

fn prepared(update: &ProjectUpdate) -> Arc<crate::project::splice::Prepared> {
    assert!(update.error.is_none(), "{:?}", update.error);
    update
        .splice
        .as_ref()
        .expect("slice reply")
        .result
        .clone()
        .unwrap()
}

#[test]
fn place_slice_inside_a_beat_of_one_play_previews_commits_and_undoes_there_only() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened) = open(&scratch.path().join("place.deadpan"), false);
    let before = (*opened.document).clone();
    let copy = copy_x(&service, &opened);
    // Four frames inside y in play 3: y plays there over 50..60.
    let first = proposal(
        &opened,
        &copy,
        "x",
        1,
        Destination::Interior {
            target: node("y"),
            at: deadpan_core::FrameDuration::new(4).unwrap(),
        },
    );
    let ready = prepared(&command(
        &service,
        ProjectRequest::PrepareSplice(first.clone()),
    ));
    // Preparation authors nothing; the proposed document is the one play.
    assert_eq!(*ready.base.document, before);
    assert_eq!(ready.range.start(), ProjectFrame(54));
    assert_eq!(ready.range.end(), ProjectFrame(64));
    assert_eq!(ready.plan.duration().frames(), 80);
    let proposed = &ready.snapshot.document;
    assert_eq!(
        children(proposed, &node("body")),
        vec![node("x"), node("y")]
    );
    assert_eq!(proposed.overrides()[&node("rep")].len(), 1);
    let owned_group = owned(proposed, &node("rep"), &play("rep", 2));
    assert_eq!(children(proposed, &owned_group).len(), 4);
    assert_eq!(ready.parent, owned_group);
    let (target, presentation) = ready.occurrence.clone().unwrap();
    assert_eq!(target.node, ready.node);
    assert_eq!(presentation.node, ready.node);

    // A refinement replaces the draft; the old identity cannot commit.
    let refined = proposal(
        &opened,
        &copy,
        "x",
        2,
        Destination::Interior {
            target: node("y"),
            at: deadpan_core::FrameDuration::new(5).unwrap(),
        },
    );
    let ready = prepared(&command(
        &service,
        ProjectRequest::PrepareSplice(refined.clone()),
    ));
    assert_eq!(ready.range.start(), ProjectFrame(55));
    let stale = command(&service, ProjectRequest::CommitSplice(first.id));
    assert!(stale.splice_commit.unwrap().result.is_err());

    let committed = command(&service, ProjectRequest::CommitSplice(refined.id.clone()));
    let receipt = committed.splice_commit.clone().unwrap().result.unwrap();
    let document = committed.workspace.as_ref().unwrap().document.clone();
    assert_eq!(*document, *ready.snapshot.document.as_ref());
    assert_eq!(document.duration().unwrap().frames(), 80);
    // The inspector continues on the placed beat in play 3 at its start.
    let scoped = receipt.scoped.as_ref().unwrap();
    assert_eq!(scoped.target, target_after(&ready));
    assert_eq!(scoped.before, refined.occurrence.as_ref().unwrap().target);
    assert_eq!(receipt.cursor, Some(ProjectFrame(55)));
    assert!(receipt.preserve_cursor);
    let restored = undo(&service, &document);
    assert_eq!(restored.nodes(), before.nodes());
    assert!(restored.overrides().is_empty());
    assert_eq!(restored.duration().unwrap().frames(), 70);
}

fn target_after(ready: &crate::project::splice::Prepared) -> deadpan_core::ScopedNodeTarget {
    ready.occurrence.clone().unwrap().0
}

#[test]
fn place_slice_replaces_a_range_of_one_play_and_cancel_saves_nothing() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened) = open(&scratch.path().join("replace.deadpan"), false);
    let copy = copy_x(&service, &opened);
    // Replace 45..55 (across x and y in play 3) by the ten-frame copy.
    let replace = proposal(
        &opened,
        &copy,
        "x",
        1,
        Destination::Replace {
            range: deadpan_core::FrameRange::new(ProjectFrame(45), ProjectFrame(55)).unwrap(),
        },
    );
    let ready = prepared(&command(
        &service,
        ProjectRequest::PrepareSplice(replace.clone()),
    ));
    assert_eq!(
        ready.removed,
        Some(deadpan_core::FrameRange::new(ProjectFrame(45), ProjectFrame(55)).unwrap())
    );
    assert_eq!(ready.range.start(), ProjectFrame(45));
    assert_eq!(ready.plan.duration().frames(), 70);
    // Cancel revokes the draft without an edit.
    command(&service, ProjectRequest::AbandonSplice(replace.id.clone()));
    let refused = command(&service, ProjectRequest::CommitSplice(replace.id));
    assert!(refused.splice_commit.unwrap().result.is_err());
    // The revision is unchanged: later proposals on it still prepare.

    // Move and group objects refuse readably in a play.
    let mut moved = proposal(&opened, &copy, "x", 2, Destination::Slot(1));
    moved.operation = SpliceOperation::Move;
    let refused = command(&service, ProjectRequest::PrepareSplice(moved));
    let error = refused.splice.unwrap().result.err().unwrap();
    assert!(
        error.contains("Move cannot target one Repeat play"),
        "{error}"
    );

    // A seam commit in play 3 selects the placed beat there.
    let seam = proposal(&opened, &copy, "x", 3, Destination::Slot(1));
    let ready = prepared(&command(
        &service,
        ProjectRequest::PrepareSplice(seam.clone()),
    ));
    assert_eq!(ready.range.start(), ProjectFrame(50));
    let committed = command(&service, ProjectRequest::CommitSplice(seam.id));
    let receipt = committed.splice_commit.unwrap().result.unwrap();
    assert_eq!(receipt.cursor, Some(ProjectFrame(50)));
    let document = committed.workspace.unwrap().document.clone();
    assert_eq!(document.duration().unwrap().frames(), 80);
    assert_eq!(
        children(&document, &node("body")),
        vec![node("x"), node("y")]
    );
}
