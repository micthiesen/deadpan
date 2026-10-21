//! Relative gain intent captures each invocation's explicit play authority.
use super::*;
use crate::preview::copied::Content;
use deadpan_core::{AudioChange, SemanticInstruction as I};

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let baseline = document(d)?.clone();
    let original = d.app().source_cursor;
    let initial = gains(d)?;
    resize(d, 960.0, 640.0)?;
    choose(d, 1)?;
    let stale = d.app().capture_macro_target();
    choose(d, 3)?;
    let target = d.app().capture_gain_target();
    let revision = d.revision();
    d.app_mut()
        .record_audio_change(Some(target), Some(stale), step());
    d.check(
        "A captured gain target cannot borrow another play's semantic authority",
        d.revision() == revision
            && equivalent(document(d)?, &baseline)
            && d.app()
                .error
                .as_deref()
                .is_some_and(|e| e.contains("plays differ"))
            && !d.app().macros.is_pending(),
        json!({"authored":"unchanged","pending":false}),
        d.snapshot(),
    )?;
    d.chord(&[Key::Q, Key::G])?;
    refuse_audio_treatments(d)?;
    let target = d.app().capture_gain_target();
    let capture = d.app().capture_macro_target();
    d.app_mut().record_audio_change(
        Some(target),
        Some(capture),
        AudioChange::Step { millidecibels: 0 },
    );
    d.check(
        "A zero gain step creates neither isolation nor a recorded instruction",
        d.revision() == revision
            && equivalent(document(d)?, &baseline)
            && d.app().macros.recording_name() == Some('g')
            && d.app().macros.instruction_count() == 0
            && !d.app().macros.is_pending(),
        json!({"instructions":0,"authored":"unchanged"}),
        d.snapshot(),
    )?;
    d.key(Key::Plus)?;
    d.changed(&revision)?;
    settled(d)?;
    d.check(
        "The gain key records one relative step on the third play and preserves its peers",
        gains(d)? == [initial[0], initial[1], initial[2] + 3000]
            && d.app().macros.instruction_count() == 1
            && d.app().sequence_cursor == 240
            && d.app().source_cursor == original,
        json!({"gain_delta_mdb":[0,0,3000],"Edit":240,"instructions":1}),
        d.snapshot(),
    )?;
    painted(d, "gain 3 dB")?;
    scenarios::footer_anchored(d, "Gain recording keys fit the minimum workspace")?;
    d.capture("One-play gain recording retains visible keys at minimum size")?;
    d.key(Key::Q)?;
    settled(d)?;
    d.check(
        "The saved one-play gain macro contains relative intent without owner or play IDs",
        saved(d, 'g') == Some(vec![I::SetAudio { change: step() }]),
        json!([I::SetAudio { change: step() }]),
        d.snapshot(),
    )?;
    undo(d, &baseline)?;
    choose(d, 1)?;
    let revision = d.revision();
    super::super::macros::execute(d, 'g', None)?;
    d.changed(&revision)?;
    settled(d)?;
    d.check(
        "A saved gain macro resolves the first play chosen now instead of its recorded third play",
        gains(d)? == [initial[0] + 3000, initial[1], initial[2]]
            && d.app().sequence_cursor == 0
            && d.app().scoped.is_some(),
        json!({"gain_delta_mdb":[3000,0,0],"Edit":0}),
        d.snapshot(),
    )?;
    undo(d, &baseline)?;
    choose(d, 1)?;
    let revision = d.revision();
    d.command("gain +=3dB")?;
    d.changed(&revision)?;
    settled(d)?;
    let first = document(d)?.clone();
    choose(d, 3)?;
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    settled(d)?;
    let both = document(d)?.clone();
    d.check(
        "Dot repeats the command's gain step on the newly chosen third play",
        gains(d)? == [initial[0] + 3000, initial[1], initial[2] + 3000]
            && d.app().sequence_cursor == 240
            && d.app().scoped.is_some()
            && d.app().source_cursor == original,
        json!({"gain_delta_mdb":[3000,0,3000],"Edit":240,"Original":original}),
        d.snapshot(),
    )?;
    painted(d, "+ letter: record macro")?;
    painted(d, "+ letter: run macro")?;
    scenarios::footer_anchored(d, "Gain macro and dot keys fit the minimum workspace")?;
    d.capture("Relative gain dot retains the selected play and its independent peers")?;
    undo(d, &first)?;
    let revision = d.revision();
    d.events(
        "Redo the one-play gain step",
        vec![
            super::super::key_event(Key::R, egui::Modifiers::CTRL, true),
            super::super::key_event(Key::R, egui::Modifiers::CTRL, false),
        ],
    )?;
    d.changed(&revision)?;
    settled(d)?;
    d.check(
        "One Redo restores the exact isolated gain document",
        equivalent(document(d)?, &both),
        json!("exact document restored"),
        d.snapshot(),
    )?;
    undo(d, &first)?;
    undo(d, &baseline)?;
    late(d, &baseline, initial)?;
    resize(d, 1280.0, 820.0)
}

