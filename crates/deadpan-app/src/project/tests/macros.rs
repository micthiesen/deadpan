//! Actor-level macro preparation, durable copies and failure receipts.

use std::num::NonZeroU32;

use deadpan_core::{
    FrameCut, RegisterName, SemanticContext, SemanticInstruction, SemanticProgram,
    SliceCaptureSelection,
};

use super::*;
use crate::project::macros::{Id, Operation, Outcome, Receipt};
use crate::project::registers::Value;
use crate::project::slice::{CaptureRequest, Captured, CopyId};

fn setup(path: &Path) -> (Harness, ProjectUpdate) {
    drop(seed_holds(path, &["a", "b", "c"]));
    let harness = Harness::new();
    let opened = command(&harness.service, ProjectRequest::Open(path.into()));
    assert!(opened.error.is_none(), "{:?}", opened.error);
    (harness, opened)
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

fn program(instructions: Vec<SemanticInstruction>) -> Arc<SemanticProgram> {
    Arc::new(SemanticProgram::new(instructions).unwrap())
}

fn cut(count: u32, register: char) -> SemanticInstruction {
    SemanticInstruction::CutFrames {
        operation: FrameCut::new(count).unwrap(),
        register: RegisterName::new(register).unwrap(),
    }
}

fn movement(count: u32) -> SemanticInstruction {
    SemanticInstruction::MoveFrames {
        forward: true,
        count: NonZeroU32::new(count).unwrap(),
    }
}

fn save(
    update: &ProjectUpdate,
    request: u64,
    register: char,
    program: Arc<SemanticProgram>,
) -> Operation {
    Operation::Save {
        id: id(update, request),
        register,
        program,
    }
}

fn run(update: &ProjectUpdate, request: u64, register: char, count: u32, cursor: i64) -> Operation {
    Operation::Run {
        id: id(update, request),
        register,
        count,
        scope: SequenceScope::default(),
        context: SemanticContext {
            parent: update.workspace.as_ref().unwrap().document.root().clone(),
            cursor: ProjectFrame(cursor),
        },
    }
}

fn send(service: &ProjectService, operation: Operation) -> ProjectUpdate {
    let update = command(service, ProjectRequest::Macro(operation));
    assert!(
        update.macros.as_ref().unwrap().result.is_ok(),
        "{:?}",
        update.macros
    );
    update
}

fn receipt(update: &ProjectUpdate) -> &Receipt {
    update.macros.as_ref().unwrap().result.as_ref().unwrap()
}

fn copied(update: &ProjectUpdate, register: char) -> &Arc<Captured> {
    let Value::Edited(copied) = &update.registers.as_ref().unwrap().entries[&register] else {
        panic!("expected copied Edit")
    };
    copied
}

fn rows(path: &Path) -> (i64, i64, i64) {
    rusqlite::Connection::open(path.join("project.sqlite")).unwrap().query_row(
        "SELECT (SELECT count(*) FROM revisions),(SELECT count(*) FROM history),(SELECT count(*) FROM transaction_steps)",
        [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
    ).unwrap()
}

fn capture(update: &ProjectUpdate, request: u64, register: char) -> ProjectRequest {
    let workspace = update.workspace.as_ref().unwrap();
    ProjectRequest::CaptureEditSlice(CaptureRequest {
        id: CopyId {
            session: workspace.session,
            project: workspace.document.project_id().clone(),
            source_revision: workspace.document.revision_id().clone(),
            request,
            persisted_version: None,
        },
        register: Some(register),
        scope: SequenceScope::default(),
        parent: workspace.document.root().clone(),
        selection: SliceCaptureSelection::Range {
            range: FrameRange::new(ProjectFrame(1), ProjectFrame(4)).unwrap(),
        },
    })
}

#[test]
fn recorded_program_runs_counted_cuts_as_one_undo_and_restores_intermediate_copy_after_reopen() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("macro-actor.deadpan");
    let (harness, opened) = setup(&path);
    let copied_before = command(&harness.service, capture(&opened, 1, 'c'));
    let body = program(vec![movement(1), cut(2, 'b')]);
    let saved = send(&harness.service, save(&copied_before, 1, 'a', body.clone()));
    assert!(Arc::ptr_eq(
        copied(&copied_before, '"'),
        copied(&saved, '"')
    ));
    assert!(Arc::ptr_eq(
        copied(&copied_before, 'c'),
        copied(&saved, 'c')
    ));
    let initial_rows = rows(&path);
    let before = saved.workspace.as_ref().unwrap().clone();
    let executed = send(&harness.service, run(&saved, 2, 'a', 2, 3));
    assert_eq!(
        executed
            .workspace
            .as_ref()
            .unwrap()
            .plan
            .duration()
            .frames(),
        26
    );
    assert_eq!(
        rows(&path),
        (initial_rows.0 + 1, initial_rows.1 + 1, initial_rows.2 + 2)
    );
    let committed = receipt(&executed).committed().unwrap();
    assert_eq!(committed.cursor, Some(ProjectFrame(5)));
    let latest = copied(&executed, 'b').clone();
    assert!(Arc::ptr_eq(&latest, copied(&executed, '"')));
    assert!(Arc::ptr_eq(
        copied(&copied_before, 'c'),
        copied(&executed, 'c')
    ));
    assert_eq!(
        latest.bounds(),
        FrameRange::new(ProjectFrame(0), ProjectFrame(28)).unwrap()
    );
    assert_ne!(&latest.id().source_revision, before.document.revision_id());
    assert_eq!(
        latest.id().persisted_version,
        Some(executed.registers.as_ref().unwrap().version)
    );
    assert_eq!(latest.source_path(), &["Your edit".to_string()]);
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly).unwrap();
    latest
        .slice()
        .validate_capture(
            &reader
                .capture_snapshot_at(&latest.id().source_revision)
                .unwrap(),
        )
        .unwrap();
    drop(reader);
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: committed.revision.clone(),
        },
    );
    let undone_workspace = undone.workspace.as_ref().unwrap();
    let mut expected_undo = serde_json::to_value(before.document.as_ref()).unwrap();
    expected_undo["revision_id"] =
        serde_json::to_value(undone_workspace.document.revision_id()).unwrap();
    assert_eq!(
        serde_json::to_value(undone_workspace.document.as_ref()).unwrap(),
        expected_undo
    );
    assert_eq!(copied(&undone, 'b').slice(), latest.slice());
    let closed = command(&harness.service, ProjectRequest::Close);
    assert!(closed.saved_macro.is_none() && closed.macros.is_none());
    let reopened = command(&harness.service, ProjectRequest::Open(path));
    assert!(reopened.saved_macro.is_none() && reopened.macros.is_none());
    let Value::Macro(restored) = &reopened.registers.as_ref().unwrap().entries[&'a'] else {
        panic!("macro")
    };
    assert_eq!(restored, &body);
    assert_eq!(copied(&reopened, 'b').slice(), latest.slice());
    assert_eq!(copied(&reopened, 'b').bounds(), latest.bounds());
}

