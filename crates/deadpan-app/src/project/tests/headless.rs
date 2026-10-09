//! Real authenticated socket requests against the native owning service.

mod macros;
mod operations;
mod preparation;
mod takes;

use super::*;
use deadpan_cli::host::Client;
use deadpan_cli::live_project::{
    self, HistoryDirection, Operation, Reply, Request, ShortOperation,
};
use deadpan_cli::render::{RenderContext, RenderOperation, RenderRequest, WorkflowTarget};
use deadpan_jobs::{AttemptId, CancellationToken, RequestId};
use serde_json::Value;

fn opened(harness: &Harness, path: &Path) -> Arc<Workspace> {
    drop(seed_holds(path, &["a"]));
    let update = command(&harness.service, ProjectRequest::Open(path.into()));
    assert!(update.error.is_none(), "{:?}", update.error);
    update.workspace.unwrap()
}

fn client(path: &Path) -> Client {
    Client::discover(path)
        .unwrap()
        .expect("native owner discovery")
}

fn change(workspace: &Workspace, revision: &str, duration: i64) -> Operation {
    Operation::Execute {
        project_id: workspace.document.project_id().clone(),
        command: Box::new(ShortOperation::Edit {
            request: Box::new(CommandRequest {
                project_id: workspace.document.project_id().clone(),
                expected_revision: workspace.document.revision_id().clone(),
                new_revision: RevisionId::new(revision).unwrap(),
                command: Command::SetHoldDuration {
                    node: node("a"),
                    duration: FrameDuration::new(duration).unwrap(),
                },
            }),
            dry_run: false,
        }),
    }
}

fn committed(reply: Reply, expected: &str) -> Value {
    let Reply::Completed {
        output,
        committed_revision,
        refresh_error,
        ..
    } = reply
    else {
        panic!("expected a completed command receipt")
    };
    assert_eq!(
        committed_revision.as_ref().map(RevisionId::as_str),
        Some(expected)
    );
    assert!(refresh_error.is_none(), "{refresh_error:?}");
    assert_eq!(output["committed"], true);
    assert_eq!(output["outcome"]["revision_id"], expected);
    output
}

fn until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !predicate() {
        assert!(Instant::now() < deadline, "service completion timed out");
        std::thread::yield_now();
    }
}

fn shutdown(harness: &Harness) {
    harness.service.shutdown();
    until(|| harness.service.is_shutdown_complete());
}

/// Hold exactly the admission bit acquired by ProjectService::submit. This
/// controlled boundary lets the service receive IPC while a native admission
/// remains pending, without relying on actor scheduling or sleeps.
struct Busy<'a>(&'a ProjectService);
impl<'a> Busy<'a> {
    fn new(service: &'a ProjectService) -> Self {
        assert!(!service.shared.busy.swap(true, Ordering::AcqRel));
        Self(service)
    }
}
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.shared.busy.store(false, Ordering::Release);
    }
}
struct PausedRender<'a>(&'a ProjectService);
impl<'a> PausedRender<'a> {
    fn new(service: &'a ProjectService) -> Self {
        service
            .shared
            .render_poll_paused
            .store(true, Ordering::Release);
        Self(service)
    }
}
impl Drop for PausedRender<'_> {
    fn drop(&mut self) {
        self.0
            .shared
            .render_poll_paused
            .store(false, Ordering::Release);
    }
}

