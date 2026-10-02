//! Production cuts and genuine receipts with controlled delivery ordering.

use super::*;
use crate::preview::copied::Content;
use crate::project::ProjectUpdate;
use crate::project::slice::{CaptureRequest, Captured, CutUpdate};
use deadpan_core::SliceCaptureSelection;
use std::sync::Arc;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    for whole in [false, true] {
        paste_round_trip(d, whole)?;
    }
    retained_receipt(d)?;
    stale_workspace_yank(d)?;
    for rejected in [false, true] {
        superseded_receipt(d, rejected)?;
    }
    Ok(())
}

fn paste_round_trip(d: &mut Driver<'_>, whole: bool) -> Result<(), String> {
    let baseline = document(d)?.clone();
    d.command("sequence")?;
    d.key(Key::Escape)?;
    d.chord(&[Key::G, Key::G])?;
    let child = d.app().selected_beat.clone().ok_or("Missing cut beat")?;
    if !whole {
        select(d, 20, 30, false)?;
    }
    let revision = d.revision();
    d.key(Key::D)?;
    if whole {
        d.key(Key::D)?;
    }
    d.changed(&revision)?;
    let copied = edited(d)?;
    let selection = if whole {
        SliceCaptureSelection::Child { node: child }
    } else {
        SliceCaptureSelection::Range {
            range: range(20, 30),
        }
    };
    let duration = if whole { 120 } else { 10 };
    d.check(
        "Visual d and whole-beat dd copy their exact pre-cut contents after one saved deletion",
        copied.slice().selection() == &selection
            && copied.slice().revision_id().as_str() == revision
            && copied.slice().duration().frames() == duration
            && d.app().sequence_length() == 120 - duration as u64
            && !d.app().copied.is_pending(),
        json!({"selection":selection,"duration":duration,"source_revision":revision}),
        d.snapshot(),
    )?;
    if !whole {
        d.capture("Saved Visual cut with its historical Edited register")?;
    }
    let cut = document(d)?.clone();
    let revision = d.revision();
    d.key(Key::P)?;
    d.changed(&revision)?;
    d.check(
        "Production p pastes the historical cut exactly once, including into an empty root",
        d.app().sequence_length() == 120 && same_edited(d, &copied),
        json!({"whole_beat":whole,"frames":120,"same_register":true}),
        d.snapshot(),
    )?;
    picture(d, if whole { 0 } else { 20 })?;
    if !whole {
        d.capture("Historical p restores the cut once with its exact first picture")?;
    }
    undo(d, &cut)?;
    undo(d, &baseline)?;
    d.check(
        "One Undo removes the paste and another restores the cut without clearing its register",
        !d.app().workspace.as_ref().unwrap().can_undo && same_edited(d, &copied),
        json!({"baseline":true,"historical_copy_retained":true}),
        d.snapshot(),
    )
}

