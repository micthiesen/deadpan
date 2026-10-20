//! Saved pause intent resolves the play and provider at each invocation.
use super::*;
use crate::preview::copied::Content;
use deadpan_core::{ExactRatio, HoldAudio, HoldVideo, PauseLength, SemanticInstruction as I};
use std::num::NonZeroU32;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let baseline = document(d)?.clone();
    let original = d.app().source_cursor;
    let root = d
        .app()
        .selected_beat
        .clone()
        .ok_or("No Repeat for pause macros")?;
    resize(d, 960.0, 640.0)?;
    record(d, &baseline, &root, 'p', false)?;
    choose(d, 1)?;
    let bank = d.app().copied.bank_version();
    let revision = d.revision();
    super::super::macros::execute(d, 'p', None)?;
    d.changed(&revision)?;
    settled(d)?;
    selected_pause(d, false, 3)?;
    d.check("A saved freeze macro resolves the newly selected first play and leaves the other plays unchanged",
        play_lengths(d, &root)? == [123,120,120] && unchanged_plays(d, &baseline, &root, &[0])? && d.app().copied.bank_version() == bank,
        json!({"play_frames":[123,120,120],"saved_bank":"unchanged"}), d.snapshot())?;
    let macro_pause = document(d)?.clone();
    let revision = d.revision();
    d.command("hold 2f")?;
    d.changed(&revision)?;
    settled(d)?;
    selected_pause(d, false, 2)?;
    let second_pause = document(d)?.clone();
    let target = d
        .app()
        .scoped_target()?
        .ok_or("Second pause lost its Hold")?;
    let revision = d.revision();
    d.command("zoom 2 target=center")?;
    d.changed(&revision)?;
    settled(d)?;
    let zoomed = d.app().scoped_target()?.ok_or("Pause Zoom lost its Hold")?;
    d.check("The macro's exact inserted Hold continues into another pause and a Zoom without retargeting",
        zoomed.target == target.target && play_lengths(d, &root)? == [125,120,120]
            && document(d)?.nodes()[&zoomed.target.node].framing.as_ref().is_some_and(|framing|
                framing.evaluate_exact(ExactRatio::ZERO, ExactRatio::integer(2)).is_ok_and(|pose| pose.scale == ExactRatio::integer(2))),
        json!({"selected":"second inserted Hold","play_frames":[125,120,120],"Zoom":2}), d.snapshot())?;
    painted(d, "This play 1/3")?;
    scenarios::footer_anchored(d, "Pause macro continuation keeps the compact footer")?;
    d.capture("A replayed freeze retains the exact Hold for another pause and Zoom")?;
    undo(d, &second_pause)?;
    undo(d, &macro_pause)?;
    undo(d, &baseline)?;

    record(d, &baseline, &root, 'r', true)?;
    choose(d, 1)?;
    let revision = d.revision();
    super::super::macros::execute(d, 'r', None)?;
    d.changed(&revision)?;
    settled(d)?;
    selected_pause(d, true, 3)?;
    d.check(
        "A saved black pause uses the invocation's first play instead of its recorded third play",
        play_lengths(d, &root)? == [123, 120, 120] && unchanged_plays(d, &baseline, &root, &[0])?,
        json!({"play_frames":[123,120,120]}),
        d.snapshot(),
    )?;
    undo(d, &baseline)?;

    choose(d, 1)?;
    let revision = d.revision();
    d.command("hold 3f video=black")?;
    d.changed(&revision)?;
    settled(d)?;
    let first = document(d)?.clone();
    choose(d, 3)?;
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    settled(d)?;
    selected_pause(d, true, 3)?;
    d.check("Dot inserts black only in the play chosen now while retaining the earlier independent pause",
        play_lengths(d, &root)? == [123,120,123] && unchanged_plays(d, &baseline, &root, &[0,2])? && d.app().sequence_cursor == 244,
        json!({"play_frames":[123,120,123],"Edit":244}),d.snapshot())?;
    painted(d, "This play 3/3")?;
    scenarios::footer_anchored(
        d,
        "Dot's selected-play black pause keeps the compact footer",
    )?;
    d.capture("Dot inserts the black pause into the newly selected third play")?;
    undo(d, &first)?;
    undo(d, &baseline)?;

    refuse(d, &baseline)?;
    delayed(d, &baseline, &root)?;
    d.check(
        "Pause macro verification restores the complete document and independent Original clock",
        equivalent(document(d)?, &baseline) && d.app().source_cursor == original,
        json!({"document":"restored","Original":original}),
        d.snapshot(),
    )?;
    resize(d, 1280.0, 820.0)
}

