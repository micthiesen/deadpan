//! Production Trim router replay with real proposals, measured PCM and Metal pairs.

use super::*;
use crate::project::trim::Prepared;
use crate::worker::{EditJunctionIdentity, JunctionSide};
use deadpan_core::{
    AudioSample, Command, CommandRequest, ProjectDocument, RevisionId, SourceTrimIntent,
    SourceTrimPolicy,
};
use deadpan_playback::{Phase, Update};
use deadpan_store::{AccessMode, ProjectStore};
use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable as _;

const HEADING: &str = "Trim keyboard controls. Tab cycles In, Out, Slip and Roll; Shift-Tab reverses. H and L adjust one project frame, Shift adjusts ten. E focuses the amount field.";
const APPLY: &str = "Apply · Enter";
const CANCEL: &str = "Cancel · Esc";
const BEATS: &str = "Current group beat outline pane";
const GRAPH: &str = "Trim stereo waveform · absolute Edit samples · authored mix before limiter";
const FEEDBACK: &str =
    "Trim feedback scroll viewport. Up/Down scroll; Page Up/Down page; Home/End ends.";

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("Trim uses real qualified source receipts, ordered project-service proposals, endpoint decoding, offscreen Metal pair submission and measured waveform PCM. One real pair or service reply may be held for delivery-order checks. Audition delivery is explicitly injected; no output device, listening, physical display, OS IME or VoiceOver claim follows. Store grouping is fixture setup only; measured edits use production keyboard/pointer input.".into());
    fixture(d)?;
    ordered_input(d)?;
    pair_readiness(d)?;
    native_input_and_modes(d)?;
    captured_absence(d)?;
    audition_and_commit(d)?;
    nested_scope(d)
}

fn fixture(d: &mut Driver<'_>) -> Result<(), String> {
    d.command("source")?;
    d.chord(&[
        Key::G,
        Key::G,
        Key::Num1,
        Key::Num0,
        Key::L,
        Key::V,
        Key::Num1,
        Key::Num4,
        Key::L,
        Key::Y,
        Key::Escape,
    ])?;
    d.check(
        "Trim fixture copies Original [10,24)",
        d.app()
            .copied
            .original()
            .is_some_and(|copy| copy.ordinals == (10..24)),
        json!([10, 24]),
        d.snapshot(),
    )?;
    d.command("sequence")?;
    let before = d.revision();
    d.key_modified(Key::P, Modifiers::SHIFT)?;
    d.changed(&before)?;
    first_beat(d)?;
    let workspace = d.app().workspace.as_ref().ok_or("No Trim fixture")?;
    d.check(
        "Trim fixture has a 14-frame A and literal Source B",
        workspace
            .document
            .children(workspace.document.root())
            .count()
            == 2
            && d.app().sequence_cursor == 3
            && d.app()
                .beat_rows
                .first()
                .is_some_and(|row| row.frames == 14),
        json!({"children":2,"A_frames":14,"cursor":3}),
        d.snapshot(),
    )
}

fn ordered_input(d: &mut Driver<'_>) -> Result<(), String> {
    let saved = document(d)?.clone();
    let entry = editor(d);
    // A single native batch includes entry and all four control transitions.
    let mut events = keys(&[
        (Key::Comma, Modifiers::NONE),
        (Key::V, Modifiers::NONE),
        (Key::L, Modifiers::NONE),
        (Key::L, Modifiers::NONE),
        (Key::Tab, Modifiers::NONE),
        (Key::H, Modifiers::NONE),
        (Key::Tab, Modifiers::NONE),
        (Key::L, Modifiers::NONE),
        (Key::L, Modifiers::NONE),
        (Key::L, Modifiers::NONE),
        (Key::Tab, Modifiers::NONE),
        (Key::L, Modifiers::NONE),
        (Key::R, Modifiers::NONE),
        (Key::R, Modifiers::NONE),
    ]);
    d.events(
        "Enter ,v and replay ordered In +2, Out -1, Slip +3, Roll +1, r, r",
        std::mem::take(&mut events),
    )?;
    wait_apply(d)?;
    let expected = SourceTrimIntent {
        in_frames: 2,
        out_frames: -1,
        slip_frames: 3,
        roll_frames: 1,
        policy: SourceTrimPolicy::Ripple,
    };
    d.check(
        "One native batch retains four values and resolves two policy toggles in order",
        prepared(d)?.accepted == expected && editor(d) == entry && *document(d)? == saved,
        json!({"in":2,"out":-1,"slip":3,"roll":1,"policy":"Ripple"}),
        state(d),
    )?;
    let accepted = prepared(d)?.accepted;
    d.key_modified(Key::Tab, Modifiers::SHIFT)?;
    wait_apply(d)?;
    d.check(
        "Changing inspection control preserves all accepted values",
        prepared(d)?.accepted == accepted && draft_state(d)["control"] == "Slip",
        json!({"control":"Slip","values_unchanged":true}),
        state(d),
    )?;
    d.key(Key::R)?;
    wait_apply(d)?;
    d.check(
        "Policy applies to the complete retained tuple",
        prepared(d)?.accepted
            == SourceTrimIntent {
                policy: SourceTrimPolicy::Overwrite,
                ..expected
            },
        json!("Overwrite with I/O/S/R retained"),
        state(d),
    )?;
    d.capture("Trim four accepted values under Overwrite")?;
    d.key(Key::Escape)?;
    closed(d)?;
    d.check(
        "Cancel restores the entry selection and both clocks without a revision",
        editor(d) == entry && *document(d)? == saved,
        json!({"entry":entry,"revision":saved.revision_id()}),
        d.snapshot(),
    )?;

    d.command("trim")?;
    wait_pair(d)?;
    d.check(
        "Zero is an explicit committed pair with no candidate or Apply",
        prepared(d)?.accepted.is_zero()
            && prepared(d)?.snapshot.is_none()
            && identity(d)?.proposal_revision.is_none()
            && d.rect(APPLY).is_err(),
        json!({"candidate":null,"apply":false}),
        state(d),
    )?;
    d.events(
        "A clamp is followed by another clamp and a reversing key in the same batch",
        keys(&[
            (Key::H, Modifiers::SHIFT),
            (Key::H, Modifiers::NONE),
            (Key::L, Modifiers::NONE),
        ]),
    )?;
    wait_apply(d)?;
    d.check(
        "Ordered clamp replay ends at -9 rather than coalescing to -10",
        prepared(d)?.accepted.in_frames == -9
            && prepared(d)?.accepted.out_frames == 0
            && editor(d) == entry,
        json!(-9),
        state(d),
    )?;
    d.key(Key::Escape)?;
    closed(d)
}

