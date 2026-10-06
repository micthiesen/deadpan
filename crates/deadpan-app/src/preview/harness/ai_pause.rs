//! AI pause pictures through the production keys, commands and inspector.
//!
//! `ai-pause` replaces only the model worker with the scripted test seam
//! (`project::generation::Backend::Scripted`): conditioning, request
//! allocation and every durable transition are real; its scripts end in an
//! unavailable runtime, cancellation or failure. `ai-variants` uses the same
//! seam's Ready ending: the synthetic worker's footage goes through real host
//! qualification, publication and Ready, then variants are listed with
//! thumbnails, chosen, previewed, auditioned (delivery simulated), accepted
//! and durably discarded. `ai-pause-ready` opens a private copy of a real
//! project whose pause holds real accepted AI pictures, and exercises Undo,
//! Ready discovery, Preview, Accept and Discard against those real bundles.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use deadpan_core::{HoldVideo, NodeId, NodeKind};
use egui::Key;

use super::*;
use crate::project::generation::{Backend, Outcome, Script, ScriptEnding, ScriptQueue};

const UNAVAILABLE: &str = "AI pauses need the development model runtime: LTX runtime source not found at /replay/missing (set DEADPAN_BRIDGE_RUNTIME_SOURCE)";
const FAILURE: &str = "Scripted replay worker failure: the model stopped";

/// Three starts: unavailable runtime, a run that waits for cancellation, then
/// a worker failure.
pub(super) fn backend() -> Backend {
    let run = |unavailable: Option<&str>, steps, ending| Script {
        unavailable: unavailable.map(str::to_owned),
        steps,
        step_interval: Duration::from_millis(30),
        ending,
    };
    Backend::Scripted(Arc::new(ScriptQueue::new([
        run(Some(UNAVAILABLE), 0, ScriptEnding::WaitForCancel),
        run(None, 8, ScriptEnding::WaitForCancel),
        run(None, 3, ScriptEnding::Fail(FAILURE.into())),
    ])))
}

/// Every start of `ai-variants` produces Ready synthetic footage.
pub(super) fn variants_backend() -> Backend {
    Backend::Scripted(Arc::new(ScriptQueue::new([Script {
        unavailable: None,
        steps: 3,
        step_interval: Duration::from_millis(20),
        ending: ScriptEnding::Ready,
    }])))
}

fn widget_text(d: &Driver<'_>) -> String {
    d.widgets().to_string()
}