fn step() -> AudioChange {
    AudioChange::Step {
        millidecibels: 3000,
    }
}

fn refuse_audio_treatments(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    let before = document(d)?.clone();
    let target = d.app().scoped_target()?;
    let bank = d.app().copied.bank_version();
    let history = d
        .app()
        .workspace
        .as_ref()
        .map(|workspace| (workspace.can_undo, workspace.can_redo));
    for command in ["tail-send reverb -6dB", "wet-send delay -6dB"] {
        d.command(command)?;
        settled(d)?;
        d.check(
            &format!("Scoped recording refuses :{command} before any direct audio edit"),
            d.revision() == revision
                && document(d)? == &before
                && d.app().scoped_target()? == target
                && d.app().copied.bank_version() == bank
                && d.app()
                    .workspace
                    .as_ref()
                    .map(|workspace| (workspace.can_undo, workspace.can_redo))
                    == history
                && d.app().macros.recording_name() == Some('g')
                && d.app().macros.instruction_count() == 0
                && !d.app().macros.is_pending()
                && !d.app().service.is_busy()
                && d.app()
                    .error
                    .as_deref()
                    .is_some_and(|error| error.contains("cannot record sends")),
            json!({"document":"unchanged","history":"unchanged","instructions":0,"pending":false}),
            d.snapshot(),
        )?;
    }
    Ok(())
}
fn choose(d: &mut Driver<'_>, play: u32) -> Result<(), String> {
    if d.app().scoped.is_none() {
        d.key(Key::Enter)?;
    }
    d.command(&format!("scope play {play}"))?;
    settled(d)
}
fn settled(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("One-play gain semantic receipt is admitted", |app| {
        !app.service.is_busy() && !app.macros.is_pending()
    })?;
    d.settled()
}
fn gains(d: &Driver<'_>) -> Result<[i32; 3], String> {
    let workspace = d.app().workspace.as_ref().ok_or("No gain workspace")?;
    let mut gains = [0; 3];
    for (index, frame) in [0, 120, 240].into_iter().enumerate() {
        let picture = workspace
            .plan
            .picture(ProjectFrame(frame))
            .map_err(|e| e.to_string())?;
        let node = &workspace.document.nodes()[&picture.instance.node];
        gains[index] = crate::gain::GainEdit::new(node.audio_treatments.clone())
            .trim()
            .millidecibels();
    }
    Ok(gains)
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
        "One Undo restores the exact gain document and removes only its isolation",
        equivalent(document(d)?, expected) && d.app().scoped.is_none(),
        json!("document restored"),
        d.snapshot(),
    )
}
fn saved(d: &Driver<'_>, name: char) -> Option<Vec<I>> {
    d.app()
        .copied
        .entries()
        .find_map(|(register, content)| match content {
            Content::Macro(program) if register == name => Some(program.instructions().to_vec()),
            _ => None,
        })
}
fn late(d: &mut Driver<'_>, baseline: &ProjectDocument, initial: [i32; 3]) -> Result<(), String> {
    choose(d, 3)?;
    d.chord(&[Key::Q, Key::V])?;
    d.app_mut().feedback.hold_project_updates = true;
    d.key(Key::Plus)?;
    d.wait_for(
        "The gain step commits before its continuation arrives",
        |app| !app.service.is_busy(),
    )?;
    let previous = format!("Previous  {}", d.app().editor_key(EditorKey::PlayPrevious));
    d.click(&previous)?;
    let cursor = d.app().sequence_cursor;
    d.app_mut().feedback.hold_project_updates = false;
    settled(d)?;
    d.check(
        "A delayed gain receipt cannot reclaim another play or append to abandoned recording",
        cursor == 120
            && d.app().sequence_cursor == cursor
            && d.app().scoped.is_none()
            && !d.app().macros.recording()
            && saved(d, 'v').is_none()
            && gains(d)? == [initial[0], initial[1], initial[2] + 3000],
        json!({"Edit":120,"gain_delta_mdb":[0,0,3000],"recording":false}),
        d.snapshot(),
    )?;
    undo(d, baseline)
}
