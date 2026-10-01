//! Edited copies use production keys, historical media and the shared renderer.

use super::*;
use crate::preview::copied::Content;
use crate::project::slice::{CaptureRequest, CaptureUpdate, Captured, CopiedView};
use crate::project::splice::PreparedMedia;
use crate::worker::Work;
use deadpan_core::{FrameRange, NodeKind, ProjectDocument};

/// Run after replacement::run, which restores its fixture and returns to root.
pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.command("sequence")?;
    d.key(Key::Escape)?;
    d.check(
        "Edited copy replay starts in the ordinary root Sequence",
        d.app().sequence_scope.groups().is_empty() && d.app().sequence_length() >= 100,
        json!("root Sequence with the qualified Original"),
        d.snapshot(),
    )?;
    let entry = document(d)?.clone();
    goto(d, 10)?;
    let revision = d.revision();
    d.command("hold 11f")?;
    d.changed(&revision)?;
    let held = document(d)?.clone();
    let source_revision = d.revision();
    d.check(
        "The copied Edit fixture contains an authored silent Hold between source fragments",
        held.nodes().values().any(|node| {
            matches!(&node.kind,
            NodeKind::Hold { recipe } if recipe.duration.frames() == 11
                && recipe.audio == deadpan_core::HoldAudio::Silence)
        }),
        json!("11 authored Hold frames"),
        d.snapshot(),
    )?;

    select(d, 8, 25)?;
    d.check(
        "Production Edit v and counted motion select the exact linked interval",
        d.app().edit_range.active && d.app().selected_edit_range() == Some(range(8, 25)?),
        json!({"range":[8,25],"active":true}),
        d.snapshot(),
    )?;
    d.key(Key::Y)?;
    d.wait_for("The project service returns the exact edited copy", |app| {
        !app.service.is_busy()
            && !app.copied.is_pending()
            && matches!(app.copied.content(), Some(Content::Edited(copied))
                if copied.slice().range().start() == ProjectFrame(8)
                    && copied.slice().range().end() == ProjectFrame(25))
    })?;
    let copied = accepted(d)?;
    d.check(
        "Edited y exits Visual and captures history-neutral owned content from its exact revision",
        !d.app().edit_range.active
            && d.revision() == source_revision
            && copied.slice().revision_id().as_str() == source_revision
            && copied.slice().duration().frames() == 17
            && *document(d)? == held,
        json!({"copied":[8,25],"duration":17,"revision":source_revision}),
        d.snapshot(),
    )?;
    rejected_yank_supersedes(d, &copied)?;
    copied_warning(d, &copied)?;
    rejected_destination(d, &copied, &held)?;
    placement(d, &copied)?;
    for uppercase in [false, true] {
        fast_replacement(d, &copied, uppercase)?;
    }
    delayed_workspace_receipt(d, &copied)?;

    // The remaining edit is the fixture Hold. Its removal must not erase the
    // historical capture or turn its final endpoint into current Edit frame 24.
    undo(d, &entry)?;
    same_register(d, &copied)?;
    goto(d, 60)?;
    d.command("splice")?;
    wait_ready(d)?;
    wait_edited_endpoints(d, 8, 25)?;
    d.key(Key::O)?;
    copied_picture(d, &copied, 8, 25, false, 13)?;
    d.check(
        "A copy retains its Hold and historical endpoint after that Hold is undone",
        copied.slice().revision_id() != document(d)?.revision_id()
            && prepared(d)?.range.duration().frames() == 17
            && document(d)?.nodes() == entry.nodes(),
        json!({"historical_last_source":13,"current_edit_frame":24,"copied_duration":17}),
        d.snapshot(),
    )?;
    cancel(d)?;
    restored(d, &entry)
}

