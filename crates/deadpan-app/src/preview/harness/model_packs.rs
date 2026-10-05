//! The Models panel through real keys: `:models`, the AI PICTURES offer,
//! license reading and acceptance, install, cancel, resume, an offline
//! import failure and removal.
//!
//! Only the installer is scripted (`model_packs::Backend::Scripted`): it
//! never downloads. Progress, cancellation and failure are deterministic; a
//! cancelled run leaves sparse partial files and a completed run writes a
//! sparse installed copy with a receipt, both in the replay's private models
//! root, so the real store reports Partial and Installed.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use deadpan_models::packs::{PackError, PackState};
use egui::Key;

use super::*;
use crate::model_packs::scripted::{Finish, Run, Script};
use crate::model_packs::{Backend, Ending, pack_failure};

const BRIDGE: &str = "ltx-2.3-q4-bridge";
const WHISPER: &str = "whisper-base-en";
const LTX: &str = "LTX-2 Community License Agreement";
const GEMMA: &str = "Gemma Terms of Use";
const OFFER: &str = "Install AI models…  :models";

fn import_failure() -> String {
    pack_failure(&PackError::ImportIncomplete {
        missing: 2,
        example: "ggml-base.en.bin".into(),
    })
}

/// A download that waits for Cancel, its completed resume, then an offline
/// import that fails.
pub(super) fn backend() -> Backend {
    let run = |steps, finish| Run {
        steps,
        interval: Duration::from_millis(40),
        finish,
    };
    Backend::Scripted(Arc::new(Script::new([
        run(6, Finish::WaitForCancel),
        run(6, Finish::Complete),
        run(3, Finish::Fail(import_failure())),
    ])))
}

fn focused(d: &Driver<'_>) -> Option<String> {
    d.widgets().as_array()?.iter().find_map(|widget| {
        (widget["focused"] == true)
            .then(|| widget["label"].as_str().map(str::to_owned))
            .flatten()
    })
}

fn widget(d: &Driver<'_>, label: &str) -> Option<Value> {
    d.widgets()
        .as_array()?
        .iter()
        .find(|widget| widget["label"].as_str() == Some(label))
        .cloned()
}

/// Tab until a control whose label starts with `prefix` has focus.
fn tab_to(d: &mut Driver<'_>, prefix: &str) -> Result<(), String> {
    for _ in 0..40 {
        if focused(d).is_some_and(|label| label.starts_with(prefix)) {
            // The reveal scroll paints on the following frame.
            return d.step(&format!("{prefix} focused"), false);
        }
        d.key(Key::Tab)?;
    }
    Err(format!(
        "Tab never focused {prefix:?}; focused {:?}",
        focused(d)
    ))
}

fn visible(d: &Driver<'_>, needle: &str) -> bool {
    let paints = scenarios::text_paint_visibility(d, needle);
    !paints.is_empty() && paints.iter().any(|paint| paint["fully_visible"] == true)
}