fn pair_readiness(d: &mut Driver<'_>) -> Result<(), String> {
    let saved = document(d)?.clone();
    let entry = editor(d);
    d.app_mut().feedback.hold_junction = true;
    d.command("trim edge=slip delta=+3f mode=ripple")?;
    d.wait_for("Actual proposed junction decode is withheld", |app| {
        app.feedback.held_junction.is_some()
    })?;
    d.check(
        "An admitted proposal cannot Apply before both actual pictures are submitted",
        prepared(d)?.snapshot.is_some() && !ready(d) && d.rect(APPLY).is_err(),
        json!({"candidate":true,"apply":false}),
        state(d),
    )?;
    d.key(Key::Enter)?;
    d.check(
        "Pending-picture Enter does not save or arm a later save",
        d.app().trim.is_some() && *document(d)? == saved,
        json!(saved.revision_id()),
        state(d),
    )?;
    release_pair(d);
    wait_apply(d)?;
    pair_slots(d, None, Some(13))?;
    let first = identity(d)?.clone();
    let shown = d.app().junction_pictures.state_for_check()["displayed"].clone();
    d.app_mut().feedback.hold_junction = true;
    d.key(Key::O)?;
    d.wait_for("Actual Slip Out pair decode is withheld", |app| {
        app.feedback.held_junction.is_some()
    })?;
    d.check(
        "Changing a boundary retains the complete prior pair with Apply disabled",
        identity(d)? != &first
            && d.app().junction_pictures.state_for_check()["displayed"] == shown
            && !ready(d)
            && editor(d) == entry,
        json!({"prior_pair_retained":true,"apply":false}),
        state(d),
    )?;
    release_pair(d);
    wait_apply(d)?;
    pair_slots(d, Some(26), Some(0))?;
    d.key(Key::B)?;
    wait_pair(d)?;
    pair_slots(d, Some(23), Some(0))?;
    d.check(
        "Before uses entry U and the same full nonzero proposal identity",
        identity(d)?.side == JunctionSide::Before
            && identity(d)?.boundary == ProjectFrame(14)
            && identity(d)?.proposal_revision.is_some()
            && d.rect(APPLY).is_err()
            && editor(d) == entry,
        json!({"side":"Before","boundary":14,"apply":false}),
        state(d),
    )?;
    waveform(d)?;
    d.capture("Trim Before has the actual outgoing and incoming boundary frames")?;
    d.key(Key::B)?;
    wait_apply(d)?;
    let old_raster = d.app().junction_pictures.state_for_check()["displayed"]["raster"].clone();
    d.harness.set_size(egui::vec2(960.0, 640.0));
    d.key(Key::Enter)?;
    d.check(
        "Resize and Enter in one frame cannot apply the old raster",
        d.app().trim.is_some() && *document(d)? == saved,
        json!(saved.revision_id()),
        state(d),
    )?;
    wait_apply(d)?;
    d.step(
        "Ready replacement raster cannot auto-apply an earlier Enter",
        false,
    )?;
    d.check(
        "Resize requires a fresh Apply and publishes one complete replacement pair",
        *document(d)? == saved
            && d.app().junction_pictures.state_for_check()["displayed"]["raster"] != old_raster
            && ready(d),
        json!({"revision":saved.revision_id(),"fresh_apply_required":true}),
        state(d),
    )?;
    for label in [
        "Trim",
        "UNSAVED · Edit junction",
        "Apply · Enter",
        "Cancel · Esc",
    ] {
        visible(d, label)?;
    }
    waveform(d)?;
    feedback_scroll(d)?;
    d.capture("Trim current Proposed pair at minimum window size")?;
    d.click(HEADING)?;
    d.harness.set_size(egui::vec2(1280.0, 240.0));
    d.key(Key::Enter)?;
    d.step(
        "Measure the short viewport with no usable picture extent",
        false,
    )?;
    d.check(
        "Hidden pictures cannot retain Apply authority in a very short viewport",
        d.app().trim.is_some() && !ready(d) && d.rect(APPLY).is_err() && *document(d)? == saved,
        json!({"viewport":[1280,240],"apply":false,"revision":saved.revision_id()}),
        state(d),
    )?;
    d.key(Key::Enter)?;
    d.check(
        "A fresh Enter with hidden pictures cannot commit",
        d.app().trim.is_some() && !ready(d) && *document(d)? == saved,
        json!(saved.revision_id()),
        state(d),
    )?;
    d.harness.set_size(egui::vec2(1280.0, 820.0));
    wait_apply(d)?;
    d.step("Restored pictures require another explicit Apply", false)?;
    d.check(
        "Restoring the viewport cannot apply an Enter from hidden pictures",
        d.app().trim.is_some() && ready(d) && *document(d)? == saved,
        json!(saved.revision_id()),
        state(d),
    )?;
    waveform(d)?;
    d.key(Key::Escape)?;
    closed(d)?;
    d.check(
        "Pair inspection and resize leave the committed document intact",
        *document(d)? == saved && editor(d) == entry,
        json!(saved.revision_id()),
        d.snapshot(),
    )?;

    // A real late pair cannot reappear after the logical draft is cancelled.
    d.app_mut().feedback.hold_junction = true;
    d.command("trim edge=slip delta=+2f mode=ripple")?;
    d.wait_for("Retain one real pair across cancellation", |app| {
        app.feedback.held_junction.is_some()
    })?;
    d.key(Key::Escape)?;
    release_pair(d);
    closed(d)?;
    d.check(
        "A late decoded pair cannot reopen Trim or replace its restored viewer",
        d.app()
            .junction_pictures
            .displayed_identity_for_check()
            .is_none()
            && *document(d)? == saved
            && editor(d) == entry,
        json!({"pair":null,"revision":saved.revision_id()}),
        d.snapshot(),
    )
}

