//! Authenticated macro requests against the native writer and runtime bank.

use std::num::NonZeroU32;

use deadpan_cli::macros::{Operation as MacroOperation, Request as MacroRequest};
use deadpan_core::{
    FrameCut, RegisterName, SemanticContext, SemanticInstruction, SemanticProgram,
    SliceCaptureSelection,
};
use deadpan_store::registers::RegisterBank;

use super::*;
use crate::project::macros::{Id as NativeId, Operation as NativeOperation};
use crate::project::registers::Value as RuntimeValue;
use crate::project::slice::{CaptureRequest, Captured, CopyId};

fn program(instructions: Vec<SemanticInstruction>) -> Arc<SemanticProgram> {
    Arc::new(SemanticProgram::new(instructions).unwrap())
}

fn movement(count: u32) -> SemanticInstruction {
    SemanticInstruction::MoveFrames {
        forward: true,
        count: NonZeroU32::new(count).unwrap(),
    }
}

fn cut(count: u32) -> SemanticInstruction {
    SemanticInstruction::CutFrames {
        operation: FrameCut::new(count).unwrap(),
        register: RegisterName::new('b').unwrap(),
    }
}

fn setup(path: &Path) -> (Harness, ProjectUpdate) {
    drop(seed_holds(path, &["a", "b", "c"]));
    let harness = Harness::new();
    let update = command(&harness.service, ProjectRequest::Open(path.into()));
    assert!(update.error.is_none(), "{:?}", update.error);
    (harness, update)
}

fn request(update: &ProjectUpdate, operation: MacroOperation, dry_run: bool) -> Operation {
    let document = &update.workspace.as_ref().unwrap().document;
    Operation::Execute {
        project_id: document.project_id().clone(),
        command: Box::new(ShortOperation::Macro {
            request: Box::new(MacroRequest {
                protocol: 1,
                project_id: document.project_id().clone(),
                expected_revision: document.revision_id().clone(),
                expected_bank_version: update.registers.as_ref().unwrap().version,
                dry_run,
                operation,
            }),
        }),
    }
}

fn save(update: &ProjectUpdate, body: Arc<SemanticProgram>, dry_run: bool) -> Operation {
    request(
        update,
        MacroOperation::Save {
            register: RegisterName::new('a').unwrap(),
            program: body,
        },
        dry_run,
    )
}

fn run(
    update: &ProjectUpdate,
    register: char,
    parent: &str,
    count: u32,
    cursor: i64,
    revision: Option<&str>,
    dry_run: bool,
) -> Operation {
    request(
        update,
        MacroOperation::Run {
            register: RegisterName::new(register).unwrap(),
            parent: node(parent),
            cursor: ProjectFrame(cursor),
            selected_child: None,
            visual_selection: None,
            count: NonZeroU32::new(count).unwrap(),
            new_revision: revision.map(|revision| RevisionId::new(revision).unwrap()),
        },
        dry_run,
    )
}

fn remote_update(harness: &Harness) -> ProjectUpdate {
    let update = harness.service.take_update().expect("native bank refresh");
    assert!(update.error.is_none(), "{:?}", update.error);
    assert!(
        update.committed.is_none(),
        "remote cursor is not native intent"
    );
    assert!(update.macros.is_none() && update.saved_macro.is_none());
    update
}

fn copied(update: &ProjectUpdate, register: char) -> &Arc<Captured> {
    let RuntimeValue::Edited(copied) = &update.registers.as_ref().unwrap().entries[&register]
    else {
        panic!("copied Edit")
    };
    copied
}

fn capture(update: &ProjectUpdate) -> ProjectRequest {
    let workspace = update.workspace.as_ref().unwrap();
    ProjectRequest::CaptureEditSlice(CaptureRequest {
        id: CopyId {
            session: workspace.session,
            project: workspace.document.project_id().clone(),
            source_revision: workspace.document.revision_id().clone(),
            request: 1,
            persisted_version: None,
        },
        register: Some('c'),
        scope: SequenceScope::default(),
        parent: workspace.document.root().clone(),
        selection: SliceCaptureSelection::Range {
            range: FrameRange::new(ProjectFrame(1), ProjectFrame(4)).unwrap(),
        },
    })
}