#[test]
fn authenticated_edit_preserves_exact_receipt_and_publishes_native_workspace() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("live-edit.deadpan");
    let harness = Harness::new();
    let initial = opened(&harness, &path);
    let mut client = client(&path);
    committed(
        live_project::request(&mut client, change(&initial, "remote-edit", 17)).unwrap(),
        "remote-edit",
    );

    // Inspect replies use their own socket channel and cannot consume the
    // refreshed workspace waiting for the native UI.
    let (context, preview) = live_project::inspect(&mut client).unwrap();
    assert_eq!(context.revision_id.as_str(), "remote-edit");
    assert!(!preview);
    let update = harness
        .service
        .take_update()
        .expect("UI refresh remains available");
    assert!(update.error.is_none());
    assert!(
        update.committed.is_none(),
        "remote edits must not invent native selection context"
    );
    let current = update.workspace.unwrap();
    assert_eq!(current.session, initial.session);
    assert_eq!(current.document.revision_id().as_str(), "remote-edit");
    assert_eq!(current.plan.node_duration(&node("a")).unwrap().frames(), 17);
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)
            .unwrap()
            .snapshot()
            .unwrap(),
        *current.document
    );
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadWrite),
        Err(deadpan_store::StoreError::AlreadyOpen)
    ));

    committed(
        live_project::request(
            &mut client,
            Operation::Execute {
                project_id: current.document.project_id().clone(),
                command: Box::new(ShortOperation::History {
                    direction: HistoryDirection::Undo,
                    expected_revision: current.document.revision_id().clone(),
                    new_revision: RevisionId::new("remote-undo").unwrap(),
                    dry_run: false,
                }),
            },
        )
        .unwrap(),
        "remote-undo",
    );
    let undone = harness.service.take_update().unwrap().workspace.unwrap();
    assert_eq!(undone.document.nodes(), initial.document.nodes());
    assert_eq!(undone.document.revision_id().as_str(), "remote-undo");
    shutdown(&harness);
}

#[test]
fn stale_and_foreign_requests_do_not_mutate_or_retarget() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("live-stale.deadpan");
    let harness = Harness::new();
    let initial = opened(&harness, &path);
    let mut client = client(&path);
    committed(
        live_project::request(&mut client, change(&initial, "current", 12)).unwrap(),
        "current",
    );
    let before = ProjectStore::open(&path, AccessMode::ReadOnly)
        .unwrap()
        .snapshot()
        .unwrap();
    let stale =
        live_project::request(&mut client, change(&initial, "must-not-exist", 19)).unwrap_err();
    assert_eq!(stale.code, "RevisionConflict");
    assert_eq!(
        stale.current_revision,
        Some(RevisionId::new("current").unwrap())
    );
    assert!(stale.committed_revision.is_none());
    let mut foreign = change(&initial, "foreign", 20);
    let Operation::Execute { project_id, .. } = &mut foreign else {
        unreachable!()
    };
    *project_id = ProjectId::new("another-project").unwrap();
    assert_eq!(
        live_project::request(&mut client, foreign)
            .unwrap_err()
            .code,
        "HostProjectChanged"
    );
    let after = ProjectStore::open(&path, AccessMode::ReadOnly)
        .unwrap()
        .snapshot()
        .unwrap();
    assert_eq!(before, after);
    assert!(!harness.service.is_busy());
    shutdown(&harness);
}

#[test]
fn dry_run_and_malformed_requests_leave_native_state_unchanged() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("live-protocol.deadpan");
    let harness = Harness::new();
    let initial = opened(&harness, &path);
    let mut client = client(&path);
    let mut preview = change(&initial, "preview-only", 15);
    let Operation::Execute { command, .. } = &mut preview else {
        unreachable!()
    };
    let ShortOperation::Edit { dry_run, .. } = command.as_mut() else {
        unreachable!()
    };
    *dry_run = true;
    let Reply::Completed {
        output,
        committed_revision,
        refresh_error,
        ..
    } = live_project::request(&mut client, preview).unwrap()
    else {
        panic!("preview reply")
    };
    assert_eq!(output["committed"], false);
    assert!(committed_revision.is_none() && refresh_error.is_none());
    for (payload, code) in [
        (
            serde_json::json!({"schema_version":99,"operation":{"operation":"inspect"}}),
            "HostProtocolUnsupported",
        ),
        (
            serde_json::json!({"schema_version":1,"operation":{"operation":"inspect"},"extra":true}),
            "HostProtocolInvalid",
        ),
        (
            serde_json::json!({"schema_version":1,"operation":{"operation":"shell","argv":["touch","forbidden"]}}),
            "HostProtocolInvalid",
        ),
        (Value::Null, "HostProtocolInvalid"),
    ] {
        let value = client.request(payload).unwrap();
        let Reply::Failed { error } = serde_json::from_value(value).unwrap() else {
            panic!("malformed request accepted")
        };
        assert_eq!(error.code, code);
    }
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)
            .unwrap()
            .snapshot()
            .unwrap(),
        *initial.document
    );
    assert!(
        harness.service.take_update().is_none(),
        "read-only and malformed requests changed UI feedback"
    );
    assert!(!harness.service.is_busy());
    shutdown(&harness);
}