fn native_input_and_modes(d: &mut Driver<'_>) -> Result<(), String> {
    let saved = document(d)?.clone();
    let entry = editor(d);
    d.chord(&[Key::Comma, Key::V])?;
    wait_pair(d)?;
    let mut events = keys(&[(Key::E, Modifiers::NONE)]);
    events.push(egui::Event::Text("e".into()));
    events.extend(keys(&[(Key::A, Modifiers::COMMAND)]));
    events.push(egui::Event::Text("+3f".into()));
    events.extend(keys(&[(Key::Enter, Modifiers::NONE)]));
    d.events(
        "Native e focus, shortcut text, select-all, amount and Enter share one batch",
        events,
    )?;
    wait_apply(d)?;
    d.check(
        "Native amount Enter accepts text without applying or leaking its shortcut text",
        prepared(d)?.accepted.in_frames == 3
            && draft_state(d)["amount"] == "+3f"
            && *document(d)? == saved,
        json!({"in":3,"amount":"+3f","saved":false}),
        state(d),
    )?;
    field(d, "In", "bad")?;
    d.click(HEADING)?;
    d.chord(&[Key::Tab, Key::L, Key::R, Key::Enter])?;
    d.check(
        "Unrelated controls cannot clear invalid native text or revive Apply",
        draft_state(d)["text_error"].is_string()
            && !ready(d)
            && *document(d)? == saved
            && editor(d) == entry,
        json!({"invalid_text":true,"apply":false}),
        state(d),
    )?;
    // Cancel and reopen so this test does not assume unrelated invalid text changed policy.
    d.key(Key::Escape)?;
    closed(d)?;
    d.command("trim edge=slip delta=+2f mode=ripple")?;
    wait_apply(d)?;
    field(d, "Slip", "+2f")?;
    d.key_modified(Key::A, Modifiers::COMMAND)?;
    d.events(
        "Synthetic IME preedit owns Enter",
        vec![
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "+4f".into(),
                active_range_chars: Some(0..3),
            }),
            key_event(Key::Enter, Modifiers::NONE, true),
            key_event(Key::Enter, Modifiers::NONE, false),
        ],
    )?;
    d.check(
        "IME preedit cannot Apply or move editor context",
        d.app().ime_composing
            && d.app().trim.is_some()
            && *document(d)? == saved
            && editor(d) == entry,
        json!("composition owns Enter"),
        d.snapshot(),
    )?;
    d.events(
        "Synthetic IME completion owns its same-batch Enter",
        vec![
            egui::Event::Ime(egui::ImeEvent::Commit("+4f".into())),
            key_event(Key::Enter, Modifiers::NONE, true),
            key_event(Key::Enter, Modifiers::NONE, false),
        ],
    )?;
    d.check(
        "IME completion cannot save",
        !d.app().ime_composing && d.app().trim.is_some() && *document(d)? == saved,
        json!(saved.revision_id()),
        state(d),
    )?;
    field(d, "Slip", "+2f")?;
    d.key(Key::Enter)?;
    wait_apply(d)?;
    d.click(HEADING)?;
    let values = prepared(d)?.accepted;
    d.events(
        "Trim excludes global edits, command entry, open and render",
        keys(&[
            (Key::U, Modifiers::NONE),
            (Key::V, Modifiers::NONE),
            (Key::Colon, Modifiers::NONE),
            (Key::O, Modifiers::COMMAND),
            (Key::E, Modifiers::COMMAND),
        ]),
    )?;
    d.check(
        "Trim mode cannot open another editor mode or mutate the saved project",
        d.app().trim.is_some()
            && d.app().camera.is_none()
            && d.app().slip.is_none()
            && d.app().splice.is_none()
            && !d.app().command_open
            && !d.app().dialogs.is_open()
            && !d.app().render.blocking()
            && !d.app().render.requested
            && prepared(d)?.accepted == values
            && *document(d)? == saved,
        json!("Trim remains the only active mode"),
        d.snapshot(),
    )?;
    d.harness.get_by_label(CANCEL).focus();
    d.step("Native accessibility focus reaches Cancel", false)?;
    let proposal_before = draft_state(d)["prepared_revision"].clone();
    let feedback_before = draft_state(d)["feedback"].clone();
    d.key(Key::H)?;
    wait_pair(d)?;
    d.check(
        "Focused Cancel owns native keys without nudging",
        prepared(d)?.accepted == values
            && draft_state(d)["waiting"] == 0
            && draft_state(d)["prepared_revision"] == proposal_before
            && draft_state(d)["feedback"] == feedback_before,
        json!("accepted tuple unchanged"),
        state(d),
    )?;
    d.key(Key::Enter)?;
    closed(d)?;
    d.check(
        "Native button Enter cancels without history",
        *document(d)? == saved && editor(d) == entry,
        json!(saved.revision_id()),
        d.snapshot(),
    )
}

