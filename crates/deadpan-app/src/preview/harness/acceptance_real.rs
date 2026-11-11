//! Acceptance session on real footage (brief item F2), through the real UI.
//!
//! `DEADPAN_ACCEPTANCE_PROJECT` names a disposable copy of a single-Original
//! project made from a real video of at least ten minutes, already
//! transcribed. `DEADPAN_ACCEPTANCE_STING` names a short WAV to place as the
//! sound effect, and `DEADPAN_ACCEPTANCE_RENDER=1` also renders the whole
//! edit (slow on a long video; the headless session renders and verifies).
//!
//! Steps use key sequences and committed document state, never footer or
//! inspector text. A step that fails is recorded as a failed check and the
//! session continues with the next step: what breaks on real footage is the
//! result. The matching headless run is `tools/acceptance/session.py`.

use super::*;
use deadpan_core::{NodeKind, ProjectDocument};
use egui::{Key, Modifiers};

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let Some(path) = std::env::var_os("DEADPAN_ACCEPTANCE_PROJECT").map(PathBuf::from) else {
        d.report.skipped.push(
            "acceptance-real: set DEADPAN_ACCEPTANCE_PROJECT to a disposable copy of a transcribed real-footage project."
                .into(),
        );
        return Ok(());
    };
    d.app_mut().dialogs = Dialogs::scripted(vec![(DialogKind::OpenProject, Some(path.clone()))]);
    d.key_modified(Key::O, Modifiers::COMMAND)?;
    d.wait_for_limit(
        "Open the real-footage project",
        Duration::from_secs(180),
        |app| {
            app.workspace
                .as_ref()
                .is_some_and(|workspace| workspace.path.file_name() == path.file_name())
                && !app.service.is_busy()
        },
    )?;
    settle(d)?;
    d.capture("Acceptance · opened")?;
    d.command("sequence")?;
    settle(d)?;
    let start_frames = frames(d);
    d.check(
        "The whole video is on the timeline of the opened project",
        start_frames >= 10 * 60 * 24,
        json!({"at_least_frames": 10 * 60 * 24}),
        json!({"frames": start_frames}),
    )?;

    attempt(
        d,
        "search a word and repeat it with gain and zoom steps",
        repeat_word,
    );
    attempt(d, "insert a pause", pause);
    attempt(d, "creep on the pause", creep);
    attempt(d, "cut away to another moment", cutaway);
    attempt(d, "change speed", speed);
    attempt(d, "reverse a passage", reverse);
    attempt(d, "place a sound effect", sound);
    attempt(d, "play", play);
    if std::env::var_os("DEADPAN_ACCEPTANCE_RENDER").is_some_and(|value| value == "1") {
        attempt(d, "render", render);
    } else {
        d.report
            .skipped
            .push("Render not requested (DEADPAN_ACCEPTANCE_RENDER=1); the headless session renders and verifies.".into());
    }
    d.capture("Acceptance · end")?;
    if d.report.checks.iter().any(|check| !check.passed) {
        return Err("Acceptance session had failing steps; see report checks".into());
    }
    Ok(())
}

/// Run one step; a failure becomes a failed check and the session goes on.
fn attempt(d: &mut Driver<'_>, name: &str, step: fn(&mut Driver<'_>) -> Result<(), String>) {
    let checks_before = d.report.checks.len();
    let outcome = step(d);
    if let Err(message) = outcome {
        let already_recorded = d.report.checks[checks_before..]
            .iter()
            .any(|check| !check.passed);
        if !already_recorded {
            d.report.checks.push(Check {
                name: format!("Step failed: {name}"),
                passed: false,
                expected: json!("step completes"),
                actual: json!(message),
            });
        }
        let _ = d.capture(&format!("Failed step · {name}"));
    }
}

fn settle(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for_limit("Real footage settled", Duration::from_secs(120), |app| {
        !app.service.is_busy()
            && !app.repeat_queue.active()
            && !app.presentation.loading()
            && !app.presentation.needs_render()
            && !app.importing()
    })?;
    d.step("Real footage settled frame", false)
}

fn document(d: &Driver<'_>) -> Result<Arc<ProjectDocument>, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.document.clone())
        .ok_or_else(|| "No open project".into())
}

fn frames(d: &Driver<'_>) -> u64 {
    d.app().sequence_length()
}

fn count_kind(d: &Driver<'_>, kind: fn(&NodeKind) -> bool) -> Result<usize, String> {
    Ok(document(d)?
        .nodes()
        .values()
        .filter(|node| kind(&node.kind))
        .count())
}

