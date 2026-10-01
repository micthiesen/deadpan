use super::*;
use crate::preview::copied::Content;
use std::sync::Arc;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let baseline = document(d)?.clone();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 640.0));
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing deletion viewport")?
        .inner_rect = Some(rect);
    d.step(
        "Resize before checking captured deletion command hints",
        false,
    )?;
    // A successful cut replaces the previous Original register only after save.
    d.command("source")?;
    d.chord(&[Key::G, Key::G, Key::V, Key::Num1, Key::Num0, Key::L, Key::Y])?;
    d.command("sequence")?;
    select(d, 20, 30, true)?;
    let revision = d.revision();
    d.key(Key::Colon)?;
    d.events(
        "Type delete and inspect its captured range",
        vec![egui::Event::Text("delete".into())],
    )?;
    paint_hint(d, "Cut captured Edit [20..30)")?;
    d.capture("Captured range deletion command at 960 by 640")?;
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    d.check(
        "The saved command cut replaces the Original register with its historical Edit range",
        d.app().sequence_cursor == 20
            && d.app().sequence_length() == 110
            && matches!(d.app().copied.content(), Some(Content::Edited(copied))
                if copied.slice().range() == range(20, 30)
                    && copied.slice().revision_id().as_str() == revision
                    && copied.slice().duration().frames() == 10),
        json!({"range":[20,30],"source_revision":revision,"copied":"Edited"}),
        d.snapshot(),
    )?;
    let Some(Content::Edited(copied)) = d.app().copied.content().cloned() else {
        return Err("Saved cut did not produce an edited copy".into());
    };
    undo(d, &baseline)?;

    select(d, 20, 20, true)?;
    d.key(Key::Colon)?;
    d.events(
        "Type delete with an empty captured range",
        vec![egui::Event::Text("delete".into())],
    )?;
    paint_hint(d, "The Edit selection is empty")?;
    d.key(Key::Escape)?;
    d.key(Key::Escape)?;

    let revision = d.revision();
    d.chord(&[Key::G, Key::G, Key::Num2, Key::Num0, Key::L, Key::S])?;
    d.changed(&revision)?;
    select(d, 30, 60, true)?;
    d.key(Key::Colon)?;
    d.events(
        "Type delete against a captured revision",
        vec![egui::Event::Text("delete".into())],
    )?;
    let revision = document(d)?.revision_id().clone();
    d.app().service.submit(ProjectRequest::Undo {
        expected_revision: revision,
    })?;
    d.wait_for("Concurrent Undo invalidates captured range", |app| {
        !app.service.is_busy() && app.selected_edit_range().is_none()
    })?;
    let saved = document(d)?.clone();
    d.key(Key::Enter)?;
    d.settled()?;
    d.check(
        "A captured deletion rejects a revision change instead of retargeting",
        *document(d)? == saved
            && matches!(d.app().copied.content(), Some(Content::Edited(current))
                if Arc::ptr_eq(current, &copied))
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("captured deletion target changed")),
        json!("stale target rejected"),
        d.snapshot(),
    )?;

    // Withhold a real edit reply until command entry captures Original absence.
    d.key(Key::Escape)?;
    d.chord(&[Key::G, Key::G, Key::Num2, Key::Num0, Key::L])?;
    d.app_mut().feedback.hold_project_updates = true;
    d.key(Key::S)?;
    d.wait_for("Split commits while completion is withheld", |app| {
        !app.service.is_busy()
    })?;
    d.command("source")?;
    d.key(Key::Colon)?;
    d.events(
        "Type delete with no command-entry Edit target",
        vec![egui::Event::Text("delete".into())],
    )?;
    paint_hint(d, "Return to Your edit")?;
    d.app_mut().feedback.hold_project_updates = false;
    d.wait_for("Late edit reply changes the visible context", |app| {
        app.view == View::Sequence && !app.service.is_busy()
    })?;
    let saved = document(d)?.clone();
    d.key(Key::Enter)?;
    d.settled()?;
    d.check(
        "Late completion cannot supply a missing deletion target",
        *document(d)? == saved
            && matches!(d.app().copied.content(), Some(Content::Edited(current))
                if Arc::ptr_eq(current, &copied))
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Return to Your edit")),
        json!("captured absence retained"),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;

    // Reopening unchanged bytes still creates a distinct project session.
    select(d, 20, 30, true)?;
    d.key(Key::Colon)?;
    d.events(
        "Type delete before a project reopen",
        vec![egui::Event::Text("delete".into())],
    )?;
    let workspace = d.app().workspace.as_ref().unwrap().clone();
    d.app().service.submit(ProjectRequest::Close)?;
    d.wait_for("Close captured deletion session", |app| {
        app.workspace.is_none() && !app.service.is_busy()
    })?;
    d.app()
        .service
        .submit(ProjectRequest::Open(workspace.path.clone()))?;
    d.wait_for("Reopen identical project with a fresh session", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|next| next.session != workspace.session)
            && !app.service.is_busy()
    })?;
    d.key(Key::Enter)?;
    d.settled()?;
    d.check(
        "A command opened before reopen cannot delete the new session",
        *document(d)? == *workspace.document
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("captured deletion target changed")),
        json!("session change rejected"),
        d.snapshot(),
    )?;
    d.key(Key::Escape)
}

fn paint_hint(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    let paint = scenarios::text_paint_visibility(d, label);
    d.check(
        "Captured deletion guidance is fully painted at the minimum viewport",
        !paint.is_empty() && paint.iter().all(|part| part["fully_visible"] == true),
        json!({"label":label,"viewport":[960,640]}),
        json!(paint),
    )
}