fn captured_absence(d: &mut Driver<'_>) -> Result<(), String> {
    let saved = document(d)?.clone();
    d.command("source")?;
    d.key(Key::Comma)?;
    d.check(
        "Comma captures Original ineligibility before its second key",
        state(d)["prefix_capture"]["error"].is_string(),
        json!("captured absence"),
        state(d),
    )?;
    d.key(Key::V)?;
    d.check(
        "Comma-v cannot retarget Original to a selected Edit beat",
        d.app().trim.is_none()
            && *document(d)? == saved
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Your edit")),
        json!("explicit Original refusal"),
        d.snapshot(),
    )?;
    d.command("sequence")?;
    first_beat(d)?;
    d.key(Key::V)?;
    let visual = d.app().edit_range.clone();
    d.command("trim")?;
    d.check(
        "Captured empty Visual selection remains an explicit refusal",
        d.app().trim.is_none()
            && d.app().edit_range == visual
            && *document(d)? == saved
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Visual")),
        json!("empty Visual selection retained"),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;

    // Publish a real paste after : has captured absence in Original. The writer
    // work is already complete; only delivery to the UI is controlled.
    d.app_mut().feedback.hold_project_updates = true;
    d.key(Key::P)?;
    d.wait_for("Real paste commits while publication is withheld", |app| {
        !app.service.is_busy()
    })?;
    let delayed = d
        .app()
        .service
        .take_update()
        .ok_or("No real paste update to hold")?;
    d.check(
        "Delayed fixture update contains a saved edit",
        delayed.committed.is_some(),
        json!(true),
        json!(delayed.committed.is_some()),
    )?;
    d.command("source")?;
    d.key(Key::Colon)?;
    d.check(
        "Command entry retains absence even while a saved edit awaits delivery",
        state(d)["command_capture"]["error"].is_string(),
        json!("captured absence"),
        state(d),
    )?;
    d.app_mut().feedback.hold_project_updates = false;
    d.app_mut().feedback.release_project_update = Some(delayed);
    d.step(
        "Late real paste supplies an eligible Edit selection after command capture",
        false,
    )?;
    d.events(
        "Submit captured Trim after late selection",
        vec![
            egui::Event::Text("trim".into()),
            key_event(Key::Enter, Modifiers::NONE, true),
            key_event(Key::Enter, Modifiers::NONE, false),
        ],
    )?;
    d.check(
        "A late eligible selection cannot replace command-entry absence",
        d.app().trim.is_none()
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Your edit")),
        json!("captured Original refusal retained"),
        d.snapshot(),
    )?;
    let pasted = d.revision();
    d.key(Key::U)?;
    d.changed(&pasted)?;
    d.check(
        "One Undo removes only the independently saved paste",
        same_authored(document(d)?, &saved),
        json!("fixture content restored"),
        d.snapshot(),
    )?;
    first_beat(d)
}

fn audition_and_commit(d: &mut Driver<'_>) -> Result<(), String> {
    let saved = document(d)?.clone();
    let entry = editor(d);
    d.command("trim edge=slip delta=+3f mode=ripple")?;
    wait_apply(d)?;
    waveform(d)?;
    let pair = d.app().junction_pictures.state_for_check()["displayed"].clone();
    let main_picture = d.app().presentation.diagnostic_snapshot();
    d.key_modified(Key::Space, Modifiers::SHIFT)?;
    let run = d
        .app()
        .transport
        .as_ref()
        .ok_or("Trim loop did not start")?;
    let start = run.window().start();
    let end = run.window().end();
    let content = AudioSample(start.0 + 17);
    let delivery = AudioSample(content.0 + 2 * (end.0 - start.0));
    let (mut feed, _callback) = deadpan_output::channel().map_err(string)?;
    let generation = feed.restart(start.0).map_err(string)?;
    let update = playback_update(d, Phase::Playing, delivery, generation, None)?;
    d.app_mut()
        .feedback
        .playback_updates
        .push_back(update.clone());
    d.step("Injected output delivery crosses two Trim loop laps", false)?;
    d.check(
        "Delivery moves only the local exact sample position and keeps the pair fixed",
        draft_state(d)["position"] == content.0
            && editor(d) == entry
            && d.app().junction_pictures.state_for_check()["displayed"] == pair
            && d.app().presentation.diagnostic_snapshot()["requested"] == main_picture["requested"],
        json!({"position":content.0,"editor":entry,"pair_fixed":true}),
        d.snapshot(),
    )?;
    d.key(Key::Space)?;
    d.check(
        "Space pauses before a separate resume input",
        d.app().transport.is_none()
            && draft_state(d)["position"] == content.0
            && editor(d) == entry
            && d.app().junction_pictures.state_for_check()["displayed"] == pair,
        json!({"paused":true,"position":content.0}),
        d.snapshot(),
    )?;
    visible(d, "Audition · Space")?;
    d.key(Key::Space)?;
    d.check(
        "Pause and resume retain the loop and exact wrapped sample",
        d.app()
            .transport
            .as_ref()
            .is_some_and(|run| run.window().looping() && run.content_sample() == Ok(content))
            && editor(d) == entry
            && d.app().junction_pictures.state_for_check()["displayed"] == pair,
        json!({"looping":true,"position":content.0}),
        d.snapshot(),
    )?;
    let resumed_ticket = d
        .app()
        .transport
        .as_ref()
        .ok_or("No resumed Trim transport")?
        .ticket;
    let mut stale = update;
    stale.sample = Some(AudioSample(
        delivery.0.checked_add(1).ok_or("Stale delivery overflow")?,
    ));
    let old_ticket = stale.ticket;
    d.app_mut().feedback.playback_updates.push_back(stale);
    d.step("Old ticket cannot move the resumed Trim clock", false)?;
    d.check(
        "Stale delivery preserves the resumed sample and editor cursors",
        old_ticket != resumed_ticket
            && draft_state(d)["position"] == content.0
            && d.app().transport.as_ref().is_some_and(|run| {
                run.ticket == resumed_ticket && run.content_sample() == Ok(content)
            })
            && editor(d) == entry,
        json!(content.0),
        state(d),
    )?;
    d.click("Pause · Space")?;
    d.check(
        "Pointer Pause retires the active transport and retains the exact sample",
        d.app().transport.is_none() && draft_state(d)["position"] == content.0,
        json!({"paused":true,"position":content.0}),
        d.snapshot(),
    )?;
    d.step("Paused caption refreshes without another user input", false)?;
    visible(d, "Audition · Space")?;
    d.click("Audition · Space")?;
    let failure = playback_update(
        d,
        Phase::Failed,
        content,
        generation,
        Some("audio output stopped: Starved".into()),
    )?;
    let viewport = feedback_viewport(d)?;
    feedback_wheel(d, viewport.center(), -500.0)?;
    d.app_mut().feedback.playback_updates.push_back(failure);
    d.step(
        "Device starvation ends Trim audition and reveals its failure",
        false,
    )?;
    let failure_paint =
        scenarios::text_paint_visibility(d, "Audition stopped: audio output stopped: Starved");
    d.check(
        "Device failure is immediately painted without scrolling and retains the proposal",
        d.app().transport.is_none()
            && d.app().resume.is_none()
            && fully_painted(&failure_paint, viewport)
            && ready(d)
            && editor(d) == entry
            && d.app().junction_pictures.state_for_check()["displayed"] == pair
            && *document(d)? == saved,
        json!({"failure_visible":true,"paused":true,"apply_ready":true,"saved":false}),
        json!({"paint":failure_paint,"trim":state(d)}),
    )?;
    d.capture("Stopped Trim audition keeps its failure visible above the retained feedback")?;
    d.click("Audition · Space")?;
    d.step(
        "Retry paints the cleared playback error before another failure",
        false,
    )?;
    d.check(
        "Retry starts a new transport and clears the previous playback fault",
        d.app().transport.is_some() && d.app().error.is_none() && ready(d),
        json!({"transport_active":true,"error":null,"apply_ready":true}),
        d.snapshot(),
    )?;
    d.click(FEEDBACK)?;
    d.key(Key::End)?;
    let handles = scenarios::text_paint_visibility(d, "Exact handles:");
    d.check(
        "Feedback owns focus and is scrolled to real handle details before the next fault",
        feedback_focused(d) && fully_painted(&handles, viewport),
        json!("focused feedback at its lower rows"),
        json!({"paint":handles,"trim":state(d)}),
    )?;
    let accepted = draft_state(d)["accepted"].clone();
    let proposal = draft_state(d)["prepared_revision"].clone();
    let failure = playback_update(
        d,
        Phase::Failed,
        content,
        generation,
        Some("audio output stopped: Starved".into()),
    )?;
    d.app_mut().feedback.playback_updates.push_back(failure);
    d.events(
        "Matching output failure and focused End arrive in one outer frame",
        keys(&[(Key::End, Modifiers::NONE)]),
    )?;
    let failure_paint =
        scenarios::text_paint_visibility(d, "Audition stopped: audio output stopped: Starved");
    d.check(
        "A new fault overrides same-frame End while preserving focus and the exact proposal",
        feedback_focused(d)
            && d.app().transport.is_none()
            && d.app().resume.is_none()
            && fully_painted(&failure_paint, viewport)
            && ready(d)
            && draft_state(d)["accepted"] == accepted
            && draft_state(d)["prepared_revision"] == proposal
            && editor(d) == entry
            && d.app().junction_pictures.state_for_check()["displayed"] == pair
            && *document(d)? == saved,
        json!({"failure_visible":true,"feedback_focused":true,"apply_ready":true,"saved":false}),
        json!({"paint":failure_paint,"trim":state(d)}),
    )?;
    d.click("Audition · Space")?;
    d.step(
        "Retry clears the fault before the native wheel witness",
        false,
    )?;
    let failure = playback_update(
        d,
        Phase::Failed,
        content,
        generation,
        Some("audio output stopped: Starved".into()),
    )?;
    d.app_mut().feedback.playback_updates.push_back(failure);
    d.events(
        "Matching output failure and a native wheel impulse share one outer frame",
        vec![
            egui::Event::PointerMoved(viewport.center()),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -500.0),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::NONE,
            },
        ],
    )?;
    let mut smoothing_frames = 0;
    for _ in 0..90 {
        let failure_paint =
            scenarios::text_paint_visibility(d, "Audition stopped: audio output stopped: Starved");
        d.check(
            "A new fault remains painted throughout the native wheel impulse and smoothing",
            fully_painted(&failure_paint, viewport)
                && d.app().transport.is_none()
                && d.app().resume.is_none()
                && ready(d)
                && draft_state(d)["accepted"] == accepted
                && draft_state(d)["prepared_revision"] == proposal
                && editor(d) == entry
                && d.app().junction_pictures.state_for_check()["displayed"] == pair
                && *document(d)? == saved,
            json!({"failure_visible":true,"apply_ready":true,"saved":false}),
            json!({"paint":failure_paint,"trim":state(d)}),
        )?;
        if !d.harness.ctx.input(|input| input.is_scrolling()) {
            break;
        }
        smoothing_frames += 1;
        d.step(
            "Native wheel smoothing advances without new user input",
            false,
        )?;
    }
    d.check(
        "The wheel witness exercised later smoothing frames and drained its gesture",
        smoothing_frames > 1 && !d.harness.ctx.input(|input| input.is_scrolling()),
        json!("multiple smoothing frames followed by native scroll idle"),
        json!(smoothing_frames),
    )?;
    d.click("Audition · Space")?;
    d.events(
        "Clear the previous fault and settle hover before the small wheel witness",
        vec![egui::Event::PointerGone],
    )?;
    for _ in 0..(d.options.hz / 3).max(1) {
        d.step(
            "Settle earlier scrolling and hover before the wake witness",
            false,
        )?;
    }
    d.check(
        "Small wheel witness starts with active audition and no prior scrolling",
        d.app().transport.is_some()
            && d.app().error.is_none()
            && !d.harness.ctx.input(|input| input.is_scrolling()),
        json!({"transport_active":true,"error":null,"scrolling":false}),
        d.snapshot(),
    )?;
    let failure = playback_update(
        d,
        Phase::Failed,
        content,
        generation,
        Some("audio output stopped: Starved".into()),
    )?;
    d.app_mut().feedback.playback_updates.push_back(failure);
    d.events(
        "Matching output failure and a small native wheel impulse share one frame",
        vec![
            egui::Event::PointerMoved(viewport.center()),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -5.0),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::NONE,
            },
        ],
    )?;
    let mut delayed_wakes = 0;
    for _ in 0..32 {
        let failure_paint =
            scenarios::text_paint_visibility(d, "Audition stopped: audio output stopped: Starved");
        d.check(
            "The small wheel fault stays painted through its requested wake",
            fully_painted(&failure_paint, viewport),
            json!("failure fully painted"),
            json!(failure_paint),
        )?;
        if !d.harness.ctx.input(|input| input.is_scrolling()) {
            break;
        }
        let delay = d.harness.output().viewport_output[&egui::ViewportId::ROOT].repaint_delay;
        d.check(
            "Pending small wheel reveal requests a bounded repaint without new input",
            delay <= Duration::from_millis(200),
            json!({"maximum_repaint_delay_ms":200}),
            json!({"repaint_delay_ms":delay.as_secs_f64() * 1000.0}),
        )?;
        delayed_wakes += usize::from(!delay.is_zero());
        // Follow the requested wake on the replay's frame clock. Do not force
        // intermediate frames that could retire an otherwise stranded guard.
        let frames = (delay.as_nanos() * u128::from(d.options.hz))
            .div_ceil(1_000_000_000)
            .max(1);
        let frames = u64::try_from(frames).map_err(string)?;
        d.frame = d
            .frame
            .checked_add(frames - 1)
            .ok_or("Wake frame overflow")?;
        d.step(
            "Native feedback advances at its requested repaint deadline",
            false,
        )?;
    }
    d.check(
        "The small impulse reaches idle through a delayed wake and preserves the exact proposal",
        delayed_wakes > 0
            && !d.harness.ctx.input(|input| input.is_scrolling())
            && d.app().transport.is_none()
            && d.app().resume.is_none()
            && ready(d)
            && draft_state(d)["accepted"] == accepted
            && draft_state(d)["prepared_revision"] == proposal
            && editor(d) == entry
            && d.app().junction_pictures.state_for_check()["displayed"] == pair
            && *document(d)? == saved,
        json!({"delayed_wake":true,"scrolling":false,"apply_ready":true,"saved":false}),
        json!({"delayed_wakes":delayed_wakes,"trim":state(d)}),
    )?;
    feedback_wheel(d, viewport.center(), -500.0)?;
    let handles = scenarios::text_paint_visibility(d, "Exact handles:");
    d.check(
        "A later deliberate wheel gesture can scroll feedback after fault reveal",
        fully_painted(&handles, viewport) && ready(d),
        json!("handle details fully painted by a new wheel gesture"),
        json!({"paint":handles,"trim":state(d)}),
    )?;
    d.click(HEADING)?;
    d.key(Key::Escape)?;
    closed(d)?;
    d.command("audition-context lead=0ms follow=0ms")?;
    d.command("trim edge=slip delta=+3f mode=ripple")?;
    wait_apply(d)?;
    let before = state(d);
    let mut committed_editor = entry.clone();
    committed_editor["selected_beat"] = json!(prepared(d)?.result.target);
    let candidate = prepared(d)?
        .snapshot
        .clone()
        .ok_or("Nonzero Trim has no proposal")?;
    d.key(Key::Space)?;
    d.check(
        "Sampleless audition reports its failure without retiring picture or Apply",
        d.app().transport.is_none()
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("no audio samples"))
            && ready(d)
            && state(d)["pair"]["displayed"] == before["pair"]["displayed"]
            && prepared(d)?
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.content == candidate.content)
            && *document(d)? == saved,
        json!({"audio_error":true,"apply_ready":true,"saved":false}),
        d.snapshot(),
    )?;
    d.capture("Zero audio context retains the exact picture proposal and usable Apply")?;
    let mut enter = keys(&[(Key::Enter, Modifiers::NONE)]);
    enter.insert(
        1,
        egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: true,
            modifiers: Modifiers::NONE,
        },
    );
    d.events("Apply after audio refusal with held Enter repeat", enter)?;
    d.changed(saved.revision_id().as_str())?;
    d.check(
        "Apply commits the exact displayed proposal once after sampleless audition",
        d.app().trim.is_none()
            && *document(d)? == *candidate.document
            && editor(d) == committed_editor,
        json!({"revision":candidate.document.revision_id(),"editor":committed_editor}),
        d.snapshot(),
    )?;
    let committed = document(d)?.clone();
    d.key(Key::U)?;
    d.changed(committed.revision_id().as_str())?;
    d.check(
        "One Undo restores all authored content with a fresh revision",
        same_authored(document(d)?, &saved) && document(d)?.revision_id() != saved.revision_id(),
        json!("exact fixture content, fresh revision"),
        d.snapshot(),
    )?;
    d.command("audition-context lead=500ms follow=750ms")?;
    first_beat(d)
}

