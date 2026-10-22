//! Actual selected-duration service commits, including a zero-net resize.
use super::*;
use crate::project::macros::{Id, Operation};
use deadpan_core::{PauseLength, SemanticContext, SemanticInstruction as I, SemanticProgram};
use std::num::NonZeroU32;

fn length(frames: u32) -> I {
    I::SetHoldDuration {
        length: PauseLength::Frames {
            frames: NonZeroU32::new(frames).unwrap(),
        },
    }
}

fn id(update: &ProjectUpdate, request: u64) -> Id {
    let workspace = update.workspace.as_ref().unwrap();
    Id {
        session: workspace.session,
        project: workspace.document.project_id().clone(),
        revision: workspace.document.revision_id().clone(),
        bank_version: update.registers.as_ref().unwrap().version,
        request,
    }
}

fn rows(path: &std::path::Path) -> (i64, i64, i64) {
    rusqlite::Connection::open(path.join("project.sqlite"))
        .unwrap()
        .query_row(
            "SELECT (SELECT count(*) FROM revisions),(SELECT count(*) FROM history),(SELECT count(*) FROM transaction_steps)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap()
}

pub(super) fn exercise(service: &ProjectService, opened: &ProjectUpdate) {
    let before = opened.workspace.as_ref().unwrap();
    let path = before.path.clone();
    let mut state = State::new(
        before,
        SequenceScope::default(),
        node("shared-outer"),
        ProjectFrame(19),
    )
    .unwrap();
    assert!(state.enter(before).unwrap());
    state.switch_plays(before, &[1, 2]).unwrap();
    assert!(state.enter(before).unwrap());
    state.select(before, 1).unwrap();
    let target = state.target(before, ProjectFrame(19)).unwrap();
    let context = SemanticContext {
        parent: before.document.root().clone(),
        cursor: target.cursor,
        selected_child: Some(target.root.clone()),
        original: None,
        visual_selection: None,
        scoped: None,
        hold_duration: None,
        shared_hold_duration: None,
        mixed_hold_duration: None,
        selected_hold_duration: Some(Box::new(
            target.semantic_selected_hold_duration(before).unwrap(),
        )),
    };
    let initial_rows = rows(&path);
    let bank = || {
        ProjectStore::open(&path, AccessMode::ReadOnly)
            .unwrap()
            .registers()
            .unwrap()
    };
    let initial_bank = bank();
    // Both native constructor and untrusted semantic context must refuse a
    // duplicate last member before even the first valid resize can be saved.
    let mut duplicate = context.clone();
    let authority = duplicate.selected_hold_duration.as_mut().unwrap();
    authority.also.push(authority.also[0].clone());
    let refused = command(
        service,
        ProjectRequest::Macro(Operation::Apply {
            id: id(opened, 211),
            instruction: length(3),
            repeat_version: None,
            scope: target.scope.clone(),
            context: duplicate,
        }),
    );
    assert!(refused.macros.as_ref().unwrap().result.is_err());
    assert_eq!(rows(&path), initial_rows);
    assert_eq!(
        *refused.workspace.as_ref().unwrap().document,
        *before.document
    );
    assert_eq!(bank(), initial_bank);

    let changed = command(
        service,
        ProjectRequest::Macro(Operation::Apply {
            id: id(opened, 212),
            instruction: length(3),
            repeat_version: None,
            scope: target.scope.clone(),
            context: context.clone(),
        }),
    );
    let receipt = changed.macros.as_ref().unwrap().result.as_ref().unwrap();
    let commit = receipt.committed().unwrap();
    let scoped = commit.scoped.as_ref().unwrap();
    let after = changed.workspace.as_ref().unwrap();
    assert_eq!(
        rows(&path),
        (initial_rows.0 + 1, initial_rows.1 + 1, initial_rows.2 + 2)
    );
    assert_eq!(
        after.plan.duration().frames(),
        45,
        "zero net is a real edit"
    );
    assert_ne!(*after.document, *before.document);
    assert_eq!(
        after.document.nodes().len(),
        before.document.nodes().len() + 4
    );
    assert_eq!(
        commit.cursor,
        Some(ProjectFrame(19)),
        "resolve after BOTH leaves"
    );
    assert_eq!(commit.selected_node.as_ref(), Some(&target.root));
    assert_eq!(scoped.before, target);
    assert!(state.reconcile(after, scoped, commit.cursor).unwrap());
    let mapped = state.target(after, ProjectFrame(19)).unwrap();
    assert_eq!(mapped.also.len(), 1);
    assert_ne!(mapped.target.node, target.target.node);
    assert_eq!(mapped.also[0].node, target.also[0].node);
    mapped.semantic_selected_hold_duration(after).unwrap();
    assert!(mapped.semantic_mixed_hold_duration(after).is_err());
    assert!(mapped.semantic_framing(after).is_err());
    for selected in std::iter::once(&mapped.target).chain(&mapped.also) {
        assert!(matches!(&after.document.nodes()[&selected.node].kind,
            NodeKind::Hold { recipe } if recipe.duration == frames(3)));
    }
    assert!(
        matches!(&after.document.nodes()[&node("shared-pause")].kind,
        NodeKind::Hold { recipe } if recipe.duration == frames(2))
    );
    assert_eq!(
        state.hold_parameter(after).as_deref(),
        Some("hold-duration 3f")
    );
    assert_eq!(bank(), initial_bank);
    assert!(changed.semantic.as_ref().unwrap().edit.is_some());

    let mut current = context.clone();
    current.selected_hold_duration = Some(Box::new(
        mapped.semantic_selected_hold_duration(after).unwrap(),
    ));
    current.cursor = ProjectFrame(19);
    let after_rows = rows(&path);
    let no_op = command(
        service,
        ProjectRequest::Macro(Operation::Apply {
            id: id(&changed, 213),
            instruction: length(3),
            repeat_version: None,
            scope: target.scope.clone(),
            context: current.clone(),
        }),
    );
    assert!(no_op.macros.as_ref().unwrap().result.is_err());
    assert_eq!(rows(&path), after_rows);
    assert_eq!(*no_op.workspace.as_ref().unwrap().document, *after.document);
    let stale = command(
        service,
        ProjectRequest::Macro(Operation::Apply {
            id: id(opened, 214),
            instruction: length(4),
            repeat_version: None,
            scope: target.scope.clone(),
            context,
        }),
    );
    assert!(stale.macros.as_ref().unwrap().result.is_err());
    assert_eq!(rows(&path), after_rows);

    let undone = command(
        service,
        ProjectRequest::Undo {
            expected_revision: after.document.revision_id().clone(),
        },
    );
    let restored = undone.workspace.as_ref().unwrap();
    assert_eq!(restored.document.nodes(), before.document.nodes());
    assert_eq!(restored.document.overrides(), before.document.overrides());
    assert_eq!(
        restored.document.audio_bindings(),
        before.document.audio_bindings()
    );
    let redone = command(
        service,
        ProjectRequest::Redo {
            expected_revision: restored.document.revision_id().clone(),
        },
    );
    let restored = redone.workspace.as_ref().unwrap();
    assert_eq!(restored.document.nodes(), after.document.nodes());
    assert_eq!(restored.document.overrides(), after.document.overrides());
    assert_eq!(
        restored.document.audio_bindings(),
        after.document.audio_bindings()
    );
    command(service, ProjectRequest::Close);
    let reopened = command(service, ProjectRequest::Open(path.clone()));
    assert_eq!(
        *reopened.workspace.as_ref().unwrap().document,
        *restored.document
    );

    let invalid = command(
        service,
        ProjectRequest::Macro(Operation::Save {
            id: id(&reopened, 215),
            register: 'z',
            program: Arc::new(SemanticProgram::new(vec![length(4), I::Split]).unwrap()),
        }),
    );
    invalid.macros.as_ref().unwrap().result.as_ref().unwrap();
    let saved_rows = rows(&path);
    let failed = command(
        service,
        ProjectRequest::Macro(Operation::Run {
            id: id(&invalid, 216),
            register: 'z',
            count: 1,
            scope: target.scope.clone(),
            context: current.clone(),
        }),
    );
    assert!(failed.macros.as_ref().unwrap().result.is_err());
    assert_eq!(
        rows(&path),
        saved_rows,
        "late unsupported instruction rolls back every leaf"
    );
    assert_eq!(
        *failed.workspace.as_ref().unwrap().document,
        *restored.document
    );

    let inner = command(
        service,
        ProjectRequest::Macro(Operation::Save {
            id: id(&failed, 217),
            register: 'x',
            program: Arc::new(SemanticProgram::new(vec![length(4)]).unwrap()),
        }),
    );
    inner.macros.as_ref().unwrap().result.as_ref().unwrap();
    let outer = command(
        service,
        ProjectRequest::Macro(Operation::Save {
            id: id(&inner, 218),
            register: 'y',
            program: Arc::new(
                SemanticProgram::new(vec![
                    I::Call {
                        register: deadpan_core::RegisterName::new('x').unwrap(),
                        count: NonZeroU32::new(1).unwrap(),
                    },
                    length(5),
                ])
                .unwrap(),
            ),
        }),
    );
    outer.macros.as_ref().unwrap().result.as_ref().unwrap();
    let call_rows = rows(&path);
    let called = command(
        service,
        ProjectRequest::Macro(Operation::Run {
            id: id(&outer, 219),
            register: 'y',
            count: 2,
            scope: target.scope.clone(),
            context: current,
        }),
    );
    let receipt = called.macros.as_ref().unwrap().result.as_ref().unwrap();
    let final_workspace = called.workspace.as_ref().unwrap();
    assert_eq!(
        rows(&path),
        (call_rows.0 + 1, call_rows.1 + 1, call_rows.2 + 8)
    );
    assert_eq!(final_workspace.plan.duration().frames(), 53);
    assert_eq!(
        final_workspace.document.nodes().len(),
        after.document.nodes().len()
    );
    assert_eq!(receipt.committed().unwrap().cursor, Some(ProjectFrame(23)));
    assert!(
        called.semantic.as_ref().unwrap().edit.is_none(),
        "named call cannot seed dot"
    );
    let scoped = receipt.committed().unwrap().scoped.as_ref().unwrap();
    assert_eq!(scoped.also.as_ref().unwrap().len(), 1);
    for selected in std::iter::once(&scoped.target).chain(scoped.also.as_ref().unwrap()) {
        assert!(
            matches!(&final_workspace.document.nodes()[&selected.node].kind,
            NodeKind::Hold { recipe } if recipe.duration == frames(5))
        );
    }
    ProjectStore::open(&path, AccessMode::ReadOnly)
        .unwrap()
        .validate_full()
        .unwrap();
}
