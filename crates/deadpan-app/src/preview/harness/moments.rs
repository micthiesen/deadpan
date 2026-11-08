//! Named Original moments through production keys, commands and the rail.

use super::*;
use deadpan_store::{AccessMode, ProjectStore, moments::MomentCatalog};
use egui::Key;

fn catalog(d: &Driver<'_>) -> Result<MomentCatalog, String> {
    ProjectStore::open(
        &d.app().workspace.as_ref().ok_or("No project")?.path,
        AccessMode::ReadOnly,
    )
    .map_err(|error| error.to_string())?
    .moment_catalog()
    .map_err(|error| error.to_string())
}

fn saved(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Named moment owner reply", |app| {
        app.moments.ready_for_check() && !app.service.is_busy()
    })?;
    d.settled()
}

/// Names, tags and range of every saved moment, in ID order.
fn summary(catalog: &MomentCatalog) -> Vec<(String, Vec<String>, u64, u64)> {
    let mut entries: Vec<_> = catalog
        .entries
        .iter()
        .map(|moment| {
            (
                moment.name.as_str().to_owned(),
                moment
                    .tags
                    .as_slice()
                    .iter()
                    .map(|tag| tag.as_str().to_owned())
                    .collect(),
                moment.start,
                moment.end,
            )
        })
        .collect();
    entries.sort();
    entries
}

fn tags(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    let sequence_cursor = d.app().sequence_cursor;
    saved(d)?;
    d.command("source")?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L, Key::V])?;
    d.chord(&[Key::Num1, Key::Num4, Key::L, Key::V])?;
    d.check(
        "The Original moment to name is selected without editing",
        d.app().moment.range() == Some(10..24) && d.revision() == revision,
        json!({"range":[10,24],"revision":revision}),
        d.snapshot(),
    )?;
    d.command("moment Big laugh tags=Reaction,detail")?;
    saved(d)?;
    d.check(
        "Saving names the selected moment with normalized tags outside history",
        summary(&catalog(d)?) == vec![("Big laugh".into(), tags(&["detail", "reaction"]), 10, 24)]
            && d.revision() == revision
            && d.app().moments.selected_name_for_check() == Some("Big laugh"),
        json!({"moment":"Big laugh","tags":["detail","reaction"],"range":[10,24],"revision":revision}),
        json!({"catalog":summary(&catalog(d)?),"state":d.snapshot()}),
    )?;
    d.key(Key::Escape)?;
    d.chord(&[Key::G, Key::G, Key::Num3, Key::Num0, Key::L, Key::V])?;
    d.chord(&[Key::Num6, Key::L, Key::V])?;
    d.command("moment Sigh tags=breath")?;
    saved(d)?;
    d.command("moment Sigh")?;
    saved(d)?;
    let duplicate_error = d.app().error.clone();
    d.check(
        "A second moment saves and a duplicate name is refused without a write",
        summary(&catalog(d)?).len() == 2
            && catalog(d)?.version == 2
            && duplicate_error
                .as_deref()
                .is_some_and(|error| error.contains("already uses this name")),
        json!({"moments":2,"version":2,"duplicate":"refused"}),
        json!({"catalog":summary(&catalog(d)?),"error":duplicate_error}),
    )?;
    d.key(Key::Escape)?;
    d.command("moment Nothing selected")?;
    saved(d)?;
    d.check(
        "Naming without an Original selection is refused",
        summary(&catalog(d)?).len() == 2
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Select a nonempty Original range")),
        json!("refused without a selection"),
        json!({"catalog":summary(&catalog(d)?),"error":d.app().error}),
    )?;
    d.capture("Two named moments in the Original rail")?;
    for label in ["MOMENTS", "Big laugh", "Sigh"] {
        let paint = scenarios::text_paint_visibility(d, label);
        d.check(
            "Saved moments are painted in the Original rail",
            !paint.is_empty() && paint.iter().any(|item| item["fully_visible"] == true),
            json!(label),
            json!(paint),
        )?;
    }

    // Leave the Original; Enter on a filtered moment comes back to it.
    d.command("sequence")?;
    d.command("moments tag:reaction")?;
    d.step("Focus the moment filter", false)?;
    d.check(
        "tag: keeps only moments with that exact tag, and the filter has focus",
        d.app().moments.visible_names_for_check() == ["Big laugh"]
            && d.harness
                .ctx
                .memory(|memory| memory.has_focus(egui::Id::new("moment-filter"))),
        json!(["Big laugh"]),
        json!({"visible":d.app().moments.visible_names_for_check(),"state":d.snapshot()}),
    )?;
    d.capture("Moments filtered by tag:reaction")?;
    d.key(Key::ArrowDown)?;
    d.key(Key::Enter)?;
    d.settled()?;
    d.check(
        "Enter selects the moment in Original at its In without editing",
        d.app().view == View::Source
            && d.app().moment.range() == Some(10..24)
            && !d.app().moment.active
            && d.app().source_cursor == 10
            && d.app().sequence_cursor == sequence_cursor
            && d.revision() == revision
            && !d
                .harness
                .ctx
                .memory(|memory| memory.has_focus(egui::Id::new("moment-filter"))),
        json!({"view":"Original","range":[10,24],"source_cursor":10,"revision":revision}),
        d.snapshot(),
    )?;
    d.capture("A named moment selected in the Original")?;

    d.command("moment-rename Huge laugh")?;
    saved(d)?;
    d.command("moment-tags reaction,breath")?;
    saved(d)?;
    d.check(
        "Rename and retag change only the selected moment's label and tags",
        summary(&catalog(d)?)
            == vec![
                ("Huge laugh".into(), tags(&["breath", "reaction"]), 10, 24),
                ("Sigh".into(), tags(&["breath"]), 30, 36),
            ]
            && d.revision() == revision,
        json!({"renamed":"Huge laugh","tags":["breath","reaction"],"revision":revision}),
        json!(summary(&catalog(d)?)),
    )?;
    d.command("moments brth")?;
    d.step("Focus the moment filter", false)?;
    d.check(
        "Loose words match tags too, listed in Original order",
        d.app().moments.visible_names_for_check() == ["Huge laugh", "Sigh"],
        json!(["Huge laugh", "Sigh"]),
        json!(d.app().moments.visible_names_for_check()),
    )?;
    d.key(Key::Escape)?;
    d.command("moment-delete")?;
    saved(d)?;
    d.check(
        "Delete removes only the selected moment and leaves the edit unchanged",
        summary(&catalog(d)?) == vec![("Sigh".into(), tags(&["breath"]), 30, 36)]
            && d.revision() == revision,
        json!({"remaining":"Sigh","revision":revision}),
        json!(summary(&catalog(d)?)),
    )?;

    // A fresh catalog read for the session (as on reopen) shows the saved list.
    d.app_mut().moments = Default::default();
    d.step("Reload named moments", false)?;
    saved(d)?;
    d.command("moments")?;
    d.step("Focus the moment filter", false)?;
    d.check(
        "Reloading the catalog shows the persisted moment",
        d.app().moments.visible_names_for_check() == ["Sigh"] && d.revision() == revision,
        json!(["Sigh"]),
        json!(d.app().moments.visible_names_for_check()),
    )?;
    d.key(Key::Escape)?;
    d.capture("The persisted moment after reload")?;
    Ok(())
}
