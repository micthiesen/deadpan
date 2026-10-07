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
    d.command("hold-provider ai")?;
    d.check(
        ":hold-provider ai on a picture beat explains that AI pictures fill a pause",
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
        "A selected pause teaches the existing-pause provider command in the inspector",
        hold.is_some()
            && widgets.contains("Generate AI pictures")
            && widgets.contains("AI PICTURES")
            && widgets.contains(":hold-provider ai"),
        json!({"hold_selected":true,"inspector":"Generate AI pictures  :hold-provider ai"}),
        json!({"hold":hold,"widgets_have_action":widgets.contains("Generate AI pictures")}),
    )?;
    d.capture("Pause selected with its AI pictures action")?;

    d.command("generate motion=moderate text=Keep the eyes open.")?;
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
            && widgets.contains("Requested motion: moderate")
            && widgets.contains("Region target: None")
            && widgets.contains("Guidance: Keep the eyes open.")
            && d.revision() == paused,
        json!({"title":"AI pauses are unavailable on this Mac","reason":UNAVAILABLE,"revision":paused,"motion":"moderate","guidance":"Keep the eyes open."}),
        json!({"outcome":format!("{:?}", outcome(d)),"revision":d.revision()}),
    )?;
    d.capture("Unavailable model runtime")?;

    d.command("generate motion=subtle target=none text=Keep the hands still.")?;
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
    d.check(
        "Motion and guidance are visible and captured by the running job",
        widgets.contains("Requested motion: subtle")
            && widgets.contains("Region target: None")
            && widgets.contains("Guidance: Keep the hands still.")
            && d.app().ai.job().is_some_and(|job| {
                job.options.motion == deadpan_jobs::MotionAmount::Subtle
                    && job.options.instructions.as_ref().map(|text| text.as_str())
                        == Some("Keep the hands still.")
            }),
        json!({"motion":"subtle","instructions":"Keep the hands still."}),
        json!({"job": format!("{:?}", d.app().ai.job())}),
    )?;
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
    d.command("hold-provider ai")?;
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
    d.check(
        "Keyboard retry retains the current motion and guidance",
        d.app().ai.job().is_some_and(|job| {
            job.options.motion == deadpan_jobs::MotionAmount::Subtle
                && job.options.instructions.as_ref().map(|text| text.as_str())
                    == Some("Keep the hands still.")
        }),
        json!({"motion":"subtle","instructions":"Keep the hands still."}),
        json!({"job": format!("{:?}", d.app().ai.job())}),
    )?;
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