fn rows(path: &Path) -> (i64, i64, i64) {
    rusqlite::Connection::open(path.join("project.sqlite"))
        .unwrap()
        .query_row(
            "SELECT (SELECT count(*) FROM revisions),\
             (SELECT count(*) FROM history),(SELECT count(*) FROM transaction_steps)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap()
}

fn durable(path: &Path) -> (ProjectDocument, RegisterBank, (i64, i64, i64)) {
    let store = ProjectStore::open(path, AccessMode::ReadOnly).unwrap();
    let (document, bank) = store.snapshot_with_registers().unwrap();
    (document, bank, rows(path))
}

fn same_document_except_revision(actual: &ProjectDocument, expected: &ProjectDocument) {
    let mut expected = serde_json::to_value(expected).unwrap();
    expected["revision_id"] = serde_json::to_value(actual.revision_id()).unwrap();
    assert_eq!(serde_json::to_value(actual).unwrap(), expected);
}

#[test]
fn remote_visual_yank_previews_oriented_selection_and_keeps_bank_receipt_on_refresh_failure() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("remote-visual-yank.deadpan");
    let (harness, opened) = setup(&path);
    let mut client = client(&path);
    live_project::request(
        &mut client,
        save(
            &opened,
            program(vec![
                SemanticInstruction::MoveFrames {
                    forward: false,
                    count: NonZeroU32::new(3).unwrap(),
                },
                SemanticInstruction::YankSelection {
                    register: RegisterName::new('b').unwrap(),
                },
            ]),
            false,
        ),
    )
    .unwrap();
    let saved = remote_update(&harness);
    let before = durable(&path);
    let mut operation = run(&saved, 'a', "root", 1, 12, Some("visual-yank-unused"), true);
    let Operation::Execute { command, .. } = &mut operation else {
        unreachable!()
    };
    let ShortOperation::Macro { request } = command.as_mut() else {
        unreachable!()
    };
    let MacroOperation::Run {
        visual_selection, ..
    } = &mut request.operation
    else {
        unreachable!()
    };
    *visual_selection = Some(deadpan_core::SemanticVisualSelection {
        anchor: ProjectFrame(18),
        head: ProjectFrame(12),
        extending: true,
    });
    let Reply::Completed {
        output,
        committed_revision,
        committed_registers,
        ..
    } = live_project::request(&mut client, operation.clone()).unwrap()
    else {
        panic!("range preview")
    };
    assert!(committed_revision.is_none() && committed_registers.is_none());
    let selection = serde_json::json!({"anchor":18,"head":9,"extending":false});
    assert_eq!(output["context"]["visual_selection"], selection);
    assert_eq!(output["context"]["cursor"], 9);
    assert_eq!(durable(&path), before);
    assert!(harness.service.take_update().is_none());
    let Operation::Execute { command, .. } = &mut operation else {
        unreachable!()
    };
    let ShortOperation::Macro { request } = command.as_mut() else {
        unreachable!()
    };
    request.dry_run = false;
    harness
        .service
        .shared
        .host_refresh_failure
        .store(true, Ordering::Release);
    let Reply::Completed {
        output,
        committed_revision,
        committed_registers,
        refresh_error,
    } = live_project::request(&mut client, operation).unwrap()
    else {
        panic!("saved range receipt")
    };
    assert!(committed_revision.is_none());
    let receipt = committed_registers.unwrap();
    assert_eq!(receipt.revision_id, *before.0.revision_id());
    assert_eq!(receipt.bank_version, before.1.version + 1);
    assert!(refresh_error.unwrap().contains("Injected failure"));
    assert_eq!(output["context"]["visual_selection"], selection);
    let update = harness.service.take_update().unwrap();
    assert!(update.error.is_some());
    assert!(update.committed.is_none() && update.macros.is_none());
    let copy = copied(&update, 'b');
    assert_eq!(copy.child_label(), None);
    assert_eq!(
        copy.slice().selection(),
        &SliceCaptureSelection::Range {
            range: FrameRange::new(ProjectFrame(9), ProjectFrame(18)).unwrap(),
        }
    );
    copy.slice().validate_capture(&before.0).unwrap();
    assert_eq!(copy.id().persisted_version, Some(receipt.bank_version));
    let after = durable(&path);
    assert_eq!(after.0, before.0);
    assert_eq!(after.2, before.2);
    shutdown(&harness);
}

