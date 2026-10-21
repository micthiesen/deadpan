//! Typed audio values resolve each invocation's explicit play authority.
use super::*;
use crate::preview::copied::Content;
use deadpan_core::{AudioChange, AudioTreatments, SemanticInstruction as I};

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let baseline = document(d)?.clone();
    let original = d.app().source_cursor;
    let mut checkpoints = Vec::new();
    resize(d, 960.0, 640.0)?;
    choose(d, 1)?;
    let foreign = d.app().capture_macro_target();
    choose(d, 3)?;
    let target = d.app().capture_gain_target();
    let revision = d.revision();
    d.app_mut()
        .record_audio_change(Some(target), Some(foreign), change("gain -6dB")?);
    d.check(
        "A typed setter cannot borrow another captured play's authority",
        d.revision() == revision
            && equivalent(document(d)?, &baseline)
            && !d.app().macros.is_pending()
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("plays differ")),
        json!("unchanged"),
        d.snapshot(),
    )?;
    d.chord(&[Key::Q, Key::T])?;
    painted(d, "save macro")?;
    painted(d, "gain 3 dB")?;
    painted(d, "mute / unmute")?;
    scenarios::footer_anchored(
        d,
        "Typed audio recording keeps the verified compact key hints",
    )?;
    refuse(d)?;
    let commands = [
        "gain -6dB",
        "saturate 6dB",
        "balance left",
        "eq bass=3dB middle=0dB treble=0dB",
    ];
    let initial = recipe(d, 240)?;
    let mut expected = initial.clone();
    for (index, command) in commands.iter().enumerate() {
        edit(d, command, &mut checkpoints)?;
        expected = change(command)?
            .apply(&expected)
            .map_err(|e| e.to_string())?;
        d.check(
            "Recording a literal audio setter changes only the selected third play",
            recipe(d, 240)? == expected
                && recipe(d, 0)? == recipe_from(&baseline, d, 0)?
                && recipe(d, 120)? == recipe_from(&baseline, d, 120)?
                && d.app().macros.instruction_count() == index + 1
                && d.app().sequence_cursor == 240,
            json!({"command":command,"instructions":index+1,"Edit":240}),
            d.snapshot(),
        )?;
    }
    let stable = document(d)?.clone();
    let target = d.app().capture_gain_target();
    let capture = d.app().capture_macro_target();
    d.app_mut()
        .record_audio_change(Some(target), Some(capture), change(commands[3])?);
    d.check(
        "A same-state typed setter appends no instruction or history",
        document(d)? == &stable
            && d.app().macros.instruction_count() == 4
            && !d.app().macros.is_pending(),
        json!({"instructions":4,"document":"unchanged"}),
        d.snapshot(),
    )?;
    save(d, 't', &commands)?;
    while let Some(previous) = checkpoints.pop() {
        undo(d, &previous)?;
    }
    choose(d, 1)?;
    execute(d, 't', &mut checkpoints)?;
    d.check(
        "A saved four-setter macro resolves the newly chosen first play in one transaction",
        recipe(d, 0)? == expected && recipe(d, 240)? == initial && d.app().sequence_cursor == 0,
        json!({"target":"first play","other_plays":"unchanged"}),
        d.snapshot(),
    )?;
    choose(d, 3)?;
    for command in [
        "gain -9dB",
        "saturate 9dB",
        "balance right",
        "eq bass=-3dB middle=0dB treble=0dB",
    ] {
        edit(d, command, &mut checkpoints)?;
    }
    let third = recipe(d, 240)?;
    let first = recipe(d, 0)?;
    choose(d, 1)?;
    let revision = d.revision();
    checkpoints.push(document(d)?.clone());
    d.key(Key::Period)?;
    d.changed(&revision)?;
    settled(d)?;
    d.check(
        "Dot replaces only EQ on a differently configured newly selected play",
        recipe(d, 0)?
            == change("eq bass=-3dB middle=0dB treble=0dB")?
                .apply(&first)
                .map_err(|e| e.to_string())?
            && recipe(d, 240)? == third
            && d.app().sequence_cursor == 0,
        json!({"dot":"literal EQ -3 dB bass","other_treatments":"retained"}),
        d.snapshot(),
    )?;
    choose(d, 3)?;
    d.chord(&[Key::Q, Key::Y])?;
    let removals = ["saturate off", "balance center", "eq off"];
    for command in removals {
        edit(d, command, &mut checkpoints)?;
    }
    save(d, 'y', &removals)?;
    choose(d, 1)?;
    execute(d, 'y', &mut checkpoints)?;
    d.check(
        "A saved removal macro clears only its named stages on the current play",
        recipe(d, 0)?.saturation().is_none()
            && recipe(d, 0)?.balance().is_none()
            && recipe(d, 0)?.equalizer().is_none()
            && recipe(d, 0)?.trim() == crate::gain::parse_db("-6")?
            && recipe(d, 240)?.trim() == crate::gain::parse_db("-9")?
            && d.app().source_cursor == original,
        json!({"stages":"removed","trim_db":[-6,-9],"Original":original}),
        d.snapshot(),
    )?;
    let last = document(d)?.clone();
    let revision = d.revision();
    super::super::macros::execute(d, 'y', None)?;
    settled(d)?;
    d.check(
        "A same-state removal call refuses without changing authored state",
        d.revision() == revision && document(d)? == &last && !d.app().macros.is_pending(),
        json!("no edit"),
        d.snapshot(),
    )?;
    d.capture("One-play audio macros retain literal values and stage removals")?;
    let previous = checkpoints.pop().ok_or("Missing setter checkpoint")?;
    undo(d, &previous)?;
    let revision = d.revision();
    d.events(
        "Redo the complete setter macro",
        vec![
            super::super::key_event(Key::R, egui::Modifiers::CTRL, true),
            super::super::key_event(Key::R, egui::Modifiers::CTRL, false),
        ],
    )?;
    d.changed(&revision)?;
    settled(d)?;
    d.check(
        "One Redo restores the exact multi-setter document",
        equivalent(document(d)?, &last),
        json!("exact document"),
        d.snapshot(),
    )?;
    undo(d, &previous)?;
    while let Some(previous) = checkpoints.pop() {
        undo(d, &previous)?;
    }
    late(d, &baseline)?;
    d.check(
        "Typed setter verification restores the baseline and independent Original cursor",
        equivalent(document(d)?, &baseline) && d.app().source_cursor == original,
        json!({"baseline":"restored","Original":original}),
        d.snapshot(),
    )?;
    resize(d, 1280.0, 820.0)
}