/// A scoped AI command must keep its authored target separate from the
/// concrete Repeat picture used for preview. All setup and actions here use
/// the production keyboard router; only model inference is synthetic.
pub(super) fn scoped(d: &mut Driver<'_>) -> Result<(), String> {
    if let Err(reason) = crate::project::generation::synthetic_tools() {
        return Err(format!(
            "ai-scoped requires the synthetic Ready worker's tools: {reason}"
        ));
    }
    d.report.skipped.push("Model inference is synthetic boundary-blend footage. Conditioning, qualification, request history, scoped keyboard navigation, Metal preview and acceptance are real. This scenario opens no audio device.".into());
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G])?;
    let before = d.revision();
    d.command("group name=\"AI local\"")?;
    d.changed(&before)?;
    d.settled()?;
    let group = d
        .app()
        .selected_beat
        .clone()
        .ok_or("The new group is not selected")?;
    d.key(Key::Enter)?;
    d.chord(&[Key::Num1, Key::Num0, Key::L])?;
    let before = d.revision();
    d.command("hold 12f")?;
    d.changed(&before)?;
    d.settled()?;
    let hold = d
        .app()
        .ai_hold()
        .ok_or("The authored local Hold is not selected")?;
    let local_frames = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No workspace")?
        .plan
        .node_duration(&group)
        .ok_or("No local group duration")?
        .frames();
    d.key(Key::Backspace)?;
    let before = d.revision();
    d.command("wrap-repeat 2")?;
    d.changed(&before)?;
    d.settled()?;
    let repeat = d
        .app()
        .selected_beat
        .clone()
        .ok_or("The Repeat is not selected")?;
    let baseline = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No workspace")?
        .document
        .clone();
    d.key(Key::Enter)?;
    scoped_play_hold(d, 2)?;
    d.settled()?;
    let capture = d.app().scoped_target()?.ok_or("No scoped Hold capture")?;
    let target = capture.target.clone();
    let picture = d
        .app()
        .scoped_presentation()?
        .ok_or("Play two Hold has no picture")?;
    let expected_start = u64::try_from(local_frames + 10).map_err(|error| error.to_string())?;
    d.check(
        "Entering the local Hold captures Play 2 and its exact visible picture interval",
        target.node == hold
            && target.repeats.last().is_some_and(|step| matches!(&step.branch,
                deadpan_core::RepeatEditBranch::Play { iteration } if iteration.ordinal == 1))
            && picture.frames == (expected_start..expected_start + 12)
            && d.app().sequence_cursor == expected_start
            && d.app().workspace.as_ref().is_some_and(|workspace| workspace.document == baseline),
        json!({"hold":hold,"play":2,"range":[expected_start,expected_start+12],"history":"unchanged"}),
        json!({"target":capture.target,"presentation":capture.presentation,"range":picture.frames,"cursor":d.app().sequence_cursor}),
    )?;
    for label in [
        "AI PICTURES",
        "12 pictures before Repeat / Retime",
        "Joins measured in this definition.",
    ] {
        scoped_painted(d, label)?;
    }
    d.check(
        "The nested Hold inspector exposes the real generation action",
        widget_text(d).contains("Generate AI pictures")
            && widget_text(d).contains(":hold-provider ai"),
        json!("Generate AI pictures  :hold-provider ai"),
        d.widgets(),
    )?;
    d.capture("Play two Hold exposes scoped AI controls")?;

    let unchanged = d.revision();
    d.command("generate 2")?;
    d.wait_for("Scoped variants reach Ready", |app| {
        app.ai.job().is_some_and(|job| !job.running()) && app.ai.variant_count() == 2
    })?;
    let job = d
        .app()
        .ai
        .job()
        .cloned()
        .ok_or("Scoped generation has no job")?;
    let request_id = job
        .request
        .clone()
        .ok_or("Scoped job has no retained request")?;
    let request = scoped_request(d, &request_id)?;
    d.check(
        "Generation retains Play 2, all 12 intrinsic pictures and the immutable worker target",
        job.target == target && matches!(job.outcome, Some(Outcome::Ready(_)))
            && request.target == target && request.origin_target == target
            && request.constraints.video.frames().frames() == 12 && d.revision() == unchanged,
        json!({"target":target,"frames":12,"revision":unchanged}),
        json!({"target":request.target,"origin_target":request.origin_target,"frames":request.constraints.video.frames().frames(),"revision":d.revision()}),
    )?;
    d.check(
        "Ready variants in the nested inspector expose Preview and Accept",
        widget_text(d).contains("Ready · 2 variants")
            && widget_text(d).contains(":preview-ai")
            && widget_text(d).contains(":accept-ai"),
        json!(["Ready · 2 variants", ":preview-ai", ":accept-ai"]),
        d.widgets(),
    )?;

    // Hold the actual service reply while keyboard navigation changes scope.
    // A late reply must never pull the viewer back to the old play.
    d.app_mut().feedback.hold_project_updates = true;
    d.command("preview-ai")?;
    d.wait_for(
        "Scoped preview is prepared while its reply is withheld",
        |app| !app.service.is_busy(),
    )?;
    scoped_play_hold(d, 1)?;
    let other_cursor = d.app().sequence_cursor;
    d.app_mut().feedback.hold_project_updates = false;
    d.settled()?;
    d.check(
        "A delayed preview cannot reclaim the previous play after scoped navigation",
        d.app().ai.preview_request().is_none()
            && d.app().ai.retained_previews() == 0
            && !d.app().presentation.displayed_candidate()
            && d.app().sequence_cursor == other_cursor
            && other_cursor == 10,
        json!({"preview":null,"Edit":10}),
        d.snapshot(),
    )?;
    scoped_play_hold(d, 2)?;
    d.command("preview-ai")?;
    d.wait_for("Scoped candidate is displayed at Play 2", |app| {
        app.ai.preview_request() == Some(&request_id)
            && app.presentation.displayed_candidate()
            && !app.presentation.loading()
            && !app.presentation.needs_render()
    })?;
    let workspace = d.app().workspace.as_ref().ok_or("No workspace")?;
    let proposed = d
        .app()
        .ai_preview_audio(workspace)
        .ok_or("No proposed scoped preview")?;
    let generated = proposed.document.nodes().values().any(|node| {
        matches!(&node.kind,
        NodeKind::Hold { recipe } if matches!(recipe.video, HoldVideo::Generated { .. }))
    });
    d.check(
        "Preview shows the proposed Play 2 provider at its captured root frame without saving",
        generated
            && d.app().sequence_cursor == expected_start
            && d.revision() == unchanged
            && proposed.document.nodes()[&hold] == baseline.nodes()[&hold]
            && proposed
                .document
                .overrides()
                .get(&repeat)
                .is_some_and(|overrides| overrides.len() == 1),
        json!({"Edit":expected_start,"overrides":1,"shared_hold":"unchanged","revision":unchanged}),
        json!({"Edit":d.app().sequence_cursor,"generated":generated,"revision":d.revision()}),
    )?;
    d.capture("Play two candidate preview before acceptance")?;
    scoped_play_hold(d, 1)?;
    d.settled()?;
    d.check(
        "Changing plays revokes the admitted preview and all cached variants",
        d.app().ai.preview_request().is_none()
            && d.app().ai.retained_previews() == 0
            && !d.app().presentation.displayed_candidate()
            && d.app().sequence_cursor == 10,
        json!({"preview":null,"retained":0,"Edit":10}),
        d.snapshot(),
    )?;
    scoped_play_hold(d, 2)?;
    d.command("preview-ai")?;
    d.wait_for("Play two preview is prepared again", |app| {
        app.ai.preview_request() == Some(&request_id)
            && app.presentation.displayed_candidate()
            && !app.presentation.loading()
            && !app.presentation.needs_render()
    })?;
    let before = d.revision();
    d.command("accept-ai")?;
    d.changed(&before)?;
    d.settled()?;
    let mapped = d
        .app()
        .scoped_target()?
        .ok_or("Acceptance lost scoped navigation")?;
    let current = scoped_request(d, &request_id)?;
    let document = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No workspace")?
        .document
        .clone();
    d.check(
        "Accept isolates only Play 2 and follows its mapped Hold and request",
        mapped.target.node != hold && mapped.target.repeats == target.repeats
            && mapped.presentation.as_ref().map(|path| &path.repeats) == Some(&picture.instance.repeats)
            && matches!(&document.nodes()[&mapped.target.node].kind,
                NodeKind::Hold { recipe } if matches!(recipe.video, HoldVideo::Generated { .. }))
            && document.nodes()[&hold] == baseline.nodes()[&hold]
            && document.overrides().get(&repeat).is_some_and(|overrides| overrides.len() == 1)
            && current.target == mapped.target && current.origin_target == target
            && d.app().sequence_cursor == expected_start && d.app().ai.preview_request().is_none(),
        json!({"mapped_target":mapped.target,"origin_target":target,"overrides":1,"shared_hold":"unchanged"}),
        json!({"target":current.target,"origin_target":current.origin_target,"cursor":d.app().sequence_cursor}),
    )?;
    d.capture("Accepted Play two keeps the mapped Hold inspector")?;
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    d.settled()?;
    let undone = scoped_request(d, &request_id)?;
    d.check(
        "Undo closes scoped inspection and restores the original request target",
        d.app().scoped.is_none()
            && undone.target == target
            && undone.origin_target == target
            && d.app()
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.document.overrides().is_empty())
            && d.app().ai.variant_count() == 2,
        json!({"scope":null,"request_target":target,"variants":2}),
        d.snapshot(),
    )?;
    let before = d.revision();
    d.key_modified(Key::R, egui::Modifiers::CTRL)?;
    d.changed(&before)?;
    d.settled()?;
    let redone = scoped_request(d, &request_id)?;
    d.check(
        "Redo restores the mapped request while immutable worker scope remains original",
        d.app().scoped.is_none()
            && redone.target == mapped.target
            && redone.origin_target == target
            && d.app().ai.variant_count() == 1,
        json!({"scope":null,"request_target":mapped.target,"origin_target":target,"variants":1}),
        d.snapshot(),
    )?;
    d.capture("Scoped acceptance and request mapping survive Redo")?;
    super::transcript::focus_your_edit(d)?;
    select_beat(d, &repeat)?;
    d.key(Key::Enter)?;
    scoped_play_hold(d, 2)?;
    let before = d.revision();
    d.command("gain +3dB")?;
    d.changed(&before)?;
    d.settled()?;
    let captured = d
        .app()
        .scoped_target()?
        .ok_or("No scoped Hold capture for reversion")?;
    let before = d.app().workspace.clone().ok_or("No workspace")?;
    let revision = d.revision();
    d.command("revert-ai")?;
    d.changed(&revision)?;
    d.settled()?;
    let after = d.app().workspace.clone().ok_or("No workspace")?;
    let mut expected = before.document.nodes()[&captured.target.node].clone();
    let NodeKind::Hold { recipe } = &mut expected.kind else {
        return Err("Scoped Hold missing".into());
    };
    let NodeKind::Hold { recipe: original } = &baseline.nodes()[&hold].kind else {
        return Err("Original Hold missing".into());
    };
    recipe.video = original.video.clone();
    d.check(
        "Revert restores only Play 2's saved fallback after an intervening gain edit",
        after.document.nodes()[&captured.target.node] == expected
            && after.document.nodes()[&hold] == baseline.nodes()[&hold]
            && after.document.overrides().len() == 1
            && after.plan.duration() == before.plan.duration(),
        json!({"play":2,"fallback":true,"other_play":"unchanged","gain":"retained","overrides":1}),
        d.snapshot(),
    )?;
    let reverted = d.revision();
    d.key(Key::U)?;
    d.changed(&reverted)?;
    d.settled()?;
    d.check(
        "One Undo restores scoped accepted pictures and retains the gain edit",
        d.app()
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.document.nodes() == before.document.nodes()),
        json!({"provider":"accepted","gain":"retained"}),
        d.snapshot(),
    )?;
    d.capture("Scoped provider reversion is one undoable edit")
}

