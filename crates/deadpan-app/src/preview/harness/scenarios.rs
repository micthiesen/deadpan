//! Complete interaction sequences. Expectations describe user-visible behavior.

use super::*;
use egui::Key;

pub(super) fn run(name: &str, d: &mut Driver<'_>) -> Result<(), String> {
    match name {
        "workspace" => workspace(d),
        "editing" => editing(d),
        "camera" => camera(d),
        "menus" => menus(d),
        "delayed-preview" => delayed(d),
        "rapid-input" => rapid(d),
        "playback-feedback" => playback(d),
        "large-project" => super::scale::run(d),
        "edit-latency" => super::edit_latency::run(d),
        "nested-pause" => super::nested_pause::run(d),
        "original-moment" => super::moment::run(d),
        "original-playback" => super::original_playback::run(d),
        "sound-playback" => super::sound_playback::run(d),
        "retime" => super::retime::run(d),
        _ => Err(format!("Unknown scenario {name}")),
    }
}

fn playback(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("Playback service updates are explicitly simulated. This scenario qualifies UI feedback and picture routing, not PCM preparation, device timing or listening.".into());
    d.click("Play edit  ·  Space")?;
    d.capture("Preparation feedback on the next UI frame")?;
    d.check(
        "Play exposes preparation and cancel within one UI frame",
        d.app()
            .transport
            .as_ref()
            .is_some_and(|run| run.phase == deadpan_playback::Phase::Preparing)
            && d.rect("Cancel preparation  ·  Space").is_ok(),
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
    d.click("Pause  ·  Space")?;
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
    d.click("Play edit  ·  Space")?;
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
    d.click("Play edit  ·  Space")?;
    d.capture("Retry exposes preparation cancellation")?;
    d.click("Cancel preparation  ·  Space")?;
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

fn viewer_visible(d: &mut Driver<'_>) -> Result<(), String> {
    let label = d
        .app()
        .presentation
        .displayed_label()
        .ok_or("No displayed picture")?;
    let viewer = d.rect(&label)?;
    let texture = d.app().target.as_ref().ok_or("No picture texture")?.texture;
    let viewport = d.harness.ctx.content_rect();
    let expected = d.app().presentation.canvas().map_or(viewer, |(w, h)| {
        let scale = (viewer.width() / w as f32).min(viewer.height() / h as f32);
        egui::Rect::from_center_size(viewer.center(), egui::vec2(w as f32, h as f32) * scale)
    });
    let picture = d.harness.output().shapes.iter().filter_map(|clipped| {
        let egui::Shape::Mesh(mesh) = &clipped.shape else { return None; };
        if mesh.texture_id != texture { return None; }
        let bounds = mesh.calc_bounds();
        Some(json!({"bounds":[bounds.min.x,bounds.min.y,bounds.max.x,bounds.max.y],
            "fills_fitted_canvas":bounds.min.distance(expected.min) <= 0.5 && bounds.max.distance(expected.max) <= 0.5,
            "visible":bounds.is_positive() && clipped.clip_rect.contains_rect(bounds) && viewer.contains_rect(bounds)}))
    }).collect::<Vec<_>>();
    d.check(
        "Picture is actually painted inside the viewer and viewport",
        viewport.contains_rect(viewer)
            && !picture.is_empty()
            && picture.iter().all(|p| p["visible"] == true && p["fills_fitted_canvas"] == true),
        json!("unclipped fitted picture"),
        json!({"viewer":[viewer.min.x,viewer.min.y,viewer.max.x,viewer.max.y],
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

fn editing(d: &mut Driver<'_>) -> Result<(), String> {
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
    d.click("Set total plays…  ·  Enter")?;
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
    d.click("Change duration…  ·  Enter")?;
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
    reveal_inspector_button(d, "Camera…  ·  ,f")?;
    d.click("Camera…  ·  ,f")?;
    d.wait_for("Camera opens on the displayed picture", |app| {
        app.camera.is_some()
    })?;
    d.step("Camera controls appear after mode entry", true)?;
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
    d.click("Cancel  ·  Esc")?;
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
    d.click("Apply  ·  Enter")?;
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
    d.capture("Focus after closing text")
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
    let initial_revision = d.revision();
    let initial_duration = d.app().sequence_length();
    let burst_start = d.report.steps.len();
    // One native batch, intentionally no service wait between edits.
    let events = (0..8)
        .flat_map(|_| {
            [
                key_event(Key::R, egui::Modifiers::NONE, true),
                key_event(Key::R, egui::Modifiers::NONE, false),
                key_event(Key::R, egui::Modifiers::NONE, true),
                key_event(Key::R, egui::Modifiers::NONE, false),
            ]
        })
        .collect();
    d.events("Eight rapid repeat operations in one input batch", events)?;
    // Worker busy can become false before the app consumes its committed
    // mailbox update. Wait for the actual revision, then inspect accounting.
    d.changed(&initial_revision)?;
    let stage_count = |name| {
        d.report.steps[burst_start..]
            .iter()
            .flat_map(|step| step.semantic["stages"].as_array().into_iter().flatten())
            .filter(|event| event["stage"] == name)
            .count()
    };
    let admitted = stage_count("command_admitted");
    let rejected = stage_count("command_rejected");
    let committed = stage_count("command_committed");
    let revisions: std::collections::BTreeSet<_> = d.report.steps[burst_start..]
        .iter()
        .filter_map(|step| step.semantic["revision"].as_str())
        .filter(|revision| *revision != initial_revision)
        .map(str::to_owned)
        .collect();
    d.check(
        "All eight rapid edit intents are admitted or explicitly rejected",
        admitted + rejected == 8 && admitted > 0,
        json!({"intents":8,"admitted_plus_rejected":8,"at_least_one_admitted":true}),
        json!({"admitted":admitted,"rejected":rejected}),
    )?;
    let expected_duration = initial_duration
        .checked_mul(
            2_u64
                .checked_pow(u32::try_from(admitted).map_err(|e| e.to_string())?)
                .ok_or("Rapid repeat multiplier overflow")?,
        )
        .ok_or("Rapid repeat duration overflow")?;
    d.check(
        "Every admitted rapid edit has a distinct committed revision and exact result",
        committed == admitted
            && revisions.len() == admitted
            && d.app().sequence_length() == expected_duration
            && d.app().beat_rows.len() == 1
            && revisions.contains(&d.revision()),
        json!({"committed":admitted,"distinct_revisions":admitted,"duration":expected_duration,"root_beats":1}),
        json!({"committed":committed,"revisions":revisions,"duration":d.app().sequence_length(),"root_beats":d.app().beat_rows.len(),"final_revision":d.revision()}),
    )?;
    if rejected > 0 {
        d.report.findings.push(Finding { severity:Severity::Warning,message:format!("{rejected} of eight rapid edit intents were rejected while the project service was busy. The report retains this interaction friction; waits were not inserted to hide it.") });
    }
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
    d.harness.output().shapes.iter().filter_map(|clipped| {
        let egui::Shape::Text(text) = &clipped.shape else { return None; };
        if !text.galley.text().contains(needle) { return None; }
        let bounds = text.visual_bounding_rect();
        let rect = |rect: egui::Rect| [rect.min.x,rect.min.y,rect.max.x,rect.max.y];
        Some(json!({
            "text":text.galley.text(),"bounds":rect(bounds),
            "clip":rect(clipped.clip_rect),"viewport":rect(viewport),
            "fully_visible":clipped.clip_rect.contains_rect(bounds) && viewport.contains_rect(bounds),
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
fn visible_help_markers(d: &Driver<'_>) -> Vec<(String, [f32; 4])> {
    const MARKERS: &[&str] = &[
        "START & MOVE",
        "RESHAPE THE SELECTED BEAT",
        "⌘N / ⌘O",
        ":monitor 25%",
        "h l · Left Right",
        "gg / G",
        ":source / :sequence",
        "Tab / Shift Tab",
        "s / :split",
        ",h / 3,h",
        ":hold 1.5s",
        "rr / 3rr",
        "dd / :delete",
        ":repeat 3",
        ":wrap-repeat 3",
        "Enter in Inspector",
        "Camera + / −",
        "Camera f · 1–5",
        "Camera r · Enter · Esc",
        ",z / ,c",
        ":hold-duration 11f",
        ",i / :insert",
        "u / Ctrl R",
        ": / Enter / Esc",
        "? / :help / Esc",
    ];
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
            (MARKERS.contains(&label) && clipped.clip_rect.intersect(rect).is_positive()).then(
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