fn rejected_yank_supersedes(d: &mut Driver<'_>, copied: &Arc<Captured>) -> Result<(), String> {
    // Replay the real late service capability. Only delivery timing is injected;
    // the capture and its immutable provenance came from production Edit y.
    let selection = d.app().edit_range.clone();
    d.app_mut().copied.expect(
        CaptureRequest {
            id: copied.id().clone(),
            scope: copied.scope().clone(),
            parent: copied.slice().parent().clone(),
            selection: copied.slice().selection().clone(),
        },
        selection,
    );
    d.click("Current group beat outline pane")?;
    d.key(Key::Tab)?;
    d.check(
        "Native Tab focuses Sounds before a rejected newer yank",
        d.app().pane == Pane::Sounds && d.app().copied.is_pending(),
        json!({"pane":"Sounds","old_copy_pending":true}),
        d.snapshot(),
    )?;
    d.key(Key::Y)?;
    let error = d.app().error.clone();
    d.check(
        "A rejected production y in Sounds supersedes the older pending edited copy",
        !d.app().copied.is_pending() && error.is_some(),
        json!({"pending":false,"error":true}),
        d.snapshot(),
    )?;
    d.app_mut().receive_copied(
        Some(CaptureUpdate {
            id: copied.id().clone(),
            result: Ok(copied.clone()),
        }),
        false,
    );
    d.step(
        "Deliver the old genuine capture after the newer yank was rejected",
        true,
    )?;
    d.check(
        "The late copy success cannot clear the newer rejection or revive pending intent",
        !d.app().copied.is_pending() && d.app().error == error,
        json!({"pending":false,"newer_error":error}),
        d.snapshot(),
    )?;
    same_register(d, copied)?;
    d.click("Current group beat outline pane")
}

fn copied_warning(d: &mut Driver<'_>, copied: &Arc<Captured>) -> Result<(), String> {
    select(d, 8, 25)?;
    let selection = d.app().edit_range.clone();
    let message = d.app().message.clone();
    let warning = "Edited slice saved, but the preview could not refresh. Reopen the project.";
    d.app_mut()
        .copied
        .expect(capture_request(copied), selection.clone());
    d.app_mut().message = Some(warning.into());
    d.app_mut().receive_copied(
        Some(CaptureUpdate {
            id: copied.id().clone(),
            result: Ok(copied.clone()),
        }),
        true,
    );
    d.step(
        "Simulate copy delivery alongside a saved edit awaiting workspace refresh",
        true,
    )?;
    d.check(
        "A matching copy reply preserves the saved warning and active Visual state while refresh is outstanding",
        d.app().message.as_deref() == Some(warning)
            && d.app().edit_range == selection && d.app().edit_range.active
            && !d.app().copied.is_pending(),
        json!({"simulated_saved_warning":warning,"active_selection_retained":true}),
        d.snapshot(),
    )?;
    same_register(d, copied)?;
    d.app_mut().message = message;
    d.key(Key::Escape)
}

fn delayed_workspace_receipt(d: &mut Driver<'_>, copied: &Arc<Captured>) -> Result<(), String> {
    select(d, 40, 50)?;
    let selection = d.app().edit_range.clone();
    let saved = document(d)?.clone();
    let revision = d.revision();
    let entry = editor(d);
    d.command("splice")?;
    wait_ready(d)?;
    d.key(Key::R)?;
    wait_ready(d)?;
    let exact = prepared(d)?;
    let id = draft(d)?.proposal_for_check().id.clone();
    d.app_mut().feedback.hold_project_updates = true;
    d.key(Key::Enter)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    let receipt = loop {
        if let Some(update) = d.app().service.take_update()
            && let Some(receipt) = update.splice_commit
            && receipt.id == id
        {
            break receipt;
        }
        if Instant::now() >= deadline {
            return Err("Saved edited receipt did not arrive".into());
        }
        d.step(
            "Hold refreshed workspace while awaiting the real durable placement receipt",
            false,
        )?;
        d.wake
            .wait_until((Instant::now() + Duration::from_millis(16)).min(deadline));
    };
    let committed = receipt.result.as_ref().map_err(Clone::clone)?;
    if committed.revision != *exact.snapshot.document.revision_id() {
        return Err("Held receipt differs from the exact edited proposal".into());
    }
    d.app_mut().receive_splice(None, Some(receipt));
    d.step(
        "Deliver the real saved receipt ahead of its withheld workspace",
        true,
    )?;
    d.check(
        "A successful receipt closes its draft without clearing or moving selection in the old visible revision",
        d.app().splice.is_none() && d.revision() == revision
            && d.app().edit_range == selection && editor(d) == entry,
        json!({"workspace_delivery_held":true,"saved_receipt_delivered":true,"entry":entry}),
        d.snapshot(),
    )?;
    // A real history-neutral query republishes the service's current workspace
    // and retained receipt. This models delivery order, not a storage failure.
    d.app()
        .service
        .submit(ProjectRequest::CaptureEditSlice(capture_request(copied)))?;
    d.app_mut().feedback.hold_project_updates = false;
    d.changed(&revision)?;
    d.check(
        "The subsequently delivered committed workspace admits the saved selection exactly once",
        *document(d)? == *exact.snapshot.document
            && d.app().selected_beat.as_ref() == Some(&exact.node)
            && d.app().selected_edit_range().is_none(),
        json!({"revision":exact.snapshot.document.revision_id(),"selected":exact.node}),
        d.snapshot(),
    )?;
    same_register(d, copied)?;
    undo(d, &saved)
}

