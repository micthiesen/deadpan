//! Counted AI pause insertion, durable failure recovery and semantic reuse.
//! Only model availability is scripted; every edit and queue transition is real.
use super::*;
use crate::preview::copied::Content;
use crate::project::generation::{Backend, Preparation, Script, ScriptEnding, ScriptQueue};
use deadpan_core::{HoldAudio, HoldVideo, NodeId, ProjectDocument, ProjectFrame};
use deadpan_store::generation_preparations::{PreparationOrigin, PreparationState};
use egui::Key;

const UNAVAILABLE: &str =
    "The AI insertion replay model is unavailable. Install the local model, then Retry.";

pub(super) fn backend() -> Backend {
    Backend::Scripted(Arc::new(ScriptQueue::new([Script {
        unavailable: Some(UNAVAILABLE.into()),
        steps: 0,
        step_interval: Duration::ZERO,
        ending: ScriptEnding::WaitForCancel,
    }])))
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("Model preflight reports a scripted missing model. Insertion, exact picture mappings, committed fallback, durable preparation, Retry, Discard, history, dot and macro execution use production paths.".into());
    spec_command(d)?;
    at(d, 10)?;
    let baseline = d.app().workspace.clone().ok_or("No workspace")?;
    let frames = crate::navigation::duration::DurationInput::half_seconds(3)
        .resolve(baseline.document.presentation_basis().frame_rate)?
        .frames();
    let revision = d.revision();
    d.chord(&[Key::Num3, Key::Comma, Key::A])?;
    d.changed(&revision)?;
    idle(d)?;
    let hold = selected_hold(d)?;
    let inserted = d.app().workspace.clone().ok_or("No workspace")?;
    let NodeKind::Hold { recipe } = &inserted.document.nodes()[&hold].kind else {
        return Err("No Hold".into());
    };
    let original_mismatch = (0..baseline.plan.duration().frames()).find_map(|frame| {
        let shifted = if frame < 10 { frame } else { frame + frames };
        match (baseline.plan.picture(ProjectFrame(frame)), inserted.plan.picture(ProjectFrame(shifted))) {
            (Ok(before), Ok(after)) if before.picture.follow_point().is_some()
                && before.picture.follow_point() == after.picture.follow_point() => None,
            (before, after) => Some(json!({"original_frame":frame,"shifted_frame":shifted,"before":format!("{before:?}"),"after":format!("{after:?}")})),
        }
    });
    let left = baseline
        .plan
        .picture(ProjectFrame(9))
        .map_err(|error| error.to_string())?;
    let (source_asset, _) = left
        .picture
        .follow_point()
        .ok_or("Left picture has no source point")?;
    let index = baseline
        .sources
        .get(source_asset)
        .and_then(|source| source.video_index.as_ref())
        .ok_or("The left picture has no qualified Original index")?;
    let expected_frame = left
        .picture
        .select_source_frame(index)
        .map_err(|error| error.to_string())?;
    // A Source is sampled at its frame center. A freeze records the measured
    // selected picture's PTS, which is a different coordinate of that picture.
    let expected_video = HoldVideo::Freeze {
        asset: source_asset.clone(),
        timestamp: deadpan_core::SourceTimestamp {
            ticks: expected_frame.pts,
            time_base: index.time_base(),
        },
    };
    let freeze_mismatch = (10..10 + frames).find_map(|frame| {
        match inserted.plan.picture(ProjectFrame(frame)) {
            Ok(picture) => {
                let selected = picture.picture.select_source_frame(index);
                (!matches!(picture.picture, deadpan_plan::Picture::Freeze { .. })
                    || !selected.as_ref().is_ok_and(|selected| *selected == expected_frame))
                    .then(|| json!({"inserted_frame":frame,"picture":picture.picture,"selected":format!("{selected:?}")}))
            }
            Err(error) => Some(json!({"inserted_frame":frame,"error":error.to_string()})),
        }
    });
    d.check(
        "3,a immediately inserts one 1.5-second silent freeze and preserves every Original picture",
        recipe.duration.frames() == frames
            && recipe.audio == HoldAudio::Silence
            && recipe.video == expected_video
            && inserted.plan.duration().frames() == baseline.plan.duration().frames() + frames
            && d.app().sequence_cursor == 10 && original_mismatch.is_none() && freeze_mismatch.is_none(),
        json!({"inserted_frames":frames,"cursor":10,"audio":"silence","originals":"preserved","fallback":"left picture"}),
        json!({"inserted_frames":recipe.duration.frames(),"audio":recipe.audio,"duration":inserted.plan.duration().frames(),"cursor":d.app().sequence_cursor,
            "originals_preserved":original_mismatch.is_none(),"first_original_mismatch":original_mismatch,
            "freeze_preserved":freeze_mismatch.is_none(),"first_freeze_mismatch":freeze_mismatch,
            "left_picture":left.picture,"expected_ordinal":expected_frame.identity,"expected_pts":expected_frame.pts,
            "expected_provider":expected_video,"actual_provider":recipe.video}),
    )?;
    let item = unavailable(d, &hold)?;
    let stored = reader(d)?
        .generation_preparation(&item.id)
        .map_err(|error| error.to_string())?
        .ok_or("Inserted preparation missing")?;
    d.check(
        "The same insertion durably records an AI request intent with fixed default controls",
        matches!(&stored.origin, PreparationOrigin::InsertedPause { options }
            if options.motion == deadpan_jobs::MotionAmount::Still
                && options.instructions.is_none()
                && options.region_target == deadpan_jobs::GenerationTarget::None)
            && stored.duration.frames() == frames
            && stored.target.node == hold
            && stored.current_revision == *inserted.document.revision_id(),
        json!({"origin":"inserted_pause","motion":"still","target":"none","frames":frames}),
        json!({"origin":stored.origin,"state":stored.state,"target":stored.target}),
    )?;
    recover(d, &item)?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "One Undo removes the entire inserted pause and its active AI preparation",
        same_document(
            &d.app().workspace.as_ref().ok_or("No workspace")?.document,
            &baseline.document,
        )? && d.app().ai_preparations().is_empty(),
        json!({"authored":"original baseline","active_preparations":0}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key_modified(Key::R, egui::Modifiers::CTRL)?;
    d.changed(&revision)?;
    idle(d)?;
    let redone = unavailable(d, &hold)?;
    d.check(
        "Redo restores the exact pause and makes a fresh revision-bound preparation claim",
        same_document(
            &d.app().workspace.as_ref().ok_or("No workspace")?.document,
            &inserted.document,
        )? && (redone.id != item.id
            || redone.revision != item.revision
            || redone.sequence != item.sequence),
        json!({"authored":"same inserted pause","fresh_claim":true}),
        json!({"old":format!("{item:?}"),"redone":format!("{redone:?}")}),
    )?;
    d.capture("Counted AI pause keeps its fallback after missing-model recovery")?;
    repeated(d, frames)?;
    recorded(d)?;
    captured_command(d)
}

fn spec_command(d: &mut Driver<'_>) -> Result<(), String> {
    at(d, 5)?;
    let baseline = d.app().workspace.clone().ok_or("No workspace")?;
    let frames = crate::navigation::duration::DurationInput::half_seconds(3)
        .resolve(baseline.document.presentation_basis().frame_rate)?
        .frames();
    let revision = d.revision();
    d.command("hold 1.5s video=ai audio=silence")?;
    d.changed(&revision)?;
    idle(d)?;
    let hold = selected_hold(d)?;
    let item = unavailable(d, &hold)?;
    let inserted = d.app().workspace.clone().ok_or("No workspace")?;
    let NodeKind::Hold { recipe } = &inserted.document.nodes()[&hold].kind else {
        return Err("The explicit AI pause command did not insert a Hold".into());
    };
    let stored = reader(d)?
        .generation_preparation(&item.id)
        .map_err(|error| error.to_string())?
        .ok_or("Explicit AI pause preparation missing")?;
    d.check(
        ":hold 1.5s video=ai audio=silence atomically inserts a silent fallback and an AI preparation",
        !baseline.document.nodes().contains_key(&hold)
            && recipe.duration.frames() == frames
            && recipe.audio == HoldAudio::Silence
            && matches!(recipe.video, HoldVideo::Freeze { .. })
            && inserted.plan.duration().frames() == baseline.plan.duration().frames() + frames
            && d.app().sequence_cursor == 5
            && stored.target.node == hold
            && stored.duration.frames() == frames
            && stored.origin_revision == *inserted.document.revision_id()
            && matches!(stored.origin, PreparationOrigin::InsertedPause { .. }),
        json!({"fresh_hold":true,"frames":frames,"cursor":5,"audio":"silence","provider":"freeze","preparation":"inserted_pause"}),
        json!({"hold":hold,"recipe":recipe,"duration":inserted.plan.duration().frames(),"cursor":d.app().sequence_cursor,"preparation":stored}),
    )?;
    d.capture("Exact spec AI hold command retains its saved fallback without a model")?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    idle(d)?;
    let cancelled = reader(d)?
        .generation_preparation(&item.id)
        .map_err(|error| error.to_string())?
        .ok_or("Undone explicit AI preparation missing")?;
    d.check(
        "One Undo removes the explicit AI pause and cancels its durable preparation",
        same_document(
            &d.app().workspace.as_ref().ok_or("No workspace")?.document,
            &baseline.document,
        )? && cancelled.state == PreparationState::Cancelled
            && !d
                .app()
                .ai_preparations()
                .iter()
                .any(|item| item.target.node == hold),
        json!({"authored":"before command","preparation":"cancelled"}),
        json!({"snapshot":d.snapshot(),"preparation":cancelled}),
    )
}

fn captured_command(d: &mut Driver<'_>) -> Result<(), String> {
    for command in ["ai-hold 2f", "hold 2f", "hold 2f video=ai audio=silence"] {
        // Create one ordinary edit so an independent real Undo can arrive
        // while the native command field remains open.
        let revision = d.revision();
        d.command("hold 1f")?;
        d.changed(&revision)?;
        idle(d)?;
        let revision = d.revision();
        let expected_revision = d
            .app()
            .workspace
            .as_ref()
            .ok_or("No workspace")?
            .document
            .revision_id()
            .clone();
        d.key(Key::Colon)?;
        d.events(
            "Type pause command before an independent history reply",
            vec![egui::Event::Text(command.into())],
        )?;
        d.app_mut().feedback.hold_project_updates = true;
        d.app()
            .service
            .submit(ProjectRequest::Undo { expected_revision })?;
        d.wait_for("Undo completes while its UI reply is held", |app| {
            !app.service.is_busy()
        })?;
        d.app_mut().feedback.hold_project_updates = false;
        d.changed(&revision)?;
        let restored = d.app().workspace.clone().ok_or("No workspace")?;
        let targets_before: Vec<_> = reader(d)?
            .generation_preparations(None, 256)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|item| item.target)
            .collect();
        d.key(Key::Enter)?;
        idle(d)?;
        let targets_after: Vec<_> = reader(d)?
            .generation_preparations(None, 256)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|item| item.target)
            .collect();
        d.check(
            &format!(":{command} rejects its stale command-entry capture after a genuine Undo reply"),
            d.app().workspace.as_ref().is_some_and(|workspace| workspace.document == restored.document)
                && targets_after == targets_before
                && d.app().error.as_deref().is_some_and(|error| error.to_ascii_lowercase().contains("changed")),
            json!({"inserted_frames":0,"new_preparations":0,"refusal":"captured context changed"}),
            json!({"revision":d.revision(),"error":d.app().error,"targets_before":targets_before,"targets_after":targets_after}),
        )?;
    }
    d.capture("Pause commands retain their entry target across delayed replies")
}

