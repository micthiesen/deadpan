//! Actual thumbnail paint, compact confirmation and cache teardown witnesses.

use super::*;

const LABEL: &str = "YouTube thumbnail for Caminandes 2: Gran Dillama";

pub(super) fn visible(d: &mut Driver<'_>) -> Result<(), String> {
    let texture = d
        .app()
        .youtube
        .thumbnail_texture()
        .ok_or("Confirmation has no thumbnail texture")?;
    let image = d.rect(LABEL)?;
    let viewport = d.harness.ctx.content_rect();
    let shapes = &d.harness.output().shapes;
    let mut pictures = Vec::new();
    for (index, clipped) in shapes.iter().enumerate() {
        let egui::Shape::Rect(rect) = &clipped.shape else {
            continue;
        };
        if rect
            .brush
            .as_ref()
            .is_none_or(|brush| brush.fill_texture_id != texture)
        {
            continue;
        }
        let bounds = rect.rect;
        let covered = shapes[index + 1..].iter().any(|later| {
            let egui::Shape::Rect(rect) = &later.shape else {
                return false;
            };
            rect.fill.is_opaque()
                && rect.brush.is_none()
                && rect.blur_width == 0.0
                && rect
                    .rect
                    .shrink(4.0)
                    .intersect(later.clip_rect)
                    .intersect(bounds)
                    .is_positive()
        });
        let visible = clipped.clip_rect.contains_rect(bounds)
            && viewport.contains_rect(bounds)
            && image.contains_rect(bounds)
            && !covered;
        let aspect_error = (bounds.width() - bounds.height() * 480.0 / 270.0).abs();
        let max_height = if viewport.height() < 720.0 {
            96.0
        } else {
            160.0
        };
        pictures.push(
            json!({"bounds":[bounds.min.x,bounds.min.y,bounds.max.x,bounds.max.y],
            "visible":visible,"covered":covered,"aspect_error":aspect_error,
            "bounded":bounds.height() > 0.0 && bounds.height() <= max_height + 0.5}),
        );
    }
    d.check(
        "Confirmation paints the labelled thumbnail unclipped at its own aspect",
        pictures.len() == 1
            && pictures.iter().all(|picture| {
                picture["visible"] == true
                    && picture["bounded"] == true
                    && picture["aspect_error"]
                        .as_f64()
                        .is_some_and(|error| error <= 0.5)
            }),
        json!({"label":LABEL,"unclipped":true,"raster":[480,270]}),
        json!(pictures),
    )
}

pub(super) fn controls(d: &mut Driver<'_>) -> Result<(), String> {
    let image = d.rect(LABEL)?;
    actions_visible(d)?;
    for label in ["Download and create  Enter", "Cancel  Esc"] {
        let rect = d.rect(label)?;
        d.check(
            "The entire confirmation action stays below the thumbnail inside the window",
            d.harness.ctx.content_rect().contains_rect(rect) && image.bottom() < rect.top(),
            json!({"label":label,"inside_window":true,"below_thumbnail":true}),
            json!({"action":[rect.min.x,rect.min.y,rect.max.x,rect.max.y],
                "image":[image.min.x,image.min.y,image.max.x,image.max.y]}),
        )?;
    }
    Ok(())
}

fn actions_visible(d: &mut Driver<'_>) -> Result<(), String> {
    painted(
        d,
        "The rights notice stays visible beside the decision",
        "You are responsible for having the rights",
    )?;
    for label in ["Download and create  Enter", "Cancel  Esc"] {
        let rect = d.rect(label)?;
        painted(d, "Confirmation action text remains fully painted", label)?;
        d.check(
            "The entire confirmation action stays inside the window",
            d.harness.ctx.content_rect().contains_rect(rect),
            json!({"label":label,"inside_window":true}),
            json!([rect.min.x, rect.min.y, rect.max.x, rect.max.y]),
        )?;
    }
    Ok(())
}

pub(super) fn absent(d: &mut Driver<'_>) -> Result<(), String> {
    let labelled = d
        .harness
        .root()
        .children_recursive()
        .filter(|node| node.accesskit_node().label().as_deref() == Some(LABEL))
        .count();
    d.check(
        "The current state has no stale thumbnail texture or accessible image",
        d.app().youtube.thumbnail_texture().is_none() && labelled == 0,
        json!({"texture":false,"labelled_images":0}),
        json!({"texture":d.app().youtube.thumbnail_texture().is_some(),"labelled_images":labelled}),
    )
}

pub(super) fn sheet_confirmation(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    let cursor = d.app().source_cursor;
    d.key_modified(Key::N, command_shift_n())?;
    d.step("Focus the new import sheet", false)?;
    d.key_modified(Key::A, Modifiers::MAC_CMD | Modifiers::COMMAND)?;
    text(
        d,
        "Paste one video into the sheet",
        vec![Event::Paste(URL.into())],
    )?;
    d.key(Key::Enter)?;
    d.wait_for("Sheet confirmation", |app| {
        app.youtube.step_name() == "confirm"
    })?;
    visible(d)?;
    controls(d)?;
    let first_action = d.rect("Download and create  Enter")?;
    d.step("Keep the first painted confirmation target", false)?;
    let next_action = d.rect("Download and create  Enter")?;
    d.check(
        "The sheet action does not move after its first confirmation paint",
        first_action == next_action,
        json!([
            first_action.min.x,
            first_action.min.y,
            first_action.max.x,
            first_action.max.y
        ]),
        json!([
            next_action.min.x,
            next_action.min.y,
            next_action.max.x,
            next_action.max.y
        ]),
    )?;
    d.capture("Thumbnail confirmation over the open project")?;
    d.harness.set_size(egui::vec2(960.0, 640.0));
    d.step("Resize the confirmation sheet", false)?;
    d.step("Settle the compact sheet", false)?;
    visible(d)?;
    controls(d)?;
    d.capture("Thumbnail confirmation sheet at 960×640")?;
    d.click("Cancel  Esc")?;
    d.wait_for("Sheet import declined", |app| {
        app.youtube.step_name() == "cancelled"
    })?;
    d.step("Paint the declined sheet", false)?;
    absent(d)?;
    d.check(
        "Declining a thumbnail confirmation keeps the existing project and cursor",
        d.revision() == revision && d.app().source_cursor == cursor,
        json!({"revision":revision,"cursor":cursor}),
        json!({"revision":d.revision(),"cursor":d.app().source_cursor}),
    )?;
    d.key(Key::Escape)?;
    d.key(Key::Escape)?;
    d.harness.set_size(egui::vec2(1280.0, 820.0));
    d.step("Return to the existing project", false)?;
    long_and_missing(d)
}