fn retained_receipt(d: &mut Driver<'_>) -> Result<(), String> {
    original(d, 5)?;
    let baseline = document(d)?.clone();
    let held = cut_held(d)?;
    let cut = receipt(&held)?.clone();
    let saved = cut.result.as_ref().map_err(Clone::clone)?;
    let revision = saved.committed.revision.clone();
    let selected = d.app().edit_range.clone();
    d.check(
        "A held durable cut leaves the old visible document and accepted register intact",
        *document(d)? == baseline && d.app().copied.is_pending() && is_original(d, 5),
        json!({"visible_revision":baseline.revision_id(),"durable_revision":revision,"pending":true}),
        d.snapshot(),
    )?;
    // An ordinary query replaces the single service update slot. Its update
    // must retain the cut receipt independently of the query's capture reply.
    d.app()
        .service
        .submit(ProjectRequest::CaptureEditSlice(cut.request.clone()))?;
    let queried = take_update(d, |update| {
        update
            .captured_slice
            .as_ref()
            .is_some_and(|capture| capture.id == cut.request.id)
    })?;
    let repeated = receipt(&queried)?;
    d.check(
        "A rejected stale capture write retains the exact successful cut receipt",
        repeated.request == cut.request
            && queried
                .captured_slice
                .as_ref()
                .is_some_and(|capture| capture.result.is_err())
            && repeated.result.as_ref().is_ok_and(|next| {
                next.committed.revision == revision && Arc::ptr_eq(&next.copied, &saved.copied)
            })
            && queried
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.document.revision_id() == &revision),
        json!({"cut_receipt_retained":true,"durable_revision":revision}),
        json!(format!("{repeated:?}")),
    )?;
    d.app_mut().receive_cut(Some(repeated.clone()));
    d.step(
        "Deliver the saved cut register receipt before its refreshed workspace",
        false,
    )?;
    d.check(
        "A cut receipt alone cannot replace the authoritative bank or consume stale selection",
        is_original(d, 5) && *document(d)? == baseline && d.app().edit_range == selected,
        json!({"bank_awaits_snapshot":true,"old_document_and_selection_retained":true}),
        d.snapshot(),
    )?;
    release(d, queried, baseline.revision_id().as_str())?;
    d.check(
        "The matching workspace applies the saved cut join exactly once",
        document(d)?.revision_id() == &revision
            && d.app().sequence_length() == 110
            && d.app().sequence_cursor == 40
            && d.app().selected_edit_range().is_none(),
        json!({"frames":110,"join":40,"revision":revision}),
        d.snapshot(),
    )?;
    d.key(Key::L)?;
    let cursor = d.app().sequence_cursor;
    d.app_mut().receive_cut(Some(cut.clone()));
    d.step(
        "Replay a consumed genuine cut receipt after navigation",
        false,
    )?;
    d.check(
        "Duplicate receipt delivery does not move the cursor or replace the accepted copy",
        d.app().sequence_cursor == cursor && same_edited(d, &saved.copied),
        json!({"cursor":cursor,"same_copy":true}),
        d.snapshot(),
    )?;
    undo(d, &baseline)?;
    failed_request(d, cut.request, &saved.copied)
}

fn stale_workspace_yank(d: &mut Driver<'_>) -> Result<(), String> {
    original(d, 5)?;
    let baseline = document(d)?.clone();
    let visible = d.app().workspace.clone();
    let held = cut_held(d)?;
    let cut = receipt(&held)?;
    let selection = d.app().edit_range.clone();
    let cursor = d.app().sequence_cursor;
    // This newer production yank supersedes the pending Cut register intent.
    // Its stale write must fail because the writer has already saved the cut.
    d.key(Key::Y)?;
    let queried = take_update(d, |update| {
        update.captured_slice.as_ref().is_some_and(|capture| {
            capture.id != cut.request.id && capture.id.source_revision == *baseline.revision_id()
        })
    })?;
    let rejected = queried
        .captured_slice
        .as_ref()
        .ok_or("Missing newer yank reply")?
        .result
        .as_ref()
        .err()
        .ok_or("A stale yank unexpectedly wrote the register")?
        .clone();
    let copied = cut.result.as_ref().map_err(Clone::clone)?.copied.clone();
    let warning = "Cut saved and copied, but the preview could not refresh: simulated replay refresh failure. Reopen this project before editing or undoing.";
    // The saved cut and rejected yank response are genuine. Only the stale
    // workspace delivery and refresh warning are injected here; service tests
    // exercise the actual refresh fault. No generic commit or Cut register
    // reply remains to protect the old selection on this delivery.
    let mut stale = queried;
    stale.workspace = visible;
    stale.committed = None;
    stale.cut_slice = None;
    stale.message = Some(format!("Stale yank refused: {rejected}"));
    stale
        .saved_cut
        .as_mut()
        .ok_or("The query lost the durable saved cut")?
        .refresh_error = Some(warning.into());
    d.app_mut().feedback.release_project_update = Some(stale);
    d.step(
        "Simulate stale workspace delivery after a saved cut and superseding production yank",
        false,
    )?;
    d.check(
        "Independent saved-cut state preserves the stale Visual selection and refresh warning after a newer yank",
        *document(d)? == baseline
            && d.app().edit_range == selection
            && d.app().edit_range.active
            && d.app().sequence_cursor == cursor
            && same_edited(d, &copied)
            && !d.app().copied.is_pending()
            && d.app().message.as_deref() == Some(warning),
        json!({"simulated_refresh_failure":true,"generic_commit":null,"cut_reply":null,"active_range":[40,50],"cursor":cursor,"warning":warning}),
        d.snapshot(),
    )?;
    // The original held cut update owns the matching refreshed workspace.
    release(d, held, baseline.revision_id().as_str())?;
    undo(d, &baseline)?;
    // A restored document has a fresh revision. The retained saved-cut receipt
    // must not make a later ordinary yank behave as an unrefreshed cut.
    select(d, 2, 4, false)?;
    let revision = d.revision();
    d.key(Key::Y)?;
    d.wait_for(
        "A yank after Undo completes against its fresh revision",
        |app| !app.service.is_busy() && !app.copied.is_pending(),
    )?;
    d.check(
        "A fresh Undo revision lets the next yank finish Visual without reviving the old cut warning",
        d.revision() == revision
            && !d.app().edit_range.active
            && d.app().selected_edit_range() == Some(range(2, 4))
            && matches!(d.app().copied.content(), Some(Content::Edited(copied))
                if copied.slice().revision_id().as_str() == revision
                    && copied.slice().range() == range(2, 4))
            && d.app().message.as_deref().is_some_and(|message| message.starts_with("Edit slice copied.")),
        json!({"fresh_revision":revision,"finished_range":[2,4],"warning_cleared":true}),
        d.snapshot(),
    )
}