fn nested_scope(d: &mut Driver<'_>) -> Result<(), String> {
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No nested Trim fixture")?
        .clone();
    let children = workspace
        .document
        .children(workspace.document.root())
        .cloned()
        .collect::<Vec<_>>();
    let group = NodeId::new("trim-replay-group").map_err(string)?;
    d.app().service.submit(ProjectRequest::Close)?;
    d.wait_for("Close writer for typed grouping fixture", |app| {
        app.workspace.is_none() && !app.service.is_busy()
    })?;
    let mut store = ProjectStore::open(&workspace.path, AccessMode::ReadWrite).map_err(string)?;
    let doc = store.snapshot().map_err(string)?;
    store
        .commit(&CommandRequest {
            project_id: doc.project_id().clone(),
            expected_revision: doc.revision_id().clone(),
            new_revision: RevisionId::new("trim-replay-group-revision").map_err(string)?,
            command: Command::Group {
                parent: doc.root().clone(),
                start: 0,
                end: children.len(),
                id: group.clone(),
                label: "Trim captured group".into(),
            },
        })
        .map_err(string)?;
    drop(store);
    d.app_mut().dialogs = Dialogs::scripted(vec![(
        DialogKind::OpenProject,
        Some(workspace.path.clone()),
    )]);
    d.key_modified(Key::O, Modifiers::COMMAND)?;
    d.wait_for(
        "Reopen grouped fixture through production picker path",
        |app| {
            !app.service.is_busy()
                && app
                    .workspace
                    .as_ref()
                    .is_some_and(|opened| opened.path == workspace.path)
        },
    )?;
    d.command("sequence")?;
    d.click(BEATS)?;
    d.chord(&[
        Key::G,
        Key::G,
        Key::Enter,
        Key::Num3,
        Key::L,
        Key::Comma,
        Key::V,
    ])?;
    wait_pair(d)?;
    d.check(
        "Trim captures an explicit entered Sequence and literal right sibling",
        prepared(d)?.target.parent == group
            && prepared(d)?.target.scope.groups() == [group.clone()]
            && prepared(d)?.target.node == children[0]
            && prepared(d)?.target.right.as_ref() == children.get(1),
        json!({"parent":group,"right":children.get(1)}),
        state(d),
    )?;
    d.key(Key::Escape)?;
    closed(d)?;
    d.key(Key::J)?;
    d.command("trim edge=in delta=+1f mode=ripple")?;
    wait_apply(d)?;
    d.check(
        "A terminal child admits non-Roll Trim with captured right absence",
        prepared(d)?.target.right.is_none() && prepared(d)?.accepted.in_frames == 1,
        json!({"right":null,"in":1}),
        state(d),
    )?;
    d.events(
        "Cycle from In backward to Roll then request unavailable right movement",
        keys(&[(Key::Tab, Modifiers::SHIFT), (Key::L, Modifiers::NONE)]),
    )?;
    wait_apply(d)?;
    d.check(
        "Refused Roll keeps the last accepted tuple and truthful terminal exterior",
        prepared(d)?.accepted.in_frames == 1
            && prepared(d)?.accepted.roll_frames == 0
            && identity(d)?.incoming.is_none()
            && draft_state(d)["feedback"]
                .as_array()
                .is_some_and(|events| events.iter().any(|event| event["error"].is_string())),
        json!({"in":1,"roll":0,"incoming":null,"refusal":true}),
        state(d),
    )?;
    d.key(Key::Escape)?;
    closed(d)?;
    d.check(
        "Nested Cancel retains entered group and releases the pair",
        d.app().sequence_scope.groups() == [group]
            && d.app()
                .junction_pictures
                .displayed_identity_for_check()
                .is_none(),
        json!("captured group retained, pair cleared"),
        d.snapshot(),
    )
}

