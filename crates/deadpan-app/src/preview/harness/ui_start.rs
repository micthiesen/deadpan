//! First run on real footage: the start screen with recent projects, a new
//! project from a long video while its Original is prepared, and the moment
//! it becomes editable, at 1280×820 and 960×640.
//!
//! `DEADPAN_UI_REAL_VIDEO` names the video. Without it the scenario only
//! reports a skip.

use super::*;
use egui::Key;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let Some(video) = std::env::var_os("DEADPAN_UI_REAL_VIDEO").map(PathBuf::from) else {
        d.report
            .skipped
            .push("ui-start: set DEADPAN_UI_REAL_VIDEO to a real-footage video.".into());
        return Ok(());
    };
    // Close the fixture project: the start screen lists it as recent.
    d.command("close")?;
    d.wait_for("Project closed", |app| {
        app.workspace.is_none() && !app.service.is_busy()
    })?;
    for frame in 0..3 {
        d.step(&format!("Start screen settles {frame}"), false)?;
    }
    d.capture("Start screen at 1280x820")?;
    resize(d, 960.0, 640.0)?;
    d.capture("Start screen at 960x640")?;
    resize(d, 1280.0, 820.0)?;

    // ⌘N chooses the real video; preparation then runs in the background.
    d.app_mut().dialogs = Dialogs::scripted(vec![(DialogKind::CreateProject, Some(video))]);
    d.key_modified(Key::N, egui::Modifiers::COMMAND)?;
    d.wait_for("Preparation starts", |app| app.preparing().is_some())?;
    let started = Instant::now();
    let mut shots = [3u64, 20, 60].into_iter().peekable();
    let mut small = false;
    while d.app().preparing().is_some() {
        if started.elapsed() > Duration::from_secs(900) {
            return Err("Preparation did not finish within 15 minutes".into());
        }
        if let Some(&at) = shots.peek()
            && started.elapsed().as_secs() >= at
        {
            shots.next();
            if small {
                resize(d, 960.0, 640.0)?;
            }
            d.capture(&format!(
                "Preparing the Original after {at} s at {}",
                if small { "960x640" } else { "1280x820" }
            ))?;
            if small {
                resize(d, 1280.0, 820.0)?;
            }
            small = !small;
            // The editor stays responsive: Help opens and closes meanwhile.
            d.key(Key::Questionmark)?;
            d.step("Help during preparation", false)?;
            d.key(Key::Escape)?;
        }
        d.step("Preparing", false)?;
        std::thread::sleep(Duration::from_millis(200));
    }
    d.wait_for_limit("Original ready", Duration::from_secs(120), |app| {
        !app.service.is_busy()
            && !app.presentation.loading()
            && app.presentation.has_displayed()
            && app.thumbnails.settled()
    })?;
    d.capture("Editable at 1280x820")?;
    resize(d, 960.0, 640.0)?;
    d.capture("Editable at 960x640")?;
    resize(d, 1280.0, 820.0)
}

fn resize(d: &mut Driver<'_>, width: f32, height: f32) -> Result<(), String> {
    d.harness.set_size(egui::vec2(width, height));
    for frame in 0..4 {
        d.step(&format!("Resize to {width}x{height} {frame}"), false)?;
    }
    Ok(())
}
