//! Complete interaction sequences. Expectations describe user-visible behavior.

use super::*;
use egui::Key;

pub(super) fn run(name: &str, d: &mut Driver<'_>) -> Result<(), String> {
    match name {
        "workspace" => workspace(d),
        "editing" => editing(d),
        "camera" => camera(d),
        "targets" => super::targets::run(d),
        "faces" => super::faces::run(d),
        "menus" => menus(d),
        "delayed-preview" => delayed(d),
        "rapid-input" => rapid(d),
        "playback-feedback" => playback(d),
        "large-project" => super::scale::run(d),
        "edit-latency" => super::edit_latency::run(d),
        "nested-pause" => super::nested_pause::run(d),
        "original-moment" => super::moment::run(d),
        "transcript" => super::transcript::run(d),
        "corrections" => super::corrections::run(d),
        "shots" => super::shots::run(d),
        "proxy-seek" => super::proxy::run(d),
        "cutaway" => super::cutaway::run(d),
        "gags" => super::gags::run(d),
        "recipes" => super::recipes::run(d),
        "hold-effects" => super::hold_effects::run(d),
        "audio-treatments" => super::audio_treatments::run(d),
        "split-edits" => super::split_edits::run(d),
        "recipe-library" => super::recipe_library::run(d),
        "captions" => super::captions::run(d),
        "zoom" => super::zoom::run(d),
        "original-layout" | "original-layout-long" => super::original_layout::run(d),
        "place-slice" => super::splice::run(d),
        "delete-range" => super::delete_range::run(d),
        "marks" => super::marks::run(d),
        "named-registers" => super::registers::run(d),
        "dot-repeat" => super::semantic::run(d),
        "creative-dot" => super::creative_dot::run(d),
        "repeat-operator" => super::repeat_operator::run(d),
        "repeat-setters" => super::repeat_operator::run_setters(d),
        "groups" => super::groups::run(d),
        "scoped-plays" => super::scoped_plays::run(d),
        "structure-copies" => super::structure_copies::run(d),
        "macros" => super::macros::run(d),
        "original-playback" => super::original_playback::run(d),
        "sound-playback" => super::sound_playback::run(d),
        "retime" => super::retime::run(d),
        "slip" => super::slip::run(d),
        "trim" => super::trim::run(d),
        "render" => super::render::run(d),
        "recovery" => super::recovery::crash(d),
        "relink" => super::recovery::relink(d),
        "storage-failure" => super::recovery::storage_failure(d),
        "diagnostics" => super::diagnostics::run(d),
        "storage" => super::storage::run(d),
        "backups" => super::backups::run(d),
        "jobs" => super::jobs::run(d),
        "full-session" => super::full_session::run(d),
        "layouts" => super::layouts::run(d),
        _ => Err(format!("Unknown scenario {name}")),
    }
}

fn playback(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("Playback service updates are explicitly simulated. This scenario qualifies UI feedback and picture routing, not PCM preparation, device timing or listening.".into());
    d.click("Play edit  Space")?;
    d.capture("Preparation feedback on the next UI frame")?;
    d.check(
        "Play exposes preparation and cancel within one UI frame",
        d.app()
            .transport
            .as_ref()
            .is_some_and(|run| run.phase == deadpan_playback::Phase::Preparing)
            && d.rect("Cancel preparation  Space").is_ok(),
        json!("preparing with cancel control"),
        d.snapshot(),
    )?;
    let (mut feed, _callback) = deadpan_output::channel().map_err(|e| e.to_string())?;
    let generation = feed.restart(0).map_err(|e| e.to_string())?;
    let update = |d: &Driver<'_>, phase, sample, error| {
        let run = d.app().transport.as_ref().expect("active playback");
        deadpan_playback::Update {
            ticket: run.ticket,
            session: run.session,
            project_id: run.project.clone(),
            revision_id: run.revision.clone(),
            content: run.content.clone(),
            phase,
            sample: Some(deadpan_core::AudioSample(sample)),
            generation: Some(generation),
            error,
        }
    };
    let playing = update(d, deadpan_playback::Phase::Playing, 4_805, None);
    d.app_mut()
        .feedback
        .playback_updates
        .push_back(playing.clone());
    d.capture("Simulated delivery starts playback")?;
    d.wait_for("Playback picture catches up", |app| {
        app.presentation.has_displayed()
            && !app.presentation.loading()
            && !app.presentation.needs_render()
    })?;
    d.click("Pause  Space")?;
    d.settled()?;
    let cursor = d.app().sequence_cursor;
    d.app_mut().feedback.playback_updates.push_back(playing);
    d.capture("Old playback update after pause")?;
    d.check(
        "Pause rejects stale delivery and retains cursor",
        d.app().transport.is_none() && d.app().sequence_cursor == cursor,
        json!(cursor),
        json!(d.app().sequence_cursor),
    )?;
    d.click("Play edit  Space")?;
    let fault = update(
        d,
        deadpan_playback::Phase::Failed,
        4_805,
        Some("UI replay simulated an output device failure".into()),
    );
    d.app_mut().feedback.playback_updates.push_back(fault);
    d.capture("Output failure is visible and stops playback")?;
    d.check(
        "Device failure cannot silently resume",
        d.app().transport.is_none()
            && d.app()
                .error
                .as_deref()
                .is_some_and(|e| e.contains("device failure")),
        json!("stopped with visible failure"),
        d.snapshot(),
    )?;
    let failure_text = "UI replay simulated an output device failure";
    let entry_paint = text_paint_visibility(d, failure_text);
    d.check(
        "Output failure entry frame contains the actual error text paint",
        !entry_paint.is_empty(),
        json!(failure_text),
        json!(entry_paint),
    )?;
    d.check(
        "Output failure notice is fully visible on its first frame",
        entry_paint
            .iter()
            .all(|paint| paint["fully_visible"] == true),
        json!("complete error text inside the actual paint clip and viewport"),
        json!(entry_paint),
    )?;
    // Keep both frames. A truthful stopped state is insufficient when the
    // user-facing notice falls outside its panel's retained first-frame size.
    d.capture("Output failure notice visible")?;
    let visible_paint = text_paint_visibility(d, failure_text);
    d.check(
        "Output device failure notice is fully painted inside its clip and viewport",
        !visible_paint.is_empty()
            && visible_paint
                .iter()
                .all(|paint| paint["fully_visible"] == true),
        json!("complete error text inside the actual paint clip and viewport"),
        json!(visible_paint),
    )?;
    let detail = format!(
        "{failure_text}. {}",
        "The device stopped; your edits remain saved. ".repeat(5)
    );
    d.app_mut().error = Some(detail.clone());
    for (width, height) in [(960.0, 640.0), (1280.0, 820.0)] {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
        let input = d.harness.input_mut();
        input.screen_rect = Some(rect);
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .ok_or("Native resize has no root viewport")?
            .inner_rect = Some(rect);
        d.capture(&format!(
            "Wrapped failure on first resize frame at {width}x{height}"
        ))?;
        let paint = text_paint_visibility(d, &detail);
        d.check(
            "Wrapped error remains fully visible on the first resize frame",
            !paint.is_empty() && paint.iter().all(|paint| paint["fully_visible"] == true),
            json!("complete notice at the new viewport size"),
            json!(paint),
        )?;
    }
    d.click("Play edit  Space")?;
    d.capture("Retry exposes preparation cancellation")?;
    d.click("Cancel preparation  Space")?;
    d.check(
        "Preparation cancellation remains responsive",
        d.app().transport.is_none(),
        json!("stopped"),
        d.snapshot(),
    )
}

