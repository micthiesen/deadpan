//! Only delivery order is injected; proposals and receipts come from the service.

use super::*;
use crate::project::splice::SpliceCommitUpdate;
use deadpan_core::ProjectDocument;

pub(super) fn commit(
    d: &mut Driver<'_>,
    copied: &Arc<Captured>,
    saved: &ProjectDocument,
) -> Result<(), String> {
    let exact = prepared(d)?;
    let id = draft(d)?.proposal_for_check().id.clone();
    let entry = editor(d);
    let selection = d.app().edit_range.clone();
    let revision = d.revision();
    d.app_mut().feedback.hold_project_updates = true;
    d.key(Key::Enter)?;
    let held = take_update(d, &id)?;
    let receipt = held
        .splice_commit
        .clone()
        .ok_or("Held Move has no receipt")?;
    let result = receipt.result.as_ref().map_err(Clone::clone)?;
    d.check(
        "The durable Move receipt retains the entire result and original session",
        result.revision == *exact.snapshot.document.revision_id()
            && result.range_selection.as_ref().is_some_and(|range| {
                range.range == exact.range
                    && range.parent == exact.parent
                    && range.session == id.session
                    && range.project == id.project
            }),
        json!({"range":[50,60],"session":id.session,"parent":exact.parent}),
        json!(format!("{result:?}")),
    )?;
    d.app_mut().receive_splice(None, Some(receipt.clone()));
    d.step(
        "Deliver the real Move receipt before its refreshed workspace",
        true,
    )?;
    d.check(
        "A receipt closes Move without consuming the old visible selection or cursor",
        d.app().splice.is_none()
            && d.revision() == revision
            && d.app().edit_range == selection
            && editor(d) == entry,
        json!({"old_revision":revision,"selection_retained":true,"entry":entry}),
        d.snapshot(),
    )?;
    d.app()
        .service
        .submit(ProjectRequest::CaptureEditSlice(edited::capture_request(
            copied,
        )))?;
    d.app_mut().feedback.hold_project_updates = false;
    d.changed(&revision)?;
    committed(d, &exact)?;
    d.capture("Move receipt selects the complete result after matching workspace delivery")?;

    select(d, 2, 4)?;
    d.key(Key::V)?;
    let navigation = editor(d);
    let selection = d.app().edit_range.clone();
    d.app()
        .service
        .submit(ProjectRequest::CaptureEditSlice(edited::capture_request(
            copied,
        )))?;
    d.wait_for(
        "A history-neutral query republishes the retained Move receipt",
        |app| !app.service.is_busy(),
    )?;
    d.settled()?;
    d.check(
        "Duplicate Move completion cannot reselect after manual range navigation",
        editor(d) == navigation
            && d.app().edit_range == selection
            && d.app().selected_edit_range() == Some(range(2, 4)?),
        json!({"manual_range":[2,4],"editor":navigation}),
        d.snapshot(),
    )?;
    new_session(d, &receipt, &exact, held)?;
    undo(d, saved)
}

pub(super) fn take_update(
    d: &mut Driver<'_>,
    id: &crate::project::splice::ProposalId,
) -> Result<crate::project::ProjectUpdate, String> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(update) = d.app().service.take_update()
            && update
                .splice_commit
                .as_ref()
                .is_some_and(|receipt| &receipt.id == id)
        {
            return Ok(update);
        }
        if Instant::now() >= deadline {
            return Err("Real Move receipt did not arrive".into());
        }
        d.step(
            "Hold the Move workspace until its durable receipt arrives",
            false,
        )?;
        d.wake
            .wait_until((Instant::now() + Duration::from_millis(16)).min(deadline));
    }
}

fn new_session(
    d: &mut Driver<'_>,
    old: &SpliceCommitUpdate,
    exact: &Arc<Prepared>,
    mut held: crate::project::ProjectUpdate,
) -> Result<(), String> {
    let path = d
        .app()
        .workspace
        .as_ref()
        .ok_or("Missing Move workspace")?
        .path
        .clone();
    d.app().service.submit(ProjectRequest::Close)?;
    d.wait_for(
        "Close the Move project before testing old-session replies",
        |app| app.workspace.is_none() && !app.service.is_busy(),
    )?;
    d.app().service.submit(ProjectRequest::Open(path))?;
    d.wait_for("Reopen the saved Move in a new project session", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.session != old.id.session)
            && !app.service.is_busy()
    })?;
    d.check(
        "A new project session clears the previous copied register",
        d.app().copied.content().is_none(),
        json!("register empty"),
        d.snapshot(),
    )?;
    d.command("sequence")?;
    copy(d, 2, 4)?;
    open(d, 90)?;
    let id = draft(d)?.proposal_for_check().id.clone();
    let current = prepared(d)?;
    let editor = editor(d);
    // Replay the real old receipt alongside the now-current workspace. The
    // document revision matches, but its range belongs to the earlier session.
    held.workspace = d.app().workspace.clone();
    held.splice = None;
    held.captured_slice = None;
    held.message = None;
    d.app_mut().feedback.release_project_update = Some(held);
    d.step("Deliver an old generic range receipt alongside a new-session workspace of the same revision", true)?;
    d.check(
        "Generic Move selection rejects a foreign session even when its revision matches",
        super::editor(d) == editor
            && d.app().selected_edit_range().is_none()
            && draft(d)?.proposal_for_check().id == id,
        json!({"new_session":id.session,"old_session":old.id.session,"editor":editor}),
        d.snapshot(),
    )?;
    d.app_mut().receive_splice(
        Some(ProposalUpdate {
            id: old.id.clone(),
            source_view: None,
            result: Ok(exact.clone()),
        }),
        Some(old.clone()),
    );
    d.step(
        "Deliver prior-session Move proposal and receipt after a new draft is ready",
        true,
    )?;
    d.check(
        "Old-session Move replies cannot close, retarget or replace the new placement",
        draft(d)?.proposal_for_check().id == id
            && Arc::ptr_eq(&prepared(d)?, &current)
            && super::editor(d) == editor
            && draft(d)?.ready_for_check(),
        json!({"new_session":id.session,"old_session":old.id.session,"new_draft_retained":true}),
        state(d),
    )?;
    cancel(d)
}
