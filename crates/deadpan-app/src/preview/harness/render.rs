//! Production Render input, captured previews and real automatic publication.
//! Only destination picker results are supplied by the replay. The concurrent
//! edit case sends a real typed service command while a decision is open.

use super::*;
use deadpan_cli::encoded_render::workflow::{WorkflowOutcome, WorkflowStage};
use deadpan_core::{AudioTreatments, ClipGain, GainDb};
use deadpan_store::{AccessMode, ProjectStore};
use egui::{Key, Modifiers};

mod history;

const COMMIT: &str = "Commit preview and render";
const DISCARD: &str = "Discard preview and render";
const KEEP: &str = "Keep editing  Esc";
const BEATS: &str = "Current group beat outline pane";

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let output = d.options.output.join("render-output");
    std::fs::create_dir(&output).map_err(|error| error.to_string())?;
    let output = output.canonicalize().map_err(|error| error.to_string())?;
    let initial = d.revision();
    for command in [false, true] {
        picker(d, None);
        if command {
            d.command("render")?;
        } else {
            render_key(d)?;
        }
        picker_finished(d)?;
        d.check(
            "Cmd-E and :render cancellation author nothing and create no render job",
            d.app().render.open
                && d.revision() == initial
                && jobs(d)?.is_empty()
                && d.app().render.error().is_none(),
            json!({"revision":initial,"jobs":0,"command_entry":command}),
            render_snapshot(d),
        )?;
        close_status(d)?;
    }
    camera_decision(d)?;
    preview_gate(d, false, "Closed Camera")?;
    gain_decision(d)?;
    preview_gate(d, false, "Closed Gain")?;
    room_tone_decision(d)?;
    preview_gate(d, false, "Closed Room tone")?;
    stale_decision(d)?;
    actual_export(d, &output.join("preview.mp4"))?;
    history::run(d, &output)
}

fn picker(d: &mut Driver<'_>, path: Option<PathBuf>) {
    d.app_mut().dialogs = Dialogs::scripted(vec![(DialogKind::Render, path)]);
}

fn render_key(d: &mut Driver<'_>) -> Result<(), String> {
    d.key_modified(Key::E, Modifiers::COMMAND)?;
    d.wait_for("Render captures after this frame's native input", |app| {
        !app.render.requested
    })?;
    d.step("Paint the queued Render action", true)
}

fn picker_finished(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Captured Render picker completes", |app| {
        !app.dialogs.is_open() && !app.render.blocking() && !app.service.is_busy()
    })
}

fn close_status(d: &mut Driver<'_>) -> Result<(), String> {
    if d.app().render.open {
        d.capture("Render status remains separate from editor feedback")?;
        d.click("Close window")?;
    }
    Ok(())
}

fn open_camera(d: &mut Driver<'_>) -> Result<(), String> {
    d.click(BEATS)?;
    d.chord(&[Key::Comma, Key::F])?;
    d.wait_for("Camera opens against the displayed picture", |app| {
        app.camera.is_some()
    })?;
    d.key(Key::Plus)?;
    d.settled()?;
    preview_gate(d, true, "Camera")
}