fn superseded_receipt(d: &mut Driver<'_>, rejected: bool) -> Result<(), String> {
    original(d, 5)?;
    let baseline = document(d)?.clone();
    let held = cut_held(d)?;
    let cut = receipt(&held)?.clone();
    let saved = cut.result.as_ref().map_err(Clone::clone)?.clone();
    d.command("source")?;
    let delivery = if rejected {
        d.command("delete")?;
        held
    } else {
        d.key(Key::Escape)?;
        d.chord(&[Key::G, Key::G, Key::V])?;
        motion(d, 7, true)?;
        d.key(Key::Y)?;
        let update = take_update(d, |update| {
            update
                .captured_original
                .as_ref()
                .is_some_and(|capture| capture.id.source_revision == *baseline.revision_id())
        })?;
        d.check(
            "Original yank against a withheld pre-cut revision is rejected by the writer",
            update
                .captured_original
                .as_ref()
                .is_some_and(|capture| capture.result.is_err()),
            json!("stale write refused"),
            json!(format!("{:?}", update.captured_original)),
        )?;
        update
    };
    let error = d.app().error.clone();
    d.check(
        "A newer input supersedes the old cut confirmation while its bank is still withheld",
        is_original(d, 5)
            && (rejected || d.app().copied.is_pending())
            && (!rejected
                || error
                    .as_deref()
                    .is_some_and(|error| error.contains("Return to Your edit"))),
        json!({"rejected_local_cut":rejected,"visible_original":[0,5]}),
        d.snapshot(),
    )?;
    d.app_mut().receive_cut(Some(cut));
    d.step(
        "Deliver the old cut receipt after a newer register intent",
        false,
    )?;
    d.check(
        "The old cut confirmation does not replace the bank or clear newer input feedback",
        is_original(d, 5) && d.app().error == error,
        json!({"visible_original":[0,5],"newer_error":error}),
        d.snapshot(),
    )?;
    release(d, delivery, baseline.revision_id().as_str())?;
    d.check(
        "Failed newer input preserves the earlier durable cut and installs its saved bank",
        document(d)?.revision_id() == &saved.committed.revision
            && d.app().sequence_length() == 110
            && same_edited(d, &saved.copied)
            && !d.app().copied.is_pending(),
        json!({"durable_revision":saved.committed.revision,"frames":110,"copied_range":[40,50]}),
        d.snapshot(),
    )?;
    undo(d, &baseline)
}

