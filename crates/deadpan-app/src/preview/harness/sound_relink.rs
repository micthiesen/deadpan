//! Real retained sound repair, with only picker answers and damaged bytes scripted.

use super::*;
use crate::project::{AudioLayoutInterpretation, RelinkState};
use egui::{Key, Modifiers};
use std::os::unix::fs::PermissionsExt;

fn submit_captured(d: &mut Driver<'_>) -> Result<(), String> {
    d.events(
        "Submit captured sound repair",
        vec![
            egui::Event::Text("sound-relink".into()),
            key_event(Key::Enter, Modifiers::NONE, true),
            key_event(Key::Enter, Modifiers::NONE, false),
        ],
    )
}

fn select_catalog_sound(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    // Placement scrolls the rail to reveal its retained sound. Use the stable
    // pane heading and production j routing, not a moving catalog hitbox.
    d.click("Original and sounds pane")?;
    d.key(Key::J)?;
    d.check(
        "Repair target is the focused catalog sound",
        d.app().sound_focused()
            && d.app().workspace.as_ref().is_some_and(|workspace| {
                d.app()
                    .selected_sound
                    .as_ref()
                    .and_then(|asset| workspace.sources.get(asset))
                    .is_some_and(|source| source.label == label)
            }),
        json!("catalog sound focus"),
        d.snapshot(),
    )
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav")
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let wrong = d
        .app()
        .feedback
        .original_fixture
        .clone()
        .ok_or("No Original fixture")?;
    d.app_mut().dialogs = Dialogs::scripted(vec![
        (DialogKind::ImportSound, Some(fixture.clone())),
        (DialogKind::RelinkSound, Some(wrong.clone())),
        (DialogKind::RelinkSound, Some(fixture.clone())),
        (DialogKind::RelinkOriginal, Some(wrong)),
        (DialogKind::RelinkSound, Some(fixture.clone())),
    ]);
    scenarios::choose_sound_interpretation(d, Some(AudioLayoutInterpretation::StereoLeftRight))?;
    d.click("Add sound…  ⌘I")?;
    d.wait_for("Sound retained and registered", |app| {
        app.sound_rows.len() == 1 && !app.service.is_busy() && !app.importing()
    })?;
    d.settled()?;
    let (asset, label, _) = d.app().sound_rows[0].clone();
    select_catalog_sound(d, &label)?;
    let before_placement = d.revision();
    d.command("sound-place")?;
    d.changed(&before_placement)?;
    let workspace = d.app().workspace.clone().ok_or("No workspace")?;
    let path = workspace.path.clone();
    let original_asset = original_asset(&workspace).ok_or("No Original")?.clone();
    let original_record = workspace.sources[&original_asset].original.clone();
    let sound_record = workspace.sources[&asset].original.clone();
    let object = path.join("Media/Originals").join(format!(
        "blake3-{}",
        sound_record.object().content().digest()
    ));
    let original_object = path.join("Media/Originals").join(format!(
        "blake3-{}",
        original_record.object().content().digest()
    ));
    let original_bytes = std::fs::read(&original_object).map_err(|e| e.to_string())?;
    let sound_bytes = std::fs::read(&fixture).map_err(|e| e.to_string())?;
    let revision = d.revision();
    let positions = (d.app().source_cursor, d.app().sequence_cursor);
    // A present file is not reported missing by the metadata-only inventory.
    // Same length but wrong content must still be repairable explicitly.
    let mut damaged = sound_bytes.clone();
    let last = damaged.last_mut().ok_or("Empty fixture")?;
    *last ^= 1;
    std::fs::set_permissions(&object, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| e.to_string())?;
    std::fs::write(&object, &damaged).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&object, std::fs::Permissions::from_mode(0o400))
        .map_err(|e| e.to_string())?;
    select_catalog_sound(d, &label)?;
    d.command("sound-relink")?;
    d.wait_for("Wrong sound replacement refused", |app| {
        app.recovery
            .relink_status()
            .is_some_and(|status| matches!(status.state, RelinkState::Failed(_)))
            && !app.service.is_busy()
    })?;
    d.check(
        "Wrong content leaves the damaged sound and original revision intact",
        std::fs::read(&object).map_err(|e| e.to_string())? == damaged && d.revision() == revision,
        json!("unchanged damaged bytes and revision"),
        d.snapshot(),
    )?;
    d.command("sound-relink")?;
    d.wait_for("Damaged sound repaired", |app| {
        app.recovery
            .relink_status()
            .is_some_and(|status| status.state == RelinkState::Restored)
            && !app.service.is_busy()
    })?;
    d.settled()?;
    d.check(
        "Identical sound repairs its project copy without changing the Original, clocks or history",
        std::fs::read(&object).map_err(|e| e.to_string())? == sound_bytes
            && std::fs::read(&original_object).map_err(|e| e.to_string())? == original_bytes
            && d.revision() == revision
            && (d.app().source_cursor, d.app().sequence_cursor) == positions
            && d.app().selected_source.as_ref() == Some(&original_asset)
            && d.app().selected_sound.as_ref() == Some(&asset),
        json!({"revision":revision,"selected_sound":asset}),
        d.snapshot(),
    )?;
    d.capture("Selected sound repaired")?;

    d.command("close")?;
    d.wait_for("Closed before missing-sound test", |app| {
        app.workspace.is_none() && !app.service.is_busy()
    })?;
    std::fs::remove_file(&object).map_err(|e| e.to_string())?;
    std::fs::remove_file(&original_object).map_err(|e| e.to_string())?;
    d.app()
        .service
        .submit(ProjectRequest::Open(path.clone()))
        .map_err(|e| e.to_string())?;
    d.wait_for("Missing Original and sound report", |app| {
        app.recovery.showing_report() && !app.service.is_busy()
    })?;
    d.step("Missing Original first action focused", true)?;
    d.key(Key::Enter)?;
    d.wait_for(
        "Original pictures recover while the sound remains missing",
        |app| {
            app.recovery.report().is_some_and(|report| {
                report.missing().count() == 1 && report.missing().all(|status| status.audio_only)
            }) && app.presentation.error().is_none()
                && !app.presentation.loading()
                && !app.service.is_busy()
        },
    )?;
    d.check(
        "Restoring the Original does not require restoring unrelated sound files",
        std::fs::read(&original_object).map_err(|e| e.to_string())? == original_bytes
            && !object.exists()
            && d.revision() == revision,
        json!("Original available, sound missing, edit unchanged"),
        d.snapshot(),
    )?;
    d.command("recovery")?;
    d.step("Missing sound report focused", true)?;
    d.check(
        "Missing sound report identifies sound rather than unavailable Original pictures",
        d.rect("Locate sound…  :relink").is_ok()
            && d.app().missing_original_notice().is_none()
            && d.app().recovery.report().is_some_and(|report| {
                report.missing().count() == 1 && report.missing().all(|status| !status.primary)
            }),
        json!("sound-specific recovery"),
        d.snapshot(),
    )?;
    d.capture("Missing sound report")?;
    d.key(Key::Enter)?;
    d.wait_for("Missing sound restored", |app| {
        app.recovery
            .report()
            .is_some_and(|report| report.missing().next().is_none())
            && !app.service.is_busy()
    })?;
    d.check(
        "Missing sound restores without a new revision",
        d.revision() == revision
            && std::fs::read(&object).map_err(|e| e.to_string())? == sound_bytes,
        json!(revision),
        d.snapshot(),
    )?;

    // Command entry captures an existing sound. A different project session
    // arriving before Enter must not supply another repair target.
    select_catalog_sound(d, &label)?;
    d.key(Key::Colon)?;
    let session = d.app().workspace.as_ref().ok_or("No workspace")?.session;
    d.app()
        .service
        .submit(ProjectRequest::Close)
        .map_err(|e| e.to_string())?;
    d.wait_for("Captured project closes before Enter", |app| {
        app.workspace.is_none() && !app.service.is_busy()
    })?;
    d.app()
        .service
        .submit(ProjectRequest::Open(path.clone()))
        .map_err(|e| e.to_string())?;
    d.wait_for("New session arrives during sound command", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.session != session)
            && !app.service.is_busy()
    })?;
    submit_captured(d)?;
    d.check(
        "Stale sound command refuses before opening a picker",
        !d.app().dialogs.is_open()
            && d.app()
                .error
                .as_ref()
                .is_some_and(|error| error.contains("captured sound")),
        json!("stale target refused"),
        d.snapshot(),
    )?;

    d.command("close")?;
    d.wait_for("Closed before absent-target test", |app| {
        app.workspace.is_none() && !app.service.is_busy()
    })?;
    d.key(Key::Colon)?;
    d.app()
        .service
        .submit(ProjectRequest::Open(path))
        .map_err(|e| e.to_string())?;
    d.wait_for("Late project open cannot supply a sound", |app| {
        app.workspace.is_some() && !app.service.is_busy()
    })?;
    submit_captured(d)?;
    d.check(
        "Absence at command entry stays absent after asynchronous open",
        !d.app().dialogs.is_open() && d.app().error.as_deref() == Some("Open a project first."),
        json!("Open a project first."),
        d.snapshot(),
    )?;
    Ok(())
}
