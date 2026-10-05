//! Native command, focus, stored history and actual recovery workflows.

use super::*;

pub(super) fn run(d: &mut Driver<'_>, output: &Path) -> Result<(), String> {
    close_status(d)?;
    let revision = d.revision();
    let saved_job = jobs(d)?
        .into_iter()
        .next()
        .ok_or("No render job for recovery")?;
    let path = d.app().workspace.as_ref().ok_or("No project")?.path.clone();

    // Reopen through the real service. No in-memory workflow remains to supply
    // these pages, and reopening must not turn the previous render into an edit.
    d.app().service.submit(ProjectRequest::Close)?;
    d.wait_for("Close releases the render owner", |app| {
        !app.service.is_busy() && app.workspace.is_none()
    })?;
    d.app().service.submit(ProjectRequest::Open(path))?;
    d.wait_for("Reopen loads the saved project", |app| {
        !app.service.is_busy() && app.workspace.is_some()
    })?;
    d.settled()?;
    d.command("renders")?;
    ready(d)?;
    d.check(
        "Saved renders load after reopening without changing authored history",
        d.revision() == revision && d.app().current_render().is_none(),
        json!({"revision":revision,"in_memory_workflow":false}),
        render_snapshot(d),
    )?;
    for (width, height) in [(960.0, 640.0), (1280.0, 820.0)] {
        resize(d, width, height)?;
        d.capture(&format!("Stored renders after reopen at {width}x{height}"))?;
        for label in [
            "Saved renders",
            "Saved edits",
            "Destinations",
            "View render attempts",
            "Back to editor  Esc",
        ] {
            visible_text(d, label, "Stored render browsing remains fully painted")?;
        }
    }
    let cursor = d.app().sequence_cursor;
    d.chord(&[Key::H, Key::D, Key::D])?;
    d.check(
        "History owns ordinary editing keys",
        d.revision() == revision && d.app().sequence_cursor == cursor,
        json!({"revision":revision,"cursor":cursor}),
        d.snapshot(),
    )?;
    activate_with_tab(d, "View render attempts")?;
    ready(d)?;
    visible_text(
        d,
        "Render this saved edit again…",
        "Historical re-encoding is explicit",
    )?;

    let attempts_before = attempts(d, &saved_job.job_id)?.len();
    focus_with_tab(d, "Save movie from attempt 1…")?;
    for (ime, key) in [
        (
            egui::ImeEvent::Preedit {
                text: "候補".into(),
                active_range_chars: Some(0..2),
            },
            Key::Enter,
        ),
        (egui::ImeEvent::Commit("候補".into()), Key::Space),
    ] {
        d.events(
            "Composition confirmation on a recovery button",
            vec![
                egui::Event::Ime(ime),
                egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
                egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
            ],
        )?;
        d.check(
            "IME confirmation never activates the focused saved-movie action",
            d.app().render.history.ready_for_check()
                && !d.app().dialogs.is_open()
                && attempts(d, &saved_job.job_id)?.len() == attempts_before,
            json!({"browser_open":true,"picker":false,"attempts":attempts_before}),
            render_snapshot(d),
        )?;
    }
    picker(d, None);
    activate_with_tab(d, "Save movie from attempt 1…")?;
    ready(d)?;
    d.check(
        "Cancelling a saved-movie destination returns to history without creating an attempt",
        d.revision() == revision && attempts(d, &saved_job.job_id)?.len() == attempts_before,
        json!({"revision":revision,"attempts":attempts_before}),
        render_snapshot(d),
    )?;
    d.key(Key::Tab)?;
    d.key(Key::Escape)?;
    d.step("Restore editor focus after the modal frame", false)?;
    d.check(
        "Closing after picker cancellation restores the editor's entry focus",
        d.harness.ctx.memory(|memory| memory.focused()) == Some(pane_id(d.app().pane)),
        json!(format!("{:?}", pane_id(d.app().pane))),
        d.snapshot(),
    )?;
    d.command("renders")?;
    ready(d)?;

    let retry_movie = output.join("saved-movie-retry.mp4");
    picker(d, Some(retry_movie.clone()));
    activate_with_tab(d, "Save movie from attempt 1…")?;
    picker_finished(d)?;
    wait_terminal(d)?;
    check_publication(d, &saved_job, &retry_movie, &revision, true)?;
    close_status(d)?;

    d.command("renders")?;
    ready(d)?;
    activate_with_tab(d, "Destinations")?;
    ready(d)?;
    d.capture("Durable movie destinations remain readable after a saved-movie retry")?;
    let original_bytes =
        std::fs::read(output.join("preview.mp4")).map_err(|error| error.to_string())?;
    let retry_bytes = std::fs::read(&retry_movie).map_err(|error| error.to_string())?;
    activate_with_tab(d, "Check previous destination")?;
    picker_finished(d)?;
    wait_terminal(d)?;
    let reconciled = d
        .app()
        .render_job
        .as_ref()
        .and_then(|update| update.workflow.as_ref())
        .ok_or("Missing reconciliation")?;
    d.check(
        "Checking a captured destination uses its original path and preserves both saved movies",
        reconciled.status.outcome == Some(WorkflowOutcome::Published)
            && reconciled.revision == saved_job.revision_id
            && d.revision() == revision
            && std::fs::read(output.join("preview.mp4")).map_err(|error| error.to_string())?
                == original_bytes
            && std::fs::read(&retry_movie).map_err(|error| error.to_string())? == retry_bytes,
        json!({"outcome":"Published","editor_revision":revision,"both_movies_unchanged":true}),
        render_snapshot(d),
    )?;
    close_status(d)?;

    d.command("renders")?;
    ready(d)?;
    activate_with_tab(d, "Saved edits")?;
    ready(d)?;
    activate_with_tab(d, "View render attempts")?;
    ready(d)?;
    let reencoded = output.join("saved-edit-reencoded.mp4");
    picker(d, Some(reencoded.clone()));
    activate_with_tab(d, "Render this saved edit again…")?;
    picker_finished(d)?;
    wait_terminal(d)?;
    check_publication(d, &saved_job, &reencoded, &revision, false)?;
    close_status(d)?;

    // Reopening a browser and Escape must leave the retained picture/edit alone.
    d.command("renders")?;
    ready(d)?;
    d.key(Key::Escape)?;
    d.check(
        "Escape closes saved renders without editing or leaving a modal input owner",
        !d.app().render.blocking() && d.revision() == revision,
        json!({"blocking":false,"revision":revision}),
        render_snapshot(d),
    )?;
    coalesced_owner_render(d, output)
}