fn waveform(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for(
        "Real local Trim waveform finishes measuring its bounded sample window",
        |app| {
            app.trim.as_ref().is_some_and(|draft| {
                draft.state_for_check(&app.junction_pictures)["waveform"]["status"] == "Complete"
            })
        },
    )?;
    let state = draft_state(d);
    let wave = &state["waveform"];
    let data = &wave["data"];
    let bins = data["bins"].as_array().ok_or("No measured waveform bins")?;
    let limit = u64::from(deadpan_audio::WaveformLimits::default().maximum_leaves());
    let prepared = prepared(d)?;
    let measured_document = match (identity(d)?.side, prepared.snapshot.as_ref()) {
        (JunctionSide::Proposed, Some(snapshot)) => &snapshot.document,
        _ => &prepared.base.document,
    };
    let expected_document = json!({"project":measured_document.project_id(),
        "revision":measured_document.revision_id(),"root":measured_document.root(),
        "rate":measured_document.presentation_basis().frame_rate,
        "duration":measured_document.duration().map_err(string)?.frames()});
    d.check(
        "Measured PCM descriptor belongs to the selected Before or Proposed document",
        data["document"] == expected_document,
        expected_document,
        data["document"].clone(),
    )?;
    d.check("Waveform belongs to this exact inspection, preserves absolute samples and stays bounded",wave["expected"]["identity"]==state["inspection"]["identity"] && data["samples"]==state["inspection"]["samples"] && data["measured_end"]==data["samples"][1] && data["stage"]=="authored_bus_pcm_before_limiter" && data["level_count"].as_u64().is_some_and(|levels|levels<=64) && bins.first().and_then(Value::as_u64).is_some_and(|count|count>0 && count<=limit),json!({"complete":true,"absolute_samples":state["inspection"]["samples"],"max_leaves":limit}),wave.clone())?;
    let graph = d.rect(GRAPH)?;
    d.check(
        "Measured waveform is fully inside its viewport",
        d.harness.ctx.content_rect().contains_rect(graph),
        json!("fully visible graph"),
        json!([graph.left(), graph.top(), graph.right(), graph.bottom()]),
    )?;
    let labels = [
        "L".to_owned(),
        "R".to_owned(),
        format!(
            "{} samples",
            data["samples"][0]
                .as_i64()
                .ok_or("Missing waveform start")?
        ),
        format!(
            "{} samples",
            data["samples"][1].as_i64().ok_or("Missing waveform end")?
        ),
    ];
    for label in labels {
        let paint = graph_text_paint(d, &label);
        d.check(
            "Waveform channel and sample labels fit the actual graph paint clip",
            paint.len() == 1 && fully_painted(&paint, graph),
            json!({"label":label,"paint_count":1,"fully_visible":true}),
            json!(paint),
        )?;
    }
    feedback_viewport(d)?;
    d.capture("Trim measured stereo waveform and accepted status")
}