fn failed_request(
    d: &mut Driver<'_>,
    mut request: CaptureRequest,
    accepted: &Arc<Captured>,
) -> Result<(), String> {
    let baseline = document(d)?.clone();
    let can_undo = d.app().workspace.as_ref().unwrap().can_undo;
    let can_redo = d.app().workspace.as_ref().unwrap().can_redo;
    request.id.request = d.app_mut().next_serial().ok_or("No request identity")?;
    // The receipt's pre-cut revision is now historical after Undo. A new cut
    // against it must fail instead of silently retargeting the current document.
    d.app_mut().copied.expect_cut(request.clone());
    d.app()
        .service
        .submit(ProjectRequest::CutEditSlice(request))?;
    d.wait_for(
        "The service rejects a cut against its old source revision",
        |app| !app.service.is_busy() && !app.copied.is_pending() && app.error.is_some(),
    )?;
    d.check(
        "A failed service cut preserves accepted contents and authored history",
        *document(d)? == baseline
            && same_edited(d, accepted)
            && d.app().workspace.as_ref().unwrap().can_undo == can_undo
            && d.app().workspace.as_ref().unwrap().can_redo == can_redo,
        json!({"unchanged":true,"can_undo":can_undo,"can_redo":can_redo}),
        d.snapshot(),
    )
}

fn cut_held(d: &mut Driver<'_>) -> Result<ProjectUpdate, String> {
    d.command("sequence")?;
    select(d, 40, 50, false)?;
    let revision = document(d)?.revision_id().clone();
    d.app_mut().feedback.hold_project_updates = true;
    d.key(Key::D)?;
    take_update(d, |update| {
        update
            .cut_slice
            .as_ref()
            .is_some_and(|cut| cut.request.id.source_revision == revision)
    })
}

fn take_update(
    d: &mut Driver<'_>,
    matches: impl Fn(&ProjectUpdate) -> bool,
) -> Result<ProjectUpdate, String> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(update) = d.app().service.take_update()
            && matches(&update)
        {
            return Ok(update);
        }
        if Instant::now() >= deadline {
            return Err("The genuine cut service update did not arrive".into());
        }
        d.step(
            "Withhold project updates while the cut service completes",
            false,
        )?;
        d.wake
            .wait_until((Instant::now() + Duration::from_millis(16)).min(deadline));
    }
}

fn release(d: &mut Driver<'_>, update: ProjectUpdate, revision: &str) -> Result<(), String> {
    d.app_mut().feedback.release_project_update = Some(update);
    d.app_mut().feedback.hold_project_updates = false;
    d.changed(revision)
}

fn receipt(update: &ProjectUpdate) -> Result<&CutUpdate, String> {
    update
        .cut_slice
        .as_ref()
        .ok_or_else(|| "Missing genuine cut receipt".into())
}

fn edited(d: &Driver<'_>) -> Result<Arc<Captured>, String> {
    match d.app().copied.content() {
        Some(Content::Edited(copied)) => Ok(copied.clone()),
        _ => Err("The saved cut has no accepted edited copy".into()),
    }
}

fn same_edited(d: &Driver<'_>, expected: &Arc<Captured>) -> bool {
    matches!(d.app().copied.content(), Some(Content::Edited(copied)) if Arc::ptr_eq(copied, expected))
}

fn original(d: &mut Driver<'_>, frames: u64) -> Result<(), String> {
    d.command("source")?;
    d.key(Key::Escape)?;
    d.chord(&[Key::G, Key::G, Key::V])?;
    motion(d, frames, true)?;
    d.key(Key::Y)?;
    d.wait_for("Original copy is durable before the next cut", |app| {
        !app.service.is_busy() && !app.copied.is_pending()
    })
}

fn is_original(d: &Driver<'_>, frames: u64) -> bool {
    d.app()
        .copied
        .original()
        .is_some_and(|copied| copied.ordinals == (0..frames))
}
