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
    let render =
        d.app().service.jobs().register(
            JobSpec::new(JobKind::Render, session).detail("scripted scheduling boundary"),
        );
    d.wait_for("Inference yields at its scripted safe boundary", |app| {
        app.service
            .jobs()
            .row_of(JobKind::AiPause, session)
            .is_some_and(|row| row.state == RowState::Paused("a render is running".into()))
    })?;
    d.step("Acknowledged inference pause", true)?;
    d.check(
        "The paused model keeps its slot and the Jobs panel explains the yield",
        !queued.admitted() && widget_text(d).contains("Paused while a render is running"),
        json!({"model_slot_retained":true,"paused_for":"render"}),
        json!({"queued_admitted":queued.admitted(),"rows":format!("{:?}", d.app().job_rows())}),
    )?;
    d.capture("Inference paused for render")?;
    drop(render);
    d.wait_for("Inference resumes without a new attempt", |app| {
        app.service
            .jobs()
            .row_of(JobKind::AiPause, session)
            .is_some_and(|row| row.state == RowState::Running)
    })?;
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
            && widgets.contains("were interrupted")
            && widgets.contains("Retry")
            && widgets.contains("Nothing is running in the background."),
        json!("AI pause pictures for “Pause” were interrupted. Retry…"),
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
    sleep_and_retry(d)?;
    conditions_and_quit(d)
}