fn graph_text_paint(d: &Driver<'_>, label: &str) -> Vec<Value> {
    scenarios::text_paint_visibility(d, label)
        .into_iter()
        .filter(|part| part["text"].as_str() == Some(label))
        .collect()
}

fn fully_painted(paint: &[Value], region: egui::Rect) -> bool {
    !paint.is_empty()
        && paint.iter().all(|part| {
            let bounds = &part["bounds"];
            let (Some(left), Some(top), Some(right), Some(bottom)) = (
                bounds[0].as_f64(),
                bounds[1].as_f64(),
                bounds[2].as_f64(),
                bounds[3].as_f64(),
            ) else {
                return false;
            };
            part["fully_visible"] == true
                && region.contains_rect(egui::Rect::from_min_max(
                    egui::pos2(left as f32, top as f32),
                    egui::pos2(right as f32, bottom as f32),
                ))
        })
}

fn feedback_viewport(d: &mut Driver<'_>) -> Result<egui::Rect, String> {
    let rect = d.rect(FEEDBACK)?;
    d.check(
        "Feedback scroll viewport fits wholly below the waveform",
        d.harness.ctx.content_rect().contains_rect(rect)
            && rect.height() >= 40.0
            && rect.top() >= d.rect(GRAPH)?.bottom(),
        json!("positive feedback viewport wholly inside the window, below the graph"),
        json!([rect.left(), rect.top(), rect.right(), rect.bottom()]),
    )?;
    Ok(rect)
}

fn feedback_scroll(d: &mut Driver<'_>) -> Result<(), String> {
    let viewport = feedback_viewport(d)?;
    let pair = d.app().junction_pictures.state_for_check()["displayed"].clone();
    let values = prepared(d)?.accepted;
    let revision = d.revision();
    let entry = editor(d);
    let top = scenarios::text_paint_visibility(d, "Project duration");
    let before = scenarios::text_paint_visibility(d, "Exact handles:");
    d.check(
        "Compact feedback starts at its first row and has real scroll overflow",
        fully_painted(&top, viewport) && !fully_painted(&before, viewport),
        json!("first row visible; complete handle details require scrolling"),
        json!({"top":top,"handles":before}),
    )?;
    feedback_wheel(d, viewport.center(), -500.0)?;
    let after = scenarios::text_paint_visibility(d, "Exact handles:");
    d.check(
        "Wheel input reveals complete handle details inside the feedback paint clip",
        fully_painted(&after, viewport)
            && prepared(d)?.accepted == values
            && d.revision() == revision
            && editor(d) == entry
            && d.app().junction_pictures.state_for_check()["displayed"] == pair
            && ready(d),
        json!("handle details fully painted; picture, accepted draft and editor unchanged"),
        json!({"paint":after,"trim":state(d)}),
    )?;
    d.capture("Trim feedback scrolled to handle details at minimum window size")?;
    feedback_wheel(d, viewport.center(), 500.0)?;
    let restored = scenarios::text_paint_visibility(d, "Project duration");
    d.check(
        "Feedback scroll returns to its first row without resizing the admitted pair",
        fully_painted(&restored, viewport)
            && d.app().junction_pictures.state_for_check()["displayed"] == pair
            && ready(d),
        json!("first row fully painted; accepted pair unchanged"),
        json!({"paint":restored,"trim":state(d)}),
    )?;
    // Leave the heading's modal Tab cycle through e, then use real native Tab
    // traversal from its amount field to the focusable feedback viewport.
    d.key(Key::E)?;
    for _ in 0..16 {
        d.key(Key::Tab)?;
        if feedback_focused(d) {
            break;
        }
    }
    d.check(
        "Native Tab traversal reaches feedback without changing the draft",
        feedback_focused(d) && prepared(d)?.accepted == values && ready(d),
        json!("feedback owns focus; accepted values unchanged"),
        state(d),
    )?;
    // No idle or End frame may initialize the focus filter first. At the top,
    // ArrowUp stays clamped and must not navigate to a control above feedback.
    d.key(Key::ArrowUp)?;
    let keyboard_up = scenarios::text_paint_visibility(d, "Project duration");
    d.check(
        "The first ArrowUp after Tab keeps feedback focus without changing the draft",
        feedback_focused(d)
            && fully_painted(&keyboard_up, viewport)
            && prepared(d)?.accepted == values
            && d.revision() == revision
            && editor(d) == entry
            && d.app().junction_pictures.state_for_check()["displayed"] == pair
            && ready(d),
        json!("feedback remains focused at its first row; draft and pictures unchanged"),
        json!({"paint":keyboard_up,"trim":state(d)}),
    )?;
    d.key(Key::End)?;
    let keyboard_end = scenarios::text_paint_visibility(d, "Exact handles:");
    d.check(
        "Focused feedback End reveals the complete handle details without a mouse",
        feedback_focused(d)
            && fully_painted(&keyboard_end, viewport)
            && prepared(d)?.accepted == values
            && d.revision() == revision
            && editor(d) == entry
            && d.app().junction_pictures.state_for_check()["displayed"] == pair
            && ready(d),
        json!("handle details fully painted by keyboard; draft and pictures unchanged"),
        json!({"paint":keyboard_end,"trim":state(d)}),
    )?;
    d.key(Key::Home)?;
    let keyboard_home = scenarios::text_paint_visibility(d, "Project duration");
    d.check(
        "Focused feedback Home restores the first row",
        feedback_focused(d) && fully_painted(&keyboard_home, viewport) && ready(d),
        json!("first feedback row fully painted"),
        json!(keyboard_home),
    )
}

fn feedback_focused(d: &Driver<'_>) -> bool {
    d.harness.root().children_recursive().any(|node| {
        let access = node.accesskit_node();
        access.label().as_deref() == Some(FEEDBACK) && access.is_focused()
    })
}

fn feedback_wheel(d: &mut Driver<'_>, point: egui::Pos2, delta: f32) -> Result<(), String> {
    d.events(
        "Wheel inside the Trim feedback viewport",
        vec![
            egui::Event::PointerMoved(point),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, delta),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::NONE,
            },
        ],
    )?;
    for _ in 0..8 {
        d.step("Trim feedback scroll settles", false)?;
    }
    Ok(())
}

