//! Accepted pause extension and durable replacement recovery through the
//! production keyboard router. Only model inference is scripted.
use super::*;
use crate::project::generation::{Backend, Outcome, Script, ScriptEnding, ScriptQueue};
use deadpan_core::HoldVideo;
use deadpan_store::generation_preparations::PreparationState;
use egui::{Key, Modifiers};

mod boundary_performance;

pub(super) fn backend() -> Backend {
    Backend::Scripted(Arc::new(ScriptQueue::new([
        Script {
            unavailable: None,
            steps: 2,
            step_interval: Duration::from_millis(20),
            ending: ScriptEnding::Ready,
        },
        Script {
            unavailable: Some(
                "The replacement replay model is unavailable. Install the local model, then Retry."
                    .into(),
            ),
            steps: 0,
            step_interval: Duration::ZERO,
            ending: ScriptEnding::WaitForCancel,
        },
    ])))
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let hold = accepted_pause(d)?;
    let accepted = d.revision();
    d.command("hold-duration 18f")?;
    d.changed(&accepted)?;
    d.wait_for("Automatic replacement records its missing model", |app| {
        app.ai_preparations()
            .iter()
            .any(|item| item.state == PreparationState::Unavailable)
    })?;
    let extended = d.revision();
    let item = d
        .app()
        .ai_preparations()
        .into_iter()
        .next()
        .ok_or("Replacement recovery row missing")?;
    let workspace = d.app().workspace.as_ref().ok_or("No project")?;
    let fallback = matches!(&workspace.document.nodes()[&hold].kind, NodeKind::Hold { recipe }
        if recipe.duration.frames() == 18 && matches!(recipe.video, HoldVideo::Freeze { .. }));
    d.check("Extending accepted pictures saves 18 fallback frames and queues one replacement automatically",
        fallback && item.frames == 18 && item.target.node == hold && item.retryable(),
        json!({"frames":18,"fallback":true,"state":"unavailable"}),
        json!({"fallback":fallback,"replacement":format!("{item:?}")}))?;
    recovery(d, item, extended)
}

fn accepted_pause(d: &mut Driver<'_>) -> Result<deadpan_core::NodeId, String> {
    crate::project::generation::synthetic_tools()
        .map_err(|reason| format!("ai-replacements requires synthetic Ready tools: {reason}"))?;
    d.report.skipped.push("Inference uses qualified synthetic footage for the first accepted candidate; replacement preflight then reports a scripted missing model. Source and duration edits, durable queue, Retry and Discard are production paths.".into());
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    let before = d.revision();
    d.command("hold 12f")?;
    d.changed(&before)?;
    d.settled()?;
    let hold = d
        .app()
        .selected_beat
        .clone()
        .ok_or("Inserted pause not selected")?;
    d.command("generate motion=still target=none text=Keep the subject still.")?;
    d.wait_for("First pictures reach Ready", |app| {
        app.ai
            .job()
            .is_some_and(|job| matches!(job.outcome, Some(Outcome::Ready(_))))
    })?;
    let paused = d.revision();
    d.command("accept-ai")?;
    d.changed(&paused)?;
    d.settled()?;
    Ok(hold)
}