#[test]
fn ipc_cannot_release_an_existing_native_admission() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("live-busy.deadpan");
    let harness = Harness::new();
    let initial = opened(&harness, &path);
    let mut client = client(&path);
    let busy = Busy::new(&harness.service);
    assert_eq!(
        live_project::request(&mut client, change(&initial, "rejected", 15))
            .unwrap_err()
            .code,
        "HostBusy"
    );
    assert!(harness.service.is_busy());
    assert_eq!(
        live_project::inspect(&mut client).unwrap().0.revision_id,
        *initial.document.revision_id()
    );
    assert!(
        harness.service.is_busy(),
        "inspection released a native command's admission"
    );
    drop(busy);
    committed(
        live_project::request(&mut client, change(&initial, "accepted", 15)).unwrap(),
        "accepted",
    );
    shutdown(&harness);
}

#[test]
fn socket_replies_do_not_consume_local_edit_selection_receipts() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("live-mailbox.deadpan");
    let harness = Harness::new();
    let initial = opened(&harness, &path);
    harness
        .service
        .submit(edit_request(
            &initial,
            ProjectEdit::HoldDuration {
                node: node("a"),
                duration: FrameDuration::new(14).unwrap(),
            },
        ))
        .unwrap();
    until(|| !harness.service.is_busy());
    let mut client = client(&path);
    let (context, _) = live_project::inspect(&mut client).unwrap();
    let pending = harness
        .service
        .take_update()
        .expect("native mailbox preserved");
    assert!(pending.error.is_none(), "{:?}", pending.error);
    assert_eq!(
        pending.committed.as_ref().unwrap().revision,
        context.revision_id
    );
    assert_eq!(pending.committed.unwrap().selected_node, Some(node("a")));
    assert_eq!(
        pending.workspace.unwrap().document.revision_id(),
        &context.revision_id
    );
    shutdown(&harness);
}

#[test]
fn remote_mutation_waits_until_native_ui_consumes_its_commit_receipt() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("live-pending-receipt.deadpan");
    let harness = Harness::new();
    let initial = opened(&harness, &path);
    harness
        .service
        .submit(edit_request(
            &initial,
            ProjectEdit::HoldDuration {
                node: node("a"),
                duration: FrameDuration::new(14).unwrap(),
            },
        ))
        .unwrap();
    until(|| !harness.service.is_busy());
    let mut client = client(&path);
    let (context, _) = live_project::inspect(&mut client).unwrap();
    let before = ProjectStore::open(&path, AccessMode::ReadOnly)
        .unwrap()
        .snapshot()
        .unwrap();
    assert_eq!(before.revision_id(), &context.revision_id);
    let mut remote = change(&initial, "remote-after-local", 19);
    let Operation::Execute { command, .. } = &mut remote else {
        unreachable!()
    };
    let ShortOperation::Edit { request, .. } = command.as_mut() else {
        unreachable!()
    };
    request.expected_revision = context.revision_id.clone();
    assert_eq!(
        live_project::request(&mut client, remote.clone())
            .unwrap_err()
            .code,
        "HostBusy"
    );
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)
            .unwrap()
            .snapshot()
            .unwrap(),
        before
    );
    let pending = harness
        .service
        .take_update()
        .expect("local commit still waiting for its UI");
    assert_eq!(
        pending.committed.as_ref().unwrap().revision,
        context.revision_id
    );
    assert_eq!(pending.committed.unwrap().selected_node, Some(node("a")));
    assert_eq!(*pending.workspace.unwrap().document, before);
    committed(
        live_project::request(&mut client, remote).unwrap(),
        "remote-after-local",
    );
    let refreshed = harness.service.take_update().unwrap().workspace.unwrap();
    assert_eq!(
        refreshed.document.revision_id().as_str(),
        "remote-after-local"
    );
    assert_eq!(
        refreshed.plan.node_duration(&node("a")).unwrap().frames(),
        19
    );
    shutdown(&harness);
}