#[test]
fn save_preserves_redo_and_motion_only_execution_never_writes_history_or_registers() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("macro-motion.deadpan");
    let (harness, opened) = setup(&path);
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: opened
                .workspace
                .as_ref()
                .unwrap()
                .document
                .revision_id()
                .clone(),
        },
    );
    let initial_rows = rows(&path);
    let saved = send(
        &harness.service,
        save(&undone, 1, 'm', program(vec![movement(3)])),
    );
    assert_eq!(rows(&path), initial_rows);
    let bank = saved.registers.as_ref().unwrap().clone();
    let executed = send(&harness.service, run(&saved, 2, 'm', 2, 4));
    assert!(receipt(&executed).committed().is_none());
    let Outcome::Executed { cursor, .. } = receipt(&executed).outcome else {
        panic!("executed")
    };
    assert_eq!(cursor, ProjectFrame(10));
    assert!(Arc::ptr_eq(&bank, executed.registers.as_ref().unwrap()));
    assert_eq!(rows(&path), initial_rows);
    let redone = command(
        &harness.service,
        ProjectRequest::Redo {
            expected_revision: executed
                .workspace
                .as_ref()
                .unwrap()
                .document
                .revision_id()
                .clone(),
        },
    );
    assert!(redone.error.is_none(), "{:?}", redone.error);
    assert_eq!(redone.workspace.unwrap().plan.duration().frames(), 30);
}

#[test]
fn stale_or_changed_identity_wrong_type_scope_and_late_failure_publish_no_partial_edits() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("macro-refused.deadpan");
    let (harness, opened) = setup(&path);
    let initial_rows = rows(&path);
    let operation = save(
        &opened,
        1,
        'a',
        program(vec![
            cut(1, 'b'),
            SemanticInstruction::Call {
                register: RegisterName::new('z').unwrap(),
                count: NonZeroU32::new(1).unwrap(),
            },
        ]),
    );
    let saved = send(&harness.service, operation.clone());
    let duplicate = send(&harness.service, operation.clone());
    assert_eq!(
        receipt(&saved).bank_version,
        receipt(&duplicate).bank_version
    );
    let mut collision = operation;
    let Operation::Save { program: body, .. } = &mut collision else {
        unreachable!()
    };
    *body = program(vec![movement(4)]);
    let mut stale_session = run(&saved, 3, 'a', 1, 0);
    let Operation::Run { id, .. } = &mut stale_session else {
        unreachable!()
    };
    id.session += 1;
    let mut wrong_scope = run(&saved, 4, 'a', 1, 0);
    let Operation::Run { context, .. } = &mut wrong_scope else {
        unreachable!()
    };
    context.parent = node("a");
    for operation in [
        collision,
        run(&opened, 2, 'a', 1, 0),
        stale_session,
        wrong_scope,
        run(&saved, 5, 'a', 1, 0),
    ] {
        let failed = command(&harness.service, ProjectRequest::Macro(operation));
        assert!(failed.macros.as_ref().unwrap().result.is_err());
        assert_eq!(
            failed.registers.as_ref().unwrap().version,
            saved.registers.as_ref().unwrap().version
        );
        assert_eq!(
            failed.workspace.as_ref().unwrap().document,
            opened.workspace.as_ref().unwrap().document
        );
        assert_eq!(failed.saved_macro.as_ref().unwrap().id, receipt(&saved).id);
        assert_eq!(rows(&path), initial_rows);
    }
    let copy = command(&harness.service, capture(&saved, 2, 'c'));
    let failed = command(
        &harness.service,
        ProjectRequest::Macro(run(&copy, 6, 'c', 1, 0)),
    );
    assert!(failed.macros.unwrap().result.unwrap_err().contains("macro"));
    assert_eq!(rows(&path), initial_rows);
}

