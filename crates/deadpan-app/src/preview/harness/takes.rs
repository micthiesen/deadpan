//! Named versions through native text, focus, commands and durable history.

use super::*;
use deadpan_store::{AccessMode, ProjectStore};
use egui::{Key, Modifiers};

fn catalog(d: &Driver<'_>) -> Result<deadpan_store::takes::TakeCatalog, String> {
    ProjectStore::open(
        &d.app().workspace.as_ref().ok_or("No project")?.path,
        AccessMode::ReadOnly,
    )
    .map_err(|error| error.to_string())?
    .take_catalog()
    .map_err(|error| error.to_string())
}

fn ready(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Named takes owner reply", |app| {
        app.takes.ready_for_check() && !app.service.is_busy()
    })?;
    d.step("Paint named takes", false)
}

fn focus(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    for _ in 0..40 {
        if d.harness.root().children_recursive().any(|node| {
            let access = node.accesskit_node();
            access.is_focused() && !access.is_disabled() && access.label().as_deref() == Some(label)
        }) {
            return Ok(());
        }
        d.key(Key::Tab)?;
        d.step("Settle take control focus", false)?;
    }
    Err(format!("Tab could not reach {label}"))
}

fn activate(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    focus(d, label)?;
    d.key(Key::Enter)
}

fn name(d: &mut Driver<'_>, value: &str) -> Result<(), String> {
    focus(d, "Take name")?;
    d.key_modified(Key::A, Modifiers::COMMAND)?;
    d.events(
        "Type a take name using native text editing",
        vec![egui::Event::Text(value.into())],
    )
}