pub(super) fn capture_request(copied: &Captured) -> CaptureRequest {
    CaptureRequest {
        id: copied.id().clone(),
        scope: copied.scope().clone(),
        parent: copied.slice().parent().clone(),
        selection: copied.slice().selection().clone(),
    }
}

fn rejected_destination(
    d: &mut Driver<'_>,
    copied: &Arc<Captured>,
    held: &ProjectDocument,
) -> Result<(), String> {
    goto(d, 40)?;
    let revision = d.revision();
    d.chord(&[Key::R, Key::R])?;
    d.changed(&revision)?;
    same_register(d, copied)?;
    let repeated = document(d)?.clone();
    goto(d, 60)?;
    d.command("splice")?;
    d.wait_for(
        "Open the copied slice at an unsupported Repeat interior",
        |app| app.splice.is_some(),
    )?;
    d.key(Key::I)?;
    wait_edited_endpoints(d, 8, 25)?;
    copied_picture(d, copied, 8, 25, true, 8)?;
    d.check(
        "A rejected Repeat-interior destination leaves copied endpoints available without Apply",
        !draft(d)?.ready_for_check() && d.rect(APPLY).is_err() && *document(d)? == repeated,
        json!({"source_ready":true,"destination_ready":false,"saved_unchanged":true}),
        state(d),
    )?;
    d.chord(&[Key::L, Key::O, Key::L])?;
    wait_edited_endpoints(d, 9, 26)?;
    copied_picture(d, copied, 9, 26, false, 14)?;
    d.check(
        "Local copied-source refinement still updates both endpoints when destination preflight fails",
        draft(d)?.proposal_for_check().source.boundaries()? == (9..26)
            && !draft(d)?.ready_for_check() && d.rect(APPLY).is_err()
            && *document(d)? == repeated,
        json!({"refined":[9,26],"apply_enabled":false}),
        state(d),
    )?;
    same_register(d, copied)?;
    cancel(d)?;
    undo(d, held)?;
    same_register(d, copied)
}