fn recover(d: &mut Driver<'_>, item: &Preparation) -> Result<(), String> {
    let revision = d.revision();
    d.command("jobs")?;
    d.step("Inserted AI preparation in Jobs", true)?;
    let widgets = d.widgets().to_string();
    d.check(
        "Jobs explains the missing model and teaches keyboard Retry and Discard",
        widgets.contains("AI PREPARATIONS")
            && widgets.contains(UNAVAILABLE)
            && widgets.contains("Unavailable; R retries")
            && widgets.contains("D discards"),
        json!([
            "AI PREPARATIONS",
            "Unavailable; R retries",
            "D discards",
            UNAVAILABLE
        ]),
        json!(widgets),
    )?;
    let target = super::super::jobs::RowKey::Preparation(item.id.clone());
    let index = d
        .app()
        .job_rows()
        .iter()
        .position(|row| row.key() == target)
        .ok_or("Inserted preparation row missing")?;
    for _ in 0..index {
        d.key(Key::J)?;
    }
    d.key(Key::R)?;
    d.wait_for(
        "Keyboard Retry claims the inserted AI preparation again",
        |app| {
            app.ai_preparations().iter().any(|retried| {
                retried.id == item.id
                    && retried.sequence > item.sequence
                    && retried.state == PreparationState::Unavailable
            })
        },
    )?;
    d.check(
        "Retry preserves the exact inserted duration and committed revision",
        d.revision() == revision,
        json!(revision),
        json!(d.revision()),
    )?;
    d.key(Key::D)?;
    d.wait_for(
        "Keyboard Discard cancels the inserted AI preparation",
        |app| app.ai_preparations().is_empty(),
    )?;
    let stored = reader(d)?
        .generation_preparation(&item.id)
        .map_err(|error| error.to_string())?
        .ok_or("Discarded preparation missing")?;
    d.check(
        "Discard durably cancels generation while retaining the inserted fallback",
        stored.state == PreparationState::Cancelled && d.revision() == revision,
        json!({"state":"cancelled","revision":revision}),
        json!({"state":stored.state,"revision":d.revision()}),
    )?;
    d.capture("Inserted AI pause remains usable without the model")?;
    d.key(Key::Escape)?;
    idle(d)
}