#[test]
fn remote_yank_retains_bank_only_receipt_and_runtime_copy_after_refresh_failure() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("remote-yank-refresh.deadpan");
    let (harness, opened) = setup(&path);
    let mut client = client(&path);
    live_project::request(
        &mut client,
        save(
            &opened,
            program(vec![SemanticInstruction::YankBeat {
                register: RegisterName::new('b').unwrap(),
            }]),
            false,
        ),
    )
    .unwrap();
    let saved = remote_update(&harness);
    let before = durable(&path);
    let mut operation = run(
        &saved,
        'a',
        "root",
        1,
        1,
        Some("unused-yank-revision"),
        false,
    );
    let Operation::Execute { command, .. } = &mut operation else {
        unreachable!()
    };
    let ShortOperation::Macro { request } = command.as_mut() else {
        unreachable!()
    };
    let MacroOperation::Run { selected_child, .. } = &mut request.operation else {
        unreachable!()
    };
    *selected_child = Some(node("b"));
    harness
        .service
        .shared
        .host_refresh_failure
        .store(true, Ordering::Release);
    let Reply::Completed {
        output,
        committed_revision,
        committed_registers,
        refresh_error,
    } = live_project::request(&mut client, operation.clone()).unwrap()
    else {
        panic!("bank-only receipt")
    };
    assert!(committed_revision.is_none());
    assert!(refresh_error.unwrap().contains("Injected failure"));
    let receipt = committed_registers.unwrap();
    assert_eq!(receipt.revision_id, *before.0.revision_id());
    assert_eq!(receipt.bank_version, before.1.version + 1);
    assert_eq!(output["context"]["cursor"], 1);
    assert_eq!(output["context"]["selected_child"], "b");
    let update = harness.service.take_update().unwrap();
    assert!(update.error.is_some());
    assert!(update.committed.is_none() && update.macros.is_none() && update.saved_macro.is_none());
    let copy = copied(&update, 'b');
    assert_eq!(copy.child_label(), Some("b"));
    assert_eq!(
        copy.slice().range(),
        FrameRange::new(ProjectFrame(10), ProjectFrame(20)).unwrap()
    );
    assert_eq!(copy.id().persisted_version, Some(receipt.bank_version));
    assert_eq!(&copy.id().source_revision, before.0.revision_id());
    let after = durable(&path);
    assert_eq!(after.0, before.0);
    assert_eq!(after.2, before.2);
    let error = live_project::request(&mut client, operation).unwrap_err();
    assert_eq!(error.code, "RegisterInvalid");
    assert!(error.committed_registers.is_none());
    assert_eq!(durable(&path), after);
    live_project::request(
        &mut client,
        save(
            &update,
            program(vec![SemanticInstruction::Paste {
                register: RegisterName::new('b').unwrap(),
                before: true,
            }]),
            false,
        ),
    )
    .unwrap();
    let saved = remote_update(&harness);
    let before_paste = durable(&path);
    let mut operation = run(&saved, 'a', "root", 1, 29, Some("remote-paste-only"), false);
    let Operation::Execute { command, .. } = &mut operation else {
        unreachable!()
    };
    let ShortOperation::Macro { request } = command.as_mut() else {
        unreachable!()
    };
    let MacroOperation::Run { selected_child, .. } = &mut request.operation else {
        unreachable!()
    };
    *selected_child = Some(node("a"));
    let Reply::Completed {
        output,
        committed_revision,
        committed_registers,
        refresh_error,
    } = live_project::request(&mut client, operation).unwrap()
    else {
        panic!("paste receipt")
    };
    assert_eq!(
        committed_revision,
        Some(RevisionId::new("remote-paste-only").unwrap())
    );
    assert!(committed_registers.is_none() && refresh_error.is_none());
    assert!(output["committed_registers"].is_null());
    let pasted = remote_update(&harness);
    assert_eq!(durable(&path).1, before_paste.1);
    assert_eq!(
        pasted.workspace.as_ref().unwrap().plan.duration().frames(),
        40
    );
    assert_eq!(output["context"]["cursor"], 0);
    shutdown(&harness);
}