#[test]
fn committed_macro_refresh_failure_retains_receipt_and_copy_and_exact_retry_is_idempotent() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("macro-refresh.deadpan");
    let (harness, opened) = setup(&path);
    let saved = send(
        &harness.service,
        save(&opened, 1, 'a', program(vec![cut(2, 'b')])),
    );
    let initial_rows = rows(&path);
    harness
        .service
        .shared
        .workspace_refresh_failure
        .store(true, Ordering::Release);
    let operation = run(&saved, 2, 'a', 2, 3);
    let executed = send(&harness.service, operation.clone());
    let retained = receipt(&executed).clone();
    let Outcome::Executed {
        refresh_error,
        committed,
        ..
    } = &retained.outcome
    else {
        panic!("executed")
    };
    assert!(refresh_error.as_ref().unwrap().contains("Reopen"));
    assert_eq!(
        executed.workspace.as_ref().unwrap().document.revision_id(),
        saved.workspace.as_ref().unwrap().document.revision_id()
    );
    assert_eq!(
        rows(&path),
        (initial_rows.0 + 1, initial_rows.1 + 1, initial_rows.2 + 2)
    );
    let duplicate = send(&harness.service, operation);
    assert_eq!(receipt(&duplicate).committed(), committed.as_deref());
    let failed = command(
        &harness.service,
        ProjectRequest::Macro(run(&executed, 3, 'a', 1, 0)),
    );
    assert!(
        failed
            .macros
            .as_ref()
            .unwrap()
            .result
            .as_ref()
            .unwrap_err()
            .contains("Reopen")
    );
    assert_eq!(
        failed.saved_macro.as_ref().unwrap().committed(),
        committed.as_deref()
    );
    assert!(Arc::ptr_eq(copied(&executed, 'b'), copied(&failed, 'b')));
    assert_eq!(
        rows(&path),
        (initial_rows.0 + 1, initial_rows.1 + 1, initial_rows.2 + 2)
    );
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(path));
    assert_eq!(
        reopened.workspace.as_ref().unwrap().document.revision_id(),
        &committed.as_ref().unwrap().revision
    );
    assert_eq!(
        copied(&reopened, 'b').slice(),
        copied(&executed, 'b').slice()
    );
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: committed.as_ref().unwrap().revision.clone(),
        },
    );
    assert_eq!(undone.workspace.unwrap().plan.duration().frames(), 30);
}

#[test]
fn nested_macro_copies_keep_their_staged_absolute_bounds_and_original_group_labels() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("nested-macro.deadpan");
    let mut store = seed_holds(&path, &["lead"]);
    seed_command(
        &mut store,
        Command::Insert {
            parent: node("root"),
            index: 1,
            subtree: Subtree {
                root: node("group"),
                nodes: BTreeMap::from([
                    (
                        node("group"),
                        BeatNode::sequence("A named group", vec![node("nested")]),
                    ),
                    (node("nested"), BeatNode::hold("Nested pause", hold(10))),
                ]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
        "nested",
    );
    drop(store);
    let harness = Harness::new();
    let opened = command(&harness.service, ProjectRequest::Open(path));
    let saved = send(
        &harness.service,
        save(&opened, 1, 'a', program(vec![cut(1, 'b')])),
    );
    let scope = SequenceScope::default()
        .descend(saved.workspace.as_ref().unwrap(), &node("group"))
        .unwrap();
    let operation = Operation::Run {
        id: id(&saved, 2),
        register: 'a',
        count: 2,
        scope: scope.clone(),
        context: SemanticContext {
            parent: node("group"),
            cursor: ProjectFrame(12),
        },
    };
    let executed = send(&harness.service, operation);
    let copied = copied(&executed, 'b');
    assert_eq!(copied.scope(), &scope);
    assert_eq!(
        copied.bounds(),
        FrameRange::new(ProjectFrame(10), ProjectFrame(19)).unwrap()
    );
    assert_eq!(
        copied.source_path(),
        &["Your edit".to_string(), "A named group".to_string()]
    );
    assert_eq!(
        copied.slice().range(),
        FrameRange::new(ProjectFrame(12), ProjectFrame(13)).unwrap()
    );
}