#[test]
fn old_clients_fail_after_close_reopen_and_same_project_id_switch() {
    let scratch = tempfile::tempdir().unwrap();
    let first_path = scratch.path().join("first.deadpan");
    let second_path = scratch.path().join("second.deadpan");
    let harness = Harness::new();
    let first = opened(&harness, &first_path);
    let mut old = client(&first_path);
    let old_owner = old.owner_id();
    let closed = command(&harness.service, ProjectRequest::Close);
    assert!(closed.workspace.is_none());
    assert_eq!(
        old.request(serde_json::to_value(Request::new(Operation::Inspect)).unwrap())
            .unwrap_err()
            .code,
        "HostOwnerChanged"
    );
    let reopened = command(&harness.service, ProjectRequest::Open(first_path.clone()))
        .workspace
        .unwrap();
    assert_ne!(reopened.session, first.session);
    let mut current = client(&first_path);
    assert_ne!(current.owner_id(), old_owner);
    assert!(
        old.request(serde_json::to_value(Request::new(Operation::Inspect)).unwrap())
            .is_err()
    );
    let second = opened(&harness, &second_path);
    assert_eq!(
        second.document.project_id(),
        first.document.project_id(),
        "fixture intentionally reuses authored project ID"
    );
    assert_ne!(second.session, reopened.session);
    assert!(live_project::inspect(&mut current).is_err());
    assert_eq!(
        live_project::inspect(&mut client(&second_path))
            .unwrap()
            .0
            .revision_id,
        *second.document.revision_id()
    );
    shutdown(&harness);
}

#[test]
fn committed_receipt_survives_native_workspace_refresh_failure() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("live-refresh-failure.deadpan");
    let harness = Harness::new();
    let initial = opened(&harness, &path);
    let mut client = client(&path);
    harness
        .service
        .shared
        .host_refresh_failure
        .store(true, Ordering::Release);
    let Reply::Completed {
        output,
        committed_revision,
        refresh_error,
        ..
    } = live_project::request(&mut client, change(&initial, "saved-before-refresh", 18)).unwrap()
    else {
        panic!("commit receipt lost")
    };
    assert_eq!(
        committed_revision,
        Some(RevisionId::new("saved-before-refresh").unwrap())
    );
    assert_eq!(output["outcome"]["revision_id"], "saved-before-refresh");
    assert!(refresh_error.unwrap().contains("Injected failure"));
    let update = harness.service.take_update().unwrap();
    assert!(update.error.is_some());
    assert_eq!(*update.workspace.unwrap().document, *initial.document);
    let durable = ProjectStore::open(&path, AccessMode::ReadOnly)
        .unwrap()
        .snapshot()
        .unwrap();
    assert_eq!(durable.revision_id().as_str(), "saved-before-refresh");
    assert_ne!(durable.nodes(), initial.document.nodes());
    assert_eq!(
        live_project::inspect(&mut client).unwrap().0.revision_id,
        *durable.revision_id()
    );
    shutdown(&harness);
}

fn render_request(workspace: &Workspace, operation: RenderOperation) -> Operation {
    Operation::Render {
        request: RenderRequest {
            schema_version: 1,
            request_id: RequestId::new(uuid::Uuid::new_v4().to_string()).unwrap(),
            context: RenderContext::from_document(&workspace.document),
            operation,
        },
    }
}

#[test]
fn remote_render_refuses_temporary_previews_and_unknown_cancel_targets() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("live-preview.deadpan");
    let harness = Harness::new();
    let initial = opened(&harness, &path);
    let mut client = client(&path);
    harness.service.set_preview_active(true);
    assert!(live_project::inspect(&mut client).unwrap().1);
    assert_eq!(
        live_project::request(
            &mut client,
            render_request(
                &initial,
                RenderOperation::Start {
                    destination: scratch.path().join("preview.mp4")
                }
            )
        )
        .unwrap_err()
        .code,
        "RenderPreviewDecisionRequired"
    );
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly).unwrap();
    assert!(reader.render_jobs(None, 8).unwrap().is_empty());
    assert_eq!(reader.snapshot().unwrap(), *initial.document);
    let target = WorkflowTarget {
        job_id: RequestId::new("absent").unwrap(),
        attempt_id: AttemptId::new("absent").unwrap(),
        cancellation_token: CancellationToken::new("absent").unwrap(),
    };
    let error = live_project::request(
        &mut client,
        render_request(&initial, RenderOperation::Cancel { target }),
    )
    .unwrap_err();
    assert_eq!(
        error.code, "RenderIdentityChanged",
        "cancellation should pass the preview gate and validate its exact target"
    );
    harness.service.set_preview_active(false);
    shutdown(&harness);
}

