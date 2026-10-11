//! Production idle scheduling and real decoded PCM, without an output device.
use super::*;
use egui::Key;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("Idle preparation uses the real PCM worker and host power policy. No output device is activated; acoustic playback, physical keyboards and OS power transitions are not measured.".into());
    d.command("source")?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    d.settled()?;
    let baseline = state(d);
    enable(d)?;
    d.wait_for(
        "Actual stopped-cursor PCM preparation becomes visible",
        |app| app.idle_audio_label().is_some(),
    )?;
    d.capture("Real idle Original preparation")?;
    d.check(
        "Idle preparation leaves all editor state and the device untouched",
        state(d) == baseline,
        baseline.clone(),
        state(d),
    )?;
    d.wait_for("Real Original PCM horizon finishes", |app| {
        app.idle_audio_label().as_deref() == Some("Audio cache checked ahead")
    })?;
    for (width, height) in [(1280.0, 820.0), (960.0, 640.0)] {
        resize(d, width, height)?;
        visible(d, "Audio cache checked ahead", width, height)?;
        d.check(
            "Prepared Original cache preserves both clocks and selection",
            state(d) == baseline,
            baseline.clone(),
            state(d),
        )?;
    }
    d.key(Key::L)?;
    d.check(
        "Navigation immediately rejects the previous cursor's cache status",
        d.app().idle_audio_label().is_none() && d.app().source_cursor == 11,
        json!({"cursor":11,"status":null}),
        json!({"cursor":d.app().source_cursor,"status":d.app().idle_audio_label()}),
    )?;
    d.app_mut().feedback.suppress_audio_warming = true;
    d.app_mut().audio_warming = None;
    d.app().playback.cancel_warming();
    resize(d, 1280.0, 820.0)?;

    d.command("sequence")?;
    d.settled()?;
    let baseline = state(d);
    enable(d)?;
    d.wait_for("Real Edit PCM horizon finishes", |app| {
        app.idle_audio_label().as_deref() == Some("Audio cache checked ahead")
    })?;
    d.capture("Prepared Edit audio leaves the edit untouched")?;
    visible(d, "Audio cache checked ahead", 1280.0, 820.0)?;
    d.check(
        "Idle Edit preparation preserves authored and navigation state",
        state(d) == baseline,
        baseline.clone(),
        state(d),
    )?;
    d.app_mut().feedback.suppress_audio_warming = true;
    d.app_mut().audio_warming = None;
    d.app().playback.cancel_warming();
    d.command("source")?;
    d.settled()?;

    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav")
        .canonicalize()
        .map_err(|error| error.to_string())?;
    d.app_mut().dialogs = Dialogs::scripted(vec![(DialogKind::ImportSound, Some(fixture))]);
    scenarios::choose_sound_interpretation(
        d,
        Some(crate::project::AudioLayoutInterpretation::StereoLeftRight),
    )?;
    scenarios::reveal_original_rail(d, "Add sound…  ⌘I", true)?;
    d.click("Add sound…  ⌘I")?;
    d.wait_for("Idle warming sound is registered", |app| {
        app.sound_rows.len() == 1 && !app.service.is_busy() && !app.importing()
    })?;
    d.settled()?;
    let sound = d.app().sound_rows[0].1.clone();
    // Catalog entries are selectable rows, not the Button role checked by
    // reveal_original_rail's final argument. Require their actual text paint.
    scenarios::reveal_original_rail(d, &sound, false)?;
    d.click(&sound)?;
    d.settled()?;
    let baseline = state(d);
    enable(d)?;
    d.wait_for("Real catalog Sound cache finishes", |app| {
        app.idle_audio_label().as_deref() == Some("Audio cache checked ahead")
    })?;
    for (width, height) in [(1280.0, 820.0), (960.0, 640.0)] {
        resize(d, width, height)?;
        visible(d, "Audio cache checked ahead", width, height)?;
        d.check(
            "Sound cache preparation leaves picture, clocks and selection intact",
            state(d) == baseline,
            baseline.clone(),
            state(d),
        )?;
    }
    d.app_mut().feedback.suppress_audio_warming = true;
    d.app_mut().audio_warming = None;
    d.app().playback.cancel_warming();
    Ok(())
}

fn enable(d: &mut Driver<'_>) -> Result<(), String> {
    d.app_mut().feedback.suppress_audio_warming = false;
    // The replay's virtual clock advances by a frame per step. Exercise the
    // real settle delay without turning it into wall-clock polling latency.
    for _ in 0..26 {
        d.step("Stopped cursor settles before optional audio work", false)?;
    }
    Ok(())
}

fn state(d: &Driver<'_>) -> Value {
    let app = d.app();
    let snapshot = d.snapshot();
    json!({"revision":d.revision(),"original":app.source_cursor,"edit":app.sequence_cursor,
        "sound":app.sound_cursor,"beat":app.selected_beat,"scope":snapshot["sequence_scope"],
        "selection":snapshot["edit_selection"],"original_selection":snapshot["original_selection"],
        "picture":snapshot["picture"]["displayed"],
        "device_generations":app.playback.diagnostics().generations,"transport":app.transport.is_some()})
}

fn resize(d: &mut Driver<'_>, width: f32, height: f32) -> Result<(), String> {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing viewport")?
        .inner_rect = Some(rect);
    d.capture(&format!("Idle audio cache at {width}x{height}"))?;
    Ok(())
}

fn visible(d: &mut Driver<'_>, label: &str, width: f32, height: f32) -> Result<(), String> {
    let paint = scenarios::text_paint_visibility(d, label);
    d.check(
        "Idle preparation status remains fully visible",
        !paint.is_empty() && paint.iter().all(|p| p["fully_visible"] == true),
        json!({"label":label,"size":[width,height]}),
        json!(paint),
    )
}
