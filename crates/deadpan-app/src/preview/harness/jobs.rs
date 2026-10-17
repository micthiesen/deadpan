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
//!
//! Then, on battery power (an injected observation; replays never read the
//! Mac's power state), `,a` and `:generate` on a second pause leave two AI
//! jobs waiting with the reason and Run anyway. O starts the first, the second then waits its
//! turn for the model, and X cancels only it. A window close asks what to
//! do with the running generation: Quit when they finish arms an automatic
//! quit with a banner, and a second close, Go back, stays open.

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
    d.settled()?;
    conditions_and_quit(d)
}

fn ai_rows(d: &Driver<'_>) -> Vec<crate::jobs::JobRow> {
    d.app()
        .service
        .jobs()
        .snapshot()
        .into_iter()
        .filter(|row| row.kind == JobKind::AiPause)
        .collect()
}

fn focused(d: &Driver<'_>) -> Option<String> {
    d.harness.root().children_recursive().find_map(|node| {
        let access = node.accesskit_node();
        access.is_focused().then(|| access.label()).flatten()
    })
}

fn request_close(d: &mut Driver<'_>) -> Result<(), String> {
    d.harness
        .input_mut()
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing replay viewport")?
        .events
        .push(egui::ViewportEvent::Close);
    d.step("Window close requested", true)?;
    d.step("Quit question focused", true)
}

/// Battery power holds both generations visibly; O runs one anyway, the
/// other waits its turn and X cancels only it; closing asks about the
/// running one.
fn conditions_and_quit(d: &mut Driver<'_>) -> Result<(), String> {
    d.app()
        .service
        .jobs()
        .set_conditions(crate::jobs::Conditions {
            power: Some("running on battery".into()),
            memory: crate::jobs::MemoryPressure::Normal,
        });
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    d.chord(&[Key::Comma, Key::A])?;
    d.wait_for("The first generation waits on battery", |app| {
        app.service
            .jobs()
            .snapshot()
            .iter()
            .any(|row| row.state == RowState::Held("running on battery".into()))
    })?;
    d.chord(&[Key::G, Key::G, Key::Num4, Key::Num0, Key::L])?;
    let before = d.revision();
    d.chord(&[Key::Comma, Key::H])?;
    d.changed(&before)?;
    d.settled()?;
    // `:generate` on that plain pause starts a second job at once.
    d.command("generate")?;
    d.wait_for("A second pause queues beside the first", |app| {
        app.service
            .jobs()
            .snapshot()
            .iter()
            .filter(|row| row.kind == JobKind::AiPause)
            .count()
            == 2
    })?;
    d.wait_for("Both generations reach battery admission", |app| {
        let rows: Vec<_> = app
            .service
            .jobs()
            .snapshot()
            .into_iter()
            .filter(|row| row.kind == JobKind::AiPause)
            .collect();
        rows.len() == 2
            && rows
                .iter()
                .all(|row| row.state == RowState::Held("running on battery".into()))
    })?;
    d.command("jobs")?;
    d.step("Jobs panel on battery", true)?;
    let widgets = widget_text(d);
    d.check(
        "On battery both AI jobs wait at admission, say why and offer Run anyway",
        ai_rows(d)
            .iter()
            .all(|row| row.state == RowState::Held("running on battery".into()))
            && widgets.contains(": Waiting while running on battery")
            && widgets.contains("O runs it anyway")
            && widgets.contains("Run anyway"),
        json!({"rows":"Waiting while running on battery · O runs it anyway","button":"Run anyway"}),
        json!({"rows":format!("{:?}", ai_rows(d))}),
    )?;
    d.capture("AI jobs waiting on battery")?;

    // O on the first row starts it despite the battery; the second then
    // waits for the model it holds.
    d.key(Key::O)?;
    d.wait_for("The first generation runs", generating)?;
    d.wait_for("The second waits for the model", |app| {
        app.service.jobs().snapshot().iter().any(|row| {
            row.kind == JobKind::AiPause
                && row.state
                    == RowState::Queued {
                        position: 1,
                        waiting_for: "AI pause pictures".into(),
                    }
        })
    })?;
    d.step("Run anyway", true)?;
    let widgets = widget_text(d);
    d.check(
        "O runs the chosen job anyway; the other waits its turn for the one model",
        widgets.contains(": Running · Generating pictures 100%")
            && widgets.contains(": Queued #1, waiting for AI pause pictures"),
        json!({"first":"Running","second":"Queued #1, waiting for AI pause pictures"}),
        json!({"rows":format!("{:?}", ai_rows(d)),"status":d.app().jobs.status}),
    )?;
    d.capture("Run anyway; second generation queued")?;

    // J chooses the queued one; X cancels exactly it.
    d.key(Key::J)?;
    d.key(Key::X)?;
    d.wait_for("Only the queued generation is cancelled", |app| {
        app.service
            .jobs()
            .snapshot()
            .iter()
            .filter(|row| row.kind == JobKind::AiPause)
            .count()
            == 1
    })?;
    d.step("Queued generation cancelled", true)?;
    d.check(
        "X cancels only the chosen queued generation; the running one continues",
        ai_rows(d).iter().all(|row| row.state == RowState::Running) && generating(d.app()),
        json!({"running":1}),
        json!({"rows":format!("{:?}", ai_rows(d))}),
    )?;
    d.key(Key::Escape)?;
    d.settled()?;

    // Closing with the generation running asks first.
    request_close(d)?;
    let widgets = widget_text(d);
    d.check(
        "Closing while a generation runs asks: quit when finished, cancel and quit, or go back",
        d.app().jobs.quit.open
            && !d.app().close_pending
            && widgets.contains("Quit while jobs run?")
            && widgets.contains("AI pause pictures")
            && widgets.contains("Cancel them and quit")
            && focused(d).is_some_and(|label| label.starts_with("Quit when they finish")),
        json!({"open":true,"closing":false,"focused":"Quit when they finish…"}),
        json!({"open":d.app().jobs.quit.open,"closing":d.app().close_pending,"focused":focused(d)}),
    )?;
    d.capture("Quit while jobs run")?;
    d.key(Key::Enter)?;
    d.step("Quit when finished", true)?;
    d.check(
        "Quit when they finish keeps the window open with a banner while the job runs",
        d.app().quit_waiting()
            && !d.app().close_pending
            && generating(d.app())
            && widget_text(d).contains("Quitting when 1 job finishes"),
        json!({"waiting":true,"closing":false}),
        json!({"waiting":d.app().quit_waiting(),"closing":d.app().close_pending}),
    )?;
    d.capture("Waiting to quit")?;
    request_close(d)?;
    d.key(Key::Escape)?;
    d.step("Went back", true)?;
    d.check(
        "Asking again and going back stays open and stops waiting; the job keeps running",
        !d.app().quit_waiting()
            && !d.app().jobs.quit.open
            && !d.app().close_pending
            && generating(d.app()),
        json!({"waiting":false,"closing":false,"running":true}),
        json!({"waiting":d.app().quit_waiting(),"closing":d.app().close_pending,"running":generating(d.app())}),
    )?;
    d.app()
        .service
        .jobs()
        .set_conditions(crate::jobs::Conditions::default());
    d.command("cancel-ai")?;
    d.wait_for("Generation cancelled", |app| {
        app.ai
            .job()
            .is_some_and(|job| job.outcome == Some(Outcome::Cancelled))
    })?;
    d.settled()
}