fn state(d: &Driver<'_>, pack: &str) -> String {
    match d.app().models.manager.state(pack) {
        Some(Ok(PackState::Installed(_))) => "installed".into(),
        Some(Ok(PackState::Partial { bytes })) => format!("partial {bytes}"),
        Some(Ok(PackState::Absent)) => "absent".into(),
        Some(Err(error)) => format!("error {error}"),
        None => "unknown".into(),
    }
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push(
        "The installer is the scripted test seam; real downloads, hashing, smoke tests and activation are covered by deadpan-models and deadpan-cli tests.".into(),
    );
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    let before = d.revision();
    d.chord(&[Key::Comma, Key::H])?;
    d.changed(&before)?;
    d.settled()?;
    let paused = d.revision();
    d.check(
        "Without the AI pack a selected pause offers it with its size and licenses",
        d.app().ai_hold().is_some()
            && widget(d, OFFER).is_some()
            && visible(d, "36.15 GB download")
            && visible(d, "two licenses to accept")
            && state(d, BRIDGE) == "absent",
        json!({"offer":OFFER,"size":"36.15 GB download","state":"absent"}),
        json!({"offer":widget(d, OFFER),"state":state(d, BRIDGE)}),
    )?;
    d.capture("AI PICTURES offers the model pack")?;

    d.click(OFFER)?;
    d.step("Models opened from the offer", true)?;
    d.check(
        "The offer opens Models focused on the AI pack's first license acceptance",
        d.app().models.open
            && d.app().models.focus.as_deref() == Some(BRIDGE)
            && focused(d).as_deref() == Some(&format!("I accept the {LTX}")),
        json!({"open":true,"focus":BRIDGE,"focused":format!("I accept the {LTX}")}),
        json!({"open":d.app().models.open,"focus":d.app().models.focus,"focused":focused(d)}),
    )?;
    d.capture("Models focused on the AI pack")?;
    d.key(Key::Escape)?;
    d.step("Models closed", false)?;
    d.check(
        "Escape closes Models without accepting, installing or editing",
        !d.app().models.open
            && d.app().models.manager.job().is_none()
            && d.app().models.accepted.get(BRIDGE).is_none_or(BTreeSet::is_empty)
            && state(d, BRIDGE) == "absent"
            && d.revision() == paused,
        json!({"open":false,"job":null,"accepted":[],"revision":paused}),
        json!({"open":d.app().models.open,"accepted":d.app().models.accepted.get(BRIDGE),"revision":d.revision()}),
    )?;

    d.command("models")?;
    d.step("Models panel", false)?;
    let install = "Install · 36.15 GB";
    d.check(
        ":models lists each pack with size, state, memory, terms and free space",
        d.app().models.open
            && visible(d, "MODELS")
            && visible(d, "English transcription and pause detection")
            && visible(d, "Not installed · 149 MB download")
            && visible(d, " free")
            && !scenarios::text_paint_visibility(d, "AI pauses (LTX-2.3").is_empty()
            && !scenarios::text_paint_visibility(d, "Free for individuals").is_empty()
            && !scenarios::text_paint_visibility(d, "Not installed · 36.15 GB download").is_empty(),
        json!({"open":true,"packs":2}),
        json!({"open":d.app().models.open,"widgets":d.widgets()}),
    )?;
    d.check(
        "Install of the AI pack is disabled until both licenses are accepted",
        widget(d, install).is_some_and(|button| button["disabled"] == true)
            && widget(d, "Install · 149 MB").is_some_and(|button| button["disabled"] == false),
        json!({"ai_install_disabled":true,"transcription_install_enabled":true}),
        json!({"ai":widget(d, install),"transcription":widget(d, "Install · 149 MB")}),
    )?;
    d.capture("Models panel")?;

    tab_to(d, &format!("Read the full {LTX}"))?;
    d.key(Key::Space)?;
    // The text appears on the next frame and its reveal scroll paints after.
    for frame in 0..3 {
        d.step(&format!("License text shown {frame}"), false)?;
    }
    d.check(
        "The compiled license text is readable in the panel",
        !scenarios::text_paint_visibility(d, "By using or distributing any portion").is_empty()
            && focused(d).is_some_and(|label| label == format!("Hide the full {LTX}")),
        json!("license text painted; toggle keeps focus"),
        json!({"focused":focused(d)}),
    )?;
    d.capture("Full LTX-2 license text")?;
    d.key(Key::Space)?;

    tab_to(d, &format!("I accept the {LTX}"))?;
    d.key(Key::Space)?;
    d.step("First license accepted", false)?;
    // The Gemma control sits right above the Install row's reason.
    tab_to(d, &format!("I accept the {GEMMA}"))?;
    d.check(
        "One accepted license still keeps Install disabled and names the other",
        widget(d, install).is_some_and(|button| button["disabled"] == true)
            && visible(d, "Accept the Gemma Terms of Use to install."),
        json!({"disabled":true}),
        json!({"install":widget(d, install)}),
    )?;
    d.capture("One license accepted")?;
    d.key(Key::Space)?;
    d.step("Both licenses accepted", false)?;
    d.check(
        "Accepting both licenses enables Install without starting anything",
        widget(d, install).is_some_and(|button| button["disabled"] == false)
            && d.app().models.manager.job().is_none(),
        json!({"disabled":false,"job":null}),
        json!({"install":widget(d, install)}),
    )?;
    d.capture("Both licenses accepted")?;

    tab_to(d, install)?;
    d.key(Key::Enter)?;
    d.wait_for("Download progress", |app| {
        app.models
            .manager
            .job()
            .is_some_and(|job| job.progress.completed_bytes > 0)
    })?;
    d.step("Progress shown", false)?;
    d.check(
        "Install shows the transfer phase and bytes",
        !scenarios::text_paint_visibility(d, "Downloading and verifying ·").is_empty()
            && widget(d, "Cancel install").is_some(),
        json!("Downloading and verifying · n / 36.15 GB"),
        json!({"job":format!("{:?}", d.app().models.manager.job())}),
    )?;
    d.capture("Downloading")?;

    tab_to(d, "Cancel install")?;
    d.key(Key::Space)?;
    d.wait_for("Install cancelled", |app| {
        app.models.manager.job().is_none()
            && app
                .models
                .manager
                .outcome()
                .is_some_and(|outcome| outcome.ending == Ending::Cancelled)
    })?;
    d.step("Cancelled", false)?;
    let resume = d.widgets().as_array().and_then(|widgets| {
        widgets
            .iter()
            .find(|widget| {
                widget["label"]
                    .as_str()
                    .is_some_and(|label| label.starts_with("Resume · "))
            })
            .cloned()
    });
    d.check(
        "Cancel keeps the partial bytes and offers Resume with the remaining size",
        state(d, BRIDGE).starts_with("partial")
            && resume.is_some()
            && !scenarios::text_paint_visibility(d, "kept for Resume").is_empty(),
        json!({"state":"partial","resume":"Resume · n GB left"}),
        json!({"state":state(d, BRIDGE),"resume":resume}),
    )?;
    d.capture("Cancelled with Resume")?;

    tab_to(d, "Resume · ")?;
    d.key(Key::Space)?;
    d.wait_for("Install finished", |app| {
        app.models.manager.job().is_none()
            && app
                .models
                .manager
                .outcome()
                .is_some_and(|outcome| outcome.ending == Ending::Installed)
    })?;
    d.step("Installed", false)?;
    d.check(
        "Resume completes; the store accepts the installed pack",
        state(d, BRIDGE) == "installed"
            && !scenarios::text_paint_visibility(d, "Installed and tested.").is_empty()
            && widget(d, "Remove · frees 36.15 GB").is_some(),
        json!({"state":"installed"}),
        json!({"state":state(d, BRIDGE)}),
    )?;
    d.capture("Installed")?;

    d.key(Key::Escape)?;
    d.step("Back to the pause", false)?;
    d.check(
        "With the pack installed AI PICTURES no longer offers it; the edit is unchanged",
        !d.app().models.open
            && widget(d, OFFER).is_none()
            && d.widgets().to_string().contains("Generate AI pictures")
            && d.revision() == paused,
        json!({"offer":null,"generate":true,"revision":paused}),
        json!({"offer":widget(d, OFFER),"revision":d.revision()}),
    )?;
    d.capture("AI PICTURES after install")?;

    d.command("models")?;
    d.step("Models reopened", false)?;
    tab_to(d, "Install from folder…")?;
    d.key(Key::Space)?;
    d.wait_for("Offline import failed", |app| {
        app.models.manager.job().is_none()
            && app
                .models
                .manager
                .outcome()
                .is_some_and(|outcome| matches!(outcome.ending, Ending::Failed(_)))
    })?;
    d.step("Import failure shown", false)?;
    let failure = format!("Install failed: {}", import_failure());
    d.check(
        "An offline import failure is shown with its exact reason",
        !scenarios::text_paint_visibility(d, &failure).is_empty()
            && d.app()
                .models
                .manager
                .outcome()
                .is_some_and(|outcome| outcome.pack_id == WHISPER),
        json!(failure),
        json!({"outcome":format!("{:?}", d.app().models.manager.outcome())}),
    )?;
    d.capture("Offline import failure")?;

    tab_to(d, "Remove · frees 36.15 GB")?;
    d.key(Key::Space)?;
    d.wait_for("Pack removed", |app| {
        app.models.manager.job().is_none()
            && app
                .models
                .manager
                .outcome()
                .is_some_and(|outcome| outcome.ending == Ending::Removed)
    })?;
    d.step("Removed", false)?;
    d.check(
        "Remove returns the AI pack to Install",
        state(d, BRIDGE) == "absent" && widget(d, install).is_some(),
        json!({"state":"absent"}),
        json!({"state":state(d, BRIDGE)}),
    )?;
    d.key(Key::Escape)?;
    d.step("Closed", false)?;
    d.check(
        "Models never edits the project",
        !d.app().models.open && d.revision() == paused,
        json!({"revision":paused}),
        json!({"revision":d.revision()}),
    )
}
