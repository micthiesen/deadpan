//! AI pause pictures through the production keys, commands and inspector.
//!
//! `ai-pause` replaces only the model worker with the scripted test seam
//! (`project::generation::Backend::Scripted`): conditioning, request
//! allocation and every durable transition are real, and the script can never
//! produce Ready pictures. `ai-pause-ready` opens a private copy of a real
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