fn coalesced_owner_render(d: &mut Driver<'_>, output: &Path) -> Result<(), String> {
    let old_session = d.app().workspace.as_ref().ok_or("No project")?.session;
    let path = d.app().workspace.as_ref().ok_or("No project")?.path.clone();
    d.app_mut().feedback.hold_project_updates = true;
    d.app().service.submit(ProjectRequest::Close)?;
    d.wait_for("Owner closes while UI delivery is held", |app| {
        !app.service.is_busy()
    })?;
    d.app().service.submit(ProjectRequest::Open(path))?;
    d.wait_for("Owner reopens while UI delivery is held", |app| {
        !app.service.is_busy()
    })?;
    let workspace = d
        .app()
        .service
        .take_update()
        .and_then(|update| update.workspace)
        .ok_or("No new owner workspace")?;
    let limits = deadpan_cli::render::default_limits().map_err(|error| error.to_string())?;
    let request = deadpan_cli::render::start_request(
        &deadpan_cli::render::RenderContext::from_document(&workspace.document),
        deadpan_jobs::render::RenderAutomaticAlgorithm::for_output(
            workspace.color_decision().output,
        ),
        output.join("coalesced-owner.mp4"),
        Instant::now(),
    )
    .map_err(|error| error.to_string())?;
    d.app().service.submit(ProjectRequest::Render(
        crate::project::ProjectRenderRequest {
            ticket: 987_654,
            context: crate::project::ProjectRenderContext {
                session: workspace.session,
                project: workspace.document.project_id().clone(),
            },
            operation: crate::project::ProjectRenderOperation::Start {
                request,
                limits: crate::project::ProjectRenderLimits {
                    encode: limits.encode,
                    verification: limits.verification,
                    media: limits.media,
                },
            },
        },
    ))?;
    d.wait_for(
        "Owner admits rendering before the UI sees the new session",
        |app| !app.service.is_busy(),
    )?;
    d.app_mut().feedback.hold_project_updates = false;
    d.wait_for("Coalesced session and render status reach the UI", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.session != old_session)
    })?;
    d.check(
        "A coalesced new-session owner render opens its matching status window",
        d.app().render.open
            && d.app()
                .current_render()
                .is_some_and(|workflow| workflow.context.session == workspace.session),
        json!({"open":true,"session":workspace.session}),
        render_snapshot(d),
    )?;
    wait_terminal(d)?;
    d.check(
        "The coalesced owner render completes normally",
        d.app()
            .current_render()
            .is_some_and(|workflow| workflow.status.outcome == Some(WorkflowOutcome::Published)),
        json!("Published"),
        render_snapshot(d),
    )
}