fn typed(d: &mut Driver<'_>, keys: &str) -> Result<(), String> {
    let mut events = Vec::new();
    for character in keys.chars() {
        let (key, modifiers) = match character {
            ',' => (Key::Comma, Modifiers::NONE),
            '/' => (Key::Slash, Modifiers::NONE),
            '"' => (Key::Quote, Modifiers::SHIFT),
            ' ' => (Key::Space, Modifiers::NONE),
            digit if digit.is_ascii_digit() => (
                Key::from_name(&digit.to_string()).ok_or("No digit key")?,
                Modifiers::NONE,
            ),
            _ => (
                Key::from_name(&character.to_ascii_uppercase().to_string())
                    .ok_or_else(|| format!("No key for {character}"))?,
                if character.is_ascii_uppercase() {
                    Modifiers::SHIFT
                } else {
                    Modifiers::NONE
                },
            ),
        };
        events.push(key_event(key, modifiers, true));
        events.push(egui::Event::Text(character.to_string()));
        events.push(key_event(key, modifiers, false));
    }
    d.events(&format!("Keys {keys}"), events)
}

/// `/` search text in Your edit (or the Original), then Enter: the cursor
/// lands on the match. The field keeps its old text and focus, so select all
/// first and leave with Escape. Fails when nothing matched.
fn search(d: &mut Driver<'_>, text: &str) -> Result<(), String> {
    let original = d.app().view != View::Sequence;
    let before = if original {
        d.app().source_cursor
    } else {
        d.app().sequence_cursor
    };
    d.key(Key::Slash)?;
    d.key_modified(Key::A, Modifiers::COMMAND)?;
    d.events(
        &format!("Search {text}"),
        vec![
            egui::Event::Text(text.into()),
            key_event(Key::Enter, Modifiers::NONE, true),
            key_event(Key::Enter, Modifiers::NONE, false),
        ],
    )?;
    settle(d)?;
    d.key(Key::Escape)?;
    settle(d)?;
    let after = if original {
        d.app().source_cursor
    } else {
        d.app().sequence_cursor
    };
    d.check(
        &format!("Search \"{text}\" moves the cursor to a match"),
        after != before,
        json!({"cursor_moved_from": before}),
        json!({"cursor": after}),
    )
}

fn edited(
    d: &mut Driver<'_>,
    name: &str,
    before: &str,
    delta_expected: impl Fn(i64) -> bool,
    frames_before: u64,
) -> Result<(), String> {
    let _ = d.wait_for_limit(&format!("{name} saved"), Duration::from_secs(120), |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.document.revision_id().as_str() != before)
    });
    settle(d)?;
    let changed = d.revision() != before;
    let delta = frames(d) as i64 - frames_before as i64;
    d.check(
        &format!("{name} commits one revision with the expected duration change"),
        changed && delta_expected(delta),
        json!({"revision_changed": true}),
        json!({"revision_changed": changed, "frame_delta": delta}),
    )
}

fn wait_revision_after(d: &mut Driver<'_>, before: &str) -> Result<(), String> {
    d.wait_for_limit("split saved", Duration::from_secs(120), |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.document.revision_id().as_str() != before)
    })?;
    settle(d)
}

fn repeat_word(d: &mut Driver<'_>) -> Result<(), String> {
    d.command("sequence")?;
    search(d, "main engine")?;
    d.capture("Search match in Your edit")?;
    let (before, revision) = (frames(d), d.revision());
    // Select the two words, then the escalating repeat: three plays, 3 dB and
    // a zoom step each.
    typed(d, "vee")?;
    settle(d)?;
    typed(d, ",e")?;
    edited(
        d,
        "Escalating repeat of the word",
        &revision,
        |delta| delta > 0,
        before,
    )?;
    let repeats = count_kind(d, |kind| matches!(kind, NodeKind::Repeat { .. }))?;
    d.check(
        "The word is a Repeat in the tree",
        repeats >= 1,
        json!({"repeats_at_least": 1}),
        json!({"repeats": repeats}),
    )?;
    d.capture("Repeated word")
}

fn pause(d: &mut Driver<'_>) -> Result<(), String> {
    d.command("sequence")?;
    search(d, "Celia")?;
    let (before, revision) = (frames(d), d.revision());
    typed(d, "3,h")?;
    edited(d, "A 1.5 s pause", &revision, |delta| delta == 36, before)?;
    let holds = count_kind(d, |kind| matches!(kind, NodeKind::Hold { .. }))?;
    d.check(
        "The pause is a Hold",
        holds >= 1,
        json!({"holds_at_least": 1}),
        json!({"holds": holds}),
    )?;
    d.capture("Pause inserted")
}

fn creep(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    typed(d, ",c")?;
    let _ = d.wait_for_limit("creep saved", Duration::from_secs(120), |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.document.revision_id().as_str() != revision)
    });
    settle(d)?;
    d.check(
        "Creep on the selected pause commits a revision",
        d.revision() != revision,
        json!({"revision_changed": true}),
        json!({"revision": d.revision()}),
    )?;
    d.capture("Creep on the pause")
}

