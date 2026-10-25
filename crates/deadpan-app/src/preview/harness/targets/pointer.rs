//! Actual primary-pointer batches against Camera's displayed target surface.
//! The coordinate oracle composes the displayed picture independently of the
//! gesture implementation; target persistence still uses the project service.

use deadpan_core::{TARGET_UNITS, TargetGeometry, TargetPoint, TargetRegion};

use super::*;

fn geometry(d: &Driver<'_>) -> Result<TargetGeometry, String> {
    let state = camera(d);
    let coordinate = |field: &str| -> Result<[u32; 2], String> {
        let values = state["region"][field]
            .as_array()
            .ok_or_else(|| format!("No pointer target {field}: {state}"))?;
        let read = |index: usize| {
            values
                .get(index)
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| format!("Invalid target {field}: {state}"))
        };
        Ok([read(0)?, read(1)?])
    };
    let center = coordinate("center")?;
    if state["region"]["point"] == true {
        Ok(TargetPoint { center }.into())
    } else {
        Ok(TargetRegion {
            center,
            size: coordinate("size")?,
        }
        .into())
    }
}

fn projection(d: &Driver<'_>) -> Result<deadpan_render::PictureGeometry, String> {
    let picture = d
        .app()
        .presentation
        .picture()
        .ok_or("Pointer replay has no displayed picture")?;
    let frame = picture.frame.as_ref().ok_or("No pointer replay frame")?;
    let (width, height) = picture.canvas.ok_or("No pointer replay canvas")?;
    deadpan_render::PictureGeometry::composed(
        frame.metadata(),
        picture.picture_context.as_deref(),
        [width, height],
        [width, height],
        deadpan_render::FitMode::Fit,
        &crate::preview::camera::render_layers(picture)?,
    )
    .map_err(|error| error.to_string())
}

fn source_at(d: &Driver<'_>, screen: egui::Pos2) -> Result<[f64; 2], String> {
    let canvas = d.rect("Target drawing surface")?;
    projection(d)?
        .canvas_to_source([
            f64::from((screen.x - canvas.left()) / canvas.width()),
            f64::from((screen.y - canvas.top()) / canvas.height()),
        ])
        .map_err(|error| error.to_string())
}

fn screen_at(d: &Driver<'_>, source: [f64; 2]) -> Result<egui::Pos2, String> {
    let point = projection(d)?
        .source_to_canvas(source)
        .map_err(|error| error.to_string())?
        .ok_or("Pointer replay source point is clipped")?;
    let canvas = d.rect("Target drawing surface")?;
    Ok(canvas.min
        + egui::vec2(
            point[0] as f32 * canvas.width(),
            point[1] as f32 * canvas.height(),
        ))
}

fn canvas_point(d: &Driver<'_>, x: f32, y: f32) -> Result<egui::Pos2, String> {
    let canvas = d.rect("Target drawing surface")?;
    Ok(canvas.min + canvas.size() * egui::vec2(x, y))
}

fn units(value: f64) -> u32 {
    (value * f64::from(TARGET_UNITS))
        .round()
        .clamp(0.0, f64::from(TARGET_UNITS)) as u32
}

fn drawn(a: [f64; 2], b: [f64; 2]) -> TargetGeometry {
    TargetRegion {
        center: [units((a[0] + b[0]) / 2.0), units((a[1] + b[1]) / 2.0)],
        size: [
            units((a[0] - b[0]).abs()).max(TARGET_UNITS / 100),
            units((a[1] - b[1]).abs()).max(TARGET_UNITS / 100),
        ],
    }
    .into()
}

fn near(a: TargetGeometry, b: TargetGeometry) -> bool {
    let pair = |a: [u32; 2], b: [u32; 2]| a.into_iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 2);
    pair(a.center(), b.center())
        && match (a.as_region(), b.as_region()) {
            (Some(a), Some(b)) => pair(a.size, b.size),
            (None, None) => true,
            _ => false,
        }
}

fn button(d: &mut Driver<'_>, point: egui::Pos2, pressed: bool) -> Result<(), String> {
    d.events(
        if pressed {
            "Press target surface"
        } else {
            "Release target surface"
        },
        vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            },
        ],
    )
}