#[test]
fn authenticated_save_and_counted_nested_run_keep_runtime_provenance_and_one_undo() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("remote-macro-nested.deadpan");
    let mut store = seed_holds(&path, &["lead", "tail"]);
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
                        BeatNode::sequence("Named group", vec![node("nested")]),
                    ),
                    (node("nested"), BeatNode::hold("Nested pause", hold(20))),
                ]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
        "nested",
    );
    drop(store);
    let harness = Harness::new();
    let opened = command(&harness.service, ProjectRequest::Open(path.clone()));
    let before = command(&harness.service, capture(&opened));
    let baseline = durable(&path);
    let body = program(vec![movement(1), cut(2)]);
    let mut client = client(&path);
    let Reply::Completed {
        committed_revision,
        committed_registers,
        refresh_error,
        ..
    } = live_project::request(&mut client, save(&before, body.clone(), false)).unwrap()
    else {
        panic!("macro save receipt")
    };
    assert!(committed_revision.is_none() && refresh_error.is_none());
    let saved = remote_update(&harness);
    let receipt = committed_registers.unwrap();
    assert_eq!(receipt.project_id, *baseline.0.project_id());
    assert_eq!(receipt.revision_id, *baseline.0.revision_id());
    assert_eq!(receipt.bank_version, baseline.1.version + 1);
    assert_eq!(
        receipt.bank_version,
        saved.registers.as_ref().unwrap().version
    );
    assert_eq!(rows(&path), baseline.2);
    assert!(Arc::ptr_eq(copied(&before, '"'), copied(&saved, '"')));
    let RuntimeValue::Macro(saved_body) = &saved.registers.as_ref().unwrap().entries[&'a'] else {
        panic!("runtime macro")
    };
    assert_eq!(saved_body, &body);

    let Reply::Completed {
        output,
        committed_revision,
        committed_registers,
        refresh_error,
    } = live_project::request(
        &mut client,
        run(&saved, 'a', "group", 2, 13, Some("remote-macro-run"), false),
    )
    .unwrap()
    else {
        panic!("macro execution receipt")
    };
    assert_eq!(
        committed_revision,
        Some(RevisionId::new("remote-macro-run").unwrap())
    );
    assert!(refresh_error.is_none());
    assert_eq!(output["context"]["parent"], "group");
    assert_eq!(output["context"]["cursor"], 15);
    let executed = remote_update(&harness);
    let workspace = executed.workspace.as_ref().unwrap();
    assert_eq!(workspace.plan.duration().frames(), 36);
    assert_eq!(
        rows(&path),
        (baseline.2.0 + 1, baseline.2.1 + 1, baseline.2.2 + 2)
    );
    let receipt = committed_registers.unwrap();
    assert_eq!(receipt.revision_id, *workspace.document.revision_id());
    assert_eq!(
        receipt.bank_version,
        executed.registers.as_ref().unwrap().version
    );
    let latest = copied(&executed, 'b').clone();
    assert!(Arc::ptr_eq(&latest, copied(&executed, '"')));
    assert!(Arc::ptr_eq(copied(&before, 'c'), copied(&executed, 'c')));
    let scope = SequenceScope::default()
        .descend(before.workspace.as_ref().unwrap(), &node("group"))
        .unwrap();
    assert_eq!(latest.scope(), &scope);
    assert_eq!(
        latest.bounds(),
        FrameRange::new(ProjectFrame(10), ProjectFrame(28)).unwrap()
    );
    assert_eq!(
        latest.source_path(),
        &["Your edit".to_string(), "Named group".to_string()]
    );
    assert_eq!(
        latest.slice().range(),
        FrameRange::new(ProjectFrame(15), ProjectFrame(17)).unwrap()
    );
    assert_eq!(latest.id().persisted_version, Some(receipt.bank_version));
    assert_ne!(&latest.id().source_revision, baseline.0.revision_id());
    assert_ne!(
        &latest.id().source_revision,
        workspace.document.revision_id()
    );
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
    let after = durable(&path);
    for (direction, expected_revision, new_revision, expected) in [
        (
            HistoryDirection::Undo,
            "remote-macro-run",
            "remote-macro-undo",
            &baseline.0,
        ),
        (
            HistoryDirection::Redo,
            "remote-macro-undo",
            "remote-macro-redo",
            &after.0,
        ),
    ] {
        committed(
            live_project::request(
                &mut client,
                Operation::Execute {
                    project_id: baseline.0.project_id().clone(),
                    command: Box::new(ShortOperation::History {
                        direction,
                        expected_revision: RevisionId::new(expected_revision).unwrap(),
                        new_revision: RevisionId::new(new_revision).unwrap(),
                        dry_run: false,
                    }),
                },
            )
            .unwrap(),
            new_revision,
        );
        let update = remote_update(&harness);
        same_document_except_revision(&update.workspace.as_ref().unwrap().document, expected);
        assert_eq!(
            durable(&path).1,
            after.1,
            "history does not rewind registers"
        );
    }
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(path));
    assert!(
        reopened.committed.is_none() && reopened.macros.is_none() && reopened.saved_macro.is_none()
    );
    let restored = copied(&reopened, 'b');
    assert_eq!(restored.slice(), latest.slice());
    assert_eq!(restored.bounds(), latest.bounds());
    assert_eq!(restored.scope(), latest.scope());
    assert_eq!(restored.source_path(), latest.source_path());
    let RuntimeValue::Macro(restored) = &reopened.registers.as_ref().unwrap().entries[&'a'] else {
        panic!("restored macro")
    };
    assert_eq!(restored, &body);
    shutdown(&harness);
}

