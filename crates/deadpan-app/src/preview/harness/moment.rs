//! Production-keyboard Original selection and single-transaction paste replay.

use super::*;
use egui::Key;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let original_duration = d.app().sequence_length();
    let initial_revision = d.revision();
    let original_nodes = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No project")?
        .document
        .nodes()
        .clone();
    d.command("source")?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L, Key::V])?;
    d.capture("Empty Visual entry teaches boundary movement")?;
    let guidance_paint = scenarios::text_paint_visibility(
        d,
        "Move with h/l to select a nonempty range. v finishes; Esc cancels.",
    );
    d.check(
        "Active empty selection teaches movement instead of starting again",
        d.app().moment.active
            && d.app().moment.range().is_none()
            && !guidance_paint.is_empty()
            && guidance_paint
                .iter()
                .all(|paint| paint["fully_visible"] == true),
        json!("active-empty selection with movement guidance"),
        json!({"state":d.snapshot(),"guidance_paint":guidance_paint}),
    )?;
    d.chord(&[Key::Num1, Key::Num4, Key::L])?;
    d.settled()?;
    d.capture("Original half-open moment, Out frame excluded")?;
    d.check(
        "Visual range is exact and does not edit the Original",
        d.app().moment.range() == Some(10..24)
            && d.app().moment.active
            && d.revision() == initial_revision,
        json!({"range":[10,24],"visual":true,"revision":initial_revision}),
        d.snapshot(),
    )?;
    d.key(Key::Y)?;
    d.check(
        "Yank leaves a reusable range and exits Visual without history",
        d.app()
            .moment
            .copied
            .as_ref()
            .is_some_and(|copy| copy.ordinals == (10..24))
            && !d.app().moment.active
            && d.revision() == initial_revision,
        json!("copied [10,24), unchanged revision"),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    d.check(
        "Escape cancels selection but preserves the copied moment",
        d.app().moment.range().is_none() && d.app().moment.copied.is_some(),
        json!("copy retained"),
        d.snapshot(),
    )?;
    d.command("sequence")?;
    d.key(Key::P)?;
    d.changed(&initial_revision)?;
    d.capture("Copied Original moment after the selected beat")?;
    d.check(
        "Paste selects a new Source in one committed revision",
        d.app().beat_rows.len() == 2
            && d.app().sequence_cursor == original_duration
            && d.app()
                .beat_rows
                .get(1)
                .is_some_and(|row| Some(&row.id) == d.app().selected_beat.as_ref()),
        json!("two beats, pasted Source selected at old edit end"),
        d.snapshot(),
    )?;
    let first_paste = d.revision();
    d.key(Key::U)?;
    d.changed(&first_paste)?;
    d.check(
        "One undo restores the exact Original structure",
        d.app()
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.document.nodes() == &original_nodes)
            && d.app().sequence_length() == original_duration,
        json!("original nodes and duration"),
        d.snapshot(),
    )?;
    let undone = d.revision();
    d.key_modified(Key::P, egui::Modifiers::SHIFT)?;
    d.changed(&undone)?;
    d.check(
        "Uppercase P pastes before the selected beat",
        d.app().beat_rows.len() == 2
            && d.app().sequence_cursor == 0
            && d.app()
                .beat_rows
                .first()
                .is_some_and(|row| Some(&row.id) == d.app().selected_beat.as_ref()),
        json!("pasted Source selected at edit start"),
        d.snapshot(),
    )?;
    d.capture("Copied Original moment before the selected beat")?;
    d.command("source")?;
    d.capture("Original controls on the frame after the context command")?;
    let before_pointer = d.revision();
    d.click("Select moment  v")?;
    d.key(Key::H)?;
    d.click("Copy moment  y")?;
    d.check(
        "Pointer copy owns Inspector focus and changes no revision",
        d.app().pane == Pane::Inspector
            && d.revision() == before_pointer
            && d.app()
                .moment
                .copied
                .as_ref()
                .is_some_and(|copy| copy.ordinals == (23..24)),
        json!("Inspector focus, copied [23,24), unchanged revision"),
        d.snapshot(),
    )?;
    d.command("sequence")?;
    d.capture("Edit controls on the frame after the context command")?;
    d.click("Paste after  p")?;
    d.changed(&before_pointer)?;
    d.check(
        "Pointer paste reaches the same committed Source path",
        d.app().beat_rows.len() == 3 && d.app().pane == Pane::Sequence,
        json!("three beats, committed Sequence focus"),
        d.snapshot(),
    )?;
    Ok(())
}
