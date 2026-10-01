//! Visual range deletion through the production UI and project service.
use super::*;
use deadpan_core::{FrameRange, ProjectDocument};
use egui::Key;

mod capture;
mod cut;
mod input;
mod nested;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let baseline = document(d)?.clone();
    let before = d.revision();
    d.chord(&[
        Key::G,
        Key::G,
        Key::Num2,
        Key::Num0,
        Key::L,
        Key::V,
        Key::Num1,
        Key::Num0,
        Key::L,
        Key::D,
    ])?;
    d.wait_for(
        "Visual d either commits or exposes its routing decision",
        |app| {
            app.workspace
                .as_ref()
                .is_some_and(|workspace| workspace.document.revision_id().as_str() != before)
                || app.bindings.pending() == "d"
                || app.error.is_some()
                || app.project_error.is_some()
        },
    )?;
    d.settled()?;
    d.check(
        "One Visual d removes [20,30) instead of arming whole-beat deletion",
        d.revision() != before && d.app().sequence_length() == 110 && d.app().sequence_cursor == 20,
        json!({"frames":110,"cursor":20,"one_visual_delete":true}),
        d.snapshot(),
    )?;
    picture(d, 30)?;
    d.key(Key::H)?;
    picture(d, 19)?;
    d.key(Key::L)?;
    picture(d, 30)?;
    undo(d, &baseline)?;

    d.check(
        "One Undo restores the protected initial edit",
        !d.app().workspace.as_ref().unwrap().can_undo,
        json!(false),
        d.snapshot(),
    )?;

    // A finished reverse selection owns its interval even after navigation.
    select(d, 60, 30, true)?;
    d.key(Key::L)?;
    let revision = d.revision();
    d.key(Key::D)?;
    d.changed(&revision)?;
    d.check(
        "Finished reverse Visual d cuts its fixed interval and selects its right join",
        d.app().sequence_cursor == 30
            && d.app().sequence_length() == 90
            && d.app().selected_edit_range().is_none(),
        json!({"cursor":30,"frames":90,"range_cleared":true}),
        d.snapshot(),
    )?;
    picture(d, 60)?;
    undo(d, &baseline)?;

    for finished in [false, true] {
        select(d, 20, 20, finished)?;
        let saved = document(d)?.clone();
        let selected = d.app().edit_range.clone();
        d.chord(&[Key::D, Key::D])?;
        d.wait_for("Empty selection produces no store request", |app| {
            !app.service.is_busy()
        })?;
        d.check(
            "Empty Visual selection never falls back to whole-beat deletion",
            *document(d)? == saved
                && d.app().edit_range == selected
                && d.app()
                    .error
                    .as_deref()
                    .is_some_and(|error| error.contains("empty")),
            json!({"finished":finished,"unchanged":true}),
            d.snapshot(),
        )?;
        d.command("delete")?;
        d.check(
            "The delete command retains an empty command-entry selection",
            *document(d)? == saved && d.app().edit_range == selected,
            json!("empty selection retained"),
            d.snapshot(),
        )?;
    }
    d.key(Key::Escape)?;
    let saved = document(d)?.clone();
    d.key(Key::D)?;
    d.check(
        "Without a Visual range the first d retains the whole-beat prefix",
        *document(d)? == saved && d.app().bindings.pending() == "d",
        json!("d"),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key(Key::D)?;
    d.wait_for("Whole-beat dd still commits", |app| {
        !app.service.is_busy() && app.sequence_length() == 0
    })?;
    d.check(
        "Without selection dd removes the whole beat once",
        d.revision() != revision,
        json!("whole beat deleted"),
        d.snapshot(),
    )?;
    undo(d, &baseline)?;

    capture::run(d)?;
    input::run(d)?;
    cut::run(d)?;
    nested::run(d)?;
    Ok(())
}

fn document<'a>(d: &'a Driver<'_>) -> Result<&'a ProjectDocument, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.document.as_ref())
        .ok_or_else(|| "No deletion workspace".into())
}

fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}

fn motion(d: &mut Driver<'_>, frames: u64, forward: bool) -> Result<(), String> {
    if frames == 0 {
        return Ok(());
    }
    for digit in frames.to_string().bytes() {
        d.key(
            [
                Key::Num0,
                Key::Num1,
                Key::Num2,
                Key::Num3,
                Key::Num4,
                Key::Num5,
                Key::Num6,
                Key::Num7,
                Key::Num8,
                Key::Num9,
            ][usize::from(digit - b'0')],
        )?;
    }
    d.key(if forward { Key::L } else { Key::H })
}

fn select(d: &mut Driver<'_>, start: u64, end: u64, finished: bool) -> Result<(), String> {
    d.key(Key::Escape)?;
    d.chord(&[Key::G, Key::G])?;
    motion(d, start - d.app().scope_start, true)?;
    d.key(Key::V)?;
    motion(d, end.abs_diff(start), end > start)?;
    if finished {
        d.key(Key::V)?;
    }
    d.settled()
}

fn undo(d: &mut Driver<'_>, expected: &ProjectDocument) -> Result<(), String> {
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    let mut actual = serde_json::to_value(document(d)?).map_err(|error| error.to_string())?;
    actual["revision_id"] = json!(expected.revision_id());
    d.check(
        "One Undo restores the complete authored snapshot",
        actual == serde_json::to_value(expected).unwrap(),
        json!("exact snapshot except fresh revision"),
        d.snapshot(),
    )
}

fn picture(d: &mut Driver<'_>, source: u64) -> Result<(), String> {
    let label = format!("Showing sequence frame {}", d.app().sequence_cursor + 1);
    d.wait_for("Deleted join decodes its retained Original frame", |app| {
        !app.presentation.loading()
            && !app.presentation.needs_render()
            && app.presentation.displayed_source_frame() == Some(SourceFrameId(source))
            && app.presentation.displayed_label().as_deref() == Some(label.as_str())
    })?;
    d.check(
        "Decoded picture matches the exact retained side of the deletion join",
        d.app().presentation.displayed_source_frame() == Some(SourceFrameId(source)),
        json!({"source_ordinal":source,"label":label}),
        d.app().presentation.diagnostic_snapshot(),
    )
}
