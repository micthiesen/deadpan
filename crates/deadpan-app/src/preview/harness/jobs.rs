//! The Jobs panel and the job coordinator through production input.
//!
//! `,a` starts a real AI pause job (only the model worker is the scripted
//! seam: it reports steps, then waits for cancellation). A second job, a
//! scripted Transcription registration the replay holds, waits for the one
//! AI model slot. `:jobs` lists both with their states, X cancels the
//! generation through the panel, and the queued job is admitted. A copy of
//! the package taken while the attempt was live is what a crash leaves; the
//! panel lists that interrupted attempt after reopening, R retries it as a
//! new variant, and D discards another copy's entry durably.

use egui::{Key, Modifiers};

use super::*;
use crate::dialogs::{DialogKind, Dialogs};
use crate::jobs::{JobKind, JobSpec, RowState};
use crate::project::generation::{Backend, Outcome, Script, ScriptEnding, ScriptQueue};

/// Every start reports four steps, then waits for cancellation.
pub(super) fn backend() -> Backend {
    Backend::Scripted(Arc::new(ScriptQueue::new([Script {
        unavailable: None,
        steps: 4,
        step_interval: std::time::Duration::from_millis(20),
        ending: ScriptEnding::WaitForCancel,
    }])))
}

fn widget_text(d: &Driver<'_>) -> String {
    d.widgets().to_string()
}

fn session(d: &Driver<'_>) -> Option<u64> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.session)
}

fn generating(app: &DeadpanApp) -> bool {
    app.ai
        .job()
        .is_some_and(|job| job.running() && job.phase.steps() == Some((4, 4)))
}

fn copy_package(from: &std::path::Path, to: &std::path::Path) -> Result<(), String> {
    let _ = std::fs::remove_dir_all(to);
    crate::jobs::crash_copy(from, to)
}