fn record(
    d: &mut Driver<'_>,
    baseline: &ProjectDocument,
    root: &NodeId,
    name: char,
    black: bool,
) -> Result<(), String> {
    choose(d, 3)?;
    let key = Key::from_name(&name.to_string()).ok_or("Invalid pause macro register")?;
    d.chord(&[Key::Q, key])?;
    let revision = d.revision();
    d.command(if black {
        "hold 3f video=black"
    } else {
        "hold 3f"
    })?;
    d.changed(&revision)?;
    settled(d)?;
    selected_pause(d, black, 3)?;
    d.check(
        "Recording a pause performs the real edit only in the chosen third play",
        d.app().macros.recording_name() == Some(name)
            && d.app().macros.instruction_count() == 1
            && play_lengths(d, root)? == [120, 120, 123]
            && unchanged_plays(d, baseline, root, &[2])?,
        json!({"recording":name,"instructions":1,"black":black,"play_frames":[120,120,123]}),
        d.snapshot(),
    )?;
    d.key(Key::Q)?;
    settled(d)?;
    let expected = I::InsertPause {
        length: PauseLength::Frames {
            frames: NonZeroU32::new(3).unwrap(),
        },
        black,
    };
    d.check("The saved pause program contains duration and picture intent without concrete owner or play IDs",
        saved(d,name) == Some(vec![expected.clone()]) && !d.app().macros.recording(),
        json!([expected]),d.snapshot())?;
    undo(d, baseline)
}

fn selected_pause(d: &mut Driver<'_>, black: bool, frames: i64) -> Result<(), String> {
    let target = d
        .app()
        .scoped_target()?
        .ok_or("Pause macro lost its inspector")?;
    let workspace = d.app().workspace.as_ref().ok_or("No pause workspace")?;
    let node = &workspace.document.nodes()[&target.target.node];
    let NodeKind::Hold { recipe } = &node.kind else {
        return Err("Pause macro did not select its inserted Hold".into());
    };
    let shown = workspace
        .plan
        .picture(ProjectFrame(
            i64::try_from(d.app().sequence_cursor).map_err(|error| error.to_string())?,
        ))
        .map_err(|error| error.to_string())?;
    d.check(
        "The pause receipt retains its exact authored Hold and concrete displayed occurrence",
        recipe.duration.frames() == frames
            && recipe.audio == HoldAudio::Silence
            && (if black {
                recipe.video == HoldVideo::Background
            } else {
                matches!(recipe.video, HoldVideo::Freeze { .. })
                    && d.app().presentation.displayed_source_frame() == Some(SourceFrameId(0))
            })
            && target.presentation.as_ref() == Some(&shown.instance)
            && target.target.node == shown.instance.node,
        json!({"selected":"inserted Hold","frames":frames,"black":black,"presentation":"exact"}),
        d.snapshot(),
    )
}

fn refuse(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    choose(d, 1)?;
    d.command("scope plays 1,2")?;
    let bank = d.app().copied.bank_version();
    d.chord(&[Key::Q, Key::S])?;
    d.check(
        "Multiple selected plays refuse pause recording without saving a macro or authored edit",
        !d.app().macros.recording()
            && equivalent(document(d)?, baseline)
            && d.app().copied.bank_version() == bank
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("This play at every Repeat")),
        json!({"recording":false,"document":"unchanged","bank":"unchanged"}),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    let revision = d.revision();
    d.command("wrap-repeat 2")?;
    d.changed(&revision)?;
    settled(d)?;
    let wrapped = document(d)?.clone();
    d.key(Key::Enter)?;
    d.command("scope play 1")?;
    d.key(Key::Enter)?;
    let mixed = d
        .app()
        .scoped_target()?
        .ok_or("Missing mixed pause authority")?;
    d.chord(&[Key::Q, Key::S])?;
    d.check("Mixed This play and All plays choices refuse a pause macro without altering the nested Repeat",
        mixed.target.repeats.len() == 2 && !d.app().macros.recording()
            && equivalent(document(d)?,&wrapped) && d.app().copied.bank_version() == bank
            && d.app().error.as_deref().is_some_and(|error|error.contains("This play at every Repeat")),
        json!({"recording":false,"mixed_scope":"unchanged","bank":"unchanged"}),d.snapshot())?;
    d.key(Key::Escape)?;
    undo(d, baseline)?;
    choose(d, 1)?;
    d.chord(&[Key::Q, Key::S])?;
    d.command("hold 3f video=ai")?;
    settled(d)?;
    d.check(
        "AI pause recording refuses before any edit or instruction is accepted",
        d.app().macros.recording_name() == Some('s')
            && d.app().macros.instruction_count() == 0
            && equivalent(document(d)?, baseline)
            && d.app().copied.bank_version() == bank
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("freeze and black pauses")),
        json!({"recording":"s","instructions":0,"AI":"refused","document":"unchanged"}),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    settled(d)
}

