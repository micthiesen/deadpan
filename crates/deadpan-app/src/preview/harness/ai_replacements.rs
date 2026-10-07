//! Accepted pause extension and durable replacement recovery through the
//! production keyboard router. Only model inference is scripted.
use super::*;
use crate::project::generation::{Backend, Outcome, Script, ScriptEnding, ScriptQueue};
use deadpan_core::HoldVideo;
use deadpan_store::generation_preparations::PreparationState;
use egui::Key;

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
    crate::project::generation::synthetic_tools()
        .map_err(|reason| format!("ai-replacements requires synthetic Ready tools: {reason}"))?;
    d.report.skipped.push("Inference uses qualified synthetic footage for the first accepted candidate; replacement preflight then reports a scripted missing model. Duration edits, durable queue, Retry and Discard are production paths.".into());
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
    d.command("jobs")?;
    d.step("Replacement recovery in Jobs", true)?;
    let widgets = d.widgets().to_string();
    d.check(
        "Jobs explains the unavailable replacement and teaches Retry R and Discard D",
        widgets.contains("AI PREPARATIONS")
            && widgets.contains("18 frames")
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
        "Retry keeps the saved duration edit and the captured authoring target",
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
        "Discard persists without undoing the extended pause",
        state == PreparationState::Cancelled && d.revision() == extended,
        json!({"state":"cancelled","revision":extended}),
        json!({"state":state,"revision":d.revision()}),
    )?;
    d.key(Key::Escape)?;
    d.settled()?;
    Ok(())
}