#[test]
fn remote_render_cancel_is_exact_and_remains_available_during_native_admission() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("live-cancel.deadpan");
    let harness = Harness::new();
    let initial = opened(&harness, &path);
    let mut client = client(&path);
    // Hold real captured preparation before qualification or encoder launch.
    // This test never asks the unit-test executable to act as a media helper.
    let paused = PausedRender::new(&harness.service);
    let Reply::Render { status, finished } = live_project::request(
        &mut client,
        render_request(
            &initial,
            RenderOperation::Start {
                destination: scratch.path().join("cancelled.mp4"),
            },
        ),
    )
    .unwrap() else {
        panic!("render not admitted")
    };
    assert!(!finished);
    let target = status.target.unwrap();
    let mut stale = target.clone();
    stale.cancellation_token = CancellationToken::new("foreign-token").unwrap();
    assert_eq!(
        live_project::request(
            &mut client,
            render_request(&initial, RenderOperation::Cancel { target: stale })
        )
        .unwrap_err()
        .code,
        "RenderIdentityChanged"
    );
    let busy = Busy::new(&harness.service);
    let Reply::Render { status, .. } = live_project::request(
        &mut client,
        render_request(
            &initial,
            RenderOperation::Cancel {
                target: target.clone(),
            },
        ),
    )
    .unwrap() else {
        panic!("cancel reply")
    };
    assert!(status.cancellation_requested);
    assert!(
        harness.service.is_busy(),
        "cancel released someone else's native admission"
    );
    drop(busy);
    drop(paused);
    let mut terminal = None;
    until(|| {
        let reply = live_project::request(
            &mut client,
            Operation::RenderStatus {
                project_id: initial.document.project_id().clone(),
                target: target.clone(),
            },
        )
        .unwrap();
        if let Reply::Render {
            status,
            finished: true,
        } = reply
        {
            terminal = Some(status);
            true
        } else {
            false
        }
    });
    let terminal = terminal.unwrap();
    assert!(terminal.cleanup_confirmed);
    assert_eq!(
        terminal.outcome,
        Some(deadpan_cli::encoded_render::workflow::WorkflowOutcome::Cancelled)
    );
    assert_eq!(terminal.captured_revision, *initial.document.revision_id());
    assert!(!scratch.path().join("cancelled.mp4").exists());
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)
            .unwrap()
            .snapshot()
            .unwrap(),
        *initial.document
    );
    shutdown(&harness);
}

/// The automatic retention check plans and scans off the writer: a remote
/// edit arriving while it plans is admitted and gets its normal receipt, and
/// a check that does nothing publishes no UI update.
#[test]
fn a_remote_edit_during_the_automatic_retention_check_is_not_refused() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("retention-remote.deadpan");
    let harness = Harness::new();
    harness
        .service
        .shared
        .retention_paused
        .store(true, Ordering::Release);
    let initial = opened(&harness, &path);
    until(|| {
        harness
            .service
            .shared
            .retention_waiting
            .load(Ordering::Acquire)
    });
    let mut client = client(&path);
    committed(
        live_project::request(&mut client, change(&initial, "during-check", 9)).unwrap(),
        "during-check",
    );
    let update = harness.service.take_update().expect("the edit's refresh");
    assert_eq!(
        update.workspace.unwrap().document.revision_id().as_str(),
        "during-check"
    );
    harness
        .service
        .shared
        .retention_paused
        .store(false, Ordering::Release);
    // The check finishes having done nothing: no update is published.
    std::thread::sleep(Duration::from_millis(200));
    assert!(harness.service.take_update().is_none());
    // And a later remote edit is still admitted.
    let later = Operation::Execute {
        project_id: initial.document.project_id().clone(),
        command: Box::new(ShortOperation::Edit {
            request: Box::new(CommandRequest {
                project_id: initial.document.project_id().clone(),
                expected_revision: RevisionId::new("during-check").unwrap(),
                new_revision: RevisionId::new("after-check").unwrap(),
                command: Command::SetHoldDuration {
                    node: node("a"),
                    duration: FrameDuration::new(11).unwrap(),
                },
            }),
            dry_run: false,
        }),
    };
    committed(
        live_project::request(&mut client, later).unwrap(),
        "after-check",
    );
    shutdown(&harness);
}