fn long_and_missing(d: &mut Driver<'_>) -> Result<(), String> {
    use deadpan_cli::youtube::thumbnail::{ThumbnailState, Unavailable};
    let revision = d.revision();
    let cursor = d.app().source_cursor;
    d.harness.set_size(egui::vec2(960.0, 640.0));
    for (state, caption) in [
        (ThumbnailState::Missing, "No thumbnail provided"),
        (
            ThumbnailState::Unavailable(Unavailable::Timeout),
            "Thumbnail unavailable",
        ),
    ] {
        let mut preview = crate::youtube::scripted::preview();
        preview.thumbnail = state;
        d.downloader()
            .ok_or("No scripted downloader")?
            .preview_next(preview);
        d.key_modified(Key::N, command_shift_n())?;
        d.step("Focus the optional-thumbnail import", false)?;
        d.key(Key::Enter)?;
        d.wait_for("Optional thumbnail confirmation", |app| {
            app.youtube.step_name() == "confirm"
        })?;
        absent(d)?;
        painted(
            d,
            "Unavailable thumbnails retain an explicit status",
            caption,
        )?;
        actions_visible(d)?;
        d.capture("Confirmation with an optional-thumbnail status")?;
        d.click("Cancel  Esc")?;
        d.wait_for("Optional thumbnail declined", |app| {
            app.youtube.step_name() == "cancelled"
        })?;
        d.key(Key::Escape)?;
        d.key(Key::Escape)?;
    }
    long_metadata(d)?;
    d.check(
        "Optional thumbnails and detail scrolling preserve the existing project",
        d.revision() == revision && d.app().source_cursor == cursor,
        json!({"revision":revision,"cursor":cursor}),
        json!({"revision":d.revision(),"cursor":d.app().source_cursor}),
    )?;
    d.harness.set_size(egui::vec2(1280.0, 820.0));
    d.step("Restore the existing project view", false)?;
    Ok(())
}

fn long_metadata(d: &mut Driver<'_>) -> Result<(), String> {
    let mut preview = crate::youtube::scripted::preview();
    preview.metadata.title = "A long title supplied by the remote video publisher. ".repeat(19);
    preview.metadata.license = Some("End of remote details".into());
    d.downloader()
        .ok_or("No scripted downloader")?
        .preview_next(preview.clone());
    d.key_modified(Key::N, command_shift_n())?;
    d.step("Focus the long-title import", false)?;
    d.key(Key::Enter)?;
    d.wait_for("Long-title confirmation", |app| {
        app.youtube.step_name() == "confirm"
    })?;
    actions_visible(d)?;
    let before = scenarios::text_paint_visibility(d, "End of remote details");
    d.check(
        "The long-title fixture needs scrolling to read the last metadata row",
        before.is_empty() || before.iter().any(|part| part["fully_visible"] != true),
        json!({"last_row_fully_visible":false}),
        json!(before),
    )?;
    d.capture("Long title with fixed confirmation actions")?;
    for _ in 0..8 {
        d.events(
            "Scroll the long confirmation details",
            vec![
                Event::PointerMoved(egui::pos2(480.0, 320.0)),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -300.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: Modifiers::NONE,
                },
            ],
        )?;
        for _ in 0..4 {
            d.step("Settle detail scrolling", false)?;
        }
        let paint = scenarios::text_paint_visibility(d, "End of remote details");
        if !paint.is_empty() && paint.iter().all(|part| part["fully_visible"] == true) {
            break;
        }
    }
    painted(
        d,
        "Scrolling reveals the last metadata row",
        "End of remote details",
    )?;
    actions_visible(d)?;
    d.capture("Long title scrolled to its final metadata row")?;
    d.click("Cancel  Esc")?;
    d.wait_for("Long-title import declined", |app| {
        app.youtube.step_name() == "cancelled"
    })?;
    d.downloader()
        .ok_or("No scripted downloader")?
        .preview_next(preview);
    d.key(Key::Enter)?;
    d.wait_for("Fresh long-title confirmation", |app| {
        app.youtube.step_name() == "confirm"
    })?;
    let fresh = scenarios::text_paint_visibility(d, "End of remote details");
    d.check(
        "A fresh confirmation forgets the previous import's scroll position",
        fresh.is_empty() || fresh.iter().any(|part| part["fully_visible"] != true),
        json!({"last_row_fully_visible":false}),
        json!(fresh),
    )?;
    actions_visible(d)?;
    d.click("Cancel  Esc")?;
    d.wait_for("Fresh long-title import declined", |app| {
        app.youtube.step_name() == "cancelled"
    })?;
    d.key(Key::Escape)?;
    d.key(Key::Escape)?;
    Ok(())
}