fn placement(d: &mut Driver<'_>, copied: &Arc<Captured>) -> Result<(), String> {
    goto(d, 60)?;
    let saved = document(d)?.clone();
    let revision = d.revision();
    let entry = editor(d);
    d.command("splice")?;
    wait_ready(d)?;
    wait_edited_endpoints(d, 8, 25)?;
    d.key(Key::I)?;
    copied_picture(d, copied, 8, 25, true, 8)?;
    d.key(Key::O)?;
    copied_picture(d, copied, 8, 25, false, 13)?;
    refine(d, copied)?;
    let source = endpoint_view(d)?;
    d.chord(&[Key::D, Key::L])?;
    wait_ready(d)?;
    d.key(Key::O)?;
    copied_picture(d, copied, 9, 27, false, 15)?;
    let moved = endpoint_view(d)?;
    d.check(
        "Changing only the destination reuses the exact immutable copied source view",
        Arc::ptr_eq(&source, &moved)
            && prepared(d)?.range == range(61, 79)?
            && d.revision() == revision
            && editor(d) == entry,
        json!({"inserted":[61,79],"same_source_view":true,"saved_unchanged":true}),
        state(d),
    )?;
    layout(d, copied)?;
    cancel(d)?;
    d.check(
        "Escape restores the entry Edit view and leaves the accepted copy and saved document unchanged",
        editor(d) == entry && *document(d)? == saved,
        json!({"entry":entry,"saved_unchanged":true}),
        d.snapshot(),
    )?;
    same_register(d, copied)?;

    d.command("splice")?;
    wait_ready(d)?;
    d.check(
        "Reopening placement starts from the accepted edited copy rather than the cancelled refinement",
        draft(d)?.proposal_for_check().source.boundaries()? == (8..25),
        json!([8,25]), state(d),
    )?;
    refine(d, copied)?;
    d.chord(&[Key::D, Key::F])?;
    wait_picture(d, 9, "Showing proposed edit frame 61")?;
    let exact = prepared(d)?;
    let PreparedMedia::Edited(media) = &exact.media else {
        return Err("Edited proposal did not retain admitted historical media".into());
    };
    exact
        .snapshot
        .validate_edit_slice_view(media.admitted())
        .map_err(|error| error.to_string())?;
    let simulated = d.app().feedback.simulate_playback;
    d.app_mut().feedback.simulate_playback = true;
    audition(d, &exact, &entry, &revision)?;
    d.app_mut().feedback.simulate_playback = simulated;
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    d.check(
        "Enter saves the exact edited preview once and selects its editable Sequence root",
        d.app().splice.is_none()
            && *document(d)? == *exact.snapshot.document
            && d.app().selected_beat.as_ref() == Some(&exact.node)
            && matches!(
                document(d)?.nodes()[&exact.node].kind,
                NodeKind::Sequence { .. }
            )
            && d.app().sequence_cursor == 60,
        json!({"inserted":[60,78],"selected":exact.node,"exact_commit":true}),
        d.snapshot(),
    )?;
    d.capture("Committed edited slice preserves its silent Hold and editable structure")?;
    same_register(d, copied)?;
    undo(d, &saved)?;
    same_register(d, copied)
}

fn refine(d: &mut Driver<'_>, copied: &Arc<Captured>) -> Result<(), String> {
    d.chord(&[Key::I, Key::L])?;
    wait_ready(d)?;
    copied_picture(d, copied, 9, 25, true, 9)?;
    d.chord(&[Key::O, Key::Num2, Key::L])?;
    wait_ready(d)?;
    wait_edited_endpoints(d, 9, 27)?;
    copied_picture(d, copied, 9, 27, false, 15)?;
    same_register(d, copied)
}

fn fast_replacement(
    d: &mut Driver<'_>,
    copied: &Arc<Captured>,
    uppercase: bool,
) -> Result<(), String> {
    select(d, 40, 50)?;
    let saved = document(d)?.clone();
    let revision = d.revision();
    let length = d.app().sequence_length();
    d.key_modified(
        Key::P,
        if uppercase {
            Modifiers::SHIFT
        } else {
            Modifiers::NONE
        },
    )?;
    d.changed(&revision)?;
    let selected = d
        .app()
        .selected_beat
        .clone()
        .ok_or("Fast edited replacement selected no root")?;
    d.check(
        "Fast p/P replaces Visual Edit time with the historical editable slice in one command",
        d.app().sequence_length() == length + 7
            && d.app().sequence_cursor == 40
            && d.app().selected_edit_range().is_none()
            && matches!(
                document(d)?.nodes()[&selected].kind,
                NodeKind::Sequence { .. }
            ),
        json!({"uppercase":uppercase,"removed":[40,50],"inserted":[40,57],"duration":length+7}),
        d.snapshot(),
    )?;
    wait_picture(d, 8, "Showing sequence frame 41")?;
    motion(d, 16, true)?;
    wait_picture(d, 13, "Showing sequence frame 57")?;
    d.key(Key::L)?;
    wait_picture(d, 39, "Showing sequence frame 58")?;
    same_register(d, copied)?;
    undo(d, &saved)
}

