//! Mute records an explicit Boolean and resolves the play chosen at invocation.
use super::*;
use crate::preview::copied::Content;
use deadpan_core::{AudioChange, SemanticInstruction as I};

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let baseline = document(d)?.clone();
    let original = d.app().source_cursor;
    d.check(
        "Mute macro fixture starts with three unmuted plays",
        states(d)? == [false; 3],
        json!([false, false, false]),
        d.snapshot(),
    )?;
    resize(d, 960.0, 640.0)?;
    choose(d, 1)?;
    let foreign = d.app().capture_macro_target();
    choose(d, 3)?;
    let target = d.app().capture_gain_target();
    let revision = d.revision();
    d.app_mut().record_audio_change(
        Some(target),
        Some(foreign),
        AudioChange::Mute { muted: true },
    );
    d.check(
        "A mute target cannot borrow another captured play's authority",
        d.revision() == revision
            && equivalent(document(d)?, &baseline)
            && !d.app().macros.is_pending()
            && d.app()
                .error
                .as_deref()
                .is_some_and(|e| e.contains("plays differ")),
        json!("unchanged"),
        d.snapshot(),
    )?;
    d.chord(&[Key::Q, Key::M])?;
    painted(d, "mute / unmute")?;
    scenarios::footer_anchored(d, "Scoped mute recording teaches its key at minimum size")?;
    d.capture("One-play mute recording retains visible keys at minimum size")?;
    d.chord(&[Key::Comma, Key::M])?;
    d.changed(&revision)?;
    settled(d)?;
    d.check(
        "The mute key records true only on the selected third play",
        states(d)? == [false, false, true]
            && d.app().macros.instruction_count() == 1
            && d.app().sequence_cursor == 240
            && d.app().source_cursor == original,
        json!({"muted":[false,false,true],"instructions":1,"Edit":240}),
        d.snapshot(),
    )?;
    let same = document(d)?.clone();
    let target = d.app().capture_gain_target();
    let capture = d.app().capture_macro_target();
    d.app_mut().record_audio_change(
        Some(target),
        Some(capture),
        AudioChange::Mute { muted: true },
    );
    d.check(
        "A same-state mute appends no instruction and creates no history",
        document(d)? == &same
            && d.app().macros.instruction_count() == 1
            && !d.app().macros.is_pending()
            && !d.app().service.is_busy(),
        json!({"instructions":1,"authored":"unchanged"}),
        d.snapshot(),
    )?;
    save(d, 'm', true)?;
    undo(d, &baseline)?;
    choose(d, 1)?;
    let revision = d.revision();
    super::super::macros::execute(d, 'm', None)?;
    d.changed(&revision)?;
    settled(d)?;
    d.check(
        "Saved true resolves the invocation's first play and preserves the others",
        states(d)? == [true, false, false]
            && d.app().sequence_cursor == 0
            && d.app().scoped.is_some(),
        json!({"muted":[true,false,false],"Edit":0}),
        d.snapshot(),
    )?;
    let first = document(d)?.clone();
    choose(d, 3)?;
    let revision = d.revision();
    d.chord(&[Key::Comma, Key::M])?;
    d.changed(&revision)?;
    settled(d)?;
    let first_third = document(d)?.clone();
    choose(d, 2)?;
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    settled(d)?;
    let all = document(d)?.clone();
    d.check(
        "Dot applies the retained true to the newly chosen unmuted second play",
        states(d)? == [true; 3] && d.app().sequence_cursor == 120 && d.app().scoped.is_some(),
        json!({"muted":[true,true,true],"Edit":120}),
        d.snapshot(),
    )?;
    choose(d, 1)?;
    d.chord(&[Key::Q, Key::N])?;
    let revision = d.revision();
    d.command("gain-mute")?;
    d.changed(&revision)?;
    settled(d)?;
    d.check(
        "The captured mute command records false from the selected muted first play",
        states(d)? == [false, true, true] && d.app().macros.instruction_count() == 1,
        json!({"muted":[false,true,true],"recorded":false}),
        d.snapshot(),
    )?;
    save(d, 'n', false)?;
    let first_unmuted = document(d)?.clone();
    choose(d, 3)?;
    let revision = d.revision();
    super::super::macros::execute(d, 'n', None)?;
    d.changed(&revision)?;
    settled(d)?;
    let both_unmuted = document(d)?.clone();
    d.check(
        "A saved false un-mutes the third play without toggling any peer",
        states(d)? == [false, true, false]
            && d.app().sequence_cursor == 240
            && d.app().source_cursor == original,
        json!({"muted":[false,true,false],"Edit":240}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    super::super::macros::execute(d, 'n', None)?;
    settled(d)?;
    d.check(
        "Reapplying saved false to an unmuted play refuses instead of toggling it",
        d.revision() == revision && document(d)? == &both_unmuted && !d.app().macros.is_pending(),
        json!("same-state refusal, unchanged document"),
        d.snapshot(),
    )?;
    d.capture("One-play mute macros retain explicit true and false intent")?;
    undo(d, &first_unmuted)?;
    let revision = d.revision();
    d.events(
        "Redo explicit one-play unmute",
        vec![
            super::super::key_event(Key::R, egui::Modifiers::CTRL, true),
            super::super::key_event(Key::R, egui::Modifiers::CTRL, false),
        ],
    )?;
    d.changed(&revision)?;
    settled(d)?;
    d.check(
        "One Redo restores the exact unmuted document",
        equivalent(document(d)?, &both_unmuted),
        json!("exact document"),
        d.snapshot(),
    )?;
    undo(d, &first_unmuted)?;
    undo(d, &all)?;
    undo(d, &first_third)?;
    undo(d, &first)?;
    undo(d, &baseline)?;
    late(d, &baseline)?;
    d.check(
        "Mute verification preserves the complete baseline and Original cursor",
        equivalent(document(d)?, &baseline) && d.app().source_cursor == original,
        json!({"baseline":"restored","Original":original}),
        d.snapshot(),
    )?;
    resize(d, 1280.0, 820.0)
}

fn choose(d: &mut Driver<'_>, play: u32) -> Result<(), String> {
    if d.app().scoped.is_none() {
        d.key(Key::Enter)?;
    }
    d.command(&format!("scope play {play}"))?;
    settled(d)
}
fn settled(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Mute semantic receipt is admitted", |app| {
        !app.service.is_busy() && !app.macros.is_pending()
    })?;
    d.settled()
}
fn states(d: &Driver<'_>) -> Result<[bool; 3], String> {
    let workspace = d.app().workspace.as_ref().ok_or("No mute workspace")?;
    let mut states = [false; 3];
    for (index, frame) in [0, 120, 240].into_iter().enumerate() {
        let picture = workspace
            .plan
            .picture(ProjectFrame(frame))
            .map_err(|e| e.to_string())?;
        states[index] = workspace.document.nodes()[&picture.instance.node]
            .audio_treatments
            .clip_gain()
            .is_some_and(deadpan_core::ClipGain::muted);
    }
    Ok(states)
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
        "One Undo restores the exact mute document and scoped isolation",
        equivalent(document(d)?, expected) && d.app().scoped.is_none(),
        json!("document restored"),
        d.snapshot(),
    )
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
fn save(d: &mut Driver<'_>, name: char, muted: bool) -> Result<(), String> {
    d.key(Key::Q)?;
    settled(d)?;
    d.check(
        "The saved mute program contains a Boolean without concrete owner or play IDs",
        saved(d, name)
            == Some(vec![I::SetAudio {
                change: AudioChange::Mute { muted },
            }]),
        json!([I::SetAudio {
            change: AudioChange::Mute { muted }
        }]),
        d.snapshot(),
    )
}
fn late(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    choose(d, 3)?;
    d.chord(&[Key::Q, Key::X])?;
    d.app_mut().feedback.hold_project_updates = true;
    d.chord(&[Key::Comma, Key::M])?;
    d.wait_for("Mute commits before its continuation arrives", |app| {
        !app.service.is_busy()
    })?;
    let previous = format!("Previous  {}", d.app().editor_key(EditorKey::PlayPrevious));
    d.click(&previous)?;
    let cursor = d.app().sequence_cursor;
    d.app_mut().feedback.hold_project_updates = false;
    settled(d)?;
    d.check(
        "A delayed mute receipt cannot reclaim navigation or append to abandoned recording",
        cursor == 120
            && d.app().sequence_cursor == cursor
            && d.app().scoped.is_none()
            && !d.app().macros.recording()
            && saved(d, 'x').is_none()
            && states(d)? == [false, false, true],
        json!({"Edit":120,"recording":false,"muted":[false,false,true]}),
        d.snapshot(),
    )?;
    undo(d, baseline)
}