fn close(d: &mut Driver<'_>) -> Result<(), String> {
    // A native field may consume its first Escape to relinquish focus.
    activate(d, "Back to editor  Esc")?;
    d.step("Restore editor focus after the take modal", false)?;
    d.settled()
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let original = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No project")?
        .document
        .clone();
    let revision = d.revision();
    let cursor = d.app().sequence_cursor;
    d.command("takes")?;
    ready(d)?;
    name(d, "First version")?;
    activate(d, "Save new take")?;
    ready(d)?;
    d.check(
        "Saving a named take retains the current edit and cursor",
        catalog(d)?.entries.len() == 1
            && d.revision() == revision
            && d.app().sequence_cursor == cursor,
        json!({"takes":1,"revision":revision,"cursor":cursor}),
        d.snapshot(),
    )?;
    activate(d, "First version")?;
    d.chord(&[Key::H, Key::D, Key::D])?;
    d.check(
        "The take modal owns ordinary editor keys",
        d.revision() == revision && d.app().sequence_cursor == cursor,
        json!({"revision":revision,"cursor":cursor}),
        d.snapshot(),
    )?;
    for (width, height) in [(960.0, 640.0), (1280.0, 820.0)] {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
        let input = d.harness.input_mut();
        input.screen_rect = Some(rect);
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .ok_or("Missing take replay viewport")?
            .inner_rect = Some(rect);
        d.step("Resize named takes", true)?;
        d.settled()?;
        d.capture(&format!("Named takes at {width}x{height}"))?;
        for label in [
            "Named takes",
            "Save new take",
            "Open selected take",
            "Update to current edit",
            "Rename selected take",
            "Delete selected take",
            "Back to editor  Esc",
        ] {
            let paint = scenarios::text_paint_visibility(d, label);
            d.check(
                "Named take controls remain painted after resize",
                !paint.is_empty() && paint.iter().all(|item| item["fully_visible"] == true),
                json!(label),
                json!(paint),
            )?;
        }
    }
    name(d, "First version")?;
    activate(d, "Save new take")?;
    ready(d)?;
    d.check(
        "Duplicate names fail without changing the saved take or head",
        catalog(d)?.entries.len() == 1
            && d.revision() == revision
            && d.app().takes.error_for_check().is_some(),
        json!({"takes":1,"revision":revision,"error":true}),
        d.snapshot(),
    )?;
    activate(d, "First version")?;
    name(d, "Alternate")?;
    activate(d, "Rename selected take")?;
    ready(d)?;
    d.check(
        "Renaming a take preserves its immutable snapshot and the edit head",
        catalog(d)?.entries[0].name.as_str() == "Alternate"
            && catalog(d)?.entries[0].revision_id == *original.revision_id()
            && d.revision() == revision,
        json!({"name":"Alternate","revision":revision}),
        d.snapshot(),
    )?;
    focus(d, "Delete selected take")?;
    for (ime, key) in [
        (
            egui::ImeEvent::Preedit {
                text: "候補".into(),
                active_range_chars: Some(0..2),
            },
            Key::Enter,
        ),
        (egui::ImeEvent::Commit("候補".into()), Key::Space),
    ] {
        d.events(
            "Composition confirmation on a take button",
            vec![
                egui::Event::Ime(ime),
                egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
                egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
            ],
        )?;
        d.check(
            "IME confirmation cannot delete a take",
            catalog(d)?.entries.len() == 1 && d.app().takes.open,
            json!({"takes":1,"open":true}),
            d.snapshot(),
        )?;
    }
    name(d, "Working version")?;
    activate(d, "Save new take")?;
    ready(d)?;
    close(d)?;
    d.check(
        "Closing takes returns keyboard focus to the editor",
        d.harness.ctx.memory(|memory| memory.focused()) == Some(pane_id(d.app().pane)),
        json!(format!("{:?}", pane_id(d.app().pane))),
        d.snapshot(),
    )?;
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    d.command("split")?;
    d.changed(&revision)?;
    let edited = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No project")?
        .document
        .clone();
    let edited_revision = d.revision();
    d.command("takes")?;
    ready(d)?;
    activate(d, "Working version")?;
    activate(d, "Update to current edit")?;
    ready(d)?;
    d.check(
        "Update explicitly moves only the selected take to the current edit",
        catalog(d)?.entries.iter().any(|take| {
            take.name.as_str() == "Working version" && take.revision_id == *edited.revision_id()
        }) && catalog(d)?.entries.iter().any(|take| {
            take.name.as_str() == "Alternate" && take.revision_id == *original.revision_id()
        }) && d.revision() == edited_revision,
        json!({"updated":"Working version","preserved":"Alternate","revision":edited_revision}),
        d.snapshot(),
    )?;
    activate(d, "Alternate")?;
    activate(d, "Open selected take")?;
    ready(d)?;
    d.check(
        "Opening the take restores the saved authored edit in a fresh revision",
        d.app()
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.document.nodes() == original.nodes())
            && d.revision() != revision
            && d.revision() != edited_revision,
        json!({"restored_nodes":true,"fresh_revision":true}),
        d.snapshot(),
    )?;
    close(d)?;
    let restored = d.revision();
    d.command("undo")?;
    d.changed(&restored)?;
    d.check(
        "One Undo returns to the edit before opening a take",
        d.app()
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.document.nodes() == edited.nodes()),
        json!("edited nodes"),
        d.snapshot(),
    )?;
    let undone = d.revision();
    d.command("redo")?;
    d.changed(&undone)?;
    d.check(
        "Redo restores the take again",
        d.app()
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.document.nodes() == original.nodes()),
        json!("saved take nodes"),
        d.snapshot(),
    )?;
    d.command("takes")?;
    ready(d)?;
    activate(d, "Alternate")?;
    let before_delete = d.revision();
    activate(d, "Delete selected take")?;
    ready(d)?;
    d.check(
        "Deleting a label leaves the edit and history intact",
        catalog(d)?.entries.len() == 1
            && catalog(d)?.entries[0].name.as_str() == "Working version"
            && d.revision() == before_delete
            && d.app()
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.can_undo),
        json!({"takes":1,"revision":before_delete,"can_undo":true}),
        d.snapshot(),
    )?;
    for index in 0..12 {
        name(d, &format!("Version {index:02}"))?;
        activate(d, "Save new take")?;
        ready(d)?;
    }
    let long_name = "Z".repeat(128);
    name(d, &long_name)?;
    activate(d, "Save new take")?;
    ready(d)?;
    focus(d, &long_name)?;
    d.key(Key::Space)?;
    d.step("Paint the focused long take name", false)?;
    let paint = scenarios::text_paint_visibility(d, &long_name);
    d.check(
        "Tab scrolls a long take name into view and Space selects it",
        // The name field scrolls horizontally; the separate saved row must
        // show the whole name and become selected through Space.
        paint.iter().any(|item| item["fully_visible"] == true)
            && d.app().takes.selected_name_for_check() == Some(long_name.as_str())
            && d.revision() == before_delete,
        json!({"long_name_visible":true,"revision":before_delete}),
        json!(paint),
    )?;
    focus(d, "Back to editor  Esc")?;
    d.key_modified(Key::Tab, Modifiers::SHIFT)?;
    d.step("Settle reverse Tab focus", false)?;
    let previous = d.harness.root().children_recursive().any(|node| {
        let access = node.accesskit_node();
        access.is_focused() && access.label().as_deref() == Some("Delete selected take")
    });
    d.check(
        "Shift Tab returns to the preceding take action",
        previous,
        json!("Delete selected take"),
        d.widgets(),
    )?;
    d.capture("Many saved takes with the longest supported name")?;
    close(d)
}
