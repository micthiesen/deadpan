//! Durable registers through production input and genuine capture service replies.
//! Only delivery order is controlled; authored edits and captures remain real.

use super::*;
use crate::preview::copied::Content;
use crate::project::ProjectUpdate;
use crate::project::slice::Captured;
use deadpan_core::{NodeKind, ProjectDocument};
use egui::{Event, Key, Modifiers};

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let baseline = document(d)?.clone();
    original(d, 5, 13, 'a', true)?;
    original(d, 30, 35, 'b', false)?;
    d.check(
        "Two named Original copies stay independent and the latest also becomes the default",
        original_range(d, 'a') == Some(5..13)
            && original_range(d, 'b') == Some(30..35)
            && original_range(d, '"') == Some(30..35)
            && d.app().copied.selected().is_none()
            && document(d)? == &baseline,
        json!({"a":[5,13],"b":[30,35],"default":[30,35],"selected":null}),
        inventory(d),
    )?;
    visible_inventory(d)?;
    original_paste(d, &baseline, 'a', 5, 8, false)?;
    original_paste(d, &baseline, 'b', 30, 5, true)?;
    rejected_and_cancelled(d, &baseline)?;
    sound_rejections(d)?;
    historical_edit(d, &baseline)?;
    historical_cut(d, &baseline)?;
    delayed_destination(d)?;
    delayed_pending_command(d)?;
    delayed_empty_command(d)?;
    superseded_capture(d)?;
    pending_original_placement(d)?;
    native_text_and_blur(d)?;
    command_blur(d)?;
    d.check(
        "Register navigation, captures and cancelled drafts leave the complete authored baseline",
        same_document(document(d)?, &baseline)? && !d.app().workspace.as_ref().unwrap().can_undo,
        json!("initial authored document; fresh Undo revision; no undoable edits"),
        d.snapshot(),
    )?;
    command_reopen(d)
}

fn original(
    d: &mut Driver<'_>,
    start: u64,
    end: u64,
    name: char,
    uppercase: bool,
) -> Result<(), String> {
    d.command("source")?;
    select(d, start, end)?;
    let revision = d.revision();
    if uppercase {
        choose(d, name, true)?;
    } else {
        d.command(&format!("register {name}"))?;
    }
    d.key(Key::Y)?;
    d.wait_for(
        "Named Original copy is saved by the project service",
        |app| !app.service.is_busy() && !app.copied.is_pending(),
    )?;
    d.check(
        "A named Original yank consumes its one-shot name and changes no revision",
        original_range(d, name) == Some(start..end)
            && d.app().copied.selected().is_none()
            && d.revision() == revision
            && !d.app().moment.active,
        json!({"register":name,"range":[start,end],"selected":null,"revision":revision}),
        inventory(d),
    )
}

fn visible_inventory(d: &mut Driver<'_>) -> Result<(), String> {
    d.key(Key::Escape)?;
    for (width, height) in [(960.0, 640.0), (1280.0, 820.0)] {
        resize(d, width, height)?;
        d.key_modified(Key::Quote, Modifiers::SHIFT)?;
        d.check(
            "Shift Quote opens the visible register prefix without changing a slot",
            d.app().bindings.pending() == "\"" && d.app().copied.selected().is_none(),
            json!("\" pending"),
            d.snapshot(),
        )?;
        painted(d, "a–z")?;
        d.capture(&format!("Pending register quote at {width} by {height}"))?;
        d.key(Key::A)?;
        painted(d, "Register a · Original")?;
        painted(d, "Copied Original [5..13)")?;
        d.capture(&format!(
            "Selected register a and its Original range at {width} by {height}"
        ))?;
        d.key(Key::Escape)?;
        d.command("registers")?;
        d.check(
            "The registers command opens the existing Keys view with live typed contents",
            d.app().help_open
                && original_range(d, 'a') == Some(5..13)
                && original_range(d, 'b') == Some(30..35),
            json!({"Keys":true,"a":"Original [5..13)","b":"Original [30..35)"}),
            inventory(d),
        )?;
        for label in [
            "REGISTERS",
            "Copied Original [5..13)",
            "Copied Original [30..35)",
        ] {
            painted(d, label)?;
        }
        d.capture(&format!(
            "Live Original register inventory at {width} by {height}"
        ))?;
        d.key(Key::Escape)?;
    }
    choose(d, 'a', false)?;
    d.key_modified(Key::Quote, Modifiers::SHIFT)?;
    d.key_modified(Key::Quote, Modifiers::SHIFT)?;
    d.check(
        "Quote followed by quote explicitly restores the default copy",
        d.app().copied.selected().is_none() && original_range(d, '"') == Some(30..35),
        json!({"selected":null,"default":[30,35]}),
        inventory(d),
    )
}