fn recovery(
    d: &mut Driver<'_>,
    item: crate::project::generation::Preparation,
    extended: String,
) -> Result<(), String> {
    d.command("jobs")?;
    d.step("Replacement recovery in Jobs", true)?;
    let widgets = d.widgets().to_string();
    d.check(
        "Jobs explains the unavailable replacement and teaches Retry R and Discard D",
        widgets.contains("AI PREPARATIONS")
            && widgets.contains(&format!("{} frames", item.frames))
            && widgets.contains("Unavailable; R retries")
            && widgets.contains("D discards")
            && widgets.contains("replacement replay model is unavailable"),
        json!(["AI PREPARATIONS", "Unavailable; R retries", "D discards"]),
        json!(widgets),
    )?;
    d.capture("Replacement unavailable with Retry and Discard")?;
    let target = super::super::jobs::RowKey::Preparation(item.id.clone());
    let index = d
        .app()
        .job_rows()
        .iter()
        .position(|row| row.key() == target)
        .ok_or("Replacement row missing")?;
    for _ in 0..index {
        d.key(Key::J)?;
    }
    d.key(Key::R)?;
    let sequence = item.sequence;
    d.wait_for(
        "Keyboard Retry performs a fresh bounded preparation",
        move |app| {
            app.ai_preparations()
                .iter()
                .any(|item| item.state == PreparationState::Unavailable && item.sequence > sequence)
        },
    )?;
    d.check(
        "Retry keeps the saved edit and the captured authoring target",
        d.revision() == extended
            && d.app()
                .ai_preparations()
                .iter()
                .any(|retried| retried.id == item.id && retried.target == item.target),
        json!({"revision":extended,"same_target":true}),
        json!({"revision":d.revision(),"rows":format!("{:?}", d.app().ai_preparations())}),
    )?;
    d.key(Key::D)?;
    d.wait_for(
        "Keyboard Discard removes the durable replacement row",
        |app| app.ai_preparations().is_empty(),
    )?;
    let workspace = d.app().workspace.as_ref().ok_or("No project")?;
    let store =
        deadpan_store::ProjectStore::open(&workspace.path, deadpan_store::AccessMode::ReadOnly)
            .map_err(|error| error.to_string())?;
    let state = store
        .generation_preparation(&item.id)
        .map_err(|error| error.to_string())?
        .ok_or("Discarded record missing")?
        .state;
    d.check(
        "Discard persists without undoing the saved edit",
        state == PreparationState::Cancelled && d.revision() == extended,
        json!({"state":"cancelled","revision":extended}),
        json!({"state":state,"revision":d.revision()}),
    )?;
    d.key(Key::Escape)?;
    d.settled()?;
    Ok(())
}