fn copied_picture(
    d: &mut Driver<'_>,
    copied: &Arc<Captured>,
    start: u64,
    out: u64,
    first: bool,
    source: u64,
) -> Result<(), String> {
    let shown = if first { start + 1 } else { out };
    wait_picture(d, source, &format!("Showing copied Edit frame {shown}"))?;
    let (view, frame) = match d.app().splice_picture_work(None) {
        Some(Work::Copied { view, frame }) => (view, frame),
        _ => return Err("The copied endpoint did not use copied-view picture work".into()),
    };
    let picture = d.app().presentation.diagnostic_snapshot();
    let expected_range = range(
        i64::try_from(start).map_err(|error| error.to_string())?,
        i64::try_from(out).map_err(|error| error.to_string())?,
    )?;
    let local = if first {
        0
    } else {
        expected_range.duration().frames() - 1
    };
    let historical = if first {
        expected_range.start()
    } else {
        ProjectFrame(expected_range.end().0 - 1)
    };
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("Missing copied endpoint workspace")?;
    d.check(
        "The main copied endpoint owns its historical view identity and cannot become a committed Camera target",
        view.id().copy == *copied.id()
            && view.id().range == expected_range
            && frame == ProjectFrame(local)
            && picture["requested"] == picture["displayed"]
            && picture["displayed"]["location"].as_str().is_some_and(|location| location.starts_with("Copied {"))
            && d.app().presentation.stable_sequence_ticket(workspace.session,
                workspace.document.revision_id(), historical).is_none(),
        json!({"range":[start,out],"local_frame":frame.0,"source_ordinal":source,"camera_target":false}),
        picture,
    )
}

fn endpoint_view(d: &Driver<'_>) -> Result<Arc<CopiedView>, String> {
    match d.app().splice_picture_work(None) {
        Some(Work::Copied { view, .. }) => Ok(view),
        _ => Err("Copied source endpoint is not selected".into()),
    }
}

fn layout(d: &mut Driver<'_>, copied: &Arc<Captured>) -> Result<(), String> {
    for (width, height) in [(960.0, 640.0), (1280.0, 820.0)] {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
        let input = d.harness.input_mut();
        input.screen_rect = Some(rect);
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .ok_or("Missing edited slice viewport")?
            .inner_rect = Some(rect);
        d.step(
            "Paint edited endpoints and placement controls at the requested viewport",
            true,
        )?;
        wait_edited_endpoints(d, 9, 27)?;
        copied_picture(d, copied, 9, 27, false, 15)?;
        for label in [
            "Place slice",
            "UNSAVED · Insert · Linked picture + sound",
            "In 9 · i",
            "Out 27 exclusive · o",
            "Destination Edit 61 · d",
            "COPIED EDIT SLICE",
            "COPIED EDIT ENDPOINT",
            "First included · copied Edit frame 10",
            "Last included · copied Edit frame 27",
            "Loop both joins · Shift Space",
            APPLY,
            CANCEL,
            "Edit boundary 61",
            "PROVISIONAL SLICE",
            "Before destination",
            "Following material",
        ] {
            paint_text(d, label)?;
        }
        endpoint_painted(d, "First included copied Edit picture, frame 10")?;
        endpoint_painted(d, "Last included copied Edit picture, frame 27")?;
        viewer_painted(d)?;
        d.capture(&format!(
            "Copied Edit endpoints and unsaved placement at {width} by {height}"
        ))?;
    }
    Ok(())
}