fn outcome(d: &Driver<'_>) -> Option<Outcome> {
    d.app().ai.job()?.outcome.clone()
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push(
        "The model worker is the scripted test seam; real inference, qualification and Ready pictures are covered by ai-pause-ready with a real project and the DEADPAN_BRIDGE_REAL service test.".into(),
    );
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    d.chord(&[Key::Comma, Key::A])?;
    d.check(
        ",a on a picture beat explains that AI pictures fill a pause",
        d.app()
            .error
            .as_deref()
            .is_some_and(|error| error.contains("AI pictures fill a pause"))
            && d.app().ai.job().is_none(),
        json!("AI pictures fill a pause…"),
        json!({"error":d.app().error}),
    )?;
    let before = d.revision();
    d.chord(&[Key::Comma, Key::H])?;
    d.changed(&before)?;
    d.settled()?;
    let paused = d.revision();
    let hold = d.app().ai_hold();
    let widgets = widget_text(d);
    d.check(
        "A selected pause teaches ,a in the inspector and footer",
        hold.is_some()
            && widgets.contains("Generate AI pictures")
            && widgets.contains("AI PICTURES")
            && widgets.contains("AI pictures"),
        json!({"hold_selected":true,"inspector":"Generate AI pictures  ,a","footer":",a AI pictures"}),
        json!({"hold":hold,"widgets_have_action":widgets.contains("Generate AI pictures")}),
    )?;
    d.capture("Pause selected with its AI pictures action")?;

    d.chord(&[Key::Comma, Key::A])?;
    d.wait_for("Unavailable runtime reported", |app| {
        app.ai
            .job()
            .is_some_and(|job| matches!(job.outcome, Some(Outcome::Unavailable(_))))
    })?;
    d.step("Unavailable runtime shown", false)?;
    let widgets = widget_text(d);
    d.check(
        "A missing runtime shows its exact reason without an edit",
        widgets.contains("AI pauses are unavailable on this Mac")
            && widgets.contains("set DEADPAN_BRIDGE_RUNTIME_SOURCE")
            && d.revision() == paused,
        json!({"title":"AI pauses are unavailable on this Mac","reason":UNAVAILABLE,"revision":paused}),
        json!({"outcome":format!("{:?}", outcome(d)),"revision":d.revision()}),
    )?;
    d.capture("Unavailable model runtime")?;

    d.command("generate")?;
    d.wait_for("Scripted generation reaches its last step", |app| {
        app.ai
            .job()
            .is_some_and(|job| job.running() && job.phase.steps() == Some((8, 8)))
    })?;
    d.step("Generation progress shown", false)?;
    let widgets = widget_text(d);
    d.check(
        "Progress shows stage, step and elapsed time in the inspector and footer",
        widgets.contains("AI pause · Generating pictures 8/8")
            && widgets.contains("Generating pictures · step 8 of 8")
            && widgets.contains("Cancel generation")
            && d.app().ai.job().is_some_and(|job| job.request.is_some())
            && d.revision() == paused,
        json!({"footer":"AI pause · Generating pictures 8/8 · m:ss","inspector":"Generating pictures · step 8 of 8","recorded_request":true}),
        json!({"job":format!("{:?}", d.app().ai.job()),"revision":d.revision()}),
    )?;
    d.capture("Generating with progress")?;
    // Editing stays immediate while the job runs.
    d.check(
        "Generation never makes the project busy",
        !d.app().service.is_busy(),
        json!(false),
        json!(d.app().service.is_busy()),
    )?;

    // Escape never cancels a long generation, even in the inspector.
    for _ in 0..6 {
        if d.app().pane == Pane::Inspector {
            break;
        }
        d.key(Key::Tab)?;
    }
    d.key(Key::Escape)?;
    d.step("Escape in the inspector", false)?;
    d.check(
        "Escape in the inspector leaves the generation running",
        d.app().ai.job().is_some_and(|job| job.running()),
        json!({"running":true}),
        json!({"job":format!("{:?}", d.app().ai.job())}),
    )?;
    d.command("cancel-ai")?;
    d.wait_for("Generation cancelled", |app| {
        app.ai
            .job()
            .is_some_and(|job| job.outcome == Some(Outcome::Cancelled))
    })?;
    d.step("Cancellation shown", false)?;
    d.check(
        ":cancel-ai cancels; the attempt is recorded and the pause is unchanged",
        d.revision() == paused && widget_text(d).contains("Cancelled. The pause is unchanged."),
        json!({"outcome":"Cancelled","revision":paused}),
        json!({"outcome":format!("{:?}", outcome(d)),"revision":d.revision(),"pane":format!("{:?}", d.app().pane)}),
    )?;
    d.capture("Cancelled")?;

    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::Comma, Key::A])?;
    d.wait_for("Scripted failure reported", |app| {
        app.ai
            .job()
            .is_some_and(|job| matches!(job.outcome, Some(Outcome::Failed(_))))
    })?;
    d.step("Failure shown", false)?;
    let widgets = widget_text(d);
    d.check(
        "A worker failure is shown with its reason and Generate stays available",
        widgets.contains("Generation failed; the pause is unchanged")
            && widgets.contains(FAILURE)
            && widgets.contains("Generate AI pictures")
            && d.revision() == paused,
        json!({"title":"Generation failed; the pause is unchanged","reason":FAILURE}),
        json!({"outcome":format!("{:?}", outcome(d)),"revision":d.revision()}),
    )?;
    d.capture("Failed")?;
    Ok(())
}

/// The ready fixture must already hold real accepted AI pictures whose
/// request is still current, so one Undo offers the candidate again.
pub(super) fn preflight(package: &Path) -> Result<(), String> {
    let store = deadpan_store::ProjectStore::open(package, deadpan_store::AccessMode::ReadOnly)
        .map_err(|error| format!("Read-only AI pause fixture preflight: {error}"))?;
    let document = store.snapshot().map_err(|error| error.to_string())?;
    let accepted = document.nodes().values().any(|node| {
        matches!(&node.kind, NodeKind::Hold { recipe }
            if matches!(recipe.video, HoldVideo::Generated { .. }))
    });
    let current = store
        .current_generation_requests()
        .map_err(|error| error.to_string())?
        .into_iter()
        .any(|request| {
            store
                .selected_generation_bundle(&request.request_id)
                .is_ok_and(|selected| selected.is_some())
        });
    let (can_undo, _) = store
        .history_availability()
        .map_err(|error| error.to_string())?;
    if !accepted || !current || !can_undo {
        return Err("ai-pause-ready needs a project whose last edit accepted real AI pictures with a current Ready request".into());
    }
    Ok(())
}

fn select_beat(d: &mut Driver<'_>, target: &NodeId) -> Result<(), String> {
    for _ in 0..64 {
        if d.app().selected_beat.as_ref() == Some(target) {
            return Ok(());
        }
        let index = d
            .app()
            .beat_rows
            .iter()
            .position(|row| &row.id == target)
            .ok_or("The pause is not a visible beat")?;
        let current = d
            .app()
            .selected_beat
            .as_ref()
            .and_then(|selected| d.app().beat_rows.iter().position(|row| &row.id == selected))
            .unwrap_or(0);
        d.key(if index > current { Key::J } else { Key::K })?;
    }
    Err("Could not select the pause".into())
}