fn original_paste(
    d: &mut Driver<'_>,
    baseline: &ProjectDocument,
    name: char,
    first: u64,
    frames: u64,
    before: bool,
) -> Result<(), String> {
    d.command("sequence")?;
    goto(d, 0)?;
    let length = d.app().sequence_length();
    choose(d, name, false)?;
    let revision = d.revision();
    d.key_modified(
        Key::P,
        if before {
            Modifiers::SHIFT
        } else {
            Modifiers::NONE
        },
    )?;
    d.changed(&revision)?;
    d.check(
        "Named p and P paste only the chosen exact Original range and consume the choice",
        d.app().sequence_length() == length + frames
            && d.app().sequence_cursor == if before { 0 } else { length }
            && d.app().copied.selected().is_none()
            && original_range(d, '"') == Some(30..35),
        json!({"register":name,"inserted_frames":frames,"before":before,"default_unchanged":true}),
        d.snapshot(),
    )?;
    picture(d, first)?;
    d.capture(&format!(
        "Register {name} pasted {} the selected beat",
        if before { "before" } else { "after" }
    ))?;
    undo(d, baseline)
}

fn rejected_and_cancelled(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    goto(d, 40)?;
    let revision = d.revision();
    let slots = inventory(d);
    choose(d, 'z', false)?;
    d.key(Key::P)?;
    d.check(
        "An empty named paste refuses instead of falling back to the populated default",
        d.revision() == revision
            && d.app().copied.selected().is_none()
            && inventory(d) == slots
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Register z is empty")),
        json!({"revision":revision,"empty_register_refused":true,"all_slots_preserved":true}),
        d.snapshot(),
    )?;
    choose(d, 'a', false)?;
    d.command("splice")?;
    d.wait_for("The named Original placement is prepared", |app| {
        !app.service.is_busy()
            && app
                .splice
                .as_ref()
                .is_some_and(|draft| draft.ready_for_check())
    })?;
    let proposed = d
        .app()
        .splice
        .as_ref()
        .ok_or("Missing named slice draft")?
        .proposal_for_check()
        .source
        .clone();
    d.check(
        "Named :splice snapshots a rather than the later unnamed copy",
        proposed.boundaries()? == (5..13)
            && d.app().copied.selected().is_none()
            && d.revision() == revision,
        json!({"source":[5,13],"revision":revision,"selected":null}),
        json!({"source":proposed.boundaries()?,"state":d.snapshot()}),
    )?;
    d.capture("Named Original a prepared in Place slice")?;
    d.key(Key::Escape)?;
    d.wait_for("Cancel named placement without saving", |app| {
        app.splice.is_none() && app.splice_abandon.is_none() && !app.service.is_busy()
    })?;
    d.settled()?;
    d.check(
        "Cancelling named Place slice preserves every register and the authored document",
        inventory(d) == slots && same_document(document(d)?, baseline)? && d.revision() == revision,
        json!({"registers_unchanged":true,"revision":revision}),
        inventory(d),
    )
}

fn historical_edit(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    goto(d, 10)?;
    let revision = d.revision();
    d.command("hold 11f")?;
    d.changed(&revision)?;
    select(d, 8, 25)?;
    choose(d, 'c', false)?;
    d.key(Key::Y)?;
    wait_copy(d, 'c')?;
    let copied = edited(d, 'c')?;
    d.check(
        "Register c captures an edited slice with its authored silent Hold and exact historical clock",
        copied.slice().range().start() == ProjectFrame(8)
            && copied.slice().range().end() == ProjectFrame(25)
            && copied.slice().duration().frames() == 17
            && copied.slice().revision_id().as_str() == d.revision()
            && d.app().copied.selected().is_none(),
        json!({"c":[8,25],"duration":17,"selected":null}),
        inventory(d),
    )?;
    d.key(Key::Escape)?;
    undo(d, baseline)?;
    goto(d, 0)?;
    choose(d, 'c', false)?;
    let revision = d.revision();
    d.key(Key::P)?;
    d.changed(&revision)?;
    d.check(
        "A named historical Edit paste retains its Hold after the source edit was undone",
        d.app().sequence_length() == 137
            && d.app().sequence_cursor == 120
            && document(d)?.nodes().values().any(|node| {
                matches!(&node.kind,
                NodeKind::Hold { recipe } if recipe.duration.frames() == 11)
            })
            && same_edited(d, 'c', &copied)
            && same_edited(d, '"', &copied),
        json!({"frames":137,"inserted_at":120,"hold_frames":11,"historical_copy_retained":true}),
        d.snapshot(),
    )?;
    picture(d, 8)?;
    d.capture("Named historical Edit slice keeps its pause after Undo")?;
    undo(d, baseline)
}