fn first_beat(d: &mut Driver<'_>) -> Result<(), String> {
    d.click(BEATS)?;
    d.chord(&[Key::G, Key::G, Key::Num3, Key::L])?;
    d.settled()
}
fn document<'a>(d: &'a Driver<'_>) -> Result<&'a ProjectDocument, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.document.as_ref())
        .ok_or_else(|| "No Trim replay project".into())
}
fn prepared(d: &Driver<'_>) -> Result<Arc<Prepared>, String> {
    d.app()
        .trim
        .as_ref()
        .and_then(|draft| draft.prepared_for_check())
        .cloned()
        .ok_or_else(|| "No accepted Trim proposal".into())
}
fn identity<'a>(d: &'a Driver<'_>) -> Result<&'a EditJunctionIdentity, String> {
    d.app()
        .trim
        .as_ref()
        .and_then(|draft| draft.identity_for_check())
        .ok_or_else(|| "No Trim inspection".into())
}
fn ready(d: &Driver<'_>) -> bool {
    d.app()
        .trim
        .as_ref()
        .is_some_and(|draft| draft.ready_for_check(&d.app().junction_pictures))
}
fn draft_state(d: &Driver<'_>) -> Value {
    d.app().trim.as_ref().map_or(Value::Null, |draft| {
        draft.state_for_check(&d.app().junction_pictures)
    })
}
pub(super) fn state(d: &Driver<'_>) -> Value {
    let capture = |value: &Option<Result<super::super::trim::Capture, String>>| match value {
        Some(Ok(value)) => value.state_for_check(),
        Some(Err(error)) => json!({"error":error}),
        None => Value::Null,
    };
    json!({"draft":draft_state(d),"pair":d.app().junction_pictures.state_for_check(),"prefix_capture":capture(&d.app().trim_prefix_target),"command_capture":capture(&d.app().trim_command_target),"abandon_count":d.app().trim_abandon.len()})
}
fn editor(d: &Driver<'_>) -> Value {
    let app = d.app();
    json!({"source_cursor":app.source_cursor,"sequence_cursor":app.sequence_cursor,"selected_beat":app.selected_beat,"selected_source":app.selected_source,"scope":app.sequence_scope.groups(),"duration":app.sequence_length(),"selection":format!("{:?}",app.edit_selection())})
}
fn same_authored(a: &ProjectDocument, b: &ProjectDocument) -> bool {
    a.nodes() == b.nodes() && a.audio_bindings() == b.audio_bindings() && a.sounds() == b.sounds()
}
fn keys(keys: &[(Key, Modifiers)]) -> Vec<egui::Event> {
    keys.iter()
        .flat_map(|(key, mods)| [key_event(*key, *mods, true), key_event(*key, *mods, false)])
        .collect()
}
fn release_pair(d: &mut Driver<'_>) {
    let held = d.app_mut().feedback.held_junction.take();
    d.app_mut().feedback.hold_junction = false;
    d.app_mut().feedback.release_junction = held;
}
fn wait_pair(d: &mut Driver<'_>) -> Result<(), String> {
    // set_size changes the next input viewport. Paint it before checking an
    // otherwise-ready pair whose required raster still describes the old size.
    d.step(
        "Measure the current junction viewport before waiting",
        false,
    )?;
    d.wait_for(
        "Current complete junction pair reaches Metal at the required raster",
        |app| {
            !app.service.is_busy()
                && app.trim.as_ref().is_some_and(|draft| {
                    draft.prepared_for_check().is_some()
                        && draft
                            .identity_for_check()
                            .is_some_and(|identity| app.junction_pictures.ready_for_apply(identity))
                })
        },
    )?;
    d.step("Paint controls for the admitted pair", false)
}
fn wait_apply(d: &mut Driver<'_>) -> Result<(), String> {
    wait_pair(d)?;
    d.check(
        "Only the current nonzero Proposed pair enables Apply",
        ready(d) && d.rect(APPLY).is_ok(),
        json!("current Proposed pair Apply enabled"),
        state(d),
    )
}
fn closed(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for(
        "Trim closes and drains exact abandoned proposals and GPU work",
        |app| {
            app.trim.is_none()
                && app.trim_abandon.is_empty()
                && !app.service.is_busy()
                && !app.junction_pictures.gpu_work_pending()
        },
    )?;
    d.settled()
}
fn field(d: &mut Driver<'_>, control: &str, text: &str) -> Result<(), String> {
    d.click(&format!("{control} amount in whole project frames, for example -3f or +5f. Enter accepts the text without applying Trim. Tab uses native focus navigation."))?;
    d.key_modified(Key::A, Modifiers::COMMAND)?;
    d.events(
        "Replace native Trim amount",
        vec![egui::Event::Text(text.into())],
    )
}
fn visible(d: &mut Driver<'_>, text: &str) -> Result<(), String> {
    let paint = scenarios::text_paint_visibility(d, text);
    d.check(
        "Trim control text stays fully painted",
        !paint.is_empty() && paint.iter().all(|part| part["fully_visible"] == true),
        json!(text),
        json!(paint),
    )
}
fn pair_slots(
    d: &mut Driver<'_>,
    outgoing: Option<u64>,
    incoming: Option<u64>,
) -> Result<(), String> {
    let state = d.app().junction_pictures.state_for_check();
    let slots = &state["displayed"]["slots"];
    let matches = |slot: &Value, ordinal: Option<u64>, exterior: &str| {
        ordinal.map_or_else(
            || slot["exterior"] == exterior,
            |ordinal| slot["ordinal"] == ordinal,
        )
    };
    d.check(
        "Atomic boundary pair retains exact decoded source ordinals or explicit exterior",
        d.app().junction_pictures.displayed_identity_for_check() == Some(identity(d)?)
            && matches(&slots[0], outgoing, "NoOutgoing")
            && matches(&slots[1], incoming, "NoIncoming"),
        json!({"outgoing":outgoing,"incoming":incoming}),
        state,
    )
}
fn playback_update(
    d: &Driver<'_>,
    phase: Phase,
    sample: AudioSample,
    generation: deadpan_output::Generation,
    error: Option<String>,
) -> Result<Update, String> {
    let run = d.app().transport.as_ref().ok_or("No Trim transport")?;
    Ok(Update {
        ticket: run.ticket,
        session: run.session,
        project_id: run.project.clone(),
        revision_id: run.revision.clone(),
        content: run.content.clone(),
        phase,
        sample: Some(sample),
        generation: Some(generation),
        error,
    })
}
fn string(error: impl std::fmt::Display) -> String {
    error.to_string()
}