/// Real service attempts, with only the system notification replaced by an
/// injected lifecycle epoch. No OS sleep or wake is requested by this replay.
fn sleep_and_retry(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push(
        "Sleep/wake is injected through the Jobs lifecycle atomics. This verifies writer interruption, retry and the Jobs UI, not native notification delivery or physical sleep.".into(),
    );
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    d.chord(&[Key::Comma, Key::A])?;
    d.wait_for("Generation running before sleep", generating)?;
    d.chord(&[Key::G, Key::G, Key::Num4, Key::Num0, Key::L])?;
    let before = d.revision();
    d.chord(&[Key::Comma, Key::H])?;
    d.changed(&before)?;
    d.settled()?;
    d.command("generate")?;
    d.wait_for("Second AI attempt queued before sleep", |app| {
        let rows = app.service.jobs().snapshot();
        rows.iter()
            .any(|row| row.kind == JobKind::AiPause && row.state == RowState::Running)
            && rows.iter().any(|row| {
                row.kind == JobKind::AiPause && matches!(row.state, RowState::Queued { .. })
            })
    })?;
    let revision = d.revision();
    let lifecycle = d.app().service.jobs().lifecycle();
    lifecycle.will_sleep();
    lifecycle.did_wake();
    d.wait_for(
        "Running and queued attempts interrupted after sleep",
        |app| {
            app.ai_interrupted().len() == 2
                && !app
                    .service
                    .jobs()
                    .snapshot()
                    .iter()
                    .any(|row| row.kind == JobKind::AiPause)
        },
    )?;
    d.command("jobs")?;
    d.step("Sleep interruptions visible", true)?;
    let widgets = widget_text(d);
    d.check(
        "Sleep interrupts both attempts and wake leaves explicit Retry controls without resuming",
        ai_rows(d).is_empty()
            && d.revision() == revision
            && widgets.contains("INTERRUPTED")
            && widgets.contains("were interrupted")
            && widgets.contains("Retry")
            && widgets.contains("Nothing is running in the background.")
            && !widgets.contains("stopped when Deadpan last closed"),
        json!({"interrupted":2,"running":0,"retry":"R","authored_revision":"unchanged"}),
        json!({"interrupted":d.app().ai_interrupted().len(),"rows":format!("{:?}", ai_rows(d)),"revision":d.revision()}),
    )?;
    d.capture("AI attempts interrupted by sleep with explicit Retry")?;
    let second = d
        .app()
        .ai_interrupted()
        .get(1)
        .cloned()
        .ok_or("Missing second sleep interruption")?;
    d.key(Key::J)?;
    d.settled()?;
    d.step("Second interrupted row scrolled into view", true)?;
    let text = crate::preview::jobs::interrupted_text(&second);
    let subject = format!(
        "interrupted AI pause pictures for “{}”",
        second
            .pause
            .as_deref()
            .ok_or("Interrupted pause is unavailable")?
    );
    let controls = |d: &Driver<'_>| {
        [
            interrupted_control_visibility(d, &text, &text),
            interrupted_control_visibility(d, &format!("Retry {subject} (R)"), "Retry"),
            interrupted_control_visibility(d, &format!("Discard {subject} (D)"), "Discard"),
        ]
    };
    let mut visibility = controls(d);
    // Scroll-to-selection animates over several simulated frames. Bound the
    // observation without sending more navigation or scrolling input.
    for _ in 0..d.options.hz / 2 {
        if visibility.iter().all(|item| item["visible"] == true) {
            break;
        }
        d.step("Keyboard row reveal animation", false)?;
        visibility = controls(d);
    }
    d.check(
        "J scrolls the second interrupted row and its complete Retry and Discard buttons into view",
        d.app().jobs.selected()
            == Some(&crate::preview::jobs::RowKey::Interrupted(
                second.request.clone(),
                second.attempt.clone(),
            ))
            && visibility.iter().all(|item| item["visible"] == true),
        json!(
            "second row selected; row text and complete action hit rectangles inside paint clips"
        ),
        json!(visibility),
    )?;
    d.capture("Second interrupted AI row selected and fully visible")?;
    d.key(Key::K)?;
    d.settled()?;
    let interrupted = d
        .app()
        .ai_interrupted()
        .first()
        .cloned()
        .ok_or("Missing sleep interruption")?;
    let path = d.app().workspace.as_ref().ok_or("No project")?.path.clone();
    let request = deadpan_jobs::RequestId::new(interrupted.request.clone())
        .map_err(|error| error.to_string())?;
    let attempt_count =
        deadpan_store::ProjectStore::open(&path, deadpan_store::AccessMode::ReadOnly)
            .and_then(|store| store.generation_attempts(&request, 0, 100))
            .map_err(|error| error.to_string())?
            .len();
    d.key(Key::R)?;
    d.wait_for("R explicitly retries after sleep", generating)?;
    d.wait_for(
        "Only the retried sleep interruption leaves the list",
        |app| app.ai_interrupted().len() == 1,
    )?;
    let attempts = deadpan_store::ProjectStore::open(&path, deadpan_store::AccessMode::ReadOnly)
        .and_then(|store| store.generation_attempts(&request, 0, 100))
        .map_err(|error| error.to_string())?;
    d.check(
        "R creates a fresh attempt of the retained request without changing the pause provider",
        attempts.len() == attempt_count + 1
            && attempts.last().is_some_and(|attempt| attempt.checkpoint.identity.attempt_id.as_str() != interrupted.attempt)
            && d.app().ai.job().is_some_and(|job| job.request.as_ref() == Some(&request))
            && d.revision() == revision,
        json!({"attempts":attempt_count+1,"request":interrupted.request,"authored_revision":"unchanged"}),
        json!({"attempts":attempts.len(),"job":format!("{:?}", d.app().ai.job()),"revision":d.revision()}),
    )?;
    d.step("Explicit sleep retry running", true)?;
    d.capture("Fresh AI attempt after explicit sleep retry")?;
    // Reopening selects the live row, rather than its stale interrupted key.
    d.key(Key::Escape)?;
    d.command("jobs")?;
    d.key(Key::X)?;
    d.wait_for("Retried sleep attempt cancelled", |app| {
        app.ai
            .job()
            .is_some_and(|job| job.outcome == Some(Outcome::Cancelled))
    })?;
    d.key(Key::Escape)?;
    d.command("jobs")?;
    d.key(Key::D)?;
    d.wait_for("Remaining sleep interruption discarded", |app| {
        app.ai_interrupted().is_empty()
    })?;
    d.key(Key::Escape)?;
    d.settled()
}