fn camera_decision(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    open_camera(d)?;
    let prior = d
        .app()
        .camera_render_edit()?
        .ok_or("Camera has no changed proposal")?;
    d.click("Framing scale, percent of input size")?;
    d.key_modified(Key::A, Modifiers::COMMAND)?;
    let render_button = d.rect("Render  ⌘E")?.center();
    d.events(
        "Native Camera text and Render click in one batch",
        vec![
            egui::Event::Text("135".into()),
            egui::Event::PointerMoved(render_button),
            egui::Event::PointerButton {
                pos: render_button,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: render_button,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ],
    )?;
    d.wait_for("Same-batch Camera text reaches the Render capture", |app| {
        !app.render.requested && app.render.blocking()
    })?;
    let current = d
        .app()
        .camera_render_edit()?
        .ok_or("Camera text did not produce a proposal")?;
    let captured = d
        .app()
        .render
        .preview_for_check()
        .ok_or("Render did not freeze the Camera proposal")?;
    let exact = captured.session == current.session
        && captured.revision == current.revision
        && captured.cursor == current.cursor
        && captured.scope == current.scope
        && matches!((&captured.edit, &current.edit, &prior.edit),
            (ProjectEdit::SetFraming { node, framing }, ProjectEdit::SetFraming { node: expected_node, framing: expected }, ProjectEdit::SetFraming { framing: old, .. })
                if node == expected_node && framing == expected && framing != old);
    let observed = format!("{:?}", captured.edit);
    d.check(
        "Render captures same-batch native Camera text instead of the earlier draft",
        exact && d.revision() == revision,
        json!({"latest_native_text":"135","revision":revision}),
        json!(observed),
    )?;
    decision_layout(d, "Camera")?;
    d.key(Key::Escape)?;
    d.check(
        "Escape from Render keeps the Camera preview and its saved revision",
        d.app().camera.is_some() && !d.app().render.blocking() && d.revision() == revision,
        json!({"camera":true,"revision":revision}),
        render_snapshot(d),
    )?;
    close_status(d)?;
    picker(d, None);
    render_key(d)?;
    d.click(DISCARD)?;
    picker_finished(d)?;
    d.check(
        "Cancelling the discard destination preserves the Camera preview and creates no job",
        d.app().camera.is_some() && d.revision() == revision && jobs(d)?.is_empty(),
        json!({"camera":true,"jobs":0,"revision":revision}),
        render_snapshot(d),
    )?;
    close_status(d)?;
    d.click("Cancel  Esc")?;
    d.settled()
}

fn prepare_gain(d: &mut Driver<'_>, value: &str) -> Result<(), String> {
    d.click(BEATS)?;
    d.command("gain")?;
    d.wait_for("Initial Gain proposal is prepared", |app| {
        !app.service.is_busy()
            && app
                .gain
                .as_ref()
                .is_some_and(|draft| draft.prepared_snapshot().is_some())
    })?;
    let fields = d
        .harness
        .root()
        .children_recursive()
        .filter(|node| {
            let access = node.accesskit_node();
            access.role() == egui::accesskit::Role::TextInput
                && access.label().as_deref() == Some("Whole beat trim · dB")
        })
        .map(|node| node.rect())
        .collect::<Vec<_>>();
    let [field] = fields.as_slice() else {
        return Err("Expected one native Gain trim field".into());
    };
    d.click_at("Whole beat trim · dB", field.center())?;
    d.key_modified(Key::A, Modifiers::COMMAND)?;
    d.events(
        "Enter a native Gain preview value",
        vec![egui::Event::Text(value.into())],
    )?;
    d.click("Set trim")?;
    d.wait_for("Exact Gain proposal is prepared", |app| {
        !app.service.is_busy()
            && app
                .gain
                .as_ref()
                .is_some_and(|draft| draft.prepared_snapshot().is_some())
    })?;
    d.click("Gain draft keyboard focus")?;
    preview_gate(d, true, "Gain")
}

fn gain_decision(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    prepare_gain(d, "-6.250")?;
    let proposal = d
        .app()
        .gain
        .as_ref()
        .and_then(|draft| draft.prepared_snapshot())
        .ok_or("No exact Gain preview")?
        .content
        .clone();
    picker(d, None);
    render_key(d)?;
    decision_layout(d, "Gain")?;
    d.click(COMMIT)?;
    picker_finished(d)?;
    d.check(
        "Cancelling a commit destination retains the exact Gain proposal without authoring",
        d.revision() == revision
            && jobs(d)?.is_empty()
            && d.app()
                .gain
                .as_ref()
                .and_then(|draft| draft.prepared_snapshot())
                .is_some_and(|prepared| prepared.content == proposal),
        json!({"revision":revision,"jobs":0,"proposal":format!("{proposal:?}")}),
        render_snapshot(d),
    )?;
    close_status(d)?;
    d.click("Cancel  Esc")?;
    d.settled()
}

fn room_tone_decision(d: &mut Driver<'_>) -> Result<(), String> {
    d.click(BEATS)?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num7, Key::L])?;
    let before = d.revision();
    d.command("hold 11f")?;
    d.changed(&before)?;
    let revision = d.revision();
    d.command("source")?;
    d.chord(&[
        Key::G,
        Key::G,
        Key::Num1,
        Key::Num0,
        Key::L,
        Key::V,
        Key::Num1,
        Key::Num0,
        Key::L,
        Key::Y,
    ])?;
    d.wait_for(
        "Original copy is durably saved before the next operation",
        |app| !app.service.is_busy() && !app.copied.is_pending(),
    )?;
    d.command("sequence")?;
    d.settled()?;
    d.command("room-tone")?;
    d.wait_for("Room tone prepares the captured Original samples", |app| {
        !app.service.is_busy()
            && app
                .room_tone
                .as_ref()
                .is_some_and(|draft| draft.prepared.is_some())
    })?;
    preview_gate(d, true, "Room tone")?;
    render_key(d)?;
    decision_layout(d, "Room tone")?;
    d.click(KEEP)?;
    d.check(
        "Keep editing restores Room tone without authoring its selected source",
        d.app().room_tone.is_some()
            && !d.app().render.blocking()
            && d.revision() == revision
            && jobs(d)?.is_empty(),
        json!({"room_tone":true,"revision":revision,"jobs":0}),
        render_snapshot(d),
    )?;
    // Cancel the original sheet through its real control before closing the
    // separate status window, which sits behind the modal.
    d.click("Cancel  Esc")?;
    close_status(d)?;
    d.click(BEATS)?;
    d.key(Key::U)?;
    d.changed(&revision)?;
    d.settled()
}