pub(super) fn ready(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push(
        "Uses a private copy of a real project whose pause holds real accepted AI pictures; the model is not run during replay.".into(),
    );
    d.command("sequence")?;
    d.settled()?;
    super::transcript::focus_your_edit(d)?;
    let workspace = d.app().workspace.clone().ok_or("No project")?;
    let hold = workspace
        .document
        .nodes()
        .iter()
        .find_map(|(id, node)| {
            matches!(&node.kind, NodeKind::Hold { recipe }
                if matches!(recipe.video, HoldVideo::Generated { .. }))
            .then(|| id.clone())
        })
        .ok_or("No accepted AI pause")?;
    select_beat(d, &hold)?;
    d.settled()?;
    d.capture("Accepted AI pause")?;

    let accepted = d.revision();
    d.key(Key::U)?;
    d.changed(&accepted)?;
    d.settled()?;
    select_beat(d, &hold)?;
    d.wait_for("Ready candidate offered after Undo", |app| {
        app.ai.candidate_count() == 1
    })?;
    let undone = d.revision();
    let widgets = widget_text(d);
    let background = matches!(
        &d.app().workspace.as_ref().ok_or("No project")?.document.nodes()[&hold].kind,
        NodeKind::Hold { recipe } if !matches!(recipe.video, HoldVideo::Generated { .. })
    );
    d.check(
        "Undo restores the previous picture and offers the retained Ready pictures again",
        background
            && widgets.contains("Ready does not change your edit.")
            && widgets.contains(":accept-ai")
            && widgets.contains(":preview-ai")
            && widgets.contains(":discard-ai"),
        json!({"hold_picture":"previous","candidate":"Ready","actions":["Preview :preview-ai","Accept :accept-ai","Discard :discard-ai"]}),
        json!({"previous_picture":background,"candidates":d.app().ai.candidate_count()}),
    )?;
    d.capture("Ready candidate")?;

    d.command("preview-ai")?;
    d.wait_for("AI preview displayed", |app| {
        app.ai.preview_request().is_some()
            && app.presentation.displayed_candidate()
            && !app.presentation.loading()
            && !app.presentation.needs_render()
    })?;
    d.step("AI preview shown", false)?;
    let widgets = widget_text(d);
    d.check(
        "Preview shows the generated picture in the viewer without an edit",
        d.revision() == undone && widgets.contains("AI PREVIEW · NOT SAVED"),
        json!({"revision":undone,"banner":"AI PREVIEW · NOT SAVED"}),
        json!({"revision":d.revision(),"snapshot":d.snapshot()}),
    )?;
    d.capture("Previewing AI pictures")?;
    for _ in 0..10 {
        d.key(Key::L)?;
    }
    d.wait_for("Later preview frame displayed", |app| {
        app.presentation.displayed_candidate()
            && !app.presentation.loading()
            && !app.presentation.needs_render()
    })?;
    d.capture("Previewing ten frames later")?;
    d.key(Key::Escape)?;
    d.wait_for("Edit picture restored", |app| {
        app.ai.preview_request().is_none()
            && !app.presentation.displayed_candidate()
            && !app.presentation.loading()
            && !app.presentation.needs_render()
    })?;
    d.check(
        "Escape leaves preview and keeps the candidate",
        d.revision() == undone && d.app().ai.candidate_count() == 1,
        json!({"revision":undone,"candidates":1}),
        json!({"revision":d.revision(),"candidates":d.app().ai.candidate_count()}),
    )?;

    d.command("accept-ai")?;
    d.changed(&undone)?;
    d.settled()?;
    let workspace = d.app().workspace.clone().ok_or("No project")?;
    let generated = matches!(
        &workspace.document.nodes()[&hold].kind,
        NodeKind::Hold { recipe } if matches!(recipe.video, HoldVideo::Generated { .. })
    );
    d.check(
        "Accept commits one undoable edit and keeps the pause selected",
        generated
            && d.app().selected_beat.as_ref() == Some(&hold)
            && workspace.can_undo
            && d.app().ai.candidate_count() == 0,
        json!({"picture":"Accepted AI","selected":hold,"candidates":0}),
        json!({"generated":generated,"selected":d.app().selected_beat,"candidates":d.app().ai.candidate_count(),"message":d.app().message}),
    )?;
    d.capture("Accepted again")?;

    let reaccepted = d.revision();
    d.key(Key::U)?;
    d.changed(&reaccepted)?;
    d.settled()?;
    select_beat(d, &hold)?;
    d.wait_for("Candidate offered again", |app| {
        app.ai.candidate_count() == 1
    })?;
    d.command("discard-ai")?;
    d.wait_for("Candidate discarded", |app| app.ai.candidate_count() == 0)?;
    d.step("Discarded", false)?;
    d.check(
        "Discard hides the candidate without an edit and Generate is offered again",
        widget_text(d).contains("Generate AI pictures"),
        json!("Generate AI pictures"),
        json!({"candidates":d.app().ai.candidate_count(),"message":d.app().message}),
    )?;
    d.capture("Discarded")?;
    Ok(())
}