fn change(command: &str) -> Result<AudioChange, String> {
    use crate::navigation::command::{Entry, parse};
    Ok(match parse(command)? {
        Entry::Gain(Some(gain)) => AudioChange::Trim { gain },
        Entry::Saturate(stage) => AudioChange::Saturation {
            drive: stage.map(deadpan_core::Saturation::drive),
        },
        Entry::Balance(balance) => AudioChange::Balance { balance },
        Entry::Equalizer(recipe) => AudioChange::Equalizer { recipe },
        _ => return Err("Expected a typed audio setter".into()),
    })
}
fn recipe(d: &Driver<'_>, frame: i64) -> Result<AudioTreatments, String> {
    let workspace = d.app().workspace.as_ref().ok_or("No setter workspace")?;
    let picture = workspace
        .plan
        .picture(ProjectFrame(frame))
        .map_err(|e| e.to_string())?;
    Ok(workspace.document.nodes()[&picture.instance.node]
        .audio_treatments
        .clone())
}
fn recipe_from(
    document: &ProjectDocument,
    d: &Driver<'_>,
    frame: i64,
) -> Result<AudioTreatments, String> {
    let workspace = d.app().workspace.as_ref().ok_or("No setter workspace")?;
    let picture = workspace
        .plan
        .picture(ProjectFrame(frame))
        .map_err(|e| e.to_string())?;
    Ok(document
        .nodes()
        .get(&picture.instance.node)
        .ok_or("An untouched peer changed identity")?
        .audio_treatments
        .clone())
}
fn choose(d: &mut Driver<'_>, play: u32) -> Result<(), String> {
    if d.app().scoped.is_none() {
        d.key(Key::Enter)?;
    }
    d.command(&format!("scope play {play}"))?;
    settled(d)
}
fn settled(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Typed audio semantic receipt is admitted", |app| {
        !app.service.is_busy() && !app.macros.is_pending()
    })?;
    d.settled()
}
fn edit(
    d: &mut Driver<'_>,
    command: &str,
    checkpoints: &mut Vec<ProjectDocument>,
) -> Result<(), String> {
    let revision = d.revision();
    checkpoints.push(document(d)?.clone());
    d.command(command)?;
    d.changed(&revision)?;
    settled(d)
}
fn execute(
    d: &mut Driver<'_>,
    name: char,
    checkpoints: &mut Vec<ProjectDocument>,
) -> Result<(), String> {
    let revision = d.revision();
    checkpoints.push(document(d)?.clone());
    super::super::macros::execute(d, name, None)?;
    d.changed(&revision)?;
    settled(d)
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
        "One Undo restores the exact setter document and occurrence isolation",
        equivalent(document(d)?, expected) && d.app().scoped.is_none(),
        json!("restored"),
        d.snapshot(),
    )
}
fn save(d: &mut Driver<'_>, name: char, commands: &[&str]) -> Result<(), String> {
    d.key(Key::Q)?;
    settled(d)?;
    let expected = commands
        .iter()
        .map(|command| change(command).map(|change| I::SetAudio { change }))
        .collect::<Result<Vec<_>, _>>()?;
    let saved = d
        .app()
        .copied
        .entries()
        .find_map(|(register, content)| match content {
            Content::Macro(program) if register == name => Some(program.instructions().to_vec()),
            _ => None,
        });
    d.check(
        "Saved audio setters contain literal typed values without concrete owner or play IDs",
        saved == Some(expected.clone()),
        json!(expected),
        d.snapshot(),
    )
}
fn refuse(d: &mut Driver<'_>) -> Result<(), String> {
    let before = document(d)?.clone();
    let bank = d.app().copied.bank_version();
    for command in [
        "gain",
        "eq",
        "gain +3dB range=4-10",
        "wet-send reverb -6dB",
        "tail-send delay -6dB",
    ] {
        d.command(command)?;
        settled(d)?;
        d.check(
            "Scoped setter recording excludes drafts, ranges and sends",
            document(d)? == &before
                && d.app().copied.bank_version() == bank
                && d.app().gain.is_none()
                && d.app().macros.recording_name() == Some('t')
                && d.app().macros.instruction_count() == 0
                && !d.app().macros.is_pending()
                && d.app().error.is_some(),
            json!({"refused":command,"instructions":0,"document":"unchanged"}),
            d.snapshot(),
        )?;
    }
    Ok(())
}
fn late(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    choose(d, 3)?;
    d.chord(&[Key::Q, Key::W])?;
    d.app_mut().feedback.hold_project_updates = true;
    d.command("balance left")?;
    d.wait_for(
        "A typed setter commits before its continuation arrives",
        |app| !app.service.is_busy(),
    )?;
    let previous = format!("Previous  {}", d.app().editor_key(EditorKey::PlayPrevious));
    d.click(&previous)?;
    let cursor = d.app().sequence_cursor;
    d.app_mut().feedback.hold_project_updates = false;
    settled(d)?;
    d.check(
        "Delayed setter completion cannot reclaim a newer play or append into abandoned recording",
        cursor == 120
            && d.app().sequence_cursor == cursor
            && d.app().scoped.is_none()
            && !d.app().macros.recording()
            && recipe(d, 240)?.balance()
                == Some(deadpan_core::StereoBalance::new(-100).map_err(|e| e.to_string())?),
        json!({"Edit":120,"recording":false,"third_play":"left"}),
        d.snapshot(),
    )?;
    undo(d, baseline)
}