fn selected_visible(d: &mut Driver<'_>) -> Result<(), String> {
    let app = d.app();
    let (index, row) = app
        .beat_rows
        .iter()
        .enumerate()
        .find(|(_, row)| Some(&row.id) == app.selected_beat.as_ref())
        .ok_or("No selected beat")?;
    let label = format!(
        "Beat {}: {}, {}, {} frames, half-open frame boundaries {} to {}",
        index + 1,
        row.label,
        row.kind,
        row.frames,
        row.start,
        row.start + row.frames
    );
    let rect = d.rect(&label)?;
    let viewport = d.harness.ctx.content_rect();
    let painted = d.harness.output().shapes.iter().any(|clipped| {
        matches!(&clipped.shape,egui::Shape::Rect(shape) if shape.rect == rect && shape.fill == style::SELECTED)
            && clipped.clip_rect.contains_rect(rect.shrink(1.0))
    });
    d.check(
        "Selected card is fully visible and painted",
        viewport.contains_rect(rect) && painted,
        json!("unclipped selection fill and complete card"),
        json!({"rect":format!("{rect:?}"),"painted":painted}),
    )
}

fn workspace(d: &mut Driver<'_>) -> Result<(), String> {
    viewer_visible(d)?;
    let cursor = d.app().sequence_cursor;
    d.click("Next  l")?;
    d.settled()?;
    d.check(
        "Pointer navigation advances the real sequence cursor",
        d.app().sequence_cursor == cursor + 1,
        json!(cursor + 1),
        json!(d.app().sequence_cursor),
    )?;
    d.command("source")?;
    d.settled()?;
    let before = d.revision();
    d.chord(&[Key::Comma, Key::I])?;
    d.changed(&before)?;
    d.check(
        "Original reuse through ,i creates one new beat",
        d.app().beat_rows.len() == 2,
        json!(2),
        json!(d.app().beat_rows.len()),
    )?;
    for _ in 0..5 {
        let before = d.revision();
        d.chord(&[Key::Comma, Key::I])?;
        d.changed(&before)?;
    }
    d.capture("Newly inserted card is revealed")?;
    selected_visible(d)?;
    d.wait_for("Visible card thumbnails rendered", |app| {
        app.thumbnails
            .rendered_for_revision(app.workspace.as_deref())
            >= 3
    })?;
    d.check(
        "Visible cards show thumbnails of the current revision through the shared renderer",
        d.app().thumbnails.rendered_for_revision(d.app().workspace.as_deref()) >= 3
            && d.app().thumbnails.retained() <= 48,
        json!({"current_revision_thumbnails":">= 3","bounded":48}),
        json!({"current":d.app().thumbnails.rendered_for_revision(d.app().workspace.as_deref()),"retained":d.app().thumbnails.retained()}),
    )?;
    d.capture("Card thumbnails at the default size")?;
    for (width, height, scale) in [
        (960.0, 640.0, 1.0),
        (1492.0, 929.0, 2.0),
        (1280.0, 820.0, 1.0),
    ] {
        // A monitor scale change is native viewport input. kittest's
        // set_pixels_per_point instead requests egui zoom, which rewrites the
        // previous content rect and can discard this frame's requested size.
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
        let input = d.harness.input_mut();
        input.screen_rect = Some(rect);
        let viewport = input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .ok_or("Native resize has no root viewport")?;
        viewport.native_pixels_per_point = Some(scale);
        viewport.inner_rect = Some(rect);
        for frame in 0..4 {
            d.step(
                &format!("Resize {width}x{height} at {scale}x, transition {frame}"),
                true,
            )?;
            footer_anchored(d, "Footer remains anchored across native resize")?;
            d.check(
                "Native resize uses the requested logical viewport and monitor scale",
                d.harness.ctx.content_rect().size() == egui::vec2(width, height)
                    && (d.harness.ctx.pixels_per_point() - scale).abs() < f32::EPSILON,
                json!({"viewport_points":[width,height],"native_pixels_per_point":scale}),
                json!({"viewport_points":[d.harness.ctx.content_rect().width(),d.harness.ctx.content_rect().height()],"pixels_per_point":d.harness.ctx.pixels_per_point()}),
            )?;
            d.check(
                "Resize retains a submitted picture",
                d.app().presentation.has_displayed(),
                json!(true),
                json!(d.app().presentation.has_displayed()),
            )?;
        }
        selected_visible(d)?;
        viewer_visible(d)?;
        if width == 960.0 {
            d.command("source")?;
            d.settled()?;
            d.capture("Original viewer at the minimum window size")?;
            viewer_visible(d)?;
            d.command("sequence")?;
            d.settled()?;
        }
    }
    // Exercise actual pointer capture and dragging on a real continuous control.
    let before = d.revision();
    let initial = d.app().monitor_gain;
    let slider = d.rect("Monitor · :monitor")?;
    let start = slider.left_center() + egui::vec2(15.0, 0.0);
    d.events(
        "Begin monitor drag",
        vec![
            egui::Event::PointerMoved(start),
            egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    )?;
    for index in 1..=6 {
        d.events(
            "Drag monitor",
            vec![egui::Event::PointerMoved(
                start + egui::vec2(index as f32 * 6.0, 0.0),
            )],
        )?;
    }
    let end = start + egui::vec2(36.0, 0.0);
    d.events(
        "End monitor drag",
        vec![egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    )?;
    d.check(
        "Monitor drag changes monitoring without an edit",
        d.app().monitor_gain != initial && d.revision() == before,
        json!("gain changed, revision unchanged"),
        json!({"before_gain":initial,"after_gain":d.app().monitor_gain,"revision":d.revision()}),
    )?;
    d.capture("Workspace after pointer drag")
}

pub(super) const PICTURE_TOLERANCE_PIXELS: f64 = 0.01;

/// Fitted f32 mesh bounds can exceed their container by numerical residue.
/// Compare in physical pixels without weakening text or control visibility.
pub(super) fn picture_contains_rect(
    outer: egui::Rect,
    inner: egui::Rect,
    pixels_per_point: f32,
) -> bool {
    if !outer.is_finite()
        || !outer.is_positive()
        || !inner.is_finite()
        || !inner.is_positive()
        || !pixels_per_point.is_finite()
        || pixels_per_point <= 0.0
    {
        return false;
    }
    // Widen before subtracting or scaling: finite f32 edges and scales cannot
    // overflow f64, and expanding an f32 rectangle can round the allowance up.
    [
        f64::from(outer.min.x) - f64::from(inner.min.x),
        f64::from(outer.min.y) - f64::from(inner.min.y),
        f64::from(inner.max.x) - f64::from(outer.max.x),
        f64::from(inner.max.y) - f64::from(outer.max.y),
    ]
    .into_iter()
    .all(|overflow| overflow * f64::from(pixels_per_point) <= PICTURE_TOLERANCE_PIXELS)
}

pub(super) fn viewer_visible(d: &mut Driver<'_>) -> Result<(), String> {
    let label = d
        .app()
        .presentation
        .displayed_label()
        .ok_or("No displayed picture")?;
    let viewer = d.rect(&label)?;
    let target = d.app().target.as_ref().ok_or("No picture texture")?;
    let texture = target.texture;
    let viewport = d.harness.ctx.content_rect();
    let pixels_per_point = d.harness.ctx.pixels_per_point();
    let viewer_in_viewport = picture_contains_rect(viewport, viewer, pixels_per_point);
    // A retained texture can outlive its decoded frame or a viewport resize.
    // Its already composed pixels must fit uniformly until a new target lands.
    let raster = egui::vec2(target.target.width() as f32, target.target.height() as f32);
    let scale = (viewer.width() / raster.x).min(viewer.height() / raster.y);
    let expected = egui::Rect::from_center_size(viewer.center(), raster * scale);
    let picture = d.harness.output().shapes.iter().filter_map(|clipped| {
        let egui::Shape::Mesh(mesh) = &clipped.shape else { return None; };
        if mesh.texture_id != texture { return None; }
        let bounds = mesh.calc_bounds();
        let inside_clip = picture_contains_rect(clipped.clip_rect, bounds, pixels_per_point);
        let inside_viewer = picture_contains_rect(viewer, bounds, pixels_per_point);
        let inside_viewport = picture_contains_rect(viewport, bounds, pixels_per_point);
        Some(json!({"bounds":[bounds.min.x,bounds.min.y,bounds.max.x,bounds.max.y],
            "clip":[clipped.clip_rect.min.x,clipped.clip_rect.min.y,clipped.clip_rect.max.x,clipped.clip_rect.max.y],
            "inside_clip":inside_clip,"inside_viewer":inside_viewer,"inside_viewport":inside_viewport,
            "fills_fitted_canvas":bounds.min.distance(expected.min) <= 0.5 && bounds.max.distance(expected.max) <= 0.5,
            "visible":inside_clip && inside_viewer && inside_viewport}))
    }).collect::<Vec<_>>();
    d.check(
        "Picture is actually painted inside the viewer and viewport",
        viewer_in_viewport
            && !picture.is_empty()
            && picture.iter().all(|p| p["visible"] == true && p["fills_fitted_canvas"] == true),
        json!("unclipped fitted picture preserving its submitted raster aspect"),
        json!({"viewer":[viewer.min.x,viewer.min.y,viewer.max.x,viewer.max.y],
            "viewer_in_viewport":viewer_in_viewport,"pixels_per_point":pixels_per_point,
            "containment_tolerance_pixels":PICTURE_TOLERANCE_PIXELS,
            "expected_canvas":[expected.min.x,expected.min.y,expected.max.x,expected.max.y],"picture":picture}),
    )?;
    for label in ["Start  gg", "Previous  h", "Next  l", "End  G"] {
        let rect = d.rect(label)?;
        let paint = text_paint_visibility(d, label);
        d.check(
            "Frame navigation is visible and does not overlap the picture",
            viewport.contains_rect(rect)
                && !rect.intersects(viewer)
                && !paint.is_empty()
                && paint.iter().all(|p| p["fully_visible"] == true),
            json!(label),
            json!({"rect":format!("{rect:?}"),"paint":paint}),
        )?;
    }
    Ok(())
}

fn replace_text(d: &mut Driver<'_>, text: &str) -> Result<(), String> {
    d.key_modified(Key::A, egui::Modifiers::COMMAND)?;
    d.events(
        "Replace focused text and submit in one batch",
        vec![
            egui::Event::Text(text.into()),
            key_event(Key::Enter, egui::Modifiers::NONE, true),
            key_event(Key::Enter, egui::Modifiers::NONE, false),
        ],
    )
}

/// Specification §31 step 3: `:repeat 3 gain-step=3dB zoom-step=0.08`
/// escalates the selected three-play Repeat without nesting or retiming it.
fn escalation(d: &mut Driver<'_>, original: u64) -> Result<(), String> {
    let repeat = d.app().selected_beat.clone().ok_or("No selected Repeat")?;
    let before = d.revision();
    d.command("repeat 3 gain-step=3dB zoom-step=0.08")?;
    d.changed(&before)?;
    d.settled()?;
    let workspace = d.app().workspace.clone().ok_or("No project")?;
    let escalation = match &workspace.document.nodes()[&repeat].kind {
        NodeKind::Repeat { escalation, .. } => *escalation,
        _ => None,
    };
    let scale = |frame: u64| -> Result<Option<f64>, String> {
        Ok(workspace
            .plan
            .picture(ProjectFrame(frame as i64))
            .map_err(|e| e.to_string())?
            .framing
            .iter()
            .find(|layer| layer.escalation)
            .and_then(|layer| layer.pose)
            .map(|pose| pose.scale.numerator() as f64 / pose.scale.denominator() as f64))
    };
    let (first, third) = (scale(1)?, scale(original * 2 + 1)?);
    d.check(
        "A Repeat setter with steps escalates each later play without nesting or retiming",
        escalation.is_some_and(|escalation| escalation.gain_step.millidecibels() == 3000)
            && first.is_none()
            && third.is_some_and(|scale| (scale - 1.16).abs() < 1e-6)
            && d.app().sequence_length() == original * 3
            && d.app().beat_rows.len() == 1,
        json!({"gain_step_mdb":3000,"first_play_scale":null,"third_play_scale":1.16,"frames":original * 3}),
        json!({"escalation":format!("{escalation:?}"),"first":first,"third":third,"frames":d.app().sequence_length(),"message":d.app().message}),
    )?;
    let gain = text_paint_visibility(d, "+3 dB");
    let zoom = text_paint_visibility(d, "+0.080");
    d.check(
        "The inspector shows the escalation steps in full",
        [&gain, &zoom]
            .iter()
            .all(|paint| !paint.is_empty() && paint.iter().all(|p| p["fully_visible"] == true)),
        json!({"gain":"+3 dB","zoom":"+0.080"}),
        json!({"gain":gain,"zoom":zoom}),
    )?;
    d.capture("Escalated Repeat")?;
    let escalated = d.revision();
    // Count, gap and steps change together as one recorded instruction.
    d.command("repeat 4 gap=6f gain-step=3dB")?;
    d.changed(&escalated)?;
    d.settled()?;
    let workspace = d.app().workspace.clone().ok_or("No project")?;
    let changed = match &workspace.document.nodes()[&repeat].kind {
        NodeKind::Repeat {
            iterations,
            gap: Some(gap),
            escalation: Some(escalation),
            ..
        } => {
            iterations.len() == 4
                && gap.duration.frames() == 6
                && escalation.gain_step.millidecibels() == 3000
        }
        _ => false,
    };
    d.check(
        "A Repeat change sets count, gap and steps together",
        changed && d.app().sequence_length() == original * 4 + 3 * 6,
        json!({"plays":4,"gap_frames":6,"frames":original * 4 + 18}),
        json!({"frames":d.app().sequence_length(),"message":d.app().message,"error":d.app().project_error}),
    )?;
    d.capture("Repeat with gaps")?;
    let gapped = d.revision();
    d.key(Key::U)?;
    d.changed(&gapped)?;
    let restored = match &d
        .app()
        .workspace
        .as_ref()
        .ok_or("No project")?
        .document
        .nodes()[&repeat]
        .kind
    {
        NodeKind::Repeat {
            iterations,
            gap,
            escalation,
            ..
        } => iterations.len() == 3 && gap.is_none() && escalation.is_some(),
        _ => false,
    };
    d.check(
        "One undo restores the three-play escalated Repeat",
        restored && d.app().sequence_length() == original * 3,
        json!({"plays":3,"gap":null}),
        json!({"frames":d.app().sequence_length()}),
    )?;
    let escalated = d.revision();
    d.key(Key::U)?;
    d.changed(&escalated)?;
    let restored = match &d
        .app()
        .workspace
        .as_ref()
        .ok_or("No project")?
        .document
        .nodes()[&repeat]
        .kind
    {
        NodeKind::Repeat { escalation, .. } => escalation.is_none(),
        _ => false,
    };
    d.check(
        "One undo removes the escalation",
        restored && d.app().sequence_length() == original * 3,
        json!({"escalation":null}),
        d.snapshot(),
    )?;
    // ,e wraps the selected beat in an escalating Repeat as one edit.
    let before = d.revision();
    d.chord(&[Key::Comma, Key::E])?;
    d.changed(&before)?;
    d.settled()?;
    let workspace = d.app().workspace.clone().ok_or("No project")?;
    let wrapper = d.app().selected_beat.clone().ok_or("No wrapper")?;
    let wrapped = matches!(
        &workspace.document.nodes()[&wrapper].kind,
        NodeKind::Repeat { iterations, escalation: Some(escalation), .. }
            if iterations.len() == 3 && escalation.gain_step.millidecibels() == 3000
    );
    d.check(
        ",e wraps the selected beat in a three-play escalating Repeat with one Undo",
        wrapped && d.app().sequence_length() == original * 9 && workspace.can_undo,
        json!({"plays":3,"gain_step_mdb":3000,"frames":original * 9}),
        json!({"wrapped":wrapped,"frames":d.app().sequence_length(),"message":d.app().message}),
    )?;
    let escalated = d.revision();
    d.key(Key::U)?;
    d.changed(&escalated)?;
    d.check(
        "One undo removes the escalating wrapper",
        d.app().sequence_length() == original * 3,
        json!(original * 3),
        json!(d.app().sequence_length()),
    )?;
    Ok(())
}

fn editing(d: &mut Driver<'_>) -> Result<(), String> {
    held_motion_after_leader(d)?;
    let original = d.app().sequence_length();
    let before = d.revision();
    d.chord(&[Key::Num3, Key::R, Key::R])?;
    d.changed(&before)?;
    d.check(
        "Three plays means three total plays",
        d.app().sequence_length() == original * 3,
        json!(original * 3),
        json!(d.app().sequence_length()),
    )?;
    d.click("Plays, gaps or escalation…  Enter")?;
    let before = d.revision();
    replace_text(d, "repeat 2")?;
    d.changed(&before)?;
    d.check(
        "Inspector setter updates rather than nests Repeat",
        d.app().sequence_length() == original * 2 && d.app().beat_rows.len() == 1,
        json!(original * 2),
        json!(d.app().sequence_length()),
    )?;
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    d.check(
        "One undo restores the previous parameter",
        d.app().sequence_length() == original * 3,
        json!(original * 3),
        json!(d.app().sequence_length()),
    )?;
    escalation(d, original)?;
    let before_pause = d.revision();
    d.command("hold 11f")?;
    d.changed(&before_pause)?;
    d.check(
        "Pause before Repeat shifts the full composite suffix",
        d.app().sequence_length() == original * 3 + 11
            && d.app().beat_rows.len() == 2
            && d.app().sequence_cursor == 0,
        json!({"frames":original * 3 + 11,"root_beats":2,"cursor":0}),
        d.snapshot(),
    )?;
    selected_visible(d)?;
    d.capture("Pause before the unchanged Repeat")?;
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    d.check(
        "Undo removes one composite pause without removing Repeat",
        d.app().sequence_length() == original * 3 && d.app().beat_rows.len() == 1,
        json!({"frames":original * 3,"root_beats":1}),
        d.snapshot(),
    )?;
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    d.chord(&[Key::G, Key::G, Key::Num4, Key::Num0, Key::L])?;
    let before = d.revision();
    d.key(Key::S)?;
    d.changed(&before)?;
    let before = d.revision();
    d.chord(&[Key::R, Key::R])?;
    d.changed(&before)?;
    let repeated = d
        .app()
        .selected_beat
        .clone()
        .ok_or("Repeat is not selected")?;
    d.chord(&[Key::K, Key::Num1, Key::Num7, Key::L])?;
    d.settled()?;
    let before_pause_document = d.app().workspace.as_ref().unwrap().document.clone();
    let before = d.revision();
    d.command("hold 11f")?;
    d.changed(&before)?;
    let document = &d.app().workspace.as_ref().unwrap().document;
    let selected_pause = d.app().selected_beat.as_ref().is_some_and(|node| {
        matches!(
            &document.nodes()[node].kind,
            deadpan_core::NodeKind::Hold { recipe }
                if recipe.duration.frames() == 11 && recipe.audio == deadpan_core::HoldAudio::Silence
        )
    });
    d.check(
        "Pause inside Original before Repeat splits once and selects the exact silent Hold",
        d.app().sequence_length() == original * 2 - 40 + 11
            && d.app().beat_rows.len() == 4
            && d.app().sequence_cursor == 17
            && selected_pause
            && document.nodes()[&repeated] == before_pause_document.nodes()[&repeated],
        json!({"frames":original * 2 - 40 + 11,"root_beats":4,"cursor":17,"selected_hold_frames":11}),
        d.snapshot(),
    )?;
    selected_visible(d)?;
    d.capture("Silent interruption inside Original before Repeat")?;
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    let restored = &d.app().workspace.as_ref().unwrap().document;
    d.check(
        "One undo removes the interior pause and its split while preserving Repeat",
        d.app().sequence_length() == original * 2 - 40
            && d.app().beat_rows.len() == 2
            && restored.nodes() == before_pause_document.nodes()
            && restored.audio_bindings() == before_pause_document.audio_bindings(),
        json!({"frames":original * 2 - 40,"root_beats":2}),
        d.snapshot(),
    )?;
    for _ in 0..2 {
        let before = d.revision();
        d.key(Key::U)?;
        d.changed(&before)?;
    }
    d.chord(&[Key::G, Key::G])?;
    d.settled()?;
    d.command("hold 11f")?;
    d.wait_for("Exact pause is selected", |app| {
        app.sequence_length() == 131
    })?;
    d.settled()?;
    selected_visible(d)?;
    let before = d.revision();
    d.click("Change duration…  Enter")?;
    replace_text(d, "hold-duration 17f")?;
    d.changed(&before)?;
    d.check(
        "Pause duration setter preserves surrounding Original",
        d.app().sequence_length() == original + 17,
        json!(original + 17),
        json!(d.app().sequence_length()),
    )?;
    d.capture("Edited pause and selected scope")
}

fn held_motion_after_leader(d: &mut Driver<'_>) -> Result<(), String> {
    d.chord(&[Key::G, Key::G])?;
    d.settled()?;
    let baseline = d
        .app()
        .workspace
        .as_ref()
        .ok_or("Held-key replay has no workspace")?
        .document
        .clone();
    let before = d.revision();
    let history = {
        let workspace = d.app().workspace.as_ref().unwrap();
        (workspace.can_undo, workspace.can_redo)
    };
    let selected = d.app().selected_beat.clone();
    let original_cursor = d.app().source_cursor;
    let pause_frames = navigation::duration::DurationInput::half_seconds(1)
        .resolve(baseline.presentation_basis().frame_rate)?
        .frames();
    d.check(
        "Held-key pause witness starts at the protected Original baseline",
        history == (false, false) && d.app().sequence_cursor == 0,
        json!({"undo":false,"redo":false,"Edit":0}),
        d.snapshot(),
    )?;

    // Keep H down at the start boundary, then press comma. Subsequent native
    // repeats must not reinterpret the held motion as the comma's pause leaf.
    d.events(
        "Hold H at the start boundary",
        vec![key_event(Key::H, egui::Modifiers::NONE, true)],
    )?;
    d.key(Key::Comma)?;
    d.check(
        "Comma waits for an explicit pause key while H is held",
        d.app().bindings.pending() == "," && d.revision() == before,
        json!({"pending":",","revision":before}),
        d.snapshot(),
    )?;
    for _ in 0..3 {
        d.events(
            "Native repeated H cannot complete the pending pause",
            vec![egui::Event::Key {
                key: Key::H,
                physical_key: None,
                pressed: true,
                repeat: true,
                modifiers: egui::Modifiers::NONE,
            }],
        )?;
        let workspace = d.app().workspace.as_ref().unwrap();
        d.check(
            "Repeated H leaves the document, history and comma prefix intact",
            workspace.document.as_ref() == baseline.as_ref()
                && (workspace.can_undo, workspace.can_redo) == history
                && !d.app().service.is_busy()
                && d.app().bindings.pending() == ","
                && d.app().selected_beat == selected
                && d.app().sequence_cursor == 0
                && d.app().source_cursor == original_cursor,
            json!({"revision":before,"pending":",","history":history,"Edit":0}),
            d.snapshot(),
        )?;
    }
    d.capture("Held H preserves the waiting pause prefix")?;
    let hint = d
        .app()
        .bindings
        .pending_hint()
        .ok_or("Comma lost its teaching")?;
    let paint = text_paint_visibility(d, &hint);
    d.check(
        "Waiting comma guidance is fully painted after ignored repeats",
        !paint.is_empty() && paint.iter().all(|part| part["fully_visible"] == true),
        json!(hint),
        json!(paint),
    )?;

    d.events(
        "Release the held H before explicit pause input",
        vec![key_event(Key::H, egui::Modifiers::NONE, false)],
    )?;
    d.key(Key::H)?;
    d.changed(&before)?;
    let workspace = d.app().workspace.as_ref().unwrap();
    let holds: Vec<_> = workspace
        .document
        .nodes()
        .values()
        .filter_map(|node| match &node.kind {
            deadpan_core::NodeKind::Hold { recipe } => Some(recipe),
            _ => None,
        })
        .collect();
    d.check(
        "Explicit H completes the retained comma as exactly one silent half-second Hold",
        holds.len() == 1
            && holds[0].duration.frames() == pause_frames
            && holds[0].audio == deadpan_core::HoldAudio::Silence
            && workspace.plan.duration().frames()
                == baseline
                    .duration()
                    .map_err(|error| error.to_string())?
                    .frames()
                    + pause_frames
            && workspace.can_undo
            && !workspace.can_redo
            && d.app().bindings.pending().is_empty()
            && d.app().sequence_cursor == 0,
        json!({"holds":1,"pause_frames":pause_frames,"pending":"","Edit":0}),
        d.snapshot(),
    )?;
    let saved = d.revision();
    d.key(Key::U)?;
    d.changed(&saved)?;
    let restored = d.app().workspace.as_ref().unwrap();
    let mut actual =
        serde_json::to_value(restored.document.as_ref()).map_err(|error| error.to_string())?;
    let expected = serde_json::to_value(baseline.as_ref()).map_err(|error| error.to_string())?;
    actual["revision_id"] = json!(baseline.revision_id());
    d.check(
        "One Undo removes exactly the explicit pause and restores every authored field",
        actual == expected
            && restored.document.revision_id() != baseline.revision_id()
            && !restored.can_undo
            && restored.can_redo
            && d.app().sequence_cursor == 0,
        json!({"document":"exact baseline apart from fresh revision","undo":false,"redo":true}),
        d.snapshot(),
    )
}

fn camera(d: &mut Driver<'_>) -> Result<(), String> {
    let before = d.revision();
    let entry = d.app().presentation.diagnostic_snapshot();
    let geometry = entry["geometry_revision"].clone();
    let framing = entry["decoded_framing"].clone();
    d.check(
        "Camera has an observable submitted entry composition",
        !framing.is_null() && entry["geometry_revision"] == entry["decoded_geometry_revision"],
        json!("framing of the submitted entry picture"),
        entry.clone(),
    )?;
    reveal_inspector_button(d, "Camera…  ,f")?;
    d.click("Camera…  ,f")?;
    d.wait_for("Camera opens on the displayed picture", |app| {
        app.camera.is_some()
    })?;
    d.step("Camera controls appear after mode entry", true)?;
    footer_anchored(d, "Camera footer fits on its first mode-entry paint")?;
    d.click("Framing scale, percent of input size")?;
    replace_text(d, "135")?;
    d.settled()?;
    let adjusted = d.app().presentation.diagnostic_snapshot();
    d.check(
        "Camera field previews without committing",
        d.revision() == before
            && adjusted["geometry_revision"] != geometry
            && adjusted["decoded_framing"] != framing
            && adjusted["geometry_revision"] == adjusted["decoded_geometry_revision"],
        json!("changed framing submitted, unchanged project revision"),
        adjusted,
    )?;
    d.click("Cancel  Esc")?;
    d.step("First paint after Camera cancellation", true)?;
    footer_anchored(d, "Camera cancellation leaves no footer gap")?;
    d.settled()?;
    let restored = d.app().presentation.diagnostic_snapshot();
    d.check(
        "Camera cancellation restores the actual submitted entry composition",
        d.app().camera.is_none()
            && d.revision() == before
            && restored["decoded_framing"] == framing
            && restored["geometry_revision"] == restored["decoded_geometry_revision"]
            && restored["source_frame"] == entry["source_frame"],
        json!({"revision":before,"framing":framing,"source_frame":entry["source_frame"]}),
        json!({"revision":d.revision(),"picture":restored}),
    )?;
    d.chord(&[Key::Comma, Key::F])?;
    d.wait_for("Camera opens again", |app| app.camera.is_some())?;
    d.key(Key::Plus)?;
    d.click("Apply  Enter")?;
    d.changed(&before)?;
    d.check(
        "Apply commits and closes Camera",
        d.app().camera.is_none(),
        json!(false),
        json!(d.app().camera.is_some()),
    )?;
    let selected = d
        .app()
        .selected_beat
        .as_ref()
        .ok_or("No selected Camera beat")?;
    d.check(
        "Committed framing reaches the actual document",
        d.app().workspace.as_ref().unwrap().document.nodes()[selected]
            .framing
            .is_some(),
        json!(true),
        json!(true),
    )?;
    d.capture("Committed Camera result")
}

fn menus(d: &mut Driver<'_>) -> Result<(), String> {
    // File is disabled while the project service works on the preceding
    // command. Show one idle frame so the accessibility tree is current.
    d.wait_for("Project service idle", |app| !app.service.is_busy())?;
    d.step("Idle frame before opening File", false)?;
    let before = d.revision();
    d.click("File")?;
    d.key(Key::D)?;
    d.key(Key::D)?;
    d.check(
        "File menu owns edit keys",
        d.revision() == before,
        json!(before),
        json!(d.revision()),
    )?;
    d.key(Key::Escape)?;
    d.click("Keys  ?")?;
    d.check(
        "Help opens through its visible control",
        d.app().help_open,
        json!(true),
        json!(d.app().help_open),
    )?;
    // The first Window pass measures its content before painting it. Preserve
    // those transition frames, then measure the settled initial reference.
    for frame in 0..8 {
        d.step(&format!("Help opening transition {frame}"), true)?;
    }
    let initial_scroll = d.app().help_scroll.diagnostic_snapshot();
    let initial_content = visible_help_markers(d);
    d.key(Key::End)?;
    for frame in 0..8 {
        d.step(&format!("Help scroll transition {frame}"), true)?;
    }
    let end_scroll = d.app().help_scroll.diagnostic_snapshot();
    let end_content = visible_help_markers(d);
    let initial_offset = scroll_value(&initial_scroll, "offset")?;
    let end_offset = scroll_value(&end_scroll, "offset")?;
    let maximum = scroll_value(&end_scroll, "maximum")?;
    d.check(
        "Help End moves the measured viewport to the bottom and changes painted content",
        maximum > 0.0
            && end_offset > initial_offset
            && (end_offset - maximum).abs() < 1.0
            && !initial_content.is_empty()
            && !end_content.is_empty()
            && initial_content != end_content,
        json!({"offset":maximum,"painted_help_changed":true}),
        json!({"initial":initial_scroll,"end":end_scroll,"before_paint":initial_content,"after_paint":end_content}),
    )?;
    let point = egui::pos2(640.0, 410.0);
    d.events(
        "Wheel up over help",
        vec![
            egui::Event::PointerMoved(point),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, 180.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    )?;
    for frame in 0..12 {
        d.step(&format!("Help wheel transition {frame}"), frame < 4)?;
    }
    let wheel_scroll = d.app().help_scroll.diagnostic_snapshot();
    let wheel_content = visible_help_markers(d);
    d.check(
        "Help pointer wheel scrolls upward and changes painted reference content",
        scroll_value(&wheel_scroll, "offset")? < end_offset
            && !wheel_content.is_empty()
            && wheel_content != end_content,
        json!({"offset_less_than":end_offset,"painted_help_changed":true}),
        json!({"scroll":wheel_scroll,"painted_help":wheel_content}),
    )?;
    help_search(d)?;
    d.key(Key::Escape)?;
    d.key(Key::Colon)?;
    d.events(
        "Text contains editing keys and punctuation",
        vec![egui::Event::Text("dd rr ,h ? é".into())],
    )?;
    d.check(
        "Text owns editing punctuation",
        d.revision() == before && d.app().command == "dd rr ,h ? é" && !d.app().help_open,
        json!("unchanged edit and intact text"),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    let expected_focus = pane_id(d.app().pane);
    let actual_focus = d.harness.ctx.memory(|memory| memory.focused());
    d.check(
        "Text cancellation restores the exact active pane focus",
        !d.app().command_open && actual_focus == Some(expected_focus),
        json!({"command_open":false,"focused_widget":format!("{expected_focus:?}")}),
        json!({"command_open":d.app().command_open,"focused_widget":actual_focus.map(|id|format!("{id:?}")),"pane":format!("{:?}",d.app().pane)}),
    )?;
    d.capture("Focus after closing text")?;
    footer_anchored(d, "First paint after Escape closes command entry")?;
    footer_transitions(d)
}

fn footer_transitions(d: &mut Driver<'_>) -> Result<(), String> {
    for (width, height) in [(960.0, 640.0), (1280.0, 820.0)] {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
        let input = d.harness.input_mut();
        input.screen_rect = Some(rect);
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .ok_or("Footer resize has no root viewport")?
            .inner_rect = Some(rect);
        d.key(Key::Colon)?;
        footer_anchored(d, "Command opens on the first resized frame")?;
        let before = d.revision();
        d.events(
            "Type exact speed and reveal its wrapped explanation",
            vec![egui::Event::Text("retime 0.75 pitch=preserve".into())],
        )?;
        footer_anchored(d, "Dynamic speed hint fits on its first text-input frame")?;
        d.check(
            "Layout retries do not duplicate native text or commit an edit",
            d.app().command == "retime 0.75 pitch=preserve" && d.revision() == before,
            json!("one unchanged text entry and revision"),
            d.snapshot(),
        )?;
        d.capture(&format!("Command explanation at {width}x{height}"))?;
        d.key(Key::Escape)?;
        footer_anchored(
            d,
            "Escape paints the closed command mode in the same input frame",
        )?;
        d.step("First paint after cancelling wrapped command", true)?;
        footer_anchored(d, "Wrapped command cancellation leaves no layout gap")?;
        d.key(Key::Colon)?;
        d.click("Original")?;
        d.step("First paint after pointer leaves command entry", true)?;
        footer_anchored(d, "Pointer command dismissal leaves no layout gap")?;
        for text in ["reuse all", "command", "keys"] {
            let paint = text_paint_visibility(d, text);
            d.check(
                "Original keyboard hints wrap completely inside the viewport",
                !paint.is_empty() && paint.iter().all(|part| part["fully_visible"] == true),
                json!(text),
                json!(paint),
            )?;
        }
        d.check(
            "First pointer action after command entry changes context",
            !d.app().command_open && d.app().view == View::Source && d.revision() == before,
            json!("Original context, command closed, unchanged revision"),
            d.snapshot(),
        )?;
        d.command("sequence")?;
        footer_anchored(
            d,
            "Enter paints the closed command mode in the same input frame",
        )?;
        d.key(Key::R)?;
        footer_anchored(
            d,
            "Pending operator immediately after command submission stays anchored",
        )?;
        d.check(
            "Layout retries preserve one pending operator",
            d.app().bindings.pending() == "r" && d.revision() == before,
            json!("r and unchanged revision"),
            d.snapshot(),
        )?;
        d.key(Key::Escape)?;
    }
    let before = d.revision();
    d.key(Key::Colon)?;
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 640.0));
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Command cancellation resize has no root viewport")?
        .inner_rect = Some(rect);
    d.events(
        "Resize, final command text and Escape in one native batch",
        vec![
            egui::Event::Text("hold 0.5s".into()),
            key_event(Key::Escape, egui::Modifiers::NONE, true),
            key_event(Key::Escape, egui::Modifiers::NONE, false),
        ],
    )?;
    footer_anchored(
        d,
        "Resizing and cancelling a changing command needs no later frame",
    )?;
    d.check(
        "Same-batch cancellation retains final text exactly once without an edit",
        d.app().command == "hold 0.5s" && !d.app().command_open && d.revision() == before,
        json!("closed entry with final text; unchanged revision"),
        d.snapshot(),
    )?;
    Ok(())
}

pub(super) fn footer_anchored(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    let observed = d.app().feedback.footer_bottom;
    let dispatches = d
        .report
        .steps
        .last()
        .and_then(|step| step.semantic["stages"].as_array())
        .map(|events| {
            events
                .iter()
                .filter(|event| event["stage"] == "input_dispatch")
                .count()
        });
    d.check(
        label,
        observed.is_some_and(|(actual, expected)| (actual - expected).abs() <= 0.5)
            && d.app().feedback.footer_command_open == d.app().command_open
            && dispatches == Some(1),
        json!("footer meets the notice panel on the first painted frame"),
        json!({"bottom":observed,"passes":d.harness.output().platform_output.num_completed_passes,"painted_command_open":d.app().feedback.footer_command_open,"command_open":d.app().command_open,"input_dispatches":dispatches}),
    )
}

fn delayed(d: &mut Driver<'_>) -> Result<(), String> {
    let initial = d.app().presentation.diagnostic_snapshot()["displayed"].clone();
    d.app_mut().feedback.hold_preview = true;
    d.key(Key::L)?;
    d.wait_for(
        "Real decoder result retained at controlled boundary",
        |app| app.feedback.held_reply.is_some(),
    )?;
    for _ in 0..8 {
        d.key(Key::L)?;
    }
    d.check(
        "Delayed decode keeps old picture with current requested cursor",
        d.app().presentation.diagnostic_snapshot()["displayed"] == initial
            && d.app().sequence_cursor == 9,
        json!("previous display retained; cursor 9"),
        d.snapshot(),
    )?;
    d.harness.set_size(egui::vec2(960.0, 640.0));
    d.capture("Resize while a newer picture is pending")?;
    let held = d.app_mut().feedback.held_reply.take();
    d.app_mut().feedback.hold_preview = false;
    d.app_mut().feedback.release_reply = held;
    d.capture("Deliver deliberately stale decoder result")?;
    d.check(
        "Stale delivery cannot replace the retained picture",
        d.app().presentation.diagnostic_snapshot()["displayed"] == initial,
        json!(initial),
        d.app().presentation.diagnostic_snapshot()["displayed"].clone(),
    )?;
    d.settled()?;
    d.check(
        "Newest desired picture eventually wins",
        d.app().presentation.displayed_label().as_deref() == Some("Showing sequence frame 10"),
        json!("Showing sequence frame 10"),
        json!(d.app().presentation.displayed_label()),
    )?;
    d.app_mut().feedback.fail_next_preview = true;
    d.key(Key::L)?;
    d.wait_for("Decoder failure is visible", |app| {
        app.presentation.error().is_some()
    })?;
    d.capture("Picture failure retains editor controls")?;
    d.key(Key::L)?;
    d.settled()?;
    d.check(
        "New navigation recovers from picture failure",
        d.app().presentation.error().is_none() && d.app().presentation.has_displayed(),
        json!("error cleared and new picture submitted"),
        d.snapshot(),
    )?;
    d.capture("Recovered picture")
}

fn rapid(d: &mut Driver<'_>) -> Result<(), String> {
    super::repeat_input::run(d)?;
    for _ in 0..30 {
        d.key(Key::L)?;
    }
    d.settled()?;
    d.check(
        "Rapid navigation retains final intent",
        d.app().sequence_cursor == 30,
        json!(30),
        json!(d.app().sequence_cursor),
    )?;
    d.capture("Final rapid-input state")?;
    // Unhovering excludes intentional tooltips from idle repaint observation.
    d.events("Pointer leaves workspace", vec![egui::Event::PointerGone])?;
    for _ in 0..120 {
        d.step("Idle settling", false)?;
    }
    let mut immediate = 0;
    for _ in 0..30 {
        d.step("Idle repaint observation", false)?;
        if d.harness
            .output()
            .viewport_output
            .values()
            .any(|viewport| viewport.repaint_delay.is_zero())
        {
            immediate += 1;
        }
    }
    d.check(
        "Stopped workspace does not request continuous repaint",
        immediate == 0,
        json!(0),
        json!(immediate),
    )?;
    if d.options.mode == RunMode::Performance {
        // Warm the two exact source positions and persistent rendering path.
        // Neither these samples nor initial import, idle, and rejected edits
        // contribute to the measured warm navigation distributions.
        for _ in 0..8 {
            d.key(Key::H)?;
            d.settled()?;
            d.key(Key::L)?;
            d.settled()?;
        }
        let cpu_start = completed_samples(d, "input_to_state_ms").len();
        let picture_start = completed_samples(d, "input_to_picture_complete_ms").len();
        for _ in 0..60 {
            let cursor = d.app().sequence_cursor;
            d.key(Key::H)?;
            d.settled()?;
            d.check(
                "Warm previous-frame input reaches its exact cursor",
                d.app().sequence_cursor == cursor - 1,
                json!(cursor - 1),
                json!(d.app().sequence_cursor),
            )?;
            d.key(Key::L)?;
            d.settled()?;
            d.check(
                "Warm next-frame input reaches its exact cursor",
                d.app().sequence_cursor == cursor,
                json!(cursor),
                json!(d.app().sequence_cursor),
            )?;
        }
        let cpu = completed_samples(d, "input_to_state_ms")[cpu_start..].to_vec();
        let picture =
            completed_samples(d, "input_to_picture_complete_ms")[picture_start..].to_vec();
        d.check(
            "Warm performance gates contain one completed sample per navigation input",
            cpu.len() == 120 && picture.len() == 120,
            json!({"inputs":120,"cpu_samples":120,"picture_completion_samples":120}),
            json!({"cpu_samples":cpu.len(),"picture_completion_samples":picture.len()}),
        )?;
        // Explicit subsets identify what each budget measures. GPU completion
        // comes from the actual offscreen submission wait, before readback.
        for (name, samples) in [
            ("warm_navigation_input_cpu_ms", cpu),
            ("warm_navigation_input_to_picture_complete_ms", picture),
        ] {
            for elapsed in samples {
                d.metric(name, elapsed, SampleOutcome::Completed);
            }
        }
        for (metric, limit) in [
            ("warm_navigation_input_cpu_ms", 8.0),
            ("warm_navigation_input_to_picture_complete_ms", 80.0),
        ] {
            let mut values = completed_samples(d, metric);
            values.sort_by(f64::total_cmp);
            let p95 = values
                .get((values.len() * 95).div_ceil(100).saturating_sub(1))
                .copied();
            d.check(
                &format!("Measured {metric} p95 below {limit} ms"),
                values.len() >= 100 && p95.is_some_and(|value| value < limit),
                json!({"minimum_samples":100,"p95_below_ms":limit}),
                json!({"n":values.len(),"p95_ms":p95}),
            )?;
        }
    }
    Ok(())
}

fn completed_samples(d: &Driver<'_>, name: &str) -> Vec<f64> {
    d.report
        .timings
        .iter()
        .find(|metric| metric.name == name)
        .map(|metric| {
            metric
                .samples
                .iter()
                .filter(|sample| sample.outcome == SampleOutcome::Completed)
                .map(|sample| sample.elapsed_ms)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn text_paint_visibility(d: &Driver<'_>, needle: &str) -> Vec<Value> {
    let viewport = d.harness.ctx.content_rect();
    let shapes = &d.harness.output().shapes;
    shapes.iter().enumerate().filter_map(|(index, clipped)| {
        let egui::Shape::Text(text) = &clipped.shape else { return None; };
        if !text.galley.text().contains(needle) { return None; }
        let bounds = text.visual_bounding_rect();
        let rect = |rect: egui::Rect| [rect.min.x,rect.min.y,rect.max.x,rect.max.y];
        // A later pane can cover correctly clipped text. Inspect the solid
        // interior of later opaque rectangles in final paint order as well.
        let occluders = shapes[index + 1..].iter().filter_map(|later| {
            let egui::Shape::Rect(painted) = &later.shape else { return None; };
            if !painted.fill.is_opaque() || painted.brush.is_some() || painted.blur_width > 0.0 { return None; }
            let radius = painted.corner_radius;
            let inset = f32::from(radius.nw.max(radius.ne).max(radius.sw).max(radius.se));
            let covered = painted.rect.shrink(inset).intersect(later.clip_rect).intersect(bounds);
            (covered.is_positive()).then(|| rect(covered))
        }).collect::<Vec<_>>();
        Some(json!({
            "text":text.galley.text(),"bounds":rect(bounds),
            "clip":rect(clipped.clip_rect),"viewport":rect(viewport),
            "fully_visible":clipped.clip_rect.contains_rect(bounds) && viewport.contains_rect(bounds) && occluders.is_empty(),
            "opaque_rect_occluders":occluders,
        }))
    }).collect()
}

fn reveal_inspector_button(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    let fully_visible = |d: &Driver<'_>| {
        let Ok(rect) = d.rect(label) else {
            return false;
        };
        d.harness.output().shapes.iter().any(|clipped| {
            matches!(&clipped.shape, egui::Shape::Text(text) if text.galley.text() == label)
                && clipped.clip_rect.contains_rect(rect.shrink(1.0))
        })
    };
    if fully_visible(d) {
        return Ok(());
    }
    d.report.findings.push(Finding {
        severity: Severity::Warning,
        message: format!("The {label:?} action needs inspector scrolling at 1280×820 before its full button is clickable. The replay includes this extra user step."),
    });
    let heading = d.rect("Selected beat inspector pane")?;
    let point = heading.center() + egui::vec2(0.0, 120.0);
    for attempt in 0..3 {
        d.events(
            &format!("Wheel inspector to reveal {label}, attempt {}", attempt + 1),
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -240.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        )?;
        for frame in 0..8 {
            d.step(
                &format!("Inspector scroll transition {attempt}:{frame}"),
                frame < 4,
            )?;
        }
        if fully_visible(d) {
            return d.check(
                "Inspector wheel reveals the complete Camera action hit target",
                true,
                json!("button and click target inside the actual paint clip"),
                json!({"label":label,"wheel_attempts":attempt+1}),
            );
        }
    }
    d.check(
        "Inspector wheel reveals the complete Camera action hit target",
        false,
        json!("button and click target inside the actual paint clip"),
        json!({"label":label,"wheel_attempts":3,"widgets":d.widgets()}),
    )
}

fn scroll_value(snapshot: &Value, name: &str) -> Result<f64, String> {
    snapshot[name]
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("Help scroll observation is missing finite {name}: {snapshot}"))
}

/// Observe actual clipped help text paint, independently of retained scroll
/// state. Marker rectangles change even when a short wheel keeps the same rows.
/// `/` searches the registry-driven reference by key, verb or name; Escape
/// leaves the field before it closes Help.
fn help_search(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    d.events(
        "Slash opens Help search and the typed key path filters it",
        vec![
            egui::Event::Key {
                key: Key::Slash,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Text("/".into()),
            egui::Event::Text("dd".into()),
        ],
    )?;
    for frame in 0..4 {
        d.step(&format!("Help search transition {frame}"), true)?;
    }
    let row = text_paint_visibility(d, "Cut the selected beat");
    let unrelated = text_paint_visibility(d, "Insert a pause");
    d.check(
        "Searching dd lists the whole-beat cut and hides unrelated actions",
        d.app().help_open
            && row.iter().any(|part| part["fully_visible"] == true)
            && unrelated.is_empty()
            && d.revision() == revision,
        json!({"visible":"Cut the selected beat","hidden":"Insert a pause"}),
        json!({"row":row,"unrelated":unrelated}),
    )?;
    d.capture("Help search for dd")?;
    d.events(
        "Replace the search with a command verb",
        vec![
            egui::Event::Key {
                key: Key::Backspace,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Key {
                key: Key::Backspace,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Text(":hold".into()),
        ],
    )?;
    for frame in 0..4 {
        d.step(&format!("Help verb search transition {frame}"), true)?;
    }
    let hold = text_paint_visibility(d, "Insert a pause");
    let usage = text_paint_visibility(d, ",h · :hold 0.5s [video=black]");
    d.check(
        "Searching :hold shows the pause action with its key and usage",
        hold.iter().any(|part| part["fully_visible"] == true) && !usage.is_empty(),
        json!("Insert a pause, ,h · :hold 0.5s [video=black]"),
        json!({"name":hold,"keys":usage}),
    )?;
    d.capture("Help search for :hold")?;
    d.key(Key::Escape)?;
    let kept = text_paint_visibility(d, "Insert a pause");
    d.check(
        "The first Escape leaves the search field and keeps Help and its filter",
        d.app().help_open && !kept.is_empty(),
        json!({"help_open":true,"filter_kept":true}),
        json!({"help_open":d.app().help_open,"row":kept}),
    )?;
    Ok(())
}

fn visible_help_markers(d: &Driver<'_>) -> Vec<(String, [f32; 4])> {
    // Section titles come from the action registry and span the sheet.
    let markers: Vec<&str> = crate::navigation::registry::Section::ALL
        .iter()
        .map(|section| section.title())
        .collect();
    d.harness
        .output()
        .shapes
        .iter()
        .filter_map(|clipped| {
            let egui::Shape::Text(text) = &clipped.shape else {
                return None;
            };
            let label = text.galley.text();
            let rect = text.visual_bounding_rect();
            (markers.contains(&label) && clipped.clip_rect.intersect(rect).is_positive()).then(
                || {
                    (
                        label.to_owned(),
                        [rect.min.x, rect.min.y, rect.max.x, rect.max.y],
                    )
                },
            )
        })
        .collect()
}

/// Choose the explicit speaker interpretation for unlabelled sound channels
/// through the real "Sound import options" disclosure and its combo box,
/// with pointer input only. Plain WAV fixtures declare no layout, so every
/// sound registration replay states its choice before Add sound.
pub(super) fn choose_sound_interpretation(
    d: &mut Driver<'_>,
    choice: Option<crate::project::AudioLayoutInterpretation>,
) -> Result<(), String> {
    let text = |choice: Option<crate::project::AudioLayoutInterpretation>| match choice {
        None => "Not chosen".to_owned(),
        Some(choice) => format!("{}, {} ch", choice.label(), choice.channels()),
    };
    let combo = |d: &Driver<'_>, value: &str| {
        let matches = d
            .harness
            .root()
            .children_recursive()
            .filter(|node| {
                let access = node.accesskit_node();
                access.role() == egui::accesskit::Role::ComboBox
                    && access.value().as_deref() == Some(value)
                    && !access.is_disabled()
                    && node.rect().is_positive()
            })
            .map(|node| node.rect())
            .collect::<Vec<_>>();
        // Retained accessibility geometry alone proves no paint: require the
        // selected text itself to be painted unclipped and uncovered.
        let paint = text_paint_visibility(d, value);
        match matches.as_slice() {
            [rect]
                if !paint.is_empty() && paint.iter().all(|item| item["fully_visible"] == true) =>
            {
                Some(*rect)
            }
            _ => None,
        }
    };
    let current = text(d.app().sound_interpretation);
    let opened = combo(d, &current).is_none() && d.rect("Unlabelled channels").is_err();
    if opened {
        d.click("Sound import options")?;
        d.settled()?;
    }
    // The expanded disclosure can extend below a short rail; wheel it into view.
    for attempt in 0..6 {
        if combo(d, &current).is_some() {
            break;
        }
        let heading = d.rect("Original and sounds pane")?;
        let point = egui::pos2(heading.center().x, d.harness.ctx.content_rect().center().y);
        d.events(
            &format!("Wheel the rail to the sound interpretation choice, attempt {attempt}"),
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -80.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        )?;
        for _ in 0..8 {
            d.step("Rail scroll settles", false)?;
        }
    }
    let rect = combo(d, &current).ok_or("The unlabelled-channel choice is not visible")?;
    d.click_at("Unlabelled channels choice", rect.center())?;
    // The popup's first sizing pass exposes provisional disabled controls;
    // click only the option's enabled, painted geometry.
    let option = text(choice);
    let mut target = None;
    for attempt in 0..=4 {
        let candidates = d
            .harness
            .root()
            .children_recursive()
            .filter(|node| {
                let access = node.accesskit_node();
                (access.label().as_deref() == Some(option.as_str())
                    || access.value().as_deref() == Some(option.as_str()))
                    && access.role() != egui::accesskit::Role::ComboBox
                    && !access.is_disabled()
                    && !access.is_hidden()
                    && node.rect().is_positive()
            })
            .map(|node| node.rect())
            .collect::<Vec<_>>();
        let painted = text_paint_visibility(d, &option)
            .iter()
            .any(|item| item["fully_visible"] == true);
        if let [rect] = candidates.as_slice()
            && painted
        {
            target = Some(*rect);
            break;
        }
        if attempt < 4 {
            d.step("Settle the unlabelled-channel popup", false)?;
        }
    }
    let target = target.ok_or_else(|| format!("Popup option {option:?} was not painted"))?;
    d.click_at(&option, target.center())?;
    d.settled()?;
    // Leave the rail as it was so later layout checks see their usual state.
    if opened {
        d.click("Sound import options")?;
        d.settled()?;
    }
    d.check(
        "The sound interpretation is chosen explicitly through the import options",
        d.app().sound_interpretation == choice,
        json!({"interpretation": choice.map(|choice| choice.wire_name())}),
        json!({"interpretation": d.app().sound_interpretation.map(|choice| choice.wire_name())}),
    )
}

#[cfg(test)]
mod picture_geometry_tests {
    use super::picture_contains_rect;
    use eframe::egui::{Rect, pos2};

    #[test]
    fn fitted_canvas_rounding_residue_is_visible_at_one_and_two_times_scale() {
        let viewer = Rect::from_min_max(pos2(208.0, 97.0), pos2(1244.0, 497.0));
        let bounds = Rect::from_min_max(pos2(370.444_43, 96.999_985), pos2(1_081.555_5, 497.0));
        assert_eq!(f64::from(bounds.min.y), 96.999_984_741_210_94);
        assert!(!viewer.contains_rect(bounds));
        for pixels_per_point in [1.0, 2.0] {
            assert!(picture_contains_rect(viewer, bounds, pixels_per_point));
        }
    }

    #[test]
    fn a_quarter_pixel_overflow_is_rejected_on_every_edge() {
        let outer = Rect::from_min_max(pos2(208.0, 97.0), pos2(1244.0, 497.0));
        for pixels_per_point in [1.0, 2.0] {
            let overflow = 0.25 / pixels_per_point;
            for edge in 0..4 {
                let mut inner = outer;
                match edge {
                    0 => inner.min.x -= overflow,
                    1 => inner.min.y -= overflow,
                    2 => inner.max.x += overflow,
                    _ => inner.max.y += overflow,
                }
                assert!(!picture_contains_rect(outer, inner, pixels_per_point));
            }
        }
    }

    #[test]
    fn allowance_is_measured_in_physical_pixels() {
        let outer = Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0));
        for pixels_per_point in [1.0, 2.0] {
            let mut inner = outer;
            inner.max.x += 0.005 / pixels_per_point;
            assert!(picture_contains_rect(outer, inner, pixels_per_point));
            inner.max.x = outer.max.x + 0.015 / pixels_per_point;
            assert!(!picture_contains_rect(outer, inner, pixels_per_point));
        }
    }

    #[test]
    fn invalid_rectangles_and_pixel_scales_cannot_pass_visibility() {
        let valid = Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0));
        for invalid in [
            Rect::from_min_max(pos2(f32::NAN, 0.0), pos2(100.0, 100.0)),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(f32::INFINITY, 100.0)),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(0.0, 100.0)),
            Rect::from_min_max(pos2(100.0, 0.0), pos2(0.0, 100.0)),
        ] {
            assert!(!picture_contains_rect(valid, invalid, 1.0));
            assert!(!picture_contains_rect(invalid, valid, 1.0));
        }
        for scale in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(!picture_contains_rect(valid, valid, scale));
        }
        assert!(picture_contains_rect(valid, valid, 1.0));
    }
}