#[test]
fn authenticated_dry_runs_and_motion_only_run_do_not_write_or_publish_native_intent() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("remote-macro-read-only.deadpan");
    let (harness, opened) = setup(&path);
    let before = durable(&path);
    let mut client = client(&path);
    let Reply::Completed {
        output,
        committed_revision,
        committed_registers,
        refresh_error,
    } = live_project::request(&mut client, save(&opened, program(vec![cut(2)]), true)).unwrap()
    else {
        panic!("save preview")
    };
    assert!(
        committed_revision.is_none() && committed_registers.is_none() && refresh_error.is_none()
    );
    assert_eq!(output["operation"], "save");
    assert_eq!(output["instruction_count"], 1);
    assert_eq!(output["bank_version"], before.1.version + 1);
    assert_eq!(durable(&path), before);
    assert!(harness.service.take_update().is_none());
    live_project::request(&mut client, save(&opened, program(vec![cut(2)]), false)).unwrap();
    let saved = remote_update(&harness);
    let before = durable(&path);
    let Reply::Completed {
        output,
        committed_revision,
        committed_registers,
        refresh_error,
    } = live_project::request(
        &mut client,
        run(&saved, 'a', "root", 2, 3, Some("preview"), true),
    )
    .unwrap()
    else {
        panic!("run preview")
    };
    assert!(
        committed_revision.is_none() && committed_registers.is_none() && refresh_error.is_none()
    );
    assert_eq!(output["context"]["cursor"], 3);
    assert_eq!(output["edit"]["duration_delta"], -4);
    assert_eq!(output["edit"]["new_revision"], "preview");
    assert_eq!(durable(&path), before);
    assert!(harness.service.take_update().is_none());
    live_project::request(&mut client, save(&saved, program(vec![movement(3)]), false)).unwrap();
    let saved = remote_update(&harness);
    let before = durable(&path);
    let Reply::Completed {
        output,
        committed_revision,
        committed_registers,
        refresh_error,
    } = live_project::request(&mut client, run(&saved, 'a', "root", 2, 4, None, false)).unwrap()
    else {
        panic!("motion receipt")
    };
    assert!(
        committed_revision.is_none() && committed_registers.is_none() && refresh_error.is_none()
    );
    assert_eq!(output["context"]["cursor"], 10);
    assert_eq!(output["context"]["parent"], "root");
    assert_eq!(output["edit"], Value::Null);
    assert_eq!(durable(&path), before);
    assert!(
        harness.service.take_update().is_none(),
        "remote motion must not move the native cursor"
    );
    shutdown(&harness);
}