fn stale_decision(d: &mut Driver<'_>) -> Result<(), String> {
    open_camera(d)?;
    let revision = d.revision();
    render_key(d)?;
    d.capture("Render holds an exact Camera proposal while another command commits")?;
    let workspace = d
        .app()
        .workspace
        .clone()
        .ok_or("No project for concurrent command")?;
    let node = d
        .app()
        .selected_beat
        .clone()
        .ok_or("No concurrent edit target")?;
    d.app().service.submit(ProjectRequest::Edit {
        expected_session: workspace.session,
        expected_revision: workspace.document.revision_id().clone(),
        cursor: ProjectFrame(
            i64::try_from(d.app().sequence_cursor).map_err(|error| error.to_string())?,
        ),
        scope: d.app().sequence_scope.clone(),
        edit: ProjectEdit::SetAudioTreatments {
            node,
            treatments: AudioTreatments::from_clip_gain(
                ClipGain::new(
                    GainDb::new(-3000).map_err(|error| error.to_string())?,
                    false,
                    Vec::new(),
                    Vec::new(),
                )
                .map_err(|error| error.to_string())?,
            ),
        },
    })?;
    d.wait_for(
        "Concurrent typed command commits while Render decision is open",
        |app| {
            !app.service.is_busy()
                && app
                    .workspace
                    .as_ref()
                    .is_some_and(|workspace| workspace.document.revision_id().as_str() != revision)
        },
    )?;
    let concurrent = d.revision();
    d.click(COMMIT)?;
    d.check(
        "A stale preview decision cannot retarget to the concurrent revision or open a picker",
        !d.app().render.blocking()
            && !d.app().dialogs.is_open()
            && d.revision() == concurrent
            && d.app()
                .render
                .error()
                .is_some_and(|error| error.contains("project changed"))
            && jobs(d)?.is_empty(),
        json!({"captured_revision":revision,"current_revision":concurrent,"jobs":0}),
        render_snapshot(d),
    )?;
    close_status(d)?;
    d.settled()
}

fn actual_export(d: &mut Driver<'_>, movie: &Path) -> Result<(), String> {
    prepare_gain(d, "-6.250")?;
    let expected = d
        .app()
        .gain
        .as_ref()
        .and_then(|draft| draft.prepared_snapshot())
        .ok_or("No prepared export proposal")?
        .document
        .nodes()
        .clone();
    let before = d.revision();
    picker(d, Some(movie.to_owned()));
    render_key(d)?;
    d.click(COMMIT)?;
    picker_finished(d)?;
    let update = d
        .app()
        .render_job
        .clone()
        .ok_or("No Render command outcome")?;
    let command = update
        .command
        .ok_or("Missing ticketed preview commit outcome")?;
    let committed = command
        .committed_revision
        .ok_or_else(|| format!("Preview was not committed: {:?}", command.result))?;
    let identity = command
        .result
        .map_err(|error| format!("Render admission: {} ({})", error.message, error.code))?;
    let workflow = update.workflow.ok_or("No admitted render workflow")?;
    d.check(
        "Commit preview and render acknowledges one exact saved revision before work starts",
        committed.as_str() != before
            && workflow.revision == committed
            && workflow.status.identity.as_ref() == Some(&identity)
            && d.app().gain.is_none()
            && d.app().workspace.as_ref().is_some_and(|workspace| {
                workspace.document.revision_id() == &committed
                    && workspace.document.nodes() == &expected
            }),
        json!({"revision":committed,"identity":format!("{identity:?}")}),
        render_snapshot(d),
    )?;
    d.click(BEATS)?;
    d.key(Key::U)?;
    d.changed(committed.as_str())?;
    let later = d.revision();
    wait_terminal(d)?;
    let workflow = d
        .app()
        .render_job
        .as_ref()
        .and_then(|update| update.workflow.as_ref())
        .ok_or("Render lost its status")?
        .clone();
    let receipt = workflow
        .status
        .receipt
        .clone()
        .ok_or_else(|| format!("No publication receipt: {:?}", workflow.status))?;
    let saved_jobs = jobs(d)?;
    d.check(
        "Real automatic render publishes the committed preview after a later editor undo",
        workflow.status.outcome == Some(WorkflowOutcome::Published)
            && workflow.status.cleanup_confirmed
            && workflow.status.observed_movie_commit
            && workflow.revision == committed
            && d.revision() == later
            && saved_jobs.len() == 1
            && saved_jobs[0].revision_id == committed
            && saved_jobs[0].policy.is_automatic()
            && receipt.movie == movie
            && std::fs::metadata(&receipt.movie)
                .map_err(|error| error.to_string())?
                .len()
                == receipt.movie_bytes
            && std::fs::metadata(&receipt.report)
                .map_err(|error| error.to_string())?
                .len()
                == receipt.report_bytes,
        json!({"render_revision":committed,"editor_revision":later,"outcome":"Published","jobs":1}),
        json!({"render":render_snapshot(d),"receipt":receipt}),
    )?;
    for (width, height) in [(960.0, 640.0), (1280.0, 820.0)] {
        resize(d, width, height)?;
        d.capture(&format!("Verified Render outcome at {width}x{height}"))?;
        for label in [
            "Full edit · automatic MP4".to_owned(),
            "Movie exported".to_owned(),
            format!("Saved revision {}", committed.as_str()),
            format!("Movie: {}", receipt.movie.display()),
            format!("Local report: {}", receipt.report.display()),
        ] {
            visible_text(d, &label, "Published Render status stays readable")?;
        }
        let close = d.rect("Close window")?;
        d.check(
            "Published status close control remains reachable",
            d.harness.ctx.content_rect().contains_rect(close),
            json!("complete close target"),
            json!(format!("{close:?}")),
        )?;
    }
    Ok(())
}

