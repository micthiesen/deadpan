//! The workspace on real footage: overview strip, transcript pane, quiet
//! inspector and footer, and picture size at 1280×820 and 960×640.
//!
//! `DEADPAN_UI_REAL_PROJECT` names a disposable single-Original project made
//! from real footage (ideally transcribed). The replay edits it in place, so
//! pass a copy. Without the variable the scenario only reports a skip.

use super::*;
use egui::Key;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let Some(path) = std::env::var_os("DEADPAN_UI_REAL_PROJECT").map(PathBuf::from) else {
        d.report.skipped.push(
            "ui-real: set DEADPAN_UI_REAL_PROJECT to a disposable copy of a real-footage project."
                .into(),
        );
        return Ok(());
    };
    d.app_mut().dialogs = Dialogs::scripted(vec![(DialogKind::OpenProject, Some(path.clone()))]);
    d.key_modified(Key::O, egui::Modifiers::COMMAND)?;
    d.wait_for_limit(
        "Open the real-footage project",
        Duration::from_secs(120),
        |app| {
            app.workspace
                .as_ref()
                .is_some_and(|workspace| workspace.path.file_name() == path.file_name())
                && !app.service.is_busy()
        },
    )?;
    settle(d)?;
    d.capture("Real footage · Original")?;
    d.command("sequence")?;
    settle(d)?;
    picture_dominates(d)?;
    d.capture("Real footage · Your edit")?;

    // About a thousand: one recorded move-and-split macro run with a count,
    // committed as one compound edit, on the untouched full Original.
    typed(d, "qa")?;
    typed(d, "12l")?;
    settle(d)?;
    typed(d, "s")?;
    settle(d)?;
    typed(d, "q")?;
    settle(d)?;
    settle(d)?;
    // A compound edit has a size limit, so run the macro in batches.
    let mut runs = 0;
    while d.app().beat_rows.len() < 1000 && runs < 20 {
        let before = d.revision();
        typed(d, "100@a")?;
        d.wait_for_limit("Counted macro commits", Duration::from_secs(240), |app| {
            !app.service.is_busy() && !app.macros.is_pending()
        })?;
        settle(d)?;
        runs += 1;
        if d.revision() == before {
            break;
        }
    }
    let beats = d.app().beat_rows.len();
    d.check(
        "Batched macro runs leave about a thousand beats",
        beats >= 900,
        json!({"beats_at_least":900}),
        json!({"beats":beats,"runs":runs,"error":d.app().error.clone()}),
    )?;
    picture_dominates(d)?;
    d.capture("Real footage · about a thousand beats")?;
    d.harness.set_size(egui::vec2(960.0, 640.0));
    for frame in 0..4 {
        d.step(
            &format!("Thousand beats at the minimum viewport {frame}"),
            false,
        )?;
    }
    settle(d)?;
    d.capture("Real footage · about a thousand beats at 960x640")?;
    d.harness.set_size(egui::vec2(1280.0, 820.0));
    for frame in 0..4 {
        d.step(
            &format!("Thousand beats at the default viewport {frame}"),
            false,
        )?;
    }
    // Undo back to the single full-Original beat.
    for _ in 0..40 {
        if d.app().beat_rows.len() <= 1 {
            break;
        }
        typed(d, "u")?;
        settle(d)?;
    }
    typed(d, "gg")?;
    settle(d)?;

    // About ten beats: pauses, cuts, a repeat of a short fragment.
    for keys in [
        "240l", ",h", "480l", "s", "k", "rr", "j", "900l", "s", "120l", ",h", "600l", "s",
    ] {
        typed(d, keys)?;
        settle(d)?;
    }
    picture_dominates(d)?;
    d.capture("Real footage · about ten beats")?;

    // The transcript: search a word and follow it.
    d.key(Key::Slash)?;
    d.events(
        "Search the transcript",
        vec![
            egui::Event::Text("the".into()),
            key_event(Key::Enter, egui::Modifiers::NONE, true),
            key_event(Key::Enter, egui::Modifiers::NONE, false),
        ],
    )?;
    settle(d)?;
    d.capture("Real footage · transcript search")?;
    d.key(Key::Escape)?;
    settle(d)?;
    // Select the word at the cursor, as the transcript shows it.
    typed(d, "viw")?;
    settle(d)?;
    d.capture("Real footage · transcript word selected")?;
    d.key(Key::Escape)?;
    settle(d)?;

    d.harness.set_size(egui::vec2(960.0, 640.0));
    for frame in 0..4 {
        d.step(&format!("Real footage minimum viewport {frame}"), false)?;
    }
    settle(d)?;
    picture_dominates(d)?;
    d.capture("Real footage · 960x640")?;
    d.key(Key::J)?;
    settle(d)?;
    d.capture("Real footage · 960x640 next beat")?;
    d.harness.set_size(egui::vec2(1280.0, 820.0));
    for frame in 0..4 {
        d.step(&format!("Real footage default viewport {frame}"), false)?;
    }
    settle(d)?;
    d.capture("Real footage · back to 1280x820")?;

    // The Keys sheet: a compact list, then a search with one row explained.
    for (width, height) in [(1280.0, 820.0), (960.0, 640.0)] {
        d.harness.set_size(egui::vec2(width, height));
        for frame in 0..4 {
            d.step(&format!("Help viewport {width}x{height} {frame}"), false)?;
        }
        d.key(Key::Questionmark)?;
        d.step("Help opens", false)?;
        d.capture(&format!("Real footage · Keys sheet at {width}x{height}"))?;
        d.key(Key::Slash)?;
        d.events(
            "Search Keys for pause",
            vec![egui::Event::Text("pause".into())],
        )?;
        d.step("Search results", false)?;
        d.capture(&format!("Real footage · Keys search at {width}x{height}"))?;
        d.key(Key::Escape)?;
        d.key(Key::Escape)?;
        d.step("Help closes", false)?;
    }
    d.harness.set_size(egui::vec2(1280.0, 820.0));
    d.step("Default viewport", false)
}