#[test]
fn authenticated_macro_failures_leave_document_bank_and_history_exactly_unchanged() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("remote-macro-refused.deadpan");
    let (harness, opened) = setup(&path);
    let copied = command(&harness.service, capture(&opened));
    let mut client = client(&path);
    let body = program(vec![
        cut(1),
        SemanticInstruction::Call {
            register: RegisterName::new('z').unwrap(),
            count: NonZeroU32::new(1).unwrap(),
        },
    ]);
    live_project::request(&mut client, save(&copied, body, false)).unwrap();
    let saved = remote_update(&harness);
    let before = durable(&path);
    let mut stale_revision = run(&saved, 'a', "root", 1, 0, Some("stale-revision"), false);
    let Operation::Execute { command, .. } = &mut stale_revision else {
        unreachable!()
    };
    let ShortOperation::Macro { request } = command.as_mut() else {
        unreachable!()
    };
    request.expected_revision = RevisionId::new("initial").unwrap();
    for (operation, code, message) in [
        (
            run(&saved, 'a', "root", 1, 0, Some("late-failure"), false),
            "InvalidCommand",
            "called macro register is empty",
        ),
        (
            run(&saved, 'c', "root", 1, 0, Some("wrong-type"), false),
            "InvalidCommand",
            "contains copied content",
        ),
        (
            run(&saved, 'a', "a", 1, 0, Some("wrong-parent"), false),
            "InvalidCommand",
            "not a Sequence",
        ),
        (
            run(&copied, 'a', "root", 1, 0, Some("stale-bank"), false),
            "RegisterInvalid",
            "bank version changed",
        ),
        (stale_revision, "RevisionConflict", "expected revision"),
    ] {
        let error = live_project::request(&mut client, operation).unwrap_err();
        assert_eq!(error.code, code);
        assert!(error.message.contains(message), "{error}");
        assert!(error.committed_revision.is_none() && error.committed_registers.is_none());
        assert_eq!(
            durable(&path),
            before,
            "a rejected request cannot leave staged cuts or registers"
        );
        assert!(harness.service.take_update().is_none());
    }
    shutdown(&harness);
}

#[test]
fn bank_only_save_and_authored_run_keep_exact_receipts_after_native_refresh_failure() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("remote-macro-refresh.deadpan");
    let (harness, opened) = setup(&path);
    let before = durable(&path);
    let body = program(vec![cut(2)]);
    let mut client = client(&path);
    harness
        .service
        .shared
        .host_refresh_failure
        .store(true, Ordering::Release);
    let Reply::Completed {
        committed_revision,
        committed_registers,
        refresh_error,
        ..
    } = live_project::request(&mut client, save(&opened, body.clone(), false)).unwrap()
    else {
        panic!("saved macro receipt")
    };
    assert!(committed_revision.is_none());
    assert!(refresh_error.unwrap().contains("Injected failure"));
    let receipt = committed_registers.unwrap();
    assert_eq!(receipt.project_id, *before.0.project_id());
    assert_eq!(receipt.revision_id, *before.0.revision_id());
    assert_eq!(receipt.bank_version, before.1.version + 1);
    let saved = harness.service.take_update().unwrap();
    assert!(saved.error.is_some());
    assert!(saved.committed.is_none() && saved.macros.is_none() && saved.saved_macro.is_none());
    assert_eq!(*saved.workspace.as_ref().unwrap().document, before.0);
    assert_eq!(
        saved.registers.as_ref().unwrap().version,
        receipt.bank_version
    );
    let RuntimeValue::Macro(actual) = &saved.registers.as_ref().unwrap().entries[&'a'] else {
        panic!("saved bank missing macro")
    };
    assert_eq!(actual, &body);
    assert_eq!(durable(&path).1.version, receipt.bank_version);
    assert_eq!(rows(&path), before.2);

    harness
        .service
        .shared
        .host_refresh_failure
        .store(true, Ordering::Release);
    let Reply::Completed {
        committed_revision,
        committed_registers,
        refresh_error,
        ..
    } = live_project::request(
        &mut client,
        run(&saved, 'a', "root", 2, 3, Some("saved-run"), false),
    )
    .unwrap()
    else {
        panic!("saved run receipt")
    };
    assert_eq!(
        committed_revision,
        Some(RevisionId::new("saved-run").unwrap())
    );
    assert!(refresh_error.unwrap().contains("Injected failure"));
    let receipt = committed_registers.unwrap();
    assert_eq!(receipt.revision_id.as_str(), "saved-run");
    let executed = harness.service.take_update().unwrap();
    assert!(executed.error.is_some());
    assert!(
        executed.committed.is_none() && executed.macros.is_none() && executed.saved_macro.is_none()
    );
    assert_eq!(*executed.workspace.as_ref().unwrap().document, before.0);
    assert_eq!(
        executed.registers.as_ref().unwrap().version,
        receipt.bank_version
    );
    let latest = copied(&executed, 'b').clone();
    assert_eq!(latest.id().persisted_version, Some(receipt.bank_version));
    let durable = durable(&path);
    assert_eq!(durable.0.revision_id().as_str(), "saved-run");
    assert_eq!(durable.0.duration().unwrap().frames(), 26);
    assert_eq!(durable.1.version, receipt.bank_version);
    assert_eq!(durable.2, (before.2.0 + 1, before.2.1 + 1, before.2.2 + 2));
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(path));
    assert_eq!(*reopened.workspace.as_ref().unwrap().document, durable.0);
    assert_eq!(copied(&reopened, 'b').slice(), latest.slice());
    shutdown(&harness);
}

