//! A real second project service owns the writer while the GUI inspects it.

use std::cell::RefCell;
use std::path::PathBuf;

use super::*;
use crate::project::ProjectUpdate;

enum ExpectedUpdate {
    Open(PathBuf),
    History {
        before: deadpan_core::RevisionId,
        message: &'static str,
    },
    Closed,
}

fn command(
    d: &mut Driver<'_>,
    owner: &ProjectService,
    request: ProjectRequest,
) -> Result<ProjectUpdate, String> {
    let expected = match &request {
        ProjectRequest::Open(path) => {
            ExpectedUpdate::Open(path.canonicalize().map_err(|error| error.to_string())?)
        }
        ProjectRequest::Undo { expected_revision } => ExpectedUpdate::History {
            before: expected_revision.clone(),
            message: "Undo saved",
        },
        ProjectRequest::Redo { expected_revision } => ExpectedUpdate::History {
            before: expected_revision.clone(),
            message: "Redo saved",
        },
        ProjectRequest::Close => ExpectedUpdate::Closed,
        _ => return Err("Unsupported second-owner replay command".into()),
    };
    // Busy clears before publication, and take_update uses try_lock. Wait for
    // the actual matching receipt instead of treating idle as its delivery.
    let delivered = RefCell::new(None);
    let _ = owner.take_update();
    owner.submit(request).map_err(|error| error.to_string())?;
    d.wait_for("Other window's project command completes", |_| {
        if delivered.borrow().is_some() {
            return true;
        }
        if owner.is_busy() {
            return false;
        }
        let Some(update) = owner.take_update() else {
            return false;
        };
        let matches = update.error.is_some()
            || match &expected {
                ExpectedUpdate::Open(path) => update
                    .workspace
                    .as_ref()
                    .is_some_and(|workspace| &workspace.path == path),
                ExpectedUpdate::History { before, message } => {
                    update.message.as_deref() == Some(*message)
                        && update
                            .workspace
                            .as_ref()
                            .is_some_and(|workspace| workspace.document.revision_id() != before)
                }
                ExpectedUpdate::Closed => update.workspace.is_none(),
            };
        if matches {
            *delivered.borrow_mut() = Some(update);
        }
        matches
    })?;
    let update = delivered.into_inner().ok_or("Other owner did not reply")?;
    if let Some(error) = &update.error {
        return Err(error.clone());
    }
    Ok(update)
}

pub(super) fn run(d: &mut Driver<'_>, path: &Path) -> Result<(), String> {
    d.app_mut().submit(ProjectRequest::Close);
    d.wait_for("Close before opening in another window", |app| {
        app.workspace.is_none()
            && !app.service.is_busy()
            && !app.storage.backups.owned_workers_active_for_check
    })?;
    let context = d.harness.ctx.clone();
    let owner = ProjectService::new(Arc::new(move || context.request_repaint()))
        .map_err(|error| error.to_string())?;
    let original = command(d, &owner, ProjectRequest::Open(path.into()))?
        .workspace
        .ok_or("Other owner has no workspace")?;
    d.app_mut().submit(ProjectRequest::Open(path.into()));
    d.wait_for(
        "Second window opens the owner's project for inspection",
        |app| {
            app.workspace
                .as_ref()
                .is_some_and(|workspace| workspace.read_only.is_some())
                && !app.service.is_busy()
        },
    )?;
    d.settled()?;
    let header = labels(d)
        .into_iter()
        .find(|label| label.starts_with("Read-only"))
        .unwrap_or_default();
    d.check(
        "Second window explains its fixed read-only view and how to refresh",
        header.contains("Another window")
            && header.contains("Reopen")
            && !labels(d).iter().any(|label| label == "Saved"),
        json!("Read-only with owner and reopen explanation"),
        json!(header),
    )?;
    d.command("sequence")?;
    super::super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    d.settled()?;
    let captured = d.revision();
    d.command("split")?;
    d.wait_for("Read-only edit finishes with a refusal", |app| {
        !app.service.is_busy()
            && app.project_error.as_deref().is_some_and(|error| {
                error.starts_with("Not saved:") && error.contains("Another window")
            })
    })?;
    d.check(
        "Navigation shows real pictures but inspection cannot commit",
        d.revision() == captured && d.app().sequence_cursor == 10,
        json!({"revision":captured,"cursor":10}),
        json!({"revision":d.revision(),"cursor":d.app().sequence_cursor}),
    )?;
    d.capture("Inspecting another window's project")?;
    let undone = command(
        d,
        &owner,
        ProjectRequest::Undo {
            expected_revision: original.document.revision_id().clone(),
        },
    )?
    .workspace
    .ok_or("Owner lost workspace on undo")?;
    d.step("The other owner committed Undo", false)?;
    d.check(
        "An owner edit does not silently replace the inspection snapshot",
        d.revision() == captured && undone.document.revision_id().as_str() != captured,
        json!(captured),
        json!(d.revision()),
    )?;
    d.app_mut().submit(ProjectRequest::Open(path.into()));
    d.changed(&captured)?;
    d.check(
        "Reopening explicitly refreshes inspection while preserving the owner",
        d.revision() == undone.document.revision_id().as_str()
            && d.app()
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.read_only.is_some()),
        json!(undone.document.revision_id().as_str()),
        json!(d.revision()),
    )?;
    let redone = command(
        d,
        &owner,
        ProjectRequest::Redo {
            expected_revision: undone.document.revision_id().clone(),
        },
    )?
    .workspace
    .ok_or("Owner lost workspace on redo")?;
    command(d, &owner, ProjectRequest::Close)?;
    owner.shutdown();
    d.wait_for("Other owner drains its service", |_| {
        owner.is_shutdown_complete()
    })?;
    let before = d.revision();
    d.app_mut().submit(ProjectRequest::Open(path.into()));
    d.changed(&before)?;
    d.check(
        "After the other owner closes, explicit Open acquires a new writable session",
        d.revision() == redone.document.revision_id().as_str()
            && d.app().workspace.as_ref().is_some_and(|workspace| workspace.read_only.is_none()),
        json!("Writable at the owner's final saved edit"),
        json!({"revision":d.revision(),"read_only":d.app().workspace.as_ref().and_then(|workspace| workspace.read_only.as_deref())}),
    )?;
    Ok(())
}