fn wait_terminal(d: &mut Driver<'_>) -> Result<(), String> {
    let started = Instant::now();
    let deadline = started + Duration::from_secs(240);
    loop {
        if let Some(status) = d
            .app()
            .render_job
            .as_ref()
            .and_then(|update| update.workflow.as_ref())
            .map(|workflow| &workflow.status)
            && matches!(
                status.stage,
                WorkflowStage::Finished | WorkflowStage::Unresolved
            )
        {
            d.metric(
                "Real automatic UI Render wall time",
                started.elapsed().as_secs_f64() * 1000.0,
                SampleOutcome::Completed,
            );
            return Ok(());
        }
        if Instant::now() >= deadline {
            d.metric(
                "Real automatic UI Render wall time",
                started.elapsed().as_secs_f64() * 1000.0,
                SampleOutcome::TimedOut,
            );
            return Err(format!(
                "Actual Render exceeded 240 seconds: {}",
                render_snapshot(d)
            ));
        }
        d.step("Actual Render worker progress", false)?;
        d.wake.wait_until(deadline);
    }
}

fn decision_layout(d: &mut Driver<'_>, proposal: &str) -> Result<(), String> {
    // egui measures a newly opened Area in an invisible, disabled sizing pass.
    // Wait for that bounded measurement before checking its painted controls.
    for _ in 0..3 {
        if d.rect(KEEP).is_ok() {
            break;
        }
        d.step("Measure the new Render decision", false)?;
    }
    for (width, height) in [(960.0, 640.0), (1280.0, 820.0)] {
        resize(d, width, height)?;
        d.capture(&format!(
            "Explicit {proposal} Render decision at {width}x{height}"
        ))?;
        for label in [COMMIT, DISCARD, KEEP] {
            let rect = d.rect(label)?;
            d.check(
                "Each preview Render choice has a complete visible hit target",
                d.harness.ctx.content_rect().contains_rect(rect),
                json!({"proposal":proposal,"label":label,"viewport":[width,height]}),
                json!(format!("{rect:?}")),
            )?;
            visible_text(d, label, "Each preview Render choice is fully painted")?;
        }
    }
    Ok(())
}

fn resize(d: &mut Driver<'_>, width: f32, height: f32) -> Result<(), String> {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing Render replay viewport")?
        .inner_rect = Some(rect);
    Ok(())
}

fn visible_text(d: &mut Driver<'_>, label: &str, name: &str) -> Result<(), String> {
    let paint = scenarios::text_paint_visibility(d, label);
    d.check(
        name,
        !paint.is_empty() && paint.iter().all(|item| item["fully_visible"] == true),
        json!(label),
        json!(paint),
    )
}

fn jobs(d: &Driver<'_>) -> Result<Vec<deadpan_jobs::render::RenderIntent>, String> {
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No project for persisted Render inspection")?;
    ProjectStore::open(&workspace.path, AccessMode::ReadOnly)
        .map_err(|error| error.to_string())?
        .render_jobs(None, 4)
        .map_err(|error| error.to_string())
}

fn render_snapshot(d: &Driver<'_>) -> Value {
    json!({"revision":d.revision(),"blocking":d.app().render.blocking(),"error":d.app().render.error(),"status":format!("{:?}", d.app().render_job)})
}

fn preview_gate(d: &mut Driver<'_>, expected: bool, label: &str) -> Result<(), String> {
    let actual = d.app().service.preview_active_for_check();
    d.check(
        &format!("{label} publishes its temporary-preview state to remote Render admission"),
        actual == expected,
        json!({"preview_active":expected}),
        json!({"preview_active":actual}),
    )
}