fn repeated(d: &mut Driver<'_>, frames: i64) -> Result<(), String> {
    at(d, 70)?;
    let before = d.app().workspace.clone().ok_or("No workspace")?;
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    idle(d)?;
    let hold = selected_hold(d)?;
    let item = unavailable(d, &hold)?;
    let after = d.app().workspace.clone().ok_or("No workspace")?;
    d.check(
        "Dot inserts the same AI pause duration at the new cursor with a fresh Hold and preparation",
        !before.document.nodes().contains_key(&hold)
            && item.frames == frames && item.target.node == hold
            && after.plan.duration().frames() == before.plan.duration().frames() + frames
            && d.app().sequence_cursor == 70,
        json!({"fresh_hold":true,"frames":frames,"cursor":70}),
        d.snapshot(),
    )
}

fn recorded(d: &mut Driver<'_>) -> Result<(), String> {
    at(d, 110)?;
    let before = d.app().workspace.clone().ok_or("No workspace")?;
    d.command("record b")?;
    let revision = d.revision();
    d.chord(&[Key::Comma, Key::A])?;
    d.changed(&revision)?;
    idle(d)?;
    let recorded_hold = selected_hold(d)?;
    unavailable(d, &recorded_hold)?;
    d.command("record-stop")?;
    d.wait_for(
        "AI insertion macro is saved in the typed register bank",
        |app| {
            !app.macros.recording()
                && !app.macros.is_pending()
                && app
                    .copied
                    .entries()
                    .any(|(name, value)| name == 'b' && matches!(value, Content::Macro(_)))
        },
    )?;
    let program = d
        .app()
        .copied
        .entries()
        .find_map(|(name, value)| match value {
            Content::Macro(program) if name == 'b' => Some(program.clone()),
            _ => None,
        })
        .ok_or("Saved AI macro missing")?;
    let expected = json!({"instructions":[{"type":"insert_ai_pause","length":{"unit":"milliseconds","milliseconds":500}}]});
    // Match the typed instruction, so a freeze-only recording cannot pass.
    let instructions = program.instructions();
    let retained_intent = matches!(instructions, [deadpan_core::SemanticInstruction::InsertAiPause {
        length: deadpan_core::PauseLength::Milliseconds { milliseconds }
    }] if milliseconds.get() == 500);
    d.check("The macro records AI insertion intent and duration rather than a generated asset or Hold ID",
        retained_intent, expected, json!(program))?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Undo removes the recorded example while retaining its reusable macro",
        same_document(
            &d.app().workspace.as_ref().ok_or("No workspace")?.document,
            &before.document,
        )?,
        json!("authored state before recording"),
        d.snapshot(),
    )?;
    at(d, 115)?;
    let before = d.app().workspace.clone().ok_or("No workspace")?;
    let revision = d.revision();
    d.command("macro b")?;
    d.changed(&revision)?;
    idle(d)?;
    let hold = selected_hold(d)?;
    let item = unavailable(d, &hold)?;
    let frames = crate::navigation::duration::DurationInput::half_seconds(1)
        .resolve(before.document.presentation_basis().frame_rate)?
        .frames();
    d.check(
        "Replaying the AI macro allocates a fresh Hold and generation intent at its new cursor",
        hold != recorded_hold
            && !before.document.nodes().contains_key(&hold)
            && item.frames == frames
            && d.app().sequence_cursor == 115
            && d.app().sequence_length()
                == u64::try_from(before.plan.duration().frames() + frames)
                    .map_err(|error| error.to_string())?,
        json!({"fresh_hold":true,"frames":frames,"cursor":115}),
        d.snapshot(),
    )?;
    d.capture("AI pause intent survives dot and macro reuse")
}