pub(super) fn boundaries(d: &mut Driver<'_>) -> Result<(), String> {
    let hold = accepted_pause(d)?;
    let before = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No project")?
        .document
        .clone();
    let NodeKind::Hold { recipe: original } = &before.nodes()[&hold].kind else {
        return Err("Accepted target is not a Hold".into());
    };
    let HoldVideo::Generated { accepted } = &original.video else {
        return Err("The initial AI candidate was not accepted".into());
    };
    let accepted = accepted.clone();

    d.click("Current group beat outline pane")?;
    d.chord(&[Key::G, Key::G])?;
    d.settled()?;
    let source = d
        .app()
        .selected_beat
        .clone()
        .ok_or("No neighboring Source selected")?;
    let first = before
        .children(before.root())
        .next()
        .cloned()
        .ok_or("No Source fragment before the pause")?;
    d.check(
        "Keyboard navigation selects the Source fragment before the accepted pause",
        source != hold && source == first,
        json!(first),
        d.snapshot(),
    )?;
    let resolution = before
        .source_slip(before.root(), &source, 1)
        .map_err(|error| error.to_string())?;
    d.check(
        "Neighboring Source has one frame of later material",
        resolution.applied_delta_frames == 1,
        json!(1),
        json!(resolution.applied_delta_frames),
    )?;
    d.command("slip +1f")?;
    d.wait_for("Changed boundary proposal has reached the GPU", |app| {
        !app.service.is_busy()
            && app
                .slip
                .as_ref()
                .is_some_and(|draft| draft.ready_for_check())
            && !app.presentation.loading()
            && !app.presentation.needs_render()
    })?;
    d.capture("Source Slip changes the pause's left conditioning picture")?;
    d.click("Slip preview keyboard controls")?;
    d.key(Key::Enter)?;
    d.changed(before.revision_id().as_str())?;
    d.wait_for(
        "Changed conditioning queues a replacement with a truthful model failure",
        |app| {
            app.ai_preparations()
                .iter()
                .any(|item| item.target.node == hold && item.state == PreparationState::Unavailable)
        },
    )?;
    let changed = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No project")?
        .document
        .clone();
    let first = d
        .app()
        .ai_preparations()
        .into_iter()
        .find(|item| item.target.node == hold)
        .ok_or("Boundary replacement missing")?;
    let fallback = matches!(&changed.nodes()[&hold].kind, NodeKind::Hold { recipe }
        if recipe.duration == original.duration && recipe.audio == original.audio
            && matches!(recipe.video, HoldVideo::Freeze { .. }));
    d.check(
        "One Slip commit changes the Source and saves a same-duration fallback with replacement work",
        changed.nodes()[&resolution.physical_source] != before.nodes()[&resolution.physical_source]
            && fallback && first.revision == *changed.revision_id()
            && first.frames == 12 && first.retryable(),
        json!({"source_changed":true,"hold_frames":12,"fallback":true,"same_revision":true}),
        json!({"fallback":fallback,"preparation":format!("{first:?}")}),
    )?;
    let path = d.app().workspace.as_ref().ok_or("No project")?.path.clone();
    let store = deadpan_store::ProjectStore::open(&path, deadpan_store::AccessMode::ReadOnly)
        .map_err(|error| error.to_string())?;
    let preparation = store
        .generation_preparation(&first.id)
        .map_err(|error| error.to_string())?
        .ok_or("Durable preparation disappeared")?;
    d.check(
        "Boundary replacement retains the exact accepted artifact and authored cause",
        matches!(&preparation.origin,
            deadpan_store::generation_preparations::PreparationOrigin::AcceptedBoundary { accepted: artifact, .. }
                if artifact.as_ref() == &accepted.artifact)
            && matches!(preparation.intent.cause,
                deadpan_store::generation_intents::IntentCause::SourceBoundaryChanged),
        json!("AcceptedBoundary with SourceBoundaryChanged"), json!(preparation),
    )?;
    store.validate_full().map_err(|error| error.to_string())?;
    drop(store);

    d.click("Current group beat outline pane")?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    d.settled()?;
    d.check(
        "Keyboard navigation displays the affected pause's fallback",
        d.app().selected_beat.as_ref() == Some(&hold) && d.app().sequence_cursor == 10,
        json!({"hold":hold,"cursor":10}),
        d.snapshot(),
    )?;
    d.capture("Affected pause shows its saved fallback while replacement is unavailable")?;

    d.key(Key::U)?;
    d.changed(changed.revision_id().as_str())?;
    d.settled()?;
    let undone = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No project")?
        .document
        .clone();
    d.check(
        "One Undo restores the Source and accepted AI pictures together",
        undone.nodes() == before.nodes()
            && undone.audio_bindings() == before.audio_bindings()
            && d.app().ai_preparations().is_empty(),
        json!("original nodes and audio with no pending replacement"),
        d.snapshot(),
    )?;
    d.key_modified(Key::R, Modifiers::CTRL)?;
    d.changed(undone.revision_id().as_str())?;
    d.wait_for("Redo creates fresh boundary replacement work", |app| {
        app.ai_preparations().iter().any(|item| {
            item.id != first.id
                && item.target.node == hold
                && item.state == PreparationState::Unavailable
        })
    })?;
    let current = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No project")?
        .document
        .clone();
    let fresh = d
        .app()
        .ai_preparations()
        .into_iter()
        .find(|item| item.target.node == hold)
        .ok_or("Redo replacement missing")?;
    d.check(
        "Redo restores the exact changed pictures and timing with a fresh activation",
        current.nodes() == changed.nodes()
            && current.audio_bindings() == changed.audio_bindings()
            && fresh.id != first.id
            && fresh.revision == *current.revision_id(),
        json!("same edit, fresh replacement"),
        d.snapshot(),
    )?;
    recovery(d, fresh, current.revision_id().to_string())?;
    let store = deadpan_store::ProjectStore::open(&path, deadpan_store::AccessMode::ReadOnly)
        .map_err(|error| error.to_string())?;
    store.validate_full().map_err(|error| error.to_string())?;
    let intents = store
        .generation_intents(None, 16)
        .map_err(|error| error.to_string())?;
    d.check(
        "Discard closes automatic intent durably",
        intents.is_empty(),
        json!(0),
        json!(intents.len()),
    )?;
    drop(store);
    if d.options.mode == RunMode::Performance {
        boundary_performance::run(d, &before, &current, &hold, &source)?;
    }
    Ok(())
}