pub(super) fn wait_edited_endpoints(
    d: &mut Driver<'_>,
    first: u64,
    out: u64,
) -> Result<(), String> {
    let first = format!("First included copied Edit picture, frame {}", first + 1);
    let last = format!("Last included copied Edit picture, frame {out}");
    let deadline = Instant::now() + Duration::from_secs(15);
    while d.rect(&first).is_err() || d.rect(&last).is_err() {
        if Instant::now() >= deadline {
            return Err(format!(
                "Copied endpoint pictures did not become visible: {}",
                state(d)
            ));
        }
        d.step("Wait for both composed copied endpoint pictures", false)?;
        d.wake
            .wait_until((Instant::now() + Duration::from_millis(16)).min(deadline));
    }
    Ok(())
}

pub(super) fn accepted(d: &Driver<'_>) -> Result<Arc<Captured>, String> {
    match d.app().copied.content() {
        Some(Content::Edited(copied)) => Ok(copied.clone()),
        _ => Err("Production yank did not retain an edited copy".into()),
    }
}

fn same_register(d: &mut Driver<'_>, copied: &Arc<Captured>) -> Result<(), String> {
    let current = accepted(d)?;
    d.check(
        "Edits, Undo, placement and local refinement preserve the accepted edited register",
        Arc::ptr_eq(&current, copied) && current.slice().range() == range(8, 25)?,
        json!({"range":[8,25],"capture_revision":copied.slice().revision_id(),"same_copy":true}),
        json!({"range":[current.slice().range().start().0,current.slice().range().end().0],
            "capture_revision":current.slice().revision_id(),"current_revision":d.revision()}),
    )
}

pub(super) fn goto(d: &mut Driver<'_>, frame: u64) -> Result<(), String> {
    d.key(Key::Escape)?;
    d.chord(&[Key::G, Key::G])?;
    motion(d, frame, true)?;
    d.settled()
}

pub(super) fn select(d: &mut Driver<'_>, start: u64, out: u64) -> Result<(), String> {
    goto(d, start)?;
    d.key(Key::V)?;
    motion(d, out - start, true)?;
    d.settled()
}

pub(super) fn motion(d: &mut Driver<'_>, frames: u64, forward: bool) -> Result<(), String> {
    if frames == 0 {
        return Ok(());
    }
    for digit in frames.to_string().bytes() {
        d.key(match digit {
            b'0' => Key::Num0,
            b'1' => Key::Num1,
            b'2' => Key::Num2,
            b'3' => Key::Num3,
            b'4' => Key::Num4,
            b'5' => Key::Num5,
            b'6' => Key::Num6,
            b'7' => Key::Num7,
            b'8' => Key::Num8,
            b'9' => Key::Num9,
            _ => unreachable!(),
        })?;
    }
    d.key(if forward { Key::L } else { Key::H })
}

pub(super) fn range(start: i64, out: i64) -> Result<FrameRange, String> {
    FrameRange::new(ProjectFrame(start), ProjectFrame(out)).map_err(|error| error.to_string())
}

pub(super) fn cancel(d: &mut Driver<'_>) -> Result<(), String> {
    d.key(Key::Escape)?;
    d.wait_for("Cancel edited placement without saving", |app| {
        app.splice.is_none() && app.splice_abandon.is_none() && !app.service.is_busy()
    })?;
    d.settled()
}

pub(super) fn undo(d: &mut Driver<'_>, saved: &ProjectDocument) -> Result<(), String> {
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    restored(d, saved)
}

fn restored(d: &mut Driver<'_>, saved: &ProjectDocument) -> Result<(), String> {
    let expected = serde_json::to_value(saved).map_err(|error| error.to_string())?;
    let mut actual = serde_json::to_value(document(d)?).map_err(|error| error.to_string())?;
    actual["revision_id"] = expected["revision_id"].clone();
    d.check(
        "One Undo restores every authored field after edited-slice placement",
        actual == expected && document(d)?.revision_id() != saved.revision_id(),
        json!({"exact_document":true,"old_revision":saved.revision_id()}),
        json!({"equal":actual == expected,"revision":d.revision()}),
    )
}