fn moved(d: &mut Driver<'_>, point: egui::Pos2) -> Result<(), String> {
    d.events(
        "Drag temporary target",
        vec![egui::Event::PointerMoved(point)],
    )
}

fn drag(d: &mut Driver<'_>, from: egui::Pos2, to: egui::Pos2) -> Result<(), String> {
    button(d, from, true)?;
    moved(d, to)?;
    button(d, to, false)
}

fn opened(d: &mut Driver<'_>) -> Result<(), String> {
    d.chord(&[Key::Comma, Key::F])?;
    d.wait_for("Camera opens for pointer target", |app| {
        app.camera.is_some()
    })?;
    // The tracking replay leaves a saved Follow. Release it only in this
    // temporary Camera so c later corrects the newly selected point.
    if !camera(d)["follow"].is_null() {
        d.key(Key::T)?;
        d.settled()?;
    }
    d.key(Key::N)?;
    d.settled()?;
    d.rect("Target drawing surface").map(|_| ())
}

fn continued(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Pointer target saved and Camera rebased", |app| {
        app.camera.as_ref().is_some_and(|camera| {
            let state = camera.harness_state();
            state["phase"] == "Adjust"
                && state["saving"] == false
                && state["rebasing"] == false
                && app.workspace.as_ref().is_some_and(|workspace| {
                    state["revision"] == workspace.document.revision_id().as_str()
                })
        })
    })?;
    d.settled()
}

fn target_count(d: &Driver<'_>) -> usize {
    d.app().workspace.as_ref().unwrap().document.targets().len()
}