fn native_id(update: &ProjectUpdate, request: u64) -> NativeId {
    let workspace = update.workspace.as_ref().unwrap();
    NativeId {
        session: workspace.session,
        project: workspace.document.project_id().clone(),
        revision: workspace.document.revision_id().clone(),
        bank_version: update.registers.as_ref().unwrap().version,
        request,
    }
}

#[test]
fn unread_native_save_motion_and_copy_continuations_block_remote_mutation() {
    for kind in ["save", "motion", "copy"] {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join(format!("unread-native-{kind}.deadpan"));
        let (harness, mut current) = setup(&path);
        let pending = match kind {
            "save" => ProjectRequest::Macro(NativeOperation::Save {
                id: native_id(&current, 1),
                register: 'a',
                program: program(vec![movement(2)]),
            }),
            "motion" => {
                let save = NativeOperation::Save {
                    id: native_id(&current, 1),
                    register: 'a',
                    program: program(vec![movement(2)]),
                };
                current = command(&harness.service, ProjectRequest::Macro(save));
                ProjectRequest::Macro(NativeOperation::Run {
                    id: native_id(&current, 2),
                    register: 'a',
                    count: 2,
                    scope: SequenceScope::default(),
                    context: SemanticContext {
                        parent: node("root"),
                        cursor: ProjectFrame(3),
                        selected_child: None,
                        visual_selection: None,
                    },
                })
            }
            "copy" => capture(&current),
            _ => unreachable!(),
        };
        harness.service.submit(pending).unwrap();
        until(|| !harness.service.is_busy());
        let before = durable(&path);
        let mut remote = save(&current, program(vec![cut(1)]), false);
        let Operation::Execute { command, .. } = &mut remote else {
            unreachable!()
        };
        let ShortOperation::Macro { request } = command.as_mut() else {
            unreachable!()
        };
        request.expected_bank_version = before.1.version;
        let mut client = client(&path);
        let error = live_project::request(&mut client, remote.clone()).unwrap_err();
        assert_eq!(error.code, "HostBusy", "unread {kind}");
        assert!(error.committed_revision.is_none());
        assert_eq!(durable(&path), before);
        let update = harness
            .service
            .take_update()
            .expect("native continuation retained");
        assert!(
            update.committed.is_none(),
            "fixture exercises bank-only or cursor-only replies"
        );
        if kind == "copy" {
            let captured = update.captured_slice.as_ref().unwrap();
            assert_eq!(captured.id.request, 1);
            assert!(captured.result.is_ok());
        } else {
            let receipt = update.macros.as_ref().unwrap().result.as_ref().unwrap();
            assert_eq!(receipt.id.request, if kind == "save" { 1 } else { 2 });
            assert!(receipt.committed().is_none());
            if kind == "motion" {
                let crate::project::macros::Outcome::Executed { cursor, .. } = receipt.outcome
                else {
                    panic!("native motion result")
                };
                assert_eq!(cursor, ProjectFrame(7));
            }
        }
        let Reply::Completed {
            committed_revision,
            committed_registers,
            refresh_error,
            ..
        } = live_project::request(&mut client, remote).unwrap()
        else {
            panic!("remote save after native receipt delivery")
        };
        assert!(committed_revision.is_none() && refresh_error.is_none());
        assert_eq!(
            committed_registers.unwrap().bank_version,
            before.1.version + 1
        );
        shutdown(&harness);
    }
}