fn chosen(d: &Driver<'_>) -> Option<usize> {
    d.app().ai.chosen_variant().map(|(number, _)| number)
}

pub(super) fn variants(d: &mut Driver<'_>) -> Result<(), String> {
    if let Err(reason) = crate::project::generation::synthetic_tools() {
        d.report.skipped.push(format!(
            "ai-variants needs the synthetic Ready worker's tools: {reason}"
        ));
        return Ok(());
    }
    d.report.skipped.push(
        "The model is the synthetic test worker (a blend of the boundary pictures with a seed-coloured band); conditioning, host qualification, publication, Ready, preview, acceptance and discard are real. Audition delivery is simulated: no audio device opens and nothing is heard.".into(),
    );
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    let before = d.revision();
    d.chord(&[Key::Comma, Key::H])?;
    d.changed(&before)?;
    d.settled()?;
    let paused = d.revision();
    let hold = d.app().ai_hold().ok_or("The new pause is not selected")?;

    d.command("generate 2")?;
    d.wait_for("First variant generating", |app| {
        app.ai
            .job()
            .is_some_and(|job| job.running() && job.variants == 2)
    })?;
    d.step("Two variants requested", false)?;
    let widgets = widget_text(d);
    d.check(
        ":generate 2 shows which variant is generating",
        widgets.contains("Variant 1 of 2") || widgets.contains("variant 1/2"),
        json!({"inspector":"Variant 1 of 2","footer":"AI pause · variant 1/2"}),
        json!({"job":format!("{:?}", d.app().ai.job())}),
    )?;
    d.wait_for("Both variants Ready", |app| {
        app.ai.job().is_some_and(|job| !job.running()) && app.ai.variant_count() == 2
    })?;
    d.wait_for("Variant thumbnails rendered", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| app.thumbnails.rendered_candidates(workspace.session) == 2)
    })?;
    d.step("Variants listed", false)?;
    let widgets = widget_text(d);
    let job = d.app().ai.job().cloned();
    d.check(
        "Two Ready variants are listed with thumbnails, the newest chosen, without an edit",
        matches!(
            job.as_ref().and_then(|job| job.outcome.clone()),
            Some(Outcome::Ready(_))
        ) && job.as_ref().is_some_and(|job| job.ready == 2)
            && widgets.contains("Ready · 2 variants")
            && widgets.contains("chosen")
            && widgets.contains(":next-ai")
            && widgets.contains("Audition")
            && chosen(d) == Some(2)
            && d.revision() == paused,
        json!({"variants":2,"thumbnails":2,"chosen":2,"revision":paused}),
        json!({"job":format!("{job:?}"),"chosen":chosen(d),"revision":d.revision()}),
    )?;
    d.capture("Two variants with thumbnails")?;

    d.command("prev-ai")?;
    d.wait_for("First variant chosen", |app| {
        app.ai
            .chosen_variant()
            .is_some_and(|(number, _)| number == 1)
    })?;
    d.check(
        ":prev-ai chooses the first variant without an edit",
        d.revision() == paused,
        json!({"chosen":1,"revision":paused}),
        json!({"chosen":chosen(d),"revision":d.revision()}),
    )?;
    let first = d.app().ai.chosen_variant().ok_or("No chosen variant")?.1;

    d.command("preview-ai")?;
    d.wait_for("First variant previewed", |app| {
        app.ai.preview_attempt() == Some(&first)
            && app.presentation.displayed_candidate()
            && !app.presentation.loading()
            && !app.presentation.needs_render()
    })?;
    d.step("First variant shown", false)?;
    d.check(
        "Preview shows the chosen variant in the viewer and names it in the footer",
        widget_text(d).contains("AI PREVIEW · VARIANT 1 OF 2 · NOT SAVED")
            && d.revision() == paused,
        json!({"footer":"AI PREVIEW · VARIANT 1 OF 2 · NOT SAVED"}),
        json!({"snapshot":d.snapshot()}),
    )?;
    d.capture("Previewing variant 1")?;
    let draft = |d: &Driver<'_>| match d.app().ai.preview_content() {
        Some(deadpan_playback::ContentIdentity::Proposed { draft, .. }) => Some(*draft),
        _ => None,
    };
    let first_draft = draft(d);

    d.command("next-ai")?;
    d.wait_for("Second variant previewed", |app| {
        app.ai
            .preview_attempt()
            .is_some_and(|attempt| attempt != &first)
            && app
                .ai
                .chosen_variant()
                .is_some_and(|(number, _)| number == 2)
            && app.presentation.displayed_candidate()
            && !app.presentation.loading()
            && !app.presentation.needs_render()
    })?;
    d.step("Second variant shown", false)?;
    d.check(
        ":next-ai while previewing shows the newly chosen variant",
        widget_text(d).contains("AI PREVIEW · VARIANT 2 OF 2 · NOT SAVED"),
        json!({"footer":"AI PREVIEW · VARIANT 2 OF 2 · NOT SAVED"}),
        json!({"snapshot":d.snapshot()}),
    )?;
    d.capture("Previewing variant 2")?;
    let second_draft = draft(d);
    d.check(
        "Each AI preview draws its draft identity from the shared proposal counter",
        first_draft
            .zip(second_draft)
            .is_some_and(|(first, second)| first < second && second <= d.app().serial),
        json!({"drafts":"increasing, from the counter Gain/Trim/Slip/Splice drafts use"}),
        json!({"first":first_draft,"second":second_draft,"serial":d.app().serial}),
    )?;
    let second = d.app().ai.chosen_variant().ok_or("No chosen variant")?.1;

    // Audition: the proposed acceptance document, admitted against this
    // exact revision, plays the pause in its loop context.
    let (start, end) = d
        .app()
        .beat_rows
        .iter()
        .find(|row| row.id == hold)
        .map(|row| (row.start, row.start + row.frames))
        .ok_or("The pause is not a visible beat")?;
    d.app_mut().feedback.simulate_playback = true;
    d.command("audition-ai")?;
    d.step("Audition started", false)?;
    let workspace = d.app().workspace.clone().ok_or("No project")?;
    let rate = workspace.document.presentation_basis().frame_rate;
    let boundary = |frame: u64| {
        rate.audio_boundary(deadpan_core::ProjectFrame(frame as i64))
            .map_err(|error| error.to_string())
    };
    let (pause_start, pause_end) = (boundary(start)?, boundary(end)?);
    let transport = d
        .app()
        .transport
        .as_ref()
        .map(|run| (run.content.clone(), run.revision.clone(), *run.window()));
    let expected_content = d.app().ai.preview_content().cloned();
    let audition = transport.as_ref().is_some_and(|(content, revision, window)| {
        Some(content) == expected_content.as_ref()
            && matches!(content, deadpan_playback::ContentIdentity::Proposed { base_revision, .. } if base_revision.as_str() == paused)
            && revision.as_str() != paused
            && window.looping()
            && window.start() < pause_start
            && window.end() > pause_end
    });
    d.check(
        ":audition-ai loops the pause with lead-in and follow-through from the proposed acceptance, admitted against the current revision",
        audition,
        json!({"content":"Proposed{base: current revision}","looping":true,"window":"pause with context"}),
        json!({"transport":format!("{transport:?}"),"expected":format!("{expected_content:?}")}),
    )?;
    let (mut feed, _callback) = deadpan_output::channel().map_err(|error| error.to_string())?;
    let loop_start = d
        .app()
        .transport
        .as_ref()
        .ok_or("No audition")?
        .window()
        .start();
    let generation = feed
        .restart(loop_start.0)
        .map_err(|error| error.to_string())?;
    let inside = boundary(start + (end - start) / 2)?;
    let update = {
        let run = d.app().transport.as_ref().ok_or("No audition")?;
        deadpan_playback::Update {
            ticket: run.ticket,
            session: run.session,
            project_id: run.project.clone(),
            revision_id: run.revision.clone(),
            content: run.content.clone(),
            phase: deadpan_playback::Phase::Playing,
            sample: Some(inside),
            generation: Some(generation),
            error: None,
        }
    };
    d.app_mut().feedback.playback_updates.push_back(update);
    d.wait_for("Audition picture inside the pause", |app| {
        (start..end).contains(&app.sequence_cursor)
            && app.presentation.displayed_candidate()
            && !app.presentation.loading()
    })?;
    d.check(
        "The heard position inside the pause shows the candidate's picture",
        d.app().transport.is_some() && d.revision() == paused,
        json!({"cursor_in_pause":true,"picture":"candidate"}),
        json!({"cursor":d.app().sequence_cursor,"pause":[start,end]}),
    )?;
    d.capture("Auditioning variant 2 with the pause's sound")?;
    d.command("audition-ai")?;
    d.step("Audition paused", false)?;
    d.check(
        "A second :audition-ai pauses the loop at the heard position and keeps the preview",
        d.app().transport.is_none()
            && d.app().resume.is_some()
            && (start..end).contains(&d.app().sequence_cursor)
            && d.app().ai.preview_attempt() == Some(&second),
        json!({"playing":false,"preview":"variant 2"}),
        json!({"snapshot":d.snapshot()}),
    )?;
    d.app_mut().feedback.simulate_playback = false;

    d.command("accept-ai")?;
    d.changed(&paused)?;
    d.settled()?;
    let accepted = d.revision();
    let workspace = d.app().workspace.clone().ok_or("No project")?;
    let generated = matches!(
        &workspace.document.nodes()[&hold].kind,
        NodeKind::Hold { recipe } if matches!(recipe.video, HoldVideo::Generated { .. })
    );
    d.wait_for("The other variant stays offered", |app| {
        app.ai.variant_count() == 1
    })?;
    d.check(
        "Accept commits the chosen variant as one undoable edit; the other variant stays offered",
        generated
            && d.app().selected_beat.as_ref() == Some(&hold)
            && workspace.can_undo
            && d.app()
                .ai
                .chosen_variant()
                .is_some_and(|(_, attempt)| attempt == first),
        json!({"picture":"variant 2","offered":["variant 1"]}),
        json!({"generated":generated,"chosen":format!("{:?}", d.app().ai.chosen_variant())}),
    )?;
    d.capture("Accepted variant 2")?;

    d.command("discard-ai")?;
    d.wait_for("Variant discarded", |app| app.ai.variant_count() == 0)?;
    d.step("Discarded", false)?;
    let durable = {
        let store =
            deadpan_store::ProjectStore::open(&workspace.path, deadpan_store::AccessMode::ReadOnly)
                .map_err(|error| error.to_string())?;
        let request = store
            .current_generation_requests()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|request| request.binding.hold_id == hold)
            .ok_or("No current request")?;
        store
            .generation_attempt(&deadpan_jobs::MessageIdentity::new(
                request.request_id,
                first.clone(),
            ))
            .map_err(|error| error.to_string())?
            .and_then(|attempt| attempt.bundle_receipt)
            .is_some_and(|receipt| {
                receipt.availability()
                    == deadpan_store::generation_attempts::CandidateAvailability::Evicted
            })
    };
    d.check(
        "Discard durably removes the variant without an edit",
        durable && d.revision() == accepted && widget_text(d).contains("Generate AI pictures"),
        json!({"stored":"evicted","revision":accepted}),
        json!({"durable":durable,"revision":d.revision(),"message":d.app().message}),
    )?;
    d.capture("Discarded variant 1")?;

    d.key(Key::U)?;
    d.changed(&accepted)?;
    d.settled()?;
    select_beat(d, &hold)?;
    d.wait_for("Accepted variant offered again after Undo", |app| {
        app.ai.variant_count() == 1
    })?;
    d.check(
        "Undo offers the accepted variant again; the discarded one stays gone",
        d.app()
            .ai
            .chosen_variant()
            .is_some_and(|(_, attempt)| attempt == second),
        json!({"offered":["variant 2"]}),
        json!({"chosen":format!("{:?}", d.app().ai.chosen_variant())}),
    )?;
    d.capture("Undo offers the accepted variant")?;
    Ok(())
}