fn cutaway(d: &mut Driver<'_>) -> Result<(), String> {
    // Copy a moment of the Original into register a, then show it over a
    // two-second range of Your edit.
    d.command("source")?;
    settle(d)?;
    search(d, "robot")?;
    typed(d, "v12l")?;
    typed(d, "\"ay")?;
    settle(d)?;
    d.command("sequence")?;
    settle(d)?;
    search(d, "freaky")?;
    let revision = d.revision();
    typed(d, "v24l")?;
    d.command("cutaway register=a")?;
    let _ = d.wait_for_limit("cutaway saved", Duration::from_secs(120), |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.document.revision_id().as_str() != revision)
    });
    settle(d)?;
    d.check(
        "The cutaway commits a revision",
        d.revision() != revision,
        json!({"revision_changed": true}),
        json!({"revision": d.revision()}),
    )?;
    d.capture("Cutaway over the range")
}

fn speed(d: &mut Driver<'_>) -> Result<(), String> {
    d.command("sequence")?;
    search(d, "autumn")?;
    let mut seen = d.revision();
    typed(d, "s")?;
    wait_revision_after(d, &seen)?;
    typed(d, "120l")?;
    seen = d.revision();
    typed(d, "s")?;
    wait_revision_after(d, &seen)?;
    typed(d, "k")?;
    let (before, revision) = (frames(d), d.revision());
    d.command("retime 2 pitch=preserve")?;
    edited(
        d,
        "Speed 2x on a five second beat",
        &revision,
        |delta| delta < 0,
        before,
    )?;
    d.capture("Speed change")
}

fn reverse(d: &mut Driver<'_>) -> Result<(), String> {
    d.command("sequence")?;
    search(d, "nightmares")?;
    let (before, revision) = (frames(d), d.revision());
    d.command("reverse 1s")?;
    edited(
        d,
        "A one second reverse",
        &revision,
        |delta| delta == 24,
        before,
    )?;
    d.capture("Reverse pause")
}

fn sound(d: &mut Driver<'_>) -> Result<(), String> {
    let sting = std::env::var_os("DEADPAN_ACCEPTANCE_STING")
        .map(PathBuf::from)
        .ok_or("DEADPAN_ACCEPTANCE_STING is not set")?;
    d.command("sound-channels stereo")?;
    d.app_mut().dialogs = Dialogs::scripted(vec![(DialogKind::ImportSound, Some(sting))]);
    d.key_modified(Key::I, Modifiers::COMMAND)?;
    d.wait_for_limit("Sound is qualified", Duration::from_secs(120), |app| {
        !app.sound_rows.is_empty() && !app.service.is_busy() && !app.importing()
    })?;
    settle(d)?;
    d.command("sequence")?;
    search(d, "Reminder")?;
    for _ in 0..6 {
        if d.app().pane == Pane::Sources {
            break;
        }
        d.key(Key::Tab)?;
    }
    for _ in 0..4 {
        if d.app().selected_sound.is_some() {
            break;
        }
        typed(d, "j")?;
    }
    let revision = d.revision();
    typed(d, ",s")?;
    let _ = d.wait_for_limit("sound saved", Duration::from_secs(120), |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.document.revision_id().as_str() != revision)
    });
    settle(d)?;
    d.check(
        "The sound effect is placed",
        d.revision() != revision,
        json!({"revision_changed": true}),
        json!({"revision": d.revision()}),
    )?;
    d.capture("Sound placed")
}

/// Offscreen replay simulates the audio device, so Space on the twelve minute
/// edit proves that playback of the real edit is requested and prepares; the
/// heard clock and dropped pictures are measured by `perf playback`
/// (tools/acceptance/README in the session report), not here.
fn play(d: &mut Driver<'_>) -> Result<(), String> {
    d.command("sequence")?;
    typed(d, "gg")?;
    settle(d)?;
    d.key(Key::Space)?;
    d.step("Playback requested", false)?;
    let preparing =
        d.rect("Cancel preparation  Space").is_ok() || d.rect("Pause edit  Space").is_ok();
    d.check(
        "Space starts playback of the twelve minute edit (preparing or playing)",
        preparing,
        json!({"control": "Cancel preparation or Pause shown"}),
        json!({"shown": preparing}),
    )?;
    d.capture("Playback requested")?;
    d.key(Key::Space)?;
    settle(d)
}

fn render(d: &mut Driver<'_>) -> Result<(), String> {
    let output = d.options.output.join("acceptance-render");
    std::fs::create_dir(&output).map_err(|error| error.to_string())?;
    let movie = output
        .canonicalize()
        .map_err(|error| error.to_string())?
        .join("acceptance.mp4");
    d.app_mut().dialogs = Dialogs::scripted(vec![(DialogKind::Render, Some(movie.clone()))]);
    d.key_modified(Key::E, Modifiers::COMMAND)?;
    super::render::wait_terminal(d)?;
    let published = d
        .app()
        .render_job
        .as_ref()
        .and_then(|update| update.workflow.clone())
        .is_some_and(|workflow| {
            workflow.status.outcome
                == Some(deadpan_cli::encoded_render::workflow::WorkflowOutcome::Published)
        });
    d.check(
        "The render publishes",
        published,
        json!("Published"),
        json!(format!("{:?}", movie)),
    )?;
    d.capture("Render finished")
}