fn settle(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for_limit("Real footage settled", Duration::from_secs(60), |app| {
        !app.service.is_busy()
            && !app.repeat_queue.active()
            && !app.presentation.loading()
            && !app.presentation.needs_render()
            && !app.importing()
            && !app.macros.is_pending()
            && app.thumbnails.settled()
    })?;
    d.step("Real footage settled frame", false)
}

/// Printable keys with their text companions, as macOS delivers them.
fn typed(d: &mut Driver<'_>, keys: &str) -> Result<(), String> {
    let mut events = Vec::new();
    for character in keys.chars() {
        let (key, modifiers) = match character {
            ',' => (Key::Comma, egui::Modifiers::NONE),
            '@' => (Key::Num2, egui::Modifiers::SHIFT),
            _ => (
                Key::from_name(&character.to_ascii_uppercase().to_string())
                    .ok_or_else(|| format!("No key for {character}"))?,
                if character.is_ascii_uppercase() {
                    egui::Modifiers::SHIFT
                } else {
                    egui::Modifiers::NONE
                },
            ),
        };
        events.push(key_event(key, modifiers, true));
        events.push(egui::Event::Text(character.to_string()));
        events.push(key_event(key, modifiers, false));
    }
    d.events(&format!("Keys {keys}"), events)
}

/// The settled picture is displayed and is the largest thing on screen.
fn picture_dominates(d: &mut Driver<'_>) -> Result<(), String> {
    let app = d.app();
    let displayed = app.presentation.has_displayed() && app.target.is_some();
    let size = d.harness.ctx.content_rect().size();
    let layout = style::Layout::for_size(size.x, size.y);
    let viewer = app.feedback.viewer_picture;
    let area = viewer.map_or(0.0, |rect| rect.area());
    let sidebar = layout.sources.max(layout.inspector) * size.y;
    d.check(
        "Settled picture is displayed and larger than either side pane",
        displayed && area > sidebar,
        json!({"displayed":true,"picture_area_above":sidebar}),
        json!({"displayed":displayed,"picture":viewer.map(|rect| [rect.width(), rect.height()]),"picture_area":area}),
    )
}
