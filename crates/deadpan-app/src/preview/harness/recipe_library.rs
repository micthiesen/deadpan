//! Specification §8.4: inspect a recipe's expansion before applying it,
//! seeded variation with a stored seed and resolved values, and saving a
//! modified group as a local recipe that can be inspected and reinserted,
//! through the production router, semantic project service and store.

use super::*;
use deadpan_core::NodeKind;
use egui::Key;

fn expansion(d: &Driver<'_>) -> Option<(String, Vec<String>)> {
    d.app().help_expansion.clone()
}

fn edit(d: &mut Driver<'_>, command: &str) -> Result<(), String> {
    let before = d.revision();
    d.command(command)?;
    d.changed(&before)?;
    d.settled()
}

/// Every gap Hold's duration in frames, in document order of their Repeat.
fn gap_frames(d: &Driver<'_>) -> Vec<i64> {
    let Some(workspace) = d.app().workspace.as_ref() else {
        return Vec::new();
    };
    let document = &workspace.document;
    let mut gaps = Vec::new();
    for node in document.nodes().values() {
        if let NodeKind::Repeat { gap: Some(gap), .. } = &node.kind {
            gaps.push(gap.duration.frames());
        }
    }
    for overrides in document.gap_overrides().values() {
        for (_, root) in overrides.iter() {
            if let Some(NodeKind::Hold { recipe }) = document.nodes().get(root).map(|n| &n.kind) {
                gaps.push(recipe.duration.frames());
            }
        }
    }
    gaps.sort_unstable();
    gaps
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.command("sequence")?;
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num3, Key::Num0, Key::L])?;
    let split = d.revision();
    d.key(Key::S)?;
    d.changed(&split)?;
    d.settled()?;

    // Inspect before applying: nothing changes.
    let before = d.revision();
    d.command("gag-inspect one-more-time plays=3 gap=12f shorten=2f vary=20% seed=7")?;
    d.settled()?;
    let shown = expansion(d);
    d.check(
        ":gag-inspect lists the exact steps, resolved gaps and pinned seed in Help without an edit",
        d.revision() == before
            && d.app().help_open
            && shown.as_ref().is_some_and(|(name, rows)| {
                name == "One More Time"
                    && rows.len() == 3
                    && rows[0] == "Repeat the selected beat for 3 plays"
                    && rows[1].starts_with("Silent freeze gaps after each play but the last: ")
                    && rows[2].contains("varied ±20% (seed 7)")
            }),
        json!({"steps":3,"revision":before}),
        json!({"expansion":shown,"revision":d.revision(),"help":d.app().help_open,"error":d.app().error}),
    )?;
    d.capture("Recipe expansion listed in Help before applying")?;
    d.key(Key::Escape)?;
    d.command("gag-inspect one-more-time vary=20%")?;
    d.settled()?;
    let unseeded = expansion(d);
    d.check(
        ":gag-inspect without seed= names the drawn seed and how to apply exactly it",
        unseeded.as_ref().is_some_and(|(_, rows)| {
            rows.last()
                .is_some_and(|row| row.contains("Apply with seed="))
        }),
        json!("Apply with seed=N"),
        json!(unseeded),
    )?;
    d.key(Key::Escape)?;
    d.settled()?;

    // Apply it: the gaps are the inspected, seeded values, stored as exact
    // gap durations, and the label pins the seed.
    let listed: Vec<i64> = shown
        .as_ref()
        .and_then(|(_, rows)| rows.get(1))
        .map(|row| {
            row.rsplit(": ")
                .next()
                .unwrap_or_default()
                .split(", ")
                .filter_map(|gap| gap.trim_end_matches('f').parse().ok())
                .collect()
        })
        .unwrap_or_default();
    edit(
        d,
        "gag one-more-time plays=3 gap=12f shorten=2f vary=20% seed=7",
    )?;
    let group = d.app().selected_beat.clone().ok_or("No gag group")?;
    let label = d
        .app()
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.document.nodes().get(&group))
        .map(|node| node.label.clone())
        .unwrap_or_default();
    let mut expected = listed.clone();
    expected.sort_unstable();
    let stored = gap_frames(d);
    d.check(
        "The applied gag stores the inspected seeded gaps and pins the seed in its label",
        listed.len() == 2
            && stored == expected
            && label.ends_with("varied ±20% (seed 7)")
            && listed != vec![12, 10],
        json!({"gaps":expected,"label":"… varied ±20% (seed 7)"}),
        json!({"gaps":stored,"listed":listed,"label":label,"error":d.app().error}),
    )?;

    // Save the gag group as local recipe a, inspect it and reinsert it.
    let before = d.revision();
    d.command("recipe-save a")?;
    d.wait_for("The local recipe is saved", |app| {
        !app.service.is_busy() && !app.macros.is_pending() && !app.copied.is_pending()
    })?;
    d.settled()?;
    let saved = d.app().copied.entries().any(|(slot, content)| {
        slot == 'a' && matches!(content, crate::preview::copied::Content::Edited(_))
    });
    d.check(
        ":recipe-save a keeps the selected group in register a without an edit",
        saved && d.revision() == before && d.app().error.is_none(),
        json!({"register":"a","revision":before}),
        json!({"saved":saved,"revision":d.revision(),"error":d.app().error}),
    )?;
    d.command("recipe-inspect a")?;
    d.settled()?;
    let outline = expansion(d);
    d.check(
        ":recipe-inspect a lists the saved group and its parts",
        outline.as_ref().is_some_and(|(name, rows)| {
            name == "Local recipe a"
                && rows
                    .first()
                    .is_some_and(|row| row.contains(&label) && row.contains("group of"))
                && rows.len() > 1
        }),
        json!({"first":label}),
        json!({"outline":outline}),
    )?;
    d.key(Key::Escape)?;
    d.settled()?;
    let grown = d.app().sequence_length();
    let group_frames = d
        .app()
        .beat_rows
        .iter()
        .find(|row| row.id == group)
        .map_or(0, |row| row.frames);
    d.chord(&[Key::G, Key::G])?;
    d.settled()?;
    edit(d, "recipe a")?;
    d.check(
        ":recipe a inserts a fresh copy of the saved group",
        group_frames > 0 && d.app().sequence_length() == grown + group_frames,
        json!(grown + group_frames),
        json!({"frames":d.app().sequence_length(),"error":d.app().error}),
    )?;
    d.capture("Local recipe reinserted")?;
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    d.settled()?;
    d.check(
        "One undo removes the inserted recipe",
        d.app().sequence_length() == grown,
        json!(grown),
        json!(d.app().sequence_length()),
    )
}