fn delayed(d: &mut Driver<'_>, baseline: &ProjectDocument, root: &NodeId) -> Result<(), String> {
    choose(d, 3)?;
    let bank = d.app().copied.bank_version();
    d.chord(&[Key::Q, Key::T])?;
    d.app_mut().feedback.hold_project_updates = true;
    let result = (|| {
        d.command("hold 3f")?;
        d.wait_for(
            "A recorded pause is durable before its scoped continuation arrives",
            |app| !app.service.is_busy(),
        )?;
        let previous = format!("Previous  {}", d.app().editor_key(EditorKey::PlayPrevious));
        d.click(&previous)?;
        let cursor = d.app().sequence_cursor;
        d.app_mut().feedback.hold_project_updates = false;
        settled(d)?;
        d.check("A delayed pause cannot reclaim another play or append into an abandoned recording",
            d.app().scoped.is_none() && d.app().sequence_cursor == cursor && cursor == 120
                && !d.app().macros.recording() && saved(d,'t').is_none()
                && d.app().copied.bank_version() == bank && play_lengths(d,root)? == [120,120,123],
            json!({"Edit":120,"saved_third_play_frames":123,"inspector":"closed","recording":false,"bank":"unchanged"}),d.snapshot())?;
        scenarios::footer_anchored(d, "Delayed pause recording keeps the compact footer")?;
        d.capture("A saved pause preserves newer play navigation and cancels abandoned recording")?;
        undo(d, baseline)
    })();
    d.app_mut().feedback.hold_project_updates = false;
    result
}

fn choose(d: &mut Driver<'_>, play: u32) -> Result<(), String> {
    if d.app().scoped.is_some() {
        d.key(Key::Escape)?;
    }
    d.key(Key::Enter)?;
    d.command(&format!("scope play {play}"))?;
    d.key(Key::L)?;
    settled(d)
}
fn settled(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Scoped pause macro receipt is admitted", |app| {
        !app.service.is_busy() && !app.macros.is_pending()
    })?;
    d.settled()
}
fn saved(d: &Driver<'_>, name: char) -> Option<Vec<I>> {
    d.app()
        .copied
        .entries()
        .find_map(|(register, value)| match value {
            Content::Macro(program) if register == name => Some(program.instructions().to_vec()),
            _ => None,
        })
}
fn play_lengths(d: &Driver<'_>, root: &NodeId) -> Result<[i64; 3], String> {
    let document = document(d)?;
    let NodeKind::Repeat {
        child, iterations, ..
    } = &document.nodes()[root].kind
    else {
        return Err("Pause macro root is not a Repeat".into());
    };
    let durations = document.durations().map_err(|error| error.to_string())?;
    let mut result = [0; 3];
    for (index, length) in result.iter_mut().enumerate() {
        let iteration = iterations
            .at(u32::try_from(index).map_err(|error| error.to_string())?)
            .ok_or("Pause macro Repeat lost a play")?;
        let child = document
            .overrides()
            .get(root)
            .and_then(|entries| entries.get(&iteration))
            .unwrap_or(child);
        *length = durations[child].frames();
    }
    Ok(result)
}
fn unchanged_plays(
    d: &Driver<'_>,
    before: &ProjectDocument,
    root: &NodeId,
    edited: &[usize],
) -> Result<bool, String> {
    let after = document(d)?;
    let NodeKind::Repeat {
        child, iterations, ..
    } = &before.nodes()[root].kind
    else {
        return Err("Missing original Repeat".into());
    };
    let NodeKind::Repeat {
        child: after_child,
        iterations: after_iterations,
        ..
    } = &after.nodes()[root].kind
    else {
        return Ok(false);
    };
    if iterations != after_iterations {
        return Ok(false);
    }
    for index in 0..3 {
        if edited.contains(&index) {
            continue;
        }
        let iteration = iterations
            .at(u32::try_from(index).map_err(|error| error.to_string())?)
            .ok_or("Missing unchanged play")?;
        let original = before
            .overrides()
            .get(root)
            .and_then(|entries| entries.get(&iteration))
            .unwrap_or(child);
        let current = after
            .overrides()
            .get(root)
            .and_then(|entries| entries.get(&iteration))
            .unwrap_or(after_child);
        if original != current {
            return Ok(false);
        }
        let mut pending = vec![original.clone()];
        while let Some(owner) = pending.pop() {
            if before.nodes().get(&owner) != after.nodes().get(&owner)
                || before.overrides().get(&owner) != after.overrides().get(&owner)
                || before.gap_overrides().get(&owner) != after.gap_overrides().get(&owner)
            {
                return Ok(false);
            }
            pending.extend(before.children(&owner).cloned());
        }
    }
    Ok(true)
}

fn equivalent(actual: &ProjectDocument, expected: &ProjectDocument) -> bool {
    let mut actual = serde_json::to_value(actual).unwrap();
    actual["revision_id"] = json!(expected.revision_id());
    actual == serde_json::to_value(expected).unwrap()
}
fn undo(d: &mut Driver<'_>, expected: &ProjectDocument) -> Result<(), String> {
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    settled(d)?;
    d.check(
        "One Undo restores the complete pre-macro pause document",
        equivalent(document(d)?, expected) && d.app().scoped.is_none(),
        json!({"document":"restored","inspector":"closed"}),
        d.snapshot(),
    )
}