fn selected_hold(d: &Driver<'_>) -> Result<NodeId, String> {
    d.app()
        .ai_hold()
        .ok_or_else(|| "Inserted AI Hold is not selected".into())
}

fn unavailable(d: &mut Driver<'_>, hold: &NodeId) -> Result<Preparation, String> {
    d.wait_for(
        "Inserted AI pause reports unavailable model without changing fallback",
        |app| {
            app.ai_preparations().iter().any(|item| {
                &item.target.node == hold && item.state == PreparationState::Unavailable
            })
        },
    )?;
    d.app()
        .ai_preparations()
        .into_iter()
        .find(|item| &item.target.node == hold)
        .ok_or_else(|| "AI preparation disappeared".into())
}

fn reader(d: &Driver<'_>) -> Result<deadpan_store::ProjectStore, String> {
    deadpan_store::ProjectStore::open(
        &d.app().workspace.as_ref().ok_or("No workspace")?.path,
        deadpan_store::AccessMode::ReadOnly,
    )
    .map_err(|error| error.to_string())
}

fn same_document(actual: &ProjectDocument, expected: &ProjectDocument) -> Result<bool, String> {
    let mut actual = serde_json::to_value(actual).map_err(|error| error.to_string())?;
    actual["revision_id"] = json!(expected.revision_id());
    Ok(actual == serde_json::to_value(expected).map_err(|error| error.to_string())?)
}

fn idle(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("AI edit and macro acknowledgment complete", |app| {
        !app.service.is_busy() && !app.macros.is_pending()
    })?;
    d.settled()
}

fn at(d: &mut Driver<'_>, frame: u64) -> Result<(), String> {
    super::transcript::focus_your_edit(d)?;
    d.key(Key::Escape)?;
    d.chord(&[Key::G, Key::G])?;
    if frame > 0 {
        for digit in frame.to_string().chars() {
            d.key(Key::from_name(&digit.to_string()).ok_or("Invalid frame digit")?)?;
        }
        d.key(Key::L)?;
    }
    d.settled()
}
