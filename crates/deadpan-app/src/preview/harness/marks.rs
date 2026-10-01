//! Real mark commands and decoded pictures through the production keyboard path.

use super::*;
use deadpan_core::{Anchor, Mark, MarkState};
use egui::Key;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    goto(d, 20)?;
    let old = d.app().capture_mark()?;
    let revision = d.revision();
    d.key(Key::M)?;
    d.check(
        "Mark prefix captures its position without writing",
        d.app().bindings.pending() == "m" && d.revision() == revision,
        json!("m pending; unchanged revision"),
        d.snapshot(),
    )?;
    d.step("A slow mark prefix keeps its meaning", false)?;
    d.capture("Visible pending mark prefix and captured Edit boundary")?;
    d.key(Key::A)?;
    d.changed(&revision)?;
    d.check(
        "Setting a mark preserves Edit cursor, pane and selected beat",
        d.app().sequence_cursor == 20
            && d.app().view == View::Sequence
            && d.app().capture_mark()?.location_matches_for_check(&old),
        json!("same navigation at Edit 20"),
        d.snapshot(),
    )?;
    d.check("Mark a retains an explicit host occurrence at local frame 20", matches!(&mark(d, 'a')?.boundary.coordinate, Anchor::Occurrence { position, .. } if *position == deadpan_core::ExactRatio::integer(20)), json!(20), json!(mark(d, 'a')?))?;

    goto(d, 40)?;
    jump(d, Key::A)?;
    cursor(d, View::Sequence, 20)?;
    d.key_modified(Key::O, egui::Modifiers::CTRL)?;
    cursor(d, View::Sequence, 40)?;
    d.key_modified(Key::I, egui::Modifiers::CTRL)?;
    cursor(d, View::Sequence, 20)?;

    // A metadata revision preserves a live Visual range and both jump branches.
    d.key(Key::V)?;
    d.chord(&[Key::Num5, Key::L])?;
    let range = d.app().selected_edit_range();
    let revision = d.revision();
    d.chord(&[Key::M, Key::B])?;
    d.changed(&revision)?;
    d.check(
        "Saving a mark does not clear or grow the active Visual range",
        d.app().edit_range.active
            && d.app().selected_edit_range() == range
            && d.app().sequence_cursor == 25,
        json!({"range":[20,25],"active":true}),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    d.key_modified(Key::O, egui::Modifiers::CTRL)?;
    cursor(d, View::Sequence, 40)?;
    d.key_modified(Key::I, egui::Modifiers::CTRL)?;
    cursor(d, View::Sequence, 25)?;

    // A new jump abandons forward history; changing a letter cannot rewrite it.
    d.key_modified(Key::O, egui::Modifiers::CTRL)?;
    goto(d, 60)?;
    let revision = d.revision();
    d.chord(&[Key::M, Key::A])?;
    d.changed(&revision)?;
    jump(d, Key::B)?;
    cursor(d, View::Sequence, 25)?;
    d.key_modified(Key::I, egui::Modifiers::CTRL)?;
    d.check(
        "New jump clears the forward branch",
        d.app().sequence_cursor == 25 && d.app().message.as_deref() == Some("No later jump."),
        json!("no later jump"),
        d.snapshot(),
    )?;
    d.key_modified(Key::O, egui::Modifiers::CTRL)?;
    cursor(d, View::Sequence, 60)?;

    d.command("source")?;
    goto(d, 10)?;
    let edit_cursor = d.app().sequence_cursor;
    let revision = d.revision();
    d.chord(&[Key::M, Key::S])?;
    d.changed(&revision)?;
    d.check(
        "Original mark save never switches to Your edit or changes its retained cursor",
        d.app().view == View::Source
            && d.app().source_cursor == 10
            && d.app().sequence_cursor == edit_cursor,
        json!({"Original":10,"Edit":edit_cursor}),
        d.snapshot(),
    )?;
    d.key_modified(Key::G, egui::Modifiers::SHIFT)?;
    let terminal = d.app().source_cursor;
    let revision = d.revision();
    d.key(Key::M)?;
    d.key_modified(Key::S, egui::Modifiers::SHIFT)?;
    d.changed(&revision)?;
    d.check(
        "Uppercase S is a separate exact terminal Original mark",
        mark(d, 's')? != mark(d, 'S')? && d.app().source_cursor == terminal,
        json!({"lower":10,"upper":terminal}),
        json!({"s":mark(d,'s')?,"S":mark(d,'S')?}),
    )?;
    d.command("sequence")?;
    jump(d, Key::S)?;
    cursor(d, View::Source, 10)?;
    d.key_modified(Key::O, egui::Modifiers::CTRL)?;
    cursor(d, View::Sequence, edit_cursor)?;
    d.key_modified(Key::I, egui::Modifiers::CTRL)?;
    cursor(d, View::Source, 10)?;

    // A delayed real mark resolution cannot override subsequent navigation.
    d.app_mut().feedback.hold_project_updates = true;
    d.chord(&[Key::Quote, Key::A])?;
    d.wait_for("Mark resolves while UI delivery is withheld", |app| {
        !app.service.is_busy()
    })?;
    d.key(Key::L)?;
    let retained = d.app().capture_mark()?;
    d.app_mut().feedback.hold_project_updates = false;
    d.step("Deliver resolved mark after explicit navigation", false)?;
    d.settled()?;
    d.check(
        "Late mark jump cannot steal the current Original picture or cursor",
        d.app().capture_mark()? == retained,
        json!("retained Original position"),
        d.snapshot(),
    )?;

    // Held movement keys must not become the second letter of a mark.
    let revision = d.revision();
    d.events(
        "Hold h before entering a mark prefix",
        vec![egui::Event::Key {
            key: Key::H,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    )?;
    d.key(Key::M)?;
    d.events(
        "Held h cannot finish mh",
        vec![egui::Event::Key {
            key: Key::H,
            physical_key: None,
            pressed: true,
            repeat: true,
            modifiers: egui::Modifiers::NONE,
        }],
    )?;
    d.check(
        "Held navigation keeps the exact mark prefix pending",
        d.app().bindings.pending() == "m" && d.revision() == revision && !has_mark(d, 'h'),
        json!("m remains pending"),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    d.events(
        "Release the held h",
        vec![egui::Event::Key {
            key: Key::H,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    )?;

    // Leave Original through an actual mark jump, so the trail contains its
    // valid departure behind the soon-to-expire Edit positions.
    jump(d, Key::A)?;
    cursor(d, View::Sequence, 60)?;
    let before = d.app().capture_mark()?;
    let revision = d.revision();
    d.command("jump z")?;
    d.wait_for("Missing mark reports its exact error", |app| {
        !app.service.is_busy() && app.error.is_some()
    })?;
    d.check(
        "A missing mark leaves the document and exact navigation unchanged",
        d.revision() == revision && d.app().capture_mark()? == before,
        json!("missing mark; unchanged"),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;

    // Rendering and native-control focus of the complete persistent mark list.
    for size in [egui::vec2(960.0, 640.0), egui::vec2(1280.0, 820.0)] {
        resize(d, size)?;
        d.command("marks")?;
        d.check(
            "Marks opens as a modal with the captured Edit context",
            d.app().marks.open,
            json!(true),
            d.snapshot(),
        )?;
        for text in [
            "Marks",
            "Save this position",
            "Back to editor · Esc",
            "Jump 'a",
            "Jump 's",
            "Jump 'S",
        ] {
            painted(d, text)?;
        }
        for _ in 0..3 {
            d.step("Marks stays stable without another input", false)?;
        }
        d.check(
            "Settled Marks layout needs one paint pass",
            d.snapshot()["layout_passes"] == json!(1),
            json!(1),
            d.snapshot()["layout_passes"].clone(),
        )?;
        d.capture("Saved Original and Edit marks with keyboard controls")?;
        let snapshot = document(d)?.clone();
        d.events(
            "IME owns mark dialog confirmation",
            vec![
                egui::Event::Ime(egui::ImeEvent::Preedit {
                    text: "a".into(),
                    active_range_chars: Some(0..1),
                }),
                egui::Event::Key {
                    key: Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        )?;
        d.check(
            "IME confirmation cannot save or jump a mark",
            *document(d)? == snapshot && d.app().marks.open,
            json!("modal and document retained"),
            d.snapshot(),
        )?;
        d.events(
            "Finish mark dialog composition",
            vec![egui::Event::Ime(egui::ImeEvent::Commit("a".into()))],
        )?;
        d.key(Key::Escape)?;
        d.wait_for("Marks closes after explicit Escape", |app| !app.marks.open)?;
    }

    d.command("marks")?;
    focus_by_tab(d, "Save this position")?;
    let revision = d.revision();
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    d.check(
        "Tab and Enter save the captured modal position without closing it",
        d.app().marks.open && d.app().sequence_cursor == 60,
        json!("modal retained at Edit 60"),
        d.snapshot(),
    )?;
    focus_by_tab(d, "Remove b")?;
    let revision = d.revision();
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    d.check(
        "Tab and Enter remove exactly the focused letter",
        !has_mark(d, 'b') && d.app().marks.open,
        json!("b removed; modal retained"),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    // A metadata Undo has a new revision. Older Edit history expires, while
    // the independently qualified Original remains reachable.
    d.command("marks")?;
    d.key_modified(Key::O, egui::Modifiers::CTRL)?;
    cursor(d, View::Source, 10)?;
    d.check(
        "Ctrl O works inside Marks and skips expired Edit positions",
        !d.app().marks.open,
        json!("modal closed; Original 10"),
        d.snapshot(),
    )?;
    d.command("marks")?;
    d.key_modified(Key::I, egui::Modifiers::CTRL)?;
    cursor(d, View::Sequence, 60)?;
    d.check(
        "Ctrl I works inside Marks and restores the current Edit departure",
        !d.app().marks.open,
        json!("modal closed; Edit 60"),
        d.snapshot(),
    )?;

    // Mark deletion is one reversible metadata transaction.
    let revision = d.revision();
    d.command("unmark b")?;
    d.changed(&revision)?;
    d.check(
        "Removing b leaves other letters and both media clocks intact",
        !has_mark(d, 'b')
            && has_mark(d, 'a')
            && has_mark(d, 's')
            && d.app().sequence_length() == 120,
        json!("only b removed"),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    d.check(
        "One Undo restores the removed mark",
        has_mark(d, 'b'),
        json!(true),
        d.snapshot(),
    )?;

    // Edits transform durable marks while raw history locations expire explicitly.
    goto(d, 0)?;
    let revision = d.revision();
    d.chord(&[Key::V, Key::Num1, Key::Num0, Key::L, Key::D])?;
    d.changed(&revision)?;
    jump(d, Key::A)?;
    cursor(d, View::Sequence, 50)?;
    d.check(
        "Persistent mark follows the original content after ripple cut",
        mark(d, 'a')?.state == MarkState::Bound && d.app().sequence_length() == 110,
        json!("old Edit 60 follows content to Edit 50"),
        json!(mark(d, 'a')?),
    )?;
    jump(d, Key::S)?;
    cursor(d, View::Source, 10)?;
    let path = d.app().workspace.as_ref().unwrap().path.clone();
    d.app().service.submit(ProjectRequest::Close)?;
    d.wait_for("Close releases the marked project", |app| {
        app.workspace.is_none() && !app.service.is_busy()
    })?;
    d.app().service.submit(ProjectRequest::Open(path))?;
    d.wait_for("Reopen loads persistent marks", |app| {
        app.workspace.is_some() && !app.service.is_busy()
    })?;
    jump(d, Key::S)?;
    cursor(d, View::Source, 10)?;
    d.key_modified(Key::O, egui::Modifiers::CTRL)?;
    cursor(d, View::Sequence, 0)?;
    // Fill the bounded list through real command entry, then traverse its
    // off-screen rows using native Tab. A hidden last row must be revealed.
    for letter in ('a'..='z').chain('A'..='Z') {
        let revision = d.revision();
        d.command(&format!("mark {letter}"))?;
        d.changed(&revision)?;
    }
    resize(d, egui::vec2(960.0, 640.0))?;
    d.command("marks")?;
    focus_by_tab(d, "Remove Z")?;
    d.check(
        "All 52 marks remain reachable with native Tab at minimum size",
        document(d)?.marks().len() == 52 && d.app().marks.open,
        json!(52),
        json!(document(d)?.marks().len()),
    )?;
    for _ in 0..3 {
        d.step("Settle final mark row reveal", false)?;
    }
    painted(d, "Remove Z")?;
    d.capture("Last mark row revealed by keyboard at minimum size")?;
    d.key(Key::Tab)?;
    d.step("Wrap mark list focus", false)?;
    d.check(
        "Native Tab wraps from the last mark action to its letter field",
        control_focused(d, "Letter"),
        json!("Letter"),
        d.widgets(),
    )?;
    d.key_modified(Key::Tab, egui::Modifiers::SHIFT)?;
    for _ in 0..3 {
        d.step("Settle reverse mark focus", false)?;
    }
    d.check(
        "Shift Tab returns to the last mark action without leaving the modal",
        control_focused(d, "Remove Z"),
        json!("Remove Z"),
        d.widgets(),
    )?;
    painted(d, "Remove Z")?;
    d.key(Key::Escape)?;
    d.report.skipped.push("Marks use real project writes, anchor transforms and decoded pictures. Delayed service delivery and IME input are explicitly injected; physical key layouts, native IME and VoiceOver remain separate acceptance.".into());
    Ok(())
}

fn document<'a>(d: &'a Driver<'_>) -> Result<&'a deadpan_core::ProjectDocument, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.document.as_ref())
        .ok_or_else(|| "Marks replay has no project".into())
}
fn mark<'a>(d: &'a Driver<'_>, letter: char) -> Result<&'a Mark, String> {
    document(d)?
        .marks()
        .get(&crate::project::marks::mark_id(letter)?)
        .ok_or_else(|| format!("Missing mark {letter}"))
}
fn has_mark(d: &Driver<'_>, letter: char) -> bool {
    mark(d, letter).is_ok()
}
fn goto(d: &mut Driver<'_>, frame: u64) -> Result<(), String> {
    d.key(Key::Escape)?;
    d.chord(&[Key::G, Key::G])?;
    if frame > 0 {
        for digit in frame.to_string().chars() {
            let key = match digit {
                '0' => Key::Num0,
                '1' => Key::Num1,
                '2' => Key::Num2,
                '3' => Key::Num3,
                '4' => Key::Num4,
                '5' => Key::Num5,
                '6' => Key::Num6,
                '7' => Key::Num7,
                '8' => Key::Num8,
                _ => Key::Num9,
            };
            d.key(key)?;
        }
        d.key(Key::L)?;
    }
    d.settled()
}
fn focus_by_tab(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    for _ in 0..128 {
        if control_focused(d, label) {
            for _ in 0..3 {
                d.step("Settle native mark row reveal", false)?;
            }
            return painted(d, label);
        }
        d.key(Key::Tab)?;
        d.step("Settle native mark control focus", false)?;
    }
    Err(format!("Native Tab could not reach mark control {label:?}"))
}
fn control_focused(d: &Driver<'_>, label: &str) -> bool {
    d.harness.root().children_recursive().any(|node| {
        let access = node.accesskit_node();
        access.is_focused()
            && access.label().as_deref() == Some(label)
            && !access.is_disabled()
            && !access.is_hidden()
    })
}
fn jump(d: &mut Driver<'_>, letter: Key) -> Result<(), String> {
    d.chord(&[Key::Quote, letter])?;
    d.wait_for("Mark navigation completes", |app| {
        !app.service.is_busy() && !app.marks.is_pending()
    })?;
    d.settled()
}
fn cursor(d: &mut Driver<'_>, view: View, at: u64) -> Result<(), String> {
    d.settled()?;
    d.check(
        "Jump retains the precise coordinate domain and boundary",
        d.app().view == view
            && match view {
                View::Source => d.app().source_cursor == at,
                View::Sequence => d.app().sequence_cursor == at,
            },
        json!({"view":format!("{view:?}"),"at":at}),
        d.snapshot(),
    )
}
fn resize(d: &mut Driver<'_>, size: egui::Vec2) -> Result<(), String> {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing marks viewport")?
        .inner_rect = Some(rect);
    d.step("Resize marks workspace", false)
}
fn painted(d: &mut Driver<'_>, text: &str) -> Result<(), String> {
    let painted = scenarios::text_paint_visibility(d, text);
    d.check(
        "Mark control text is completely visible",
        !painted.is_empty() && painted.iter().all(|entry| entry["fully_visible"] == true),
        json!(text),
        json!(painted),
    )
}