/// The transport's content, window and heard content sample.
fn heard(
    d: &Driver<'_>,
) -> Option<(
    deadpan_playback::ContentIdentity,
    deadpan_playback::Window,
    deadpan_core::AudioSample,
)> {
    let run = d.app().transport.as_ref()?;
    Some((
        run.content.clone(),
        *run.window(),
        run.content_sample().ok()?,
    ))
}

/// Before / variant comparison at the same frame and the same heard sample,
/// through the production `,x` and `,n` keys, both stopped and auditioning.
pub(super) fn compare(d: &mut Driver<'_>) -> Result<(), String> {
    use deadpan_playback::ContentIdentity;
    if let Err(reason) = crate::project::generation::synthetic_tools() {
        d.report.skipped.push(format!(
            "ai-compare needs the synthetic Ready worker's tools: {reason}"
        ));
        return Ok(());
    }
    d.report.skipped.push(
        "The model is the synthetic test worker; conditioning, host qualification, publication, Ready and preview are real. Audition delivery is simulated: no audio device opens and nothing is heard, so the heard sample is injected.".into(),
    );
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    let before = d.revision();
    d.chord(&[Key::Comma, Key::H])?;
    d.changed(&before)?;
    d.settled()?;
    let paused = d.revision();
    let hold = d.app().ai_hold().ok_or("The new pause is not selected")?;
    d.command("generate 2")?;
    d.wait_for("Both variants Ready", |app| {
        app.ai.job().is_some_and(|job| !job.running()) && app.ai.variant_count() == 2
    })?;
    d.wait_for("Variant thumbnails rendered", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| app.thumbnails.rendered_candidates(workspace.session) == 2)
    })?;
    d.step("Variants listed", false)?;
    let widgets = widget_text(d);
    d.check(
        "The footer and inspector teach ,x before / after and ,n next variant",
        widgets.contains("AI before / after")
            && widgets.contains("next AI variant")
            && widgets.contains("Compare before / after")
            && d.revision() == paused,
        json!({",x":"AI before / after",",n":"next AI variant"}),
        json!({"widgets":widgets}),
    )?;
    let second = d.app().ai.chosen_variant().ok_or("No chosen variant")?.1;

    // Stopped: `,x` previews the chosen variant, then switches to Before and
    // back at the same frame.
    d.chord(&[Key::Comma, Key::X])?;
    d.wait_for("Chosen variant previewed", |app| {
        app.ai.preview_attempt() == Some(&second)
            && app.presentation.displayed_candidate()
            && !app.presentation.loading()
            && !app.presentation.needs_render()
    })?;
    d.chord(&[Key::L, Key::L, Key::L])?;
    let frame = d.app().sequence_cursor;
    let (start, end) = d
        .app()
        .beat_rows
        .iter()
        .find(|row| row.id == hold)
        .map(|row| (row.start, row.start + row.frames))
        .ok_or("The pause is not a visible beat")?;
    d.wait_for("Variant picture at the inspected frame", |app| {
        app.presentation.displayed_candidate()
            && !app.presentation.loading()
            && !app.presentation.needs_render()
    })?;
    d.step("Variant 2 inside the pause", false)?;
    d.check(
        ",x shows the chosen variant inside the pause and teaches the switch back",
        (start..end).contains(&frame)
            && widget_text(d).contains("AI PREVIEW · VARIANT 2 OF 2 · NOT SAVED")
            && widget_text(d).contains("show before"),
        json!({"footer":"AI PREVIEW · VARIANT 2 OF 2 · NOT SAVED","hint":"show before"}),
        json!({"frame":frame,"pause":[start,end],"snapshot":d.snapshot()}),
    )?;
    d.capture("Comparing: variant 2")?;

    d.chord(&[Key::Comma, Key::X])?;
    d.check(
        ",x switches to Before at once, at the same frame, without an edit",
        d.app().ai.comparing_before()
            && d.app().sequence_cursor == frame
            && d.app().ai.preview_attempt() == Some(&second)
            && d.revision() == paused,
        json!({"before":true,"frame":frame}),
        json!({"before":d.app().ai.comparing_before(),"frame":d.app().sequence_cursor}),
    )?;
    d.wait_for("Committed picture at the same frame", |app| {
        !app.presentation.displayed_candidate()
            && !app.presentation.loading()
            && !app.presentation.needs_render()
    })?;
    d.step("Before at the same frame", false)?;
    d.check(
        "Before shows the committed pause picture and names the comparison",
        widget_text(d).contains("AI COMPARE · BEFORE · NOT SAVED")
            && widget_text(d).contains("Showing Before")
            && d.app().sequence_cursor == frame,
        json!({"footer":"AI COMPARE · BEFORE · NOT SAVED"}),
        json!({"snapshot":d.snapshot()}),
    )?;
    d.capture("Comparing: Before")?;
    d.chord(&[Key::Comma, Key::X])?;
    d.wait_for("Variant picture again", |app| {
        !app.ai.comparing_before()
            && app.presentation.displayed_candidate()
            && !app.presentation.loading()
            && !app.presentation.needs_render()
    })?;
    d.check(
        ",x returns to the variant at the same frame",
        d.app().sequence_cursor == frame,
        json!({"frame":frame}),
        json!({"frame":d.app().sequence_cursor}),
    )?;

    // Auditioning: every switch continues from the exact heard sample.
    let workspace = d.app().workspace.clone().ok_or("No project")?;
    let rate = workspace.document.presentation_basis().frame_rate;
    let inside = rate
        .audio_boundary(deadpan_core::ProjectFrame(
            i64::try_from(frame).map_err(|error| error.to_string())?,
        ))
        .map_err(|error| error.to_string())?;
    let inside = deadpan_core::AudioSample(inside.0 + 137);
    d.app_mut().feedback.simulate_playback = true;
    d.command("audition-ai")?;
    d.step("Audition started", false)?;
    let window = *d.app().transport.as_ref().ok_or("No audition")?.window();
    let (mut feed, _callback) = deadpan_output::channel().map_err(|error| error.to_string())?;
    let generation = feed
        .restart(window.start().0)
        .map_err(|error| error.to_string())?;
    let update = {
        let run = d.app().transport.as_ref().ok_or("No audition")?;
        deadpan_playback::Update {
            ticket: run.ticket,
            session: run.session,
            project_id: run.project.clone(),
            revision_id: run.revision.clone(),
            content: run.content.clone(),
            phase: deadpan_playback::Phase::Playing,
            sample: Some(inside),
            generation: Some(generation),
            error: None,
        }
    };
    d.app_mut().feedback.playback_updates.push_back(update);
    d.wait_for("Heard inside the pause", |app| {
        app.sequence_cursor == frame
            && app
                .transport
                .as_ref()
                .is_some_and(|run| run.content_sample().ok() == Some(inside))
    })?;
    let variant_content = d.app().ai.preview_content().cloned();
    d.chord(&[Key::Comma, Key::X])?;
    d.step("Before while auditioning", false)?;
    let now = heard(d);
    d.check(
        ",x while auditioning plays Before from the same heard sample in the same window",
        now.as_ref().is_some_and(|(content, now_window, sample)| {
            *content == ContentIdentity::Committed && *now_window == window && *sample == inside
        }) && d.app().ai.comparing_before()
            && d.app().sequence_cursor == frame,
        json!({"content":"Committed","sample":inside.0,"window":format!("{window:?}")}),
        json!({"heard":format!("{now:?}"),"frame":d.app().sequence_cursor}),
    )?;
    d.capture("Auditioning Before at the heard sample")?;
    d.chord(&[Key::Comma, Key::X])?;
    let now = heard(d);
    d.check(
        ",x again plays the variant from the same heard sample",
        now.as_ref().is_some_and(|(content, now_window, sample)| {
            Some(content) == variant_content.as_ref() && *now_window == window && *sample == inside
        }),
        json!({"content":format!("{variant_content:?}"),"sample":inside.0}),
        json!({"heard":format!("{now:?}")}),
    )?;
    d.chord(&[Key::Comma, Key::N])?;
    d.wait_for("Variant 1 admitted and continuing", |app| {
        app.ai
            .preview_attempt()
            .is_some_and(|attempt| attempt != &second)
            && app
                .transport
                .as_ref()
                .is_some_and(|run| Some(&run.content) == app.ai.preview_content())
    })?;
    let first = d.app().ai.preview_attempt().cloned().ok_or("No preview")?;
    let now = heard(d);
    d.check(
        ",n prepares the next variant and continues the audition from the same heard sample",
        now.as_ref()
            .is_some_and(|(_, now_window, sample)| *now_window == window && *sample == inside)
            && d.app().ai.retained_previews() == 1
            && d.app()
                .ai
                .chosen_variant()
                .is_some_and(|(number, _)| number == 1),
        json!({"variant":1,"sample":inside.0,"retained":1}),
        json!({"heard":format!("{now:?}"),"retained":d.app().ai.retained_previews()}),
    )?;
    d.capture("Auditioning variant 1 at the heard sample")?;
    d.chord(&[Key::Comma, Key::N])?;
    let now = heard(d);
    d.check(
        ",n back to an already admitted variant switches at once at the same heard sample",
        d.app().ai.preview_attempt() == Some(&second)
            && now.as_ref().is_some_and(|(content, now_window, sample)| {
                Some(content) == d.app().ai.preview_content()
                    && *now_window == window
                    && *sample == inside
            }),
        json!({"variant":2,"sample":inside.0}),
        json!({"preview":format!("{:?}", d.app().ai.preview_attempt()),"heard":format!("{now:?}")}),
    )?;
    d.wait_for("The store records the chosen variant", |app| {
        app.ai
            .chosen_variant()
            .is_some_and(|(_, attempt)| attempt == second)
    })?;
    // Two `,n` in one input batch advance twice from what is shown, before
    // the store's selection reply: 2 -> 1 -> 2.
    let key = |key, pressed| super::key_event(key, egui::Modifiers::NONE, pressed);
    d.events(
        "Batched ,n,n",
        vec![
            key(Key::Comma, true),
            key(Key::Comma, false),
            key(Key::N, true),
            key(Key::N, false),
            key(Key::Comma, true),
            key(Key::Comma, false),
            key(Key::N, true),
            key(Key::N, false),
        ],
    )?;
    let now = heard(d);
    d.check(
        "Batched ,n,n advance twice from the shown variant at the same heard sample",
        d.app().ai.preview_attempt() == Some(&second)
            && now
                .as_ref()
                .is_some_and(|(_, now_window, sample)| *now_window == window && *sample == inside),
        json!({"variant":2,"sample":inside.0}),
        json!({"preview":format!("{:?}", d.app().ai.preview_attempt()),"heard":format!("{now:?}")}),
    )?;
    d.wait_for("The store records the twice-advanced variant", |app| {
        app.ai
            .chosen_variant()
            .is_some_and(|(_, attempt)| attempt == second)
    })?;

    // Paused: the exact sample survives the switch and Space resumes there.
    d.command("audition-ai")?;
    d.step("Audition paused", false)?;
    let paused_at = d.app().resume.is_some() && d.app().transport.is_none();
    d.chord(&[Key::Comma, Key::X])?;
    let retargeted = d.app().resume.as_ref().is_some_and(|resume| {
        resume.shows(
            workspace.document.revision_id(),
            &ContentIdentity::Committed,
        )
    });
    d.key(Key::Space)?;
    let now = heard(d);
    d.check(
        "A paused comparison keeps its exact sample: Before resumes there with Space",
        paused_at
            && retargeted
            && now.as_ref().is_some_and(|(content, _, sample)| {
                *content == ContentIdentity::Committed && *sample == inside
            }),
        json!({"paused":true,"retargeted":true,"resumed":inside.0}),
        json!({"paused":paused_at,"retargeted":retargeted,"heard":format!("{now:?}")}),
    )?;
    d.app_mut().feedback.simulate_playback = false;
    d.key(Key::Escape)?;
    d.key(Key::Escape)?;
    d.wait_for("Back to your edit", |app| {
        app.ai.preview_request().is_none() && !app.ai.comparing_before()
    })?;
    d.check(
        "Esc leaves the comparison without an edit; both variants stay offered",
        d.revision() == paused && d.app().ai.variant_count() == 2 && first != second,
        json!({"revision":paused,"offered":2}),
        json!({"revision":d.revision(),"offered":d.app().ai.variant_count()}),
    )?;
    d.capture("Comparison closed")?;
    Ok(())
}