/// Open `path` with ⌘O through the scripted picker and answer its report.
fn reopen(d: &mut Driver<'_>, path: &std::path::Path) -> Result<(), String> {
    d.app_mut().dialogs = Dialogs::scripted(vec![(DialogKind::OpenProject, Some(path.into()))]);
    d.key_modified(Key::O, Modifiers::COMMAND)?;
    let expected = path.to_path_buf();
    d.wait_for("Crash copy opened with its recovery report", move |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.path == expected)
            && app.recovery.showing_report()
            && !app.service.is_busy()
    })?;
    d.key(Key::Escape)?;
    // The report returns focus to the editor on the following frame.
    d.step("Recovery report answered", false)?;
    d.step("Editor focus restored", false)?;
    d.settled()
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push(
        "The model worker is the scripted test seam and the queued Transcription is a registration the replay holds; admission, the panel, cancellation, durable attempt records and reopening are real. The crash is a package copy taken while the attempt was live.".into(),
    );
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    let before = d.revision();
    d.chord(&[Key::Comma, Key::H])?;
    d.changed(&before)?;
    d.settled()?;
    d.chord(&[Key::Comma, Key::A])?;
    d.wait_for("Scripted generation reaches its last step", generating)?;
    let session = session(d);
    let queued = d
        .app()
        .service
        .jobs()
        .register(JobSpec::new(JobKind::Transcription, session).detail("scripted replay job"));
    let path = d.app().workspace.as_ref().ok_or("No project")?.path.clone();
    let parent = path.parent().ok_or("Replay project has no parent")?;
    let (retry_copy, discard_copy) = (
        parent.join("jobs-crash-retry.deadpan"),
        parent.join("jobs-crash-discard.deadpan"),
    );
    copy_package(&path, &retry_copy)?;
    copy_package(&path, &discard_copy)?;

    d.command("jobs")?;
    d.step("Jobs panel", true)?;
    let widgets = widget_text(d);
    d.check(
        ":jobs lists the running generation and the job queued behind it",
        d.app().jobs.open
            && widgets.contains("AI pause pictures · Pause")
            && widgets.contains(": Running · Generating pictures 100%")
            && widgets.contains(
                "Transcription · scripted replay job: Queued #1, waiting for AI pause pictures",
            ),
        json!({"running":"AI pause pictures · Pause: Running","queued":"Transcription: Queued #1, waiting for AI pause pictures"}),
        json!({"open":d.app().jobs.open,"rows":format!("{:?}", d.app().job_rows())}),
    )?;
    d.capture("Running and queued jobs")?;
    d.check(
        "Editor keys do not reach the project while the panel is open",
        !d.app().service.is_busy(),
        json!(false),
        json!(d.app().service.is_busy()),
    )?;

    // X on the first row (the running generation) cancels it.
    d.key(Key::X)?;
    d.wait_for("Generation cancelled from the panel", |app| {
        app.ai
            .job()
            .is_some_and(|job| job.outcome == Some(Outcome::Cancelled))
    })?;
    d.wait_for("The queued job is admitted", |app| {
        app.service
            .jobs()
            .row_of(JobKind::Transcription, session)
            .is_some_and(|row| row.state == RowState::Running)
    })?;
    d.step("After cancel", true)?;
    let widgets = widget_text(d);
    d.check(
        "Cancelling the model job admits the queued one, and the panel says so",
        queued.admitted() && widgets.contains("Transcription · scripted replay job: Running"),
        json!({"admitted":true}),
        json!({"admitted":queued.admitted(),"rows":format!("{:?}", d.app().job_rows())}),
    )?;
    d.capture("Queued job admitted after cancel")?;
    drop(queued);
    d.key(Key::Escape)?;
    d.settled()?;
    d.check(
        "Escape closes the panel",
        !d.app().jobs.open,
        json!(false),
        json!(d.app().jobs.open),
    )?;

    // A crash copy: the attempt was live, so reopening interrupts it.
    reopen(d, &retry_copy)?;
    d.command("jobs")?;
    d.step("Interrupted attempt listed", true)?;
    let widgets = widget_text(d);
    d.check(
        "After reopening, the interrupted attempt is offered with Retry and Discard",
        widgets.contains("INTERRUPTED")
            && widgets.contains("stopped when Deadpan last closed. Retry generates a new variant")
            && widgets.contains("Nothing is running in the background."),
        json!("AI pause pictures for “Pause” stopped when Deadpan last closed. Retry…"),
        json!({"rows":format!("{:?}", d.app().job_rows())}),
    )?;
    d.capture("Interrupted attempt after reopening")?;
    d.key(Key::R)?;
    d.wait_for("Retry generates again", generating)?;
    d.wait_for("The retried attempt leaves the list", |app| {
        app.ai_interrupted().is_empty()
    })?;
    d.step("Retry running", true)?;
    d.check(
        "R retries the pause as a new attempt of the same request",
        d.app()
            .ai
            .job()
            .is_some_and(|job| job.running() && job.request.is_some())
            && widget_text(d).contains("AI pause pictures · Pause"),
        json!({"running":true,"interrupted":0}),
        json!({"job":format!("{:?}", d.app().ai.job()),"rows":format!("{:?}", d.app().job_rows())}),
    )?;
    d.capture("Retried generation running")?;
    // The chosen (retried) row has gone: X refuses rather than cancel the
    // job that moved into its place, and K chooses the running job.
    d.key(Key::X)?;
    d.step("X on a finished row", false)?;
    d.check(
        "An action on a row that has gone refuses instead of acting on another",
        d.app().ai.job().is_some_and(|job| job.running())
            && d.app()
                .jobs
                .status
                .as_deref()
                .is_some_and(|status| status.contains("already finished")),
        json!({"running":true,"status":"The chosen row has already finished…"}),
        json!({"job":format!("{:?}", d.app().ai.job()),"status":d.app().jobs.status}),
    )?;
    d.key(Key::K)?;
    d.key(Key::X)?;
    d.wait_for("Retry cancelled", |app| {
        app.ai
            .job()
            .is_some_and(|job| job.outcome == Some(Outcome::Cancelled))
    })?;
    d.key(Key::Escape)?;
    d.settled()?;

    // Another crash copy: Discard removes the entry for good.
    reopen(d, &discard_copy)?;
    d.command("jobs")?;
    d.wait_for("Interrupted attempt listed again", |app| {
        !app.ai_interrupted().is_empty()
    })?;
    d.key(Key::D)?;
    d.wait_for("Discarded", |app| app.ai_interrupted().is_empty())?;
    d.step("Discarded", true)?;
    let still =
        deadpan_store::ProjectStore::open(&discard_copy, deadpan_store::AccessMode::ReadOnly)
            .and_then(|store| store.interrupted_generation_attempts())
            .map(|listed| listed.attempts)
            .map_err(|error| error.to_string())?;
    d.check(
        "D discards the interrupted attempt durably; the pause is unchanged",
        still.is_empty() && widget_text(d).contains("Nothing is running in the background."),
        json!({"interrupted":0}),
        json!({"interrupted":still.len()}),
    )?;
    d.capture("Interrupted attempt discarded")?;
    d.key(Key::Escape)?;
    d.settled()
}
