//! `:scope plays`, `:explode` and `:duplicate` through native command entry,
//! the project service, SQLite and Metal.

use super::*;
use deadpan_core::{
    GainDb, NodeId, NodeKind, ProjectDocument, RegisterName, RegisterValue, SemanticInstruction,
    SemanticSelector,
};
use deadpan_store::{AccessMode, ProjectStore};
use egui::Key;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    idle(d)?;
    let frames = d.app().sequence_length();
    let before = d.revision();
    d.command("wrap-repeat 3")?;
    d.changed(&before)?;
    idle(d)?;
    let repeat = d.app().selected_beat.clone().ok_or("No Repeat selected")?;
    let shared = match &document(d)?.nodes()[&repeat].kind {
        NodeKind::Repeat { child, .. } => child.clone(),
        _ => return Err("Expected a Repeat".into()),
    };
    several_plays(d, &repeat, &shared)?;
    exploded(d, &repeat, frames * 3)?;
    duplicated(d, &repeat, frames * 3)?;
    d.report.skipped.push("Keyboard input is scripted production event batches through the native router; physical keys, IME and VoiceOver are separate checks. Exact picture and PCM preservation of explode is covered by the core and decoded-audio tests.".into());
    Ok(())
}

fn several_plays(d: &mut Driver<'_>, repeat: &NodeId, shared: &NodeId) -> Result<(), String> {
    d.key(Key::Enter)?;
    let browsed = document(d)?.clone();
    d.command("scope plays 2-3")?;
    idle(d)?;
    d.check(
        "Choosing plays 2-3 is read-only and browses play 2",
        document(d)? == &browsed && d.app().scoped.is_some() && d.app().sequence_cursor == 120,
        json!({"Edit":120,"history":"unchanged"}),
        d.snapshot(),
    )?;
    painted(d, "Plays 2, 3 edited together  :scope plays")?;
    d.capture("Two plays selected together in Repeat contents")?;
    let before = d.revision();
    d.command("gain -6")?;
    d.changed(&before)?;
    idle(d)?;
    let roots = play_roots(d, repeat)?;
    d.check(
        "One gain edit isolates exactly plays 2 and 3 with the same value",
        roots.len() == 2
            && roots
                .iter()
                .all(|root| gain(d, root).ok() == crate::gain::parse_db("-6").ok())
            && gain(d, shared)? == crate::gain::parse_db("0")?
            && d.app().scoped.is_some(),
        json!({"overrides":2,"plays_2_3_db":-6,"play_1_db":0}),
        d.snapshot(),
    )?;
    let before = d.revision();
    d.command("gain -3dB range=10-20")?;
    d.changed(&before)?;
    idle(d)?;
    let ranged = play_roots(d, repeat)?
        .iter()
        .map(|root| {
            document(d).map(|document| {
                document.nodes()[root]
                    .audio_treatments
                    .clip_gain()
                    .is_some_and(|clip| clip.envelopes().len() == 1)
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    d.check(
        "A ranged step lands inside each selected play only, keeping its -6 dB trim",
        ranged == [true, true]
            && play_roots(d, repeat)?
                .iter()
                .all(|root| gain(d, root).ok() == crate::gain::parse_db("-6").ok())
            && document(d)?.nodes()[shared].audio_treatments.is_empty(),
        json!({"ranged_plays":[2,3],"play_1":"unchanged"}),
        d.snapshot(),
    )?;
    d.capture("Ranged gain inside plays 2 and 3")?;
    d.key(Key::Escape)?;
    idle(d)
}

fn exploded(d: &mut Driver<'_>, repeat: &NodeId, frames: u64) -> Result<(), String> {
    let before = d.revision();
    d.command("explode")?;
    d.changed(&before)?;
    idle(d)?;
    let children = sequence_children(d, repeat)?;
    d.check(
        "Explode turns the Repeat into three independent plays at the same length",
        children.len() == 3
            && d.app().sequence_length() == frames
            && d.app().selected_beat.as_ref() == Some(repeat)
            && document(d)?.overrides().is_empty(),
        json!({"children":3,"frames":frames,"selected":"same beat"}),
        d.snapshot(),
    )?;
    d.capture("Exploded Repeat is an ordinary group of plays")?;
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "One Undo restores the Repeat and its play overrides",
        matches!(document(d)?.nodes()[repeat].kind, NodeKind::Repeat { .. })
            && document(d)?
                .overrides()
                .get(repeat)
                .is_some_and(|overrides| overrides.len() == 2),
        json!({"kind":"Repeat","overrides":2}),
        d.snapshot(),
    )?;
    if d.app().selected_beat.as_ref() != Some(repeat) {
        return Err("Undo did not keep the Repeat selected".into());
    }
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Dot repeats Explode on the selected Repeat",
        sequence_children(d, repeat)?.len() == 3 && d.app().sequence_length() == frames,
        json!({"children":3}),
        d.snapshot(),
    )
}

fn duplicated(d: &mut Driver<'_>, group: &NodeId, frames: u64) -> Result<(), String> {
    let root = document(d)?.root().clone();
    let count = sequence_children(d, &root)?.len();
    d.chord(&[Key::Q, Key::Y])?;
    let before = d.revision();
    d.command("duplicate")?;
    d.changed(&before)?;
    idle(d)?;
    d.key(Key::Q)?;
    d.wait_for("Duplicate macro is durably saved", |app| {
        !app.service.is_busy() && !app.macros.recording() && !app.macros.is_pending()
    })?;
    idle(d)?;
    let saved = ProjectStore::open(
        &d.app().workspace.as_ref().ok_or("No workspace")?.path,
        AccessMode::ReadOnly,
    )
    .and_then(|store| store.registers())
    .map_err(|error| error.to_string())?;
    let expected = [SemanticInstruction::Duplicate {
        selector: SemanticSelector::SelectedBeat,
    }];
    d.check(
        "Recording stores Duplicate as one selected-beat instruction",
        matches!(
            saved.entries.get(&RegisterName::new('y').map_err(|error| error.to_string())?).map(AsRef::as_ref),
            Some(RegisterValue::Macro { program }) if program.instructions() == expected
        ),
        json!(expected),
        d.snapshot(),
    )?;
    let after = sequence_children(d, &root)?;
    let copy = d.app().selected_beat.clone().ok_or("No copy selected")?;
    d.check(
        "Duplicate inserts a fresh copy after the group and selects it",
        after.len() == count + 1
            && after
                .iter()
                .position(|node| node == group)
                .map(|index| index + 1)
                == after.iter().position(|node| node == &copy)
            && copy != *group
            && d.app().sequence_length() == frames * 2,
        json!({"root_children":count + 1,"frames":frames * 2}),
        d.snapshot(),
    )?;
    d.capture("Duplicated group follows its original")?;
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "One Undo removes the duplicate",
        sequence_children(d, &root)?.len() == count && d.app().sequence_length() == frames,
        json!({"root_children":count}),
        d.snapshot(),
    )
}

fn play_roots(d: &Driver<'_>, repeat: &NodeId) -> Result<Vec<NodeId>, String> {
    Ok(document(d)?
        .overrides()
        .get(repeat)
        .map(|overrides| overrides.iter().map(|(_, root)| root.clone()).collect())
        .unwrap_or_default())
}
fn sequence_children(d: &Driver<'_>, node: &NodeId) -> Result<Vec<NodeId>, String> {
    match &document(d)?.nodes()[node].kind {
        NodeKind::Sequence { children } => Ok(children.clone()),
        _ => Ok(Vec::new()),
    }
}
fn document<'a>(d: &'a Driver<'_>) -> Result<&'a ProjectDocument, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.document.as_ref())
        .ok_or_else(|| "No workspace".into())
}
fn gain(d: &Driver<'_>, node: &NodeId) -> Result<GainDb, String> {
    Ok(crate::gain::GainEdit::new(document(d)?.nodes()[node].audio_treatments.clone()).trim())
}
fn idle(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Structure edit receipt is admitted", |app| {
        !app.service.is_busy() && !app.repeat_queue.active()
    })?;
    d.settled()
}
fn painted(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    let parts = scenarios::text_paint_visibility(d, label);
    d.check(
        &format!("Label is fully painted: {label}"),
        !parts.is_empty() && parts.iter().all(|part| part["fully_visible"] == true),
        json!("text fits its paint clip without later opaque overlap"),
        json!(parts),
    )
}