fn ready(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("The exact stored-render query completes", |app| {
        !app.service.is_busy() && !app.dialogs.is_open() && app.render.history.ready_for_check()
    })?;
    d.step("Paint the admitted saved-render page", false)
}

fn activate_with_tab(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    focus_with_tab(d, label)?;
    d.key(Key::Enter)
}

fn focus_with_tab(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    for _ in 0..40 {
        let focused = d.harness.root().children_recursive().any(|node| {
            let access = node.accesskit_node();
            access.is_focused() && !access.is_disabled() && access.label().as_deref() == Some(label)
        });
        if focused {
            visible_text(d, label, "Native Tab reveals the complete recovery action")?;
            return Ok(());
        }
        d.key(Key::Tab)?;
        d.step("Settle recovery control focus", false)?;
    }
    Err(format!("Native Tab could not reach {label}"))
}

fn attempts(
    d: &Driver<'_>,
    job: &deadpan_jobs::RequestId,
) -> Result<Vec<deadpan_store::render_jobs::StoredRenderAttempt>, String> {
    let path = &d.app().workspace.as_ref().ok_or("No project")?.path;
    ProjectStore::open(path, AccessMode::ReadOnly)
        .map_err(|error| error.to_string())?
        .render_attempts(job, 0, 16)
        .map_err(|error| error.to_string())
}

fn check_publication(
    d: &mut Driver<'_>,
    job: &deadpan_jobs::render::RenderIntent,
    movie: &Path,
    editor_revision: &str,
    reused: bool,
) -> Result<(), String> {
    let workflow = d
        .app()
        .render_job
        .as_ref()
        .and_then(|update| update.workflow.as_ref())
        .ok_or("Missing recovery workflow")?;
    let status = &workflow.status;
    let original_encoding = attempts(d, &job.job_id)?
        .into_iter()
        .find(|attempt| attempt.ordinal == 1)
        .ok_or("Missing original encoding")?
        .attempt_id;
    let retained_owner = status
        .attempt
        .as_ref()
        .and_then(|attempt| attempt.checkpoint_attempt_id.as_ref());
    d.check(if reused { "Saved-movie retry freshly verifies the original encoding and immutable edit" } else { "Render-again creates a fresh encoding of the historical edit" },
        status.outcome == Some(WorkflowOutcome::Published) && status.cleanup_confirmed
            && workflow.revision == job.revision_id && d.revision() == editor_revision
            && status.receipt.as_ref().is_some_and(|receipt| receipt.movie == movie)
            && retained_owner.is_some_and(|owner| (owner == &original_encoding) == reused),
        json!({"movie":movie,"render_revision":job.revision_id,"editor_revision":editor_revision,"reused_encoding":reused}), render_snapshot(d))
}