/// Both pauses have the same label. The later matching accessible control
/// belongs to the second row; require its whole hit target and final text
/// paint to be visible, rather than trusting an offscreen accessibility node.
fn interrupted_control_visibility(d: &Driver<'_>, label: &str, text: &str) -> Value {
    let rect = d
        .harness
        .root()
        .children_recursive()
        .filter_map(|node| {
            let access = node.accesskit_node();
            (access.label().as_deref() == Some(label)
                && !access.is_hidden()
                && !access.is_disabled()
                && access.bounding_box().is_some())
            .then(|| node.rect())
        })
        .max_by(|a, b| a.center().y.total_cmp(&b.center().y));
    let paint = scenarios::text_paint_visibility(d, text);
    let visible = rect.is_some_and(|rect| {
        rect.is_positive()
            && d.harness.ctx.content_rect().contains_rect(rect)
            && d.harness.output().shapes.iter().any(|clipped| {
                matches!(&clipped.shape, egui::Shape::Text(painted)
                    if painted.galley.text().contains(text)
                        && rect.contains_rect(painted.visual_bounding_rect()))
                    && clipped.clip_rect.contains_rect(rect)
            })
    }) && paint
        .last()
        .is_some_and(|part| part["fully_visible"] == true);
    json!({"label":label,"visible":visible,"hit_rect":format!("{rect:?}"),"paint":paint})
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
            ..Default::default()
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

    // Replace only the observation: the same two queued jobs now expose
    // their selected model estimate and the Mac's reserve in the real panel.
    const GIB: u64 = 1024 * 1024 * 1024;
    let memory_reason = "available memory is 21.47 GB; the model estimates 22.55 GB plus 8.59 GB reserved for the Mac";
    d.app()
        .service
        .jobs()
        .set_conditions(crate::jobs::Conditions {
            memory_headroom: Some(deadpan_cli::system_conditions::MemoryHeadroom {
                total_bytes: 64 * GIB,
                available_bytes: 20 * GIB,
            }),
            ..Default::default()
        });
    d.step("Jobs panel waiting for model memory", true)?;
    let rows = ai_rows(d);
    let widgets = widget_text(d);
    let paint = scenarios::text_paint_visibility(d, memory_reason);
    let cancel_paint = scenarios::text_paint_visibility(d, "Cancel");
    d.check(
        "Both AI jobs show their full memory estimate and reserve with Cancel only",
        rows.len() == 2
            && rows
                .iter()
                .all(|row| row.state == RowState::Held(memory_reason.into()))
            && widgets.contains(memory_reason)
            && rows.iter().all(|row| !row.can_run_anyway)
            && !widgets.contains("Run anyway")
            && !widgets.contains("O runs it anyway")
            && widgets.contains("Cancel")
            && !paint.is_empty()
            && paint
                .iter()
                .all(|item| item["fully_visible"] == true && item["elided"] == false)
            && !cancel_paint.is_empty()
            && cancel_paint
                .iter()
                .all(|item| item["fully_visible"] == true),
        json!({"reason":memory_reason,"action":"Cancel","fully_visible":true}),
        json!({"rows":format!("{rows:?}"),"memory_paint":paint,"cancel_paint":cancel_paint}),
    )?;
    d.capture("AI jobs waiting for model memory")?;
    d.key(Key::O)?;
    d.step("Memory capacity cannot be overridden", true)?;
    d.check(
        "O cannot launch either memory-held job and explains the capacity limit",
        ai_rows(d).len() == 2
            && ai_rows(d).iter().all(|row| {
                row.state == RowState::Held(memory_reason.into()) && !row.can_run_anyway
            })
            && d.app().jobs.status.as_deref().is_some_and(|status| {
                status.contains(memory_reason) && status.contains("This wait cannot be overridden")
            }),
        json!({"held_jobs":2,"override":false}),
        json!({"rows":format!("{:?}", ai_rows(d)),"status":d.app().jobs.status}),
    )?;
    d.app()
        .service
        .jobs()
        .set_conditions(crate::jobs::Conditions {
            power: Some("running on battery".into()),
            ..Default::default()
        });
    d.step("Battery admission restored", true)?;
    d.check(
        "Restoring battery conditions preserves both queued targets before Run anyway",
        ai_rows(d).len() == 2
            && ai_rows(d)
                .iter()
                .all(|row| row.state == RowState::Held("running on battery".into())),
        json!({"held_on_battery":2}),
        json!({"rows":format!("{:?}", ai_rows(d))}),
    )?;

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