fn sound_rejections(d: &mut Driver<'_>) -> Result<(), String> {
    d.click("Current group beat outline pane")?;
    d.key(Key::Tab)?;
    d.check(
        "Native pane cycling reaches the separate Placed sounds context",
        d.app().pane == Pane::Sounds,
        json!("Sounds"),
        d.snapshot(),
    )?;
    let revision = d.revision();
    let slots = inventory(d);
    for command in [false, true] {
        choose(d, 'a', false)?;
        if command {
            d.command("delete")?;
        } else {
            d.key(Key::P)?;
        }
        d.check(
            "Rejected picture paste or command cut in Sounds consumes the name without a write",
            d.app().copied.selected().is_none()
                && d.app().error.is_some()
                && d.revision() == revision
                && inventory(d) == slots,
            json!({"command_cut":command,"selected":null,"revision":revision,"slots_preserved":true}),
            d.snapshot(),
        )?;
    }
    choose(d, 'a', false)?;
    d.chord(&[Key::D, Key::D])?;
    d.check(
        "The separate sound-delete shortcut leaves the picture register choice available",
        d.app().copied.selected() == Some('a') && d.revision() == revision && inventory(d) == slots,
        json!({"selected":"a","revision":revision,"slots_preserved":true}),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    d.click("Current group beat outline pane")
}

fn historical_cut(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    select(d, 20, 30)?;
    choose(d, 'd', false)?;
    let revision = d.revision();
    d.key(Key::D)?;
    d.changed(&revision)?;
    let cut = edited(d, 'd')?;
    d.check(
        "A named Visual cut writes its exact pre-cut contents to d and the default only after saving",
        d.app().sequence_length() == 110
            && cut.slice().revision_id().as_str() == revision
            && cut.slice().range().start() == ProjectFrame(20)
            && cut.slice().range().end() == ProjectFrame(30)
            && same_edited(d, '"', &cut)
            && d.app().copied.selected().is_none(),
        json!({"d":[20,30],"source_revision":revision,"frames_after_cut":110}),
        inventory(d),
    )?;
    undo(d, baseline)?;
    // A later Original copy changes only the default; d remains historical.
    original(d, 40, 43, 'b', false)?;
    d.command("sequence")?;
    goto(d, 0)?;
    choose(d, 'd', false)?;
    let revision = d.revision();
    d.key_modified(Key::P, Modifiers::SHIFT)?;
    d.changed(&revision)?;
    d.check(
        "Named cut d remains reusable after Undo and a later copy into another slot",
        d.app().sequence_length() == 130
            && d.app().sequence_cursor == 0
            && same_edited(d, 'd', &cut)
            && original_range(d, '"') == Some(40..43),
        json!({"pasted":"d","frames":130,"default":[40,43]}),
        d.snapshot(),
    )?;
    picture(d, 20)?;
    d.capture("Historical cut d pasted before the unchanged Original")?;
    undo(d, baseline)
}

fn delayed_destination(d: &mut Driver<'_>) -> Result<(), String> {
    select(d, 45, 51)?;
    choose(d, 'f', false)?;
    d.app_mut().feedback.hold_project_updates = true;
    d.key(Key::Y)?;
    let update = capture_update(d, 45, 51)?;
    choose(d, 'g', false)?;
    release(d, update)?;
    let copied = edited(d, 'f')?;
    d.check(
        "Selecting g while a genuine f capture is delayed cannot retarget its completion",
        copied.slice().range().start() == ProjectFrame(45)
            && copied.slice().range().end() == ProjectFrame(51)
            && content(d, 'g').is_none()
            && d.app().copied.selected() == Some('g')
            && same_edited(d, '"', &copied),
        json!({"f":[45,51],"g":"empty","next_register":"g","injected":"delivery timing only"}),
        inventory(d),
    )?;
    d.key(Key::Escape)
}

fn delayed_pending_command(d: &mut Driver<'_>) -> Result<(), String> {
    select(d, 55, 61)?;
    choose(d, 'e', false)?;
    d.app_mut().feedback.hold_project_updates = true;
    d.key(Key::Y)?;
    let update = capture_update(d, 55, 61)?;
    choose(d, 'e', false)?;
    let revision = d.revision();
    d.key(Key::Colon)?;
    d.check(
        "Command entry captures the pending-copy refusal before delayed delivery",
        d.app().command_open && content(d, 'e').is_none() && d.app().copied.is_pending(),
        json!({"command_open":true,"e":"empty","capture_pending":true}),
        inventory(d),
    )?;
    release(d, update)?;
    let copied = edited(d, 'e')?;
    d.events(
        "Submit :splice after the real e capture arrives",
        vec![
            Event::Text("splice".into()),
            key_event(Key::Enter, Modifiers::NONE, true),
            key_event(Key::Enter, Modifiers::NONE, false),
        ],
    )?;
    d.check(
        "A command opened while copying cannot acquire a placement target from its later success",
        d.app().splice.is_none()
            && d.revision() == revision
            && d.app().copied.selected().is_none()
            && same_edited(d, 'e', &copied)
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Wait for the copy to finish saving")),
        json!({"splice":false,"revision":revision,"e_filled_but_entry_stays_unavailable":true}),
        d.snapshot(),
    )?;
    d.capture("Command opened during copying retains its waiting refusal")?;
    d.key(Key::Escape)
}

fn delayed_empty_command(d: &mut Driver<'_>) -> Result<(), String> {
    select(d, 75, 81)?;
    choose(d, 'k', false)?;
    let baseline = document(d)?.clone();
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("Missing empty register workspace")?
        .clone();
    let request = crate::project::slice::CaptureRequest {
        id: crate::project::slice::CopyId {
            session: workspace.session,
            project: workspace.document.project_id().clone(),
            source_revision: workspace.document.revision_id().clone(),
            request: d
                .app_mut()
                .next_serial()
                .ok_or("No independent copy request identity")?,
            persisted_version: None,
        },
        register: Some('k'),
        scope: d.app().sequence_scope.clone(),
        parent: d.app().sequence_scope.resolve(&workspace)?.owner.clone(),
        selection: deadpan_core::SliceCaptureSelection::Range {
            range: deadpan_core::FrameRange::new(ProjectFrame(75), ProjectFrame(81))
                .map_err(|error| error.to_string())?,
        },
    };
    let history = (workspace.can_undo, workspace.can_redo);
    d.key(Key::Colon)?;
    d.check(
        "Command entry captures an empty named register before any write is submitted",
        d.app().command_open && content(d, 'k').is_none() && !d.app().copied.is_pending(),
        json!({"command_open":true,"k":"empty","pending":false}),
        inventory(d),
    )?;
    // This independent request uses the real typed service and current revision.
    // Only its delivery order is controlled; no bank or receipt is fabricated.
    d.app_mut().feedback.hold_project_updates = true;
    d.app()
        .service
        .submit(ProjectRequest::CaptureEditSlice(request))?;
    let update = capture_update(d, 75, 81)?;
    release(d, update)?;
    let copied = edited(d, 'k')?;
    d.events(
        "Submit :splice after the independent durable k write arrives",
        vec![
            Event::Text("splice".into()),
            key_event(Key::Enter, Modifiers::NONE, true),
            key_event(Key::Enter, Modifiers::NONE, false),
        ],
    )?;
    let current = d
        .app()
        .workspace
        .as_ref()
        .ok_or("Lost independent copy workspace")?;
    d.check("An empty command-entry register cannot acquire a later independently saved copy",
        d.app().splice.is_none() && document(d)? == &baseline
            && (current.can_undo, current.can_redo) == history
            && d.app().copied.selected().is_none() && same_edited(d, 'k', &copied)
            && copied.slice().range().start() == ProjectFrame(75)
            && copied.slice().range().end() == ProjectFrame(81)
            && d.app().error.as_deref().is_some_and(|error| error.contains("Register k is empty")),
        json!({"splice":false,"captured_empty":true,"k_saved":[75,81],"document_and_history_unchanged":true}), d.snapshot())?;
    d.capture("Captured empty register refuses a later independent durable copy")?;
    d.key(Key::Escape)
}

fn superseded_capture(d: &mut Driver<'_>) -> Result<(), String> {
    select(d, 65, 70)?;
    choose(d, 'h', false)?;
    d.app_mut().feedback.hold_project_updates = true;
    d.key(Key::Y)?;
    let update = capture_update(d, 65, 70)?;
    let historical = update
        .captured_slice
        .as_ref()
        .ok_or("Missing saved h capture")?
        .result
        .as_ref()
        .map_err(Clone::clone)?
        .clone();
    // Submit a newer real Original write while delivery of the old bank and its
    // confirmation remains held. The newer bank must retain the saved h slot.
    d.command("source")?;
    select(d, 70, 74)?;
    choose(d, 'a', false)?;
    d.key(Key::Y)?;
    let newer = original_update(d, 70, 74)?;
    release(d, newer)?;
    let baseline = document(d)?.clone();
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("Missing supersession workspace")?;
    let history = (workspace.can_undo, workspace.can_redo);
    let selected = d.app().copied.selected();
    let moment = d.app().moment.range();
    let edit_selection = d.app().edit_range.clone();
    let slots: Vec<_> = d
        .app()
        .copied
        .entries()
        .map(|(name, content)| (name, content.source()))
        .collect();
    release(d, update)?;
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("Superseded reply lost its workspace")?;
    let current: Vec<_> = d
        .app()
        .copied
        .entries()
        .map(|(name, content)| (name, content.source()))
        .collect();
    d.check(
        "A newer durable write retains saved h while an older bank and confirmation cannot regress it",
        same_edited(d, 'h', &historical)
            && original_range(d, 'a') == Some(70..74)
            && original_range(d, '"') == Some(70..74)
            && !d.app().copied.is_pending()
            && d.app().copied.selected() == selected
            && d.app().moment.range() == moment
            && d.app().edit_range == edit_selection
            && document(d)? == &baseline
            && (workspace.can_undo, workspace.can_redo) == history
            && current == slots
            && d.app().error.is_none()
            && !d.app().message.as_deref().is_some_and(|message| message.starts_with("Edit slice copied.")),
        json!({"h":[65,70],"a":[70,74],"default":[70,74],"all_slots_and_selections_preserved":true,"history_and_revision_unchanged":true,"stale_copy_feedback":false}),
        json!({"registers":inventory(d),"state":d.snapshot()}),
    )
}

fn pending_original_placement(d: &mut Driver<'_>) -> Result<(), String> {
    d.command("source")?;
    select(d, 90, 94)?;
    choose(d, 'j', false)?;
    let baseline = document(d)?.clone();
    let slots = inventory(d);
    d.app_mut().feedback.hold_project_updates = true;
    d.key(Key::Y)?;
    let update = original_update(d, 90, 94)?;
    d.command("sequence")?;
    d.key(Key::P)?;
    d.check(
        "Paste cannot use the previous bank while a newer Original write awaits delivery",
        !d.app().service.is_busy()
            && d.app().copied.is_pending()
            && document(d)? == &baseline
            && inventory(d) == slots
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Wait for the copy to finish saving")),
        json!({"service_idle":true,"confirmation_pending":true,"unchanged_document_and_bank":true}),
        d.snapshot(),
    )?;
    release(d, update)?;
    d.check(
        "The withheld Original success installs its exact durable name",
        original_range(d, 'j') == Some(90..94)
            && !d.app().copied.is_pending()
            && document(d)? == &baseline,
        json!([90, 94]),
        inventory(d),
    )?;
    goto(d, 0)?;
    choose(d, 'j', false)?;
    let revision = d.revision();
    d.key_modified(Key::P, Modifiers::SHIFT)?;
    d.changed(&revision)?;
    d.check(
        "After confirmation the named Original can be pasted normally",
        d.app().sequence_length() == 124 && d.app().sequence_cursor == 0,
        json!({"frames":124,"join":0}),
        d.snapshot(),
    )?;
    picture(d, 90)?;
    undo(d, &baseline)
}

fn native_text_and_blur(d: &mut Driver<'_>) -> Result<(), String> {
    d.key(Key::Escape)?;
    choose(d, 'a', false)?;
    d.events(
        "Native focus loss cancels the one-shot register choice",
        vec![Event::WindowFocused(false)],
    )?;
    d.check(
        "Losing native focus clears the pending register choice without erasing its contents",
        d.app().copied.selected().is_none()
            && original_range(d, 'a') == Some(70..74)
            && d.app().message.as_deref() == Some("Register choice cancelled."),
        json!({"selected":null,"a":[70,74],"message":"Register choice cancelled."}),
        json!({"registers":inventory(d),"message":d.app().message}),
    )?;
    d.events(
        "Restore native window focus",
        vec![Event::WindowFocused(true)],
    )?;
    choose(d, 'a', false)?;
    d.key(Key::Escape)?;
    d.check(
        "Escape replaces the selected-register instruction with explicit cancellation",
        d.app().copied.selected().is_none()
            && original_range(d, 'a') == Some(70..74)
            && d.app().message.as_deref() == Some("Register choice cancelled."),
        json!({"selected":null,"a":[70,74],"message":"Register choice cancelled."}),
        json!({"registers":inventory(d),"message":d.app().message}),
    )?;
    d.key(Key::Colon)?;
    d.events(
        "Quote and letter remain native command text",
        vec![
            key_event(Key::Quote, Modifiers::SHIFT, true),
            Event::Text("\"".into()),
            key_event(Key::Quote, Modifiers::SHIFT, false),
            key_event(Key::A, Modifiers::NONE, true),
            Event::Text("a".into()),
            key_event(Key::A, Modifiers::NONE, false),
        ],
    )?;
    d.check(
        "Register keys cannot consume text while the native command field owns input",
        d.app().command_open && d.app().command == "\"a" && d.app().copied.selected().is_none(),
        json!({"command":"\"a","selected":null}),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    d.command("sequence")?;
    d.key(Key::Escape)
}

fn command_blur(d: &mut Driver<'_>) -> Result<(), String> {
    for command in ["yank", "delete", "delete-frames 1f", "paste", "splice"] {
        goto(d, 20)?;
        choose(d, 'a', false)?;
        let baseline = document(d)?.clone();
        let revision = d.revision();
        let slots: Vec<_> = d
            .app()
            .copied
            .entries()
            .map(|(name, content)| (name, content.source()))
            .collect();
        let workspace = d
            .app()
            .workspace
            .as_ref()
            .ok_or("Missing command-blur workspace")?;
        let history = (workspace.can_undo, workspace.can_redo);
        d.key(Key::Colon)?;
        d.events(
            "Lose native focus after command entry captured a named register",
            vec![Event::WindowFocused(false)],
        )?;
        d.events(
            "Refocus the still-open native command field",
            vec![Event::WindowFocused(true)],
        )?;
        d.events(
            &format!("Submit :{command} after native focus cancelled its named register"),
            vec![
                Event::Text(command.into()),
                key_event(Key::Enter, Modifiers::NONE, true),
                key_event(Key::Enter, Modifiers::NONE, false),
            ],
        )?;
        d.wait_for(
            "Cancelled named command has no pending authored work",
            |app| !app.service.is_busy() && !app.copied.is_pending(),
        )?;
        let workspace = d
            .app()
            .workspace
            .as_ref()
            .ok_or("Named command lost its workspace")?;
        let current: Vec<_> = d
            .app()
            .copied
            .entries()
            .map(|(name, content)| (name, content.source()))
            .collect();
        d.check(
            "Blurred named commands reject explicitly without reviving the captured choice or using the default",
            d.app().error.as_deref().is_some_and(|error| error.contains("Register choice was cancelled"))
                && !d.app().command_open
                && d.app().copied.selected().is_none()
                && d.app().splice.is_none()
                && d.app().splice_abandon.is_none()
                && document(d)? == &baseline
                && d.revision() == revision
                && (workspace.can_undo, workspace.can_redo) == history
                && current == slots,
            json!({"command":command,"explicit_cancellation":true,"revision":revision,"history_and_all_slots_unchanged":true,"draft":null}),
            json!({"state":d.snapshot(),"registers":inventory(d),"history":[workspace.can_undo,workspace.can_redo]}),
        )?;
    }
    d.capture("Named command refuses after native focus cancels its register")?;

    // Ordinary default-copy commands have no one-shot named choice to revoke.
    goto(d, 20)?;
    let revision = d.revision();
    let named: Vec<_> = d
        .app()
        .copied
        .entries()
        .filter(|(name, _)| *name != '"')
        .map(|(name, content)| (name, content.source()))
        .collect();
    d.key(Key::Colon)?;
    d.events(
        "Blur an ordinary default-copy command",
        vec![Event::WindowFocused(false)],
    )?;
    d.events(
        "Refocus the default-copy command",
        vec![Event::WindowFocused(true)],
    )?;
    d.events(
        "Submit the unaffected default :yank command",
        vec![
            Event::Text("yank".into()),
            key_event(Key::Enter, Modifiers::NONE, true),
            key_event(Key::Enter, Modifiers::NONE, false),
        ],
    )?;
    wait_copy(d, '"')?;
    let copied = edited(d, '"')?;
    let current: Vec<_> = d
        .app()
        .copied
        .entries()
        .filter(|(name, _)| *name != '"')
        .map(|(name, content)| (name, content.source()))
        .collect();
    d.check(
        "Blur leaves an ordinary default-copy command usable without changing named slots",
        d.app().error.is_none()
            && d.revision() == revision
            && copied.slice().duration().frames() == 120
            && current == named,
        json!({"default_edit_frames":120,"revision":revision,"named_slots_unchanged":true}),
        inventory(d),
    )
}

fn command_reopen(d: &mut Driver<'_>) -> Result<(), String> {
    goto(d, 20)?;
    choose(d, 'a', false)?;
    let baseline = document(d)?.clone();
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("Missing reopen workspace")?;
    let path = workspace.path.clone();
    let old_session = workspace.session;
    let history = (workspace.can_undo, workspace.can_redo);
    let stored = bank_contents(d);
    let old_edited = edited(d, 'c')?;
    d.key(Key::Colon)?;
    d.events(
        "Type named :yank before the project session closes",
        vec![Event::Text("yank".into())],
    )?;
    d.app().service.submit(ProjectRequest::Close)?;
    d.wait_for(
        "Real service closes the project with its command field still open",
        |app| app.workspace.is_none() && !app.service.is_busy(),
    )?;
    d.check(
        "Closing the project clears runtime registers while preserving the open command entry",
        d.app().command_open
            && d.app().command == "yank"
            && d.app().copied.entries().next().is_none()
            && d.app().copied.selected().is_none()
            && !d.app().copied.is_pending(),
        json!({"command":"yank","registers":"empty","selected":null}),
        json!({"command":d.app().command,"state":d.snapshot(),"registers":inventory(d)}),
    )?;
    d.app().service.submit(ProjectRequest::Open(path))?;
    d.wait_for(
        "Real service reopens the same project in a fresh session",
        |app| {
            !app.service.is_busy()
                && app
                    .workspace
                    .as_ref()
                    .is_some_and(|workspace| workspace.session != old_session)
        },
    )?;
    d.key(Key::Enter)?;
    d.wait_for(
        "The stale named command submits no authored or capture work",
        |app| !app.service.is_busy() && !app.copied.is_pending(),
    )?;
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("Reopened command lost its workspace")?;
    let new_session = workspace.session;
    d.check(
        "Reopen restores every durable slot while the old named command is rejected",
        d.app().error.as_deref().is_some_and(|error| error.contains("Register choice was cancelled"))
            && !d.app().command_open
            && bank_contents(d) == stored
            && d.app().copied.selected().is_none()
            && d.app().splice.is_none()
            && workspace.session != old_session
            && document(d)? == &baseline
            && (workspace.can_undo, workspace.can_redo) == history,
        json!({"explicit_cancellation":true,"registers":"restored exactly","same_document_and_history":true,"fresh_session":true}),
        json!({"state":d.snapshot(),"registers":inventory(d),"old_session":old_session,"new_session":workspace.session}),
    )?;
    d.check(
        "Restored Original copies bind to the newly opened project session",
        matches!(content(d, 'a'), Some(Content::Original(copied))
            if copied.identity.session == new_session && copied.identity.session != old_session
                && copied.ordinals == (70..74)),
        json!({"fresh_session":true,"a":[70,74]}),
        inventory(d),
    )?;
    let restored = edited(d, 'c')?;
    d.check(
        "Restored Edited copies have fresh runtime identities and unchanged historical payloads",
        restored.id().session != old_edited.id().session
            && restored.id().persisted_version.is_some()
            && restored.slice() == old_edited.slice()
            && restored.scope() == old_edited.scope()
            && restored.bounds() == old_edited.bounds()
            && restored.source_path() == old_edited.source_path()
            && restored.child_label() == old_edited.child_label(),
        json!({"fresh_session":true,"persisted_namespace":true,"exact_historical_content":true}),
        json!({"old":format!("{:?}",old_edited.id()),"new":format!("{:?}",restored.id())}),
    )?;
    d.capture("Durable Original and Edited registers restored after reopen")?;
    d.key(Key::Escape)?;
    let copy = edited(d, '"')?;
    let request = crate::project::slice::CaptureRequest {
        id: copy.id().clone(),
        register: Some('z'),
        scope: copy.scope().clone(),
        parent: copy.slice().parent().clone(),
        selection: copy.slice().selection().clone(),
    };
    d.app_mut().copied.expect(
        request.clone(),
        crate::preview::edit_range::Selection::default(),
    );
    d.app()
        .service
        .submit(ProjectRequest::CaptureEditSlice(request))?;
    d.wait_for(
        "A restored identity cannot become a fresh register write",
        |app| !app.service.is_busy() && !app.copied.is_pending() && app.error.is_some(),
    )?;
    d.check(
        "Re-capture with a restored identity refuses without changing any slot or authored field",
        d.app()
            .error
            .as_deref()
            .is_some_and(|error| error.contains("Restored copy identities"))
            && bank_contents(d) == stored
            && document(d)? == &baseline,
        json!({"restored_capture_refused":true,"unchanged":true}),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    d.command("registers")?;
    d.capture("Restored typed register inventory")?;
    d.key(Key::Escape)?;
    d.command("sequence")?;
    for (name, frames, first_picture) in [('a', 4, 70), ('c', 17, 8)] {
        goto(d, 0)?;
        choose(d, name, false)?;
        let revision = d.revision();
        d.key_modified(Key::P, Modifiers::SHIFT)?;
        d.changed(&revision)?;
        d.check("A restored typed register pastes its exact historical contents in one edit",
            d.app().sequence_length() == 120 + frames && d.app().sequence_cursor == 0
                && bank_contents(d) == stored
                && (name != 'c' || document(d)?.nodes().values().any(|beat| matches!(&beat.kind, NodeKind::Hold { recipe } if recipe.duration.frames() == 11))),
            json!({"name":name,"added_frames":frames,"unchanged_registers":true}), d.snapshot())?;
        picture(d, first_picture)?;
        undo(d, &baseline)?;
    }
    Ok(())
}

fn choose(d: &mut Driver<'_>, name: char, uppercase: bool) -> Result<(), String> {
    let key = Key::from_name(&name.to_ascii_uppercase().to_string())
        .ok_or("Invalid replay register key")?;
    d.key_modified(Key::Quote, Modifiers::SHIFT)?;
    d.key_modified(
        key,
        if uppercase {
            Modifiers::SHIFT
        } else {
            Modifiers::NONE
        },
    )?;
    d.check(
        "Production quote and letter choose one normalized named register",
        d.app().copied.selected() == Some(name) && d.app().bindings.pending().is_empty(),
        json!({"selected":name,"uppercase_input":uppercase}),
        inventory(d),
    )
}

fn content<'a>(d: &'a Driver<'_>, name: char) -> Option<&'a Content> {
    d.app()
        .copied
        .entries()
        .find_map(|(key, value)| (key == name).then_some(value))
}

fn original_range(d: &Driver<'_>, name: char) -> Option<std::ops::Range<u64>> {
    match content(d, name) {
        Some(Content::Original(copied)) => Some(copied.ordinals.clone()),
        _ => None,
    }
}

fn edited(d: &Driver<'_>, name: char) -> Result<Arc<Captured>, String> {
    match content(d, name) {
        Some(Content::Edited(copied)) => Ok(copied.clone()),
        _ => Err(format!("Register {name} has no accepted edited capture")),
    }
}

fn same_edited(d: &Driver<'_>, name: char, expected: &Arc<Captured>) -> bool {
    matches!(content(d, name), Some(Content::Edited(actual)) if Arc::ptr_eq(actual, expected))
}

fn bank_contents(d: &Driver<'_>) -> Value {
    json!(d.app().copied.entries().map(|(name, content)| {
        let value = match content {
            Content::Original(copied) => json!({"type":"original","asset":copied.identity.asset,"qualification":copied.identity.qualification,"ordinals":copied.ordinals}),
            Content::Edited(copied) => json!({"type":"edited","slice":copied.slice(),"groups":copied.scope().groups(),"bounds":copied.bounds(),"path":copied.source_path(),"child":copied.child_label()}),
        };
        (name.to_string(), value)
    }).collect::<std::collections::BTreeMap<_, _>>())
}

fn inventory(d: &Driver<'_>) -> Value {
    json!(
        d.app()
            .copied
            .entries()
            .map(|(name, value)| (name.to_string(), value.label()))
            .collect::<std::collections::BTreeMap<_, _>>()
    )
}

fn wait_copy(d: &mut Driver<'_>, name: char) -> Result<(), String> {
    d.wait_for(
        "Named edited capture completes on the real service",
        |app| {
            !app.service.is_busy()
                && !app.copied.is_pending()
                && app
                    .copied
                    .entries()
                    .any(|(key, value)| key == name && matches!(value, Content::Edited(_)))
        },
    )
}

fn capture_update(d: &mut Driver<'_>, start: i64, end: i64) -> Result<ProjectUpdate, String> {
    let revision = d.revision();
    let session = d
        .app()
        .workspace
        .as_ref()
        .ok_or("Missing capture workspace")?
        .session;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(update) = d.app().service.take_update()
            && let Some(capture) = update.captured_slice.as_ref()
            && capture.id.source_revision.as_str() == revision
            && capture.id.session == session
        {
            let copied = capture.result.as_ref().map_err(Clone::clone)?;
            if copied.slice().revision_id().as_str() == revision
                && copied.slice().range().start() == ProjectFrame(start)
                && copied.slice().range().end() == ProjectFrame(end)
            {
                return Ok(update);
            }
        }
        if Instant::now() >= deadline {
            return Err("The genuine named-register capture reply did not arrive".into());
        }
        d.step(
            "Withhold only delivery while the genuine register capture completes",
            false,
        )?;
        d.wake
            .wait_until((Instant::now() + Duration::from_millis(16)).min(deadline));
    }
}

fn original_update(d: &mut Driver<'_>, start: u64, end: u64) -> Result<ProjectUpdate, String> {
    let revision = d.revision();
    let session = d
        .app()
        .workspace
        .as_ref()
        .ok_or("Missing Original capture workspace")?
        .session;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(update) = d.app().service.take_update()
            && let Some(capture) = update.captured_original.as_ref()
            && capture.id.source_revision.as_str() == revision
            && capture.id.session == session
        {
            capture.result.as_ref().map_err(Clone::clone)?;
            let exact = update
                .registers
                .as_ref()
                .and_then(|bank| bank.entries.get(&'"'));
            if matches!(exact, Some(crate::project::registers::Value::Original { ordinals, .. }) if *ordinals == (start..end))
            {
                return Ok(update);
            }
        }
        if Instant::now() >= deadline {
            return Err("The genuine Original register reply did not arrive".into());
        }
        d.step(
            "Withhold delivery while the durable Original copy completes",
            false,
        )?;
        d.wake
            .wait_until((Instant::now() + Duration::from_millis(16)).min(deadline));
    }
}

fn release(d: &mut Driver<'_>, update: ProjectUpdate) -> Result<(), String> {
    d.app_mut().feedback.release_project_update = Some(update);
    d.app_mut().feedback.hold_project_updates = false;
    d.step("Release the genuine delayed project capture update", false)
}

fn select(d: &mut Driver<'_>, start: u64, end: u64) -> Result<(), String> {
    goto(d, start)?;
    d.key(Key::V)?;
    motion(d, end - start)?;
    d.settled()
}

fn goto(d: &mut Driver<'_>, frame: u64) -> Result<(), String> {
    d.key(Key::Escape)?;
    d.chord(&[Key::G, Key::G])?;
    motion(d, frame)?;
    d.settled()
}

fn motion(d: &mut Driver<'_>, frames: u64) -> Result<(), String> {
    if frames == 0 {
        return Ok(());
    }
    for digit in frames.to_string().chars() {
        d.key(Key::from_name(&digit.to_string()).ok_or("Invalid replay count key")?)?;
    }
    d.key(Key::L)
}

fn document<'a>(d: &'a Driver<'_>) -> Result<&'a ProjectDocument, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.document.as_ref())
        .ok_or_else(|| "Named register replay lost its workspace".into())
}