fn resize(d: &mut Driver<'_>, size: egui::Vec2) -> Result<(), String> {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Pointer replay has no root viewport")?
        .inner_rect = Some(rect);
    d.step("Resize during target drag", false)
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    let count = target_count(d);
    opened(d)?;
    let from = canvas_point(d, 0.18, 0.20)?;
    let to = canvas_point(d, 0.36, 0.38)?;
    let expected = drawn(source_at(d, from)?, source_at(d, to)?);
    drag(d, from, to)?;
    let actual = geometry(d)?;
    d.check(
        "Dragging the picture draws an upright-source rectangle without saving",
        near(actual, expected) && d.revision() == revision && target_count(d) == count,
        json!({"geometry":expected,"revision":revision,"targets":count}),
        json!({"geometry":actual,"revision":d.revision(),"targets":target_count(d)}),
    )?;

    let from = screen_at(
        d,
        actual
            .center()
            .map(|value| f64::from(value) / f64::from(TARGET_UNITS)),
    )?;
    let to = from + egui::vec2(26.0, 18.0);
    let a = source_at(d, from)?;
    let b = source_at(d, to)?;
    let region = actual
        .as_region()
        .ok_or("Draw did not produce a rectangle")?;
    let expected: TargetGeometry = TargetRegion {
        center: [
            units(f64::from(region.center[0]) / f64::from(TARGET_UNITS) + b[0] - a[0]),
            units(f64::from(region.center[1]) / f64::from(TARGET_UNITS) + b[1] - a[1]),
        ],
        size: region.size,
    }
    .into();
    drag(d, from, to)?;
    let actual = geometry(d)?;
    d.check(
        "Dragging the rectangle body preserves its size and the initial grab offset",
        near(actual, expected)
            && actual
                .as_region()
                .is_some_and(|value| value.size == region.size),
        json!(expected),
        json!(actual),
    )?;

    let region = actual.as_region().ok_or("Move changed the geometry kind")?;
    let unit = f64::from(TARGET_UNITS);
    let fixed = [
        (f64::from(region.center[0]) - f64::from(region.size[0]) / 2.0) / unit,
        (f64::from(region.center[1]) - f64::from(region.size[1]) / 2.0) / unit,
    ];
    let corner = [
        (f64::from(region.center[0]) + f64::from(region.size[0]) / 2.0) / unit,
        (f64::from(region.center[1]) + f64::from(region.size[1]) / 2.0) / unit,
    ];
    let from = screen_at(d, corner)?;
    let to = screen_at(d, [fixed[0] - 0.025, fixed[1] - 0.035])?;
    let expected = drawn(fixed, source_at(d, to)?);
    drag(d, from, to)?;
    let actual = geometry(d)?;
    d.check(
        "A corner drag can cross the fixed opposite corner without changing coordinate space",
        near(actual, expected) && d.revision() == revision,
        json!(expected),
        json!(actual),
    )?;
    d.capture("Pointer rectangle after reverse corner drag")?;

    let center = screen_at(d, actual.center().map(|value| f64::from(value) / unit))?;
    button(d, center, true)?;
    let inside = center + egui::vec2(14.0, 12.0);
    moved(d, inside)?;
    let last_inside = geometry(d)?;
    d.check(
        "A small rectangle keeps a movable body despite overlapping corner hit areas",
        last_inside.as_region().map(|region| region.size)
            == actual.as_region().map(|region| region.size)
            && last_inside.center() != actual.center()
            && d.revision() == revision,
        json!({"size":actual.as_region().map(|region| region.size),"center_moves":true}),
        json!(last_inside),
    )?;
    let outside = d.rect("Target drawing surface")?.min - egui::vec2(20.0, 20.0);
    moved(d, outside)?;
    button(d, outside, false)?;
    moved(d, inside + egui::vec2(20.0, 20.0))?;
    d.check(
        "Moving and releasing outside the picture keeps the last valid draft and ends capture",
        geometry(d)? == last_inside && d.revision() == revision,
        json!(last_inside),
        json!(geometry(d)?),
    )?;

    let center = screen_at(d, last_inside.center().map(|value| f64::from(value) / unit))?;
    button(d, center, true)?;
    moved(d, center + egui::vec2(12.0, 10.0))?;
    d.key(Key::ArrowRight)?;
    let keyboard = geometry(d)?;
    moved(d, center + egui::vec2(48.0, 36.0))?;
    button(d, center + egui::vec2(48.0, 36.0), false)?;
    d.check(
        "A keyboard adjustment revokes a held pointer gesture instead of being overwritten",
        geometry(d)? == keyboard && d.revision() == revision,
        json!(keyboard),
        json!(geometry(d)?),
    )?;

    let old_size = d.harness.ctx.content_rect().size();
    let center = screen_at(d, keyboard.center().map(|value| f64::from(value) / unit))?;
    button(d, center, true)?;
    moved(d, center + egui::vec2(11.0, 8.0))?;
    let before_resize = geometry(d)?;
    let new_size = if old_size.x > 1000.0 {
        egui::vec2(960.0, 640.0)
    } else {
        egui::vec2(1280.0, 820.0)
    };
    resize(d, new_size)?;
    d.settled()?;
    let changed_point = canvas_point(d, 0.65, 0.65)?;
    moved(d, changed_point)?;
    button(d, changed_point, false)?;
    d.check(
        "Resizing the displayed raster revokes held capture without retargeting the draft",
        geometry(d)? == before_resize && d.revision() == revision,
        json!(before_resize),
        json!(geometry(d)?),
    )?;
    resize(d, old_size)?;
    d.settled()?;
    let canvas = d.rect("Target drawing surface")?;
    let before_help = geometry(d)?;
    d.click("Keys  ?")?;
    d.step("Help covers the target drawing surface", false)?;
    d.check(
        "Opening Help abandons the temporary pointer target without saving",
        d.app().help_open
            && d.app().camera.is_none()
            && d.revision() == revision
            && target_count(d) == count,
        json!({"camera_open":false,"revision":revision,"targets":count}),
        json!({"discarded_geometry":before_help,"camera_open":d.app().camera.is_some(),"revision":d.revision(),"targets":target_count(d)}),
    )?;
    let overlap = d.rect("Keys · reshape one Original")?.intersect(canvas);
    if !overlap.is_positive() {
        return Err("Help did not cover the target picture for its opaque-hit check".into());
    }
    let covered = overlap.center();
    drag(d, covered, covered + egui::vec2(24.0, 18.0))?;
    d.check(
        "An opaque Help window owns pointer input over the target picture",
        d.app().help_open
            && d.app().camera.is_none()
            && d.revision() == revision
            && target_count(d) == count,
        json!({"camera_open":false,"help_open":true,"targets":count}),
        json!({"camera":camera(d),"help_open":d.app().help_open,"covered":format!("{overlap:?}")}),
    )?;
    d.key(Key::Escape)?;
    d.step("Help closes before returning to the target", false)?;
    opened(d)?;
    let from = canvas_point(d, 0.18, 0.20)?;
    let to = canvas_point(d, 0.36, 0.38)?;
    drag(d, from, to)?;
    let escaped = geometry(d)?;
    d.key(Key::Escape)?;
    d.check(
        "Escape discards all pointer target changes without a target transaction",
        camera(d)["phase"] == "Adjust" && d.revision() == revision && target_count(d) == count,
        json!({"phase":"Adjust","revision":revision,"targets":count}),
        json!({"discarded_geometry":escaped,"camera":camera(d)}),
    )?;
    d.key(Key::Escape)?;
    d.settled()?;

    // Point and manual correction use exactly the same save path as keys.
    opened(d)?;
    d.key(Key::N)?;
    let from = canvas_point(d, 0.25, 0.28)?;
    let to = canvas_point(d, 0.43, 0.39)?;
    let expected: TargetGeometry = TargetPoint {
        center: source_at(d, to)?.map(units),
    }
    .into();
    drag(d, from, to)?;
    let point = geometry(d)?;
    d.check(
        "Pointer placement of a point has no serialized rectangle size and remains temporary",
        near(point, expected)
            && point.is_point()
            && camera(d)["region"]["size"].is_null()
            && d.revision() == revision,
        json!(expected),
        json!(point),
    )?;
    d.key(Key::Enter)?;
    continued(d)?;
    let selected = camera(d)["selected"]
        .as_str()
        .ok_or("Saved pointer point is not selected")?
        .to_owned();
    let saved = target(d, &selected).ok_or("Saved pointer point is missing")?;
    d.check(
        "Enter saves the pointer point once and continues Camera on the saved revision",
        saved.region == point && target_count(d) == count + 1 && d.revision() != revision,
        json!({"geometry":point,"targets":count + 1}),
        json!({"target":saved,"camera":camera(d)}),
    )?;
    let saved_revision = d.revision();
    d.key(Key::C)?;
    d.settled()?;
    let from = screen_at(d, point.center().map(|value| f64::from(value) / unit))?;
    let to = from + egui::vec2(34.0, 22.0);
    drag(d, from, to)?;
    let correction = geometry(d)?;
    d.check(
        "Pointer correction keeps a point target kind locked and does not save on release",
        correction.is_point()
            && correction != point
            && camera(d)["region"]["kind_locked"] == true
            && d.revision() == saved_revision,
        json!({"point":true,"kind_locked":true,"saved":false}),
        camera(d),
    )?;
    d.key(Key::Enter)?;
    continued(d)?;
    let corrected = target(d, &selected).ok_or("Corrected pointer point is missing")?;
    d.check(
        "A pointer correction at the span start replaces its seed in one target edit",
        corrected.region == correction
            && corrected.corrections.is_empty()
            && target_count(d) == count + 1
            && d.revision() != saved_revision,
        json!({"geometry":correction,"corrections":0,"targets":count+1}),
        json!(corrected),
    )?;
    d.capture("Saved pointer point correction")?;
    d.key(Key::Escape)?;
    d.settled()?;
    let before = d.revision();
    d.command("undo")?;
    d.changed(&before)?;
    d.check(
        "One Undo removes the complete pointer correction while retaining its saved point",
        target(d, &selected)
            .is_some_and(|target| target.region == point && target.corrections.is_empty())
            && target_count(d) == count + 1,
        json!({"geometry":point,"corrections":0}),
        json!(target(d, &selected)),
    )?;
    let before = d.revision();
    d.command("undo")?;
    d.changed(&before)?;
    d.check(
        "A second Undo removes the single pointer creation transaction",
        target(d, &selected).is_none() && target_count(d) == count,
        json!({"targets":count}),
        json!({"targets":target_count(d)}),
    )?;
    d.capture("Pointer target history restored")
}