fn scoped_request(
    d: &Driver<'_>,
    request: &deadpan_jobs::RequestId,
) -> Result<deadpan_store::generation::StoredGenerationRequest, String> {
    let workspace = d.app().workspace.as_ref().ok_or("No workspace")?;
    let store =
        deadpan_store::ProjectStore::open(&workspace.path, deadpan_store::AccessMode::ReadOnly)
            .map_err(|error| error.to_string())?;
    store
        .generation_request(request)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "The scoped generation request is missing".into())
}

fn scoped_play_hold(d: &mut Driver<'_>, play: u32) -> Result<(), String> {
    d.command(&format!("scope play {play}"))?;
    d.key(Key::Enter)?;
    d.key(Key::J)
}

fn scoped_painted(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    let parts = scenarios::text_paint_visibility(d, label);
    d.check(
        &format!("Scoped AI text is fully painted: {label}"),
        !parts.is_empty() && parts.iter().all(|part| part["fully_visible"] == true),
        json!("visible inside the nested inspector paint clip"),
        json!(parts),
    )
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
    let timing_label = d
        .harness
        .root()
        .children_recursive()
        .find_map(|node| {
            node.accesskit_node()
                .label()
                .filter(|label| label.starts_with("Variant 2 timing"))
                .map(|label| label.to_string())
        })
        .ok_or("Chosen variant timing label is missing")?;
    let timing_paint = scenarios::text_paint_visibility(d, &timing_label);
    d.check(
        "Timing follows the chosen Ready variant and is fully visible",
        widgets.contains("Variant 2 timing")
            && !widgets.contains("Variant 1 timing")
            && job.as_ref().is_some_and(|job| job.plan.is_some())
            && !timing_paint.is_empty()
            && timing_paint
                .iter()
                .all(|item| item["fully_visible"] == true && item["elided"] == false),
        json!({"timing":"variant 2","plan":"admitted","fully_visible":true}),
        json!({"widgets":widgets,"plan":job.as_ref().and_then(|job| job.plan.as_ref()),"paint":timing_paint}),
    )?;
    d.capture("Two variants with thumbnails")?;

    d.wait_for("Chosen variant quality report read", |app| {
        app.ai.chosen_quality().is_some()
    })?;
    d.step("Quality coverage shown", false)?;
    let quality = d.app().ai.chosen_quality().unwrap_or_default();
    let compact = d.app().ai.chosen_quality_compact().unwrap_or_default();
    let paint = scenarios::text_paint_visibility(d, &compact);
    d.check(
        "The chosen variant visibly reports measured quality coverage before acceptance",
        quality.starts_with("Motion/lighting sampled;")
            && quality.contains("Both edit joins checked for gross discontinuity.")
            && quality.contains(
                "Face geometry unavailable: no reliable track connects both input pictures.",
            )
            && quality.contains("Mouth motion unavailable: no reliable eye and lip track.")
            && quality.contains("Selected-region check unavailable:")
            && compact.starts_with("Motion coverage ")
            && widget_text(d).contains(&quality)
            && !paint.is_empty()
            && paint
                .iter()
                .all(|item| item["fully_visible"] == true && item["elided"] == false),
        json!({"quality":"Motion/lighting sampled; audition before accepting."}),
        json!({"quality":quality,"compact":compact,"paint":paint}),
    )?;
    d.capture("Chosen coverage revealed inside the inspector")?;

    let motion_top = |d: &Driver<'_>| {
        scenarios::text_paint_visibility(d, "Requested motion: still")
            .first()
            .and_then(|paint| paint["bounds"][1].as_f64())
    };
    let before_scroll = motion_top(d).ok_or("Missing generation controls paint")?;
    d.events(
        "Scroll the inspector back to the top after revealing a candidate",
        vec![
            egui::Event::PointerMoved(egui::pos2(1200.0, 450.0)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, 2048.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    )?;
    for _ in 0..8 {
        d.step("Candidate inspector scroll settles", false)?;
    }
    let after_scroll = motion_top(d).ok_or("Missing scrolled generation controls paint")?;
    d.check(
        "Revealing a chosen variant leaves subsequent inspector scrolling alone",
        after_scroll > before_scroll + 1.0 && chosen(d) == Some(2) && d.revision() == paused,
        json!({"native_scroll":true,"chosen":2,"revision":paused}),
        json!({"before_y":before_scroll,"after_y":after_scroll,"chosen":chosen(d),"revision":d.revision()}),
    )?;

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
    d.step("First variant timing shown", false)?;
    d.check(
        "Choosing a variant also chooses its timing report",
        widget_text(d).contains("Variant 1 timing") && !widget_text(d).contains("Variant 2 timing"),
        json!({"timing":"variant 1"}),
        json!({"widgets":widget_text(d)}),
    )?;
    let focused_heading = {
        let root = d.harness.root();
        let control = root
            .children_recursive()
            .find(|node| {
                let access = node.accesskit_node();
                access
                    .label()
                    .is_some_and(|label| label.starts_with("Variant 1 timing"))
                    && !access.is_disabled()
                    && !access.is_hidden()
            })
            .ok_or("Chosen variant timing control is missing")?;
        let label = control
            .accesskit_node()
            .label()
            .ok_or("Timing control has no label")?
            .to_owned();
        control.focus();
        label
    };
    d.step("Focus the chosen variant timing details", false)?;
    for _ in 0..16 {
        if timing_control_visible(d, &focused_heading) {
            break;
        }
        d.step("Settle native timing focus scroll", false)?;
    }
    let heading_paint = scenarios::text_paint_visibility(d, &focused_heading);
    d.check(
        "Native focus reveals the whole chosen timing heading and its hit target",
        timing_control_visible(d, &focused_heading),
        json!({"heading":"Variant 1 timing","fully_painted":true,"hit_target_visible":true}),
        json!({"paint":heading_paint,"hit_target":d.rect(&focused_heading).ok().map(|rect| [rect.min.x,rect.min.y,rect.max.x,rect.max.y])}),
    )?;
    d.key(Key::Enter)?;
    d.step("Exact timing expanded with Enter", false)?;
    let intervals = [
        "Inserted pause:",
        "Requested boundary span:",
        "Native boundary span:",
        "Native movie:",
        "Motion speed:",
    ];
    for _ in 0..16 {
        if intervals.iter().all(|label| timing_text_visible(d, label)) {
            break;
        }
        d.step("Settle expanded timing report and inspector scroll", false)?;
    }
    let details = widget_text(d);
    let interval_paint: Vec<_> = intervals
        .iter()
        .map(|label| json!({"interval":label,"paint":scenarios::text_paint_visibility(d, label)}))
        .collect();
    d.check(
        "Timing details opened with Enter visibly distinguish all exact intervals",
        details.contains("Inserted pause:")
            && details.contains("Requested boundary span:")
            && details.contains("Native boundary span:")
            && details.contains("Native movie:")
            && details.contains("Motion speed:")
            && intervals.iter().all(|label| timing_text_visible(d, label))
            && timing_control_visible(d, &focused_heading)
            && d.revision() == paused,
        json!({"intervals":["inserted","requested boundary","native boundary","native movie","motion speed"],"fully_painted":true,"unchanged":true}),
        json!({"interval_paint":interval_paint,"heading_paint":scenarios::text_paint_visibility(d, &focused_heading),"revision":d.revision()}),
    )?;
    d.capture("Keyboard-expanded AI timing report with exact visible intervals")?;
    d.key(Key::Enter)?;
    super::transcript::focus_your_edit(d)?;
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
    d.step("Accepted timing retained", false)?;
    d.check(
        "Acceptance reports committed timing separately from the remaining candidate",
        widget_text(d).contains("Accepted timing") && widget_text(d).contains("Variant 1 timing"),
        json!({"timing":["accepted","offered variant 1"]}),
        json!({"widgets":widget_text(d)}),
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
    revert_after_edit(d, &hold)
}

fn timing_text_visible(d: &Driver<'_>, label: &str) -> bool {
    let paint = scenarios::text_paint_visibility(d, label);
    !paint.is_empty()
        && paint
            .iter()
            .all(|part| part["fully_visible"] == true && part["elided"] == false)
}

fn timing_control_visible(d: &Driver<'_>, label: &str) -> bool {
    let Ok(rect) = d.rect(label) else {
        return false;
    };
    timing_text_visible(d, label)
        && d.harness.output().shapes.iter().any(|clipped| {
            matches!(&clipped.shape, egui::Shape::Text(text) if text.galley.text() == label)
                && clipped.clip_rect.contains_rect(rect.shrink(1.0))
        })
}

fn revert_after_edit(d: &mut Driver<'_>, hold: &NodeId) -> Result<(), String> {
    let before = d.revision();
    d.key_modified(Key::R, egui::Modifiers::CTRL)?;
    d.changed(&before)?;
    d.settled()?;
    select_beat(d, hold)?;
    let accepted = d.revision();
    d.check(
        "Accepted pictures teach the explicit fallback command",
        widget_text(d).contains("Restore fallback") && widget_text(d).contains(":revert-ai"),
        json!("Restore fallback  :revert-ai"),
        d.widgets(),
    )?;
    // Open the command first, then let an independent typed edit finish while
    // its UI reply is held. A native semantic edit would retain its pending
    // acknowledgment and correctly prevent another command from opening.
    let workspace = d.app().workspace.clone().ok_or("No workspace")?;
    let mut gain =
        crate::gain::GainEdit::new(workspace.document.nodes()[hold].audio_treatments.clone());
    gain.set_trim(deadpan_core::GainDb::new(3000).map_err(|error| error.to_string())?)?;
    let independent_edit = ProjectRequest::Edit {
        expected_session: workspace.session,
        expected_revision: workspace.document.revision_id().clone(),
        cursor: ProjectFrame(
            i64::try_from(d.app().sequence_cursor).map_err(|error| error.to_string())?,
        ),
        scope: d.app().sequence_scope.clone(),
        edit: crate::project::ProjectEdit::SetAudioTreatments {
            node: hold.clone(),
            treatments: gain.recipe().clone(),
        },
    };
    d.key(Key::Colon)?;
    d.events(
        "Type revert-ai before an independent gain edit",
        vec![egui::Event::Text("revert-ai".into())],
    )?;
    d.check("Revert command captures the accepted pause before the independent edit",
        d.app().command_open && d.app().command == "revert-ai" && d.revision() == accepted,
        json!({"command":"revert-ai","revision":accepted}),
        json!({"command_open":d.app().command_open,"command":d.app().command,"revision":d.revision()}))?;
    d.app_mut().feedback.hold_project_updates = true;
    d.app().service.submit(independent_edit)?;
    d.wait_for("Gain saved before its UI reply", |app| {
        !app.service.is_busy()
    })?;
    d.app_mut().feedback.hold_project_updates = false;
    d.changed(&accepted)?;
    d.settled()?;
    let before = d.app().workspace.clone().ok_or("No workspace")?;
    d.key(Key::Enter)?;
    d.wait_for(
        "The stale provider refusal reaches the native project feedback",
        |app| {
            !app.service.is_busy()
                && app
                    .project_error
                    .as_deref()
                    .is_some_and(|error| error.contains("Project changed"))
        },
    )?;
    let generated = matches!(&before.document.nodes()[hold].kind,
        NodeKind::Hold { recipe } if matches!(recipe.video, HoldVideo::Generated { .. }));
    d.check(
        "A stale provider command cannot apply after the captured revision changes",
        generated
            && d.app().workspace.as_ref().is_some_and(|workspace| workspace.document == before.document)
            && d.app()
                .project_error
                .as_deref()
                .is_some_and(|error| error.contains("Project changed")),
        json!({"provider":"accepted","revision":before.document.revision_id(),"project_error":"Project changed"}),
        json!({"generated":generated,"revision":d.revision(),"project_error":d.app().project_error}),
    )?;
    let revision = d.revision();
    d.command("hold-provider fallback")?;
    d.changed(&revision)?;
    d.settled()?;
    let after = d.app().workspace.clone().ok_or("No workspace")?;
    let NodeKind::Hold { recipe: prior } = &before.document.nodes()[hold].kind else {
        return Err("Hold missing".into());
    };
    let NodeKind::Hold { recipe: restored } = &after.document.nodes()[hold].kind else {
        return Err("Hold missing".into());
    };
    let mut preserved = restored.clone();
    preserved.video = prior.video.clone();
    d.check(
        "The fallback command changes only the accepted picture provider after a gain edit",
        matches!(restored.video, HoldVideo::Freeze { .. })
            && preserved == *prior
            && before.document.nodes()[hold].audio_treatments
                == after.document.nodes()[hold].audio_treatments
            && before.plan.duration() == after.plan.duration(),
        json!({"provider":"freeze","timing":"unchanged","gain":"retained"}),
        d.snapshot(),
    )?;
    let reverted = d.revision();
    d.key(Key::U)?;
    d.changed(&reverted)?;
    d.settled()?;
    d.check(
        "One Undo restores accepted pictures while retaining the intervening edit",
        d.app()
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.document.nodes() == before.document.nodes()),
        json!({"provider":"accepted","gain":"retained"}),
        d.snapshot(),
    )?;
    d.capture("Fallback reversion preserves later edits")
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