fn same_document(actual: &ProjectDocument, expected: &ProjectDocument) -> Result<bool, String> {
    let mut actual = serde_json::to_value(actual).map_err(|error| error.to_string())?;
    actual["revision_id"] = json!(expected.revision_id());
    Ok(actual == serde_json::to_value(expected).map_err(|error| error.to_string())?)
}

fn undo(d: &mut Driver<'_>, expected: &ProjectDocument) -> Result<(), String> {
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    d.check(
        "One Undo restores every authored field after the named-register edit",
        same_document(document(d)?, expected)? && d.revision() != expected.revision_id().as_str(),
        json!("exact authored snapshot with a fresh revision"),
        d.snapshot(),
    )
}

fn picture(d: &mut Driver<'_>, source: u64) -> Result<(), String> {
    d.wait_for(
        "Named paste displays its exact first included source picture",
        |app| {
            !app.presentation.loading()
                && !app.presentation.needs_render()
                && app.presentation.displayed_source_frame() == Some(SourceFrameId(source))
        },
    )?;
    d.check(
        "Named paste displays the chosen source ordinal",
        d.app().presentation.displayed_source_frame() == Some(SourceFrameId(source)),
        json!(source),
        d.app().presentation.diagnostic_snapshot(),
    )
}

fn resize(d: &mut Driver<'_>, width: f32, height: f32) -> Result<(), String> {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing replay viewport")?
        .inner_rect = Some(rect);
    d.step("Resize the actual register workspace", false)?;
    d.settled()
}

fn painted(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    let paint = scenarios::text_paint_visibility(d, label);
    d.check(
        "Register name, type and guidance remain visibly painted within their clips",
        !paint.is_empty() && paint.iter().all(|part| part["fully_visible"] == true),
        json!(label),
        json!(paint),
    )
}
