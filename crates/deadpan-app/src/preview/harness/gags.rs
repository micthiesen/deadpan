//! Specification §8.4 and §31 step 8: a built-in gag expands to ordinary,
//! editable beats under a group that pins its recipe, and pauses record in
//! macros.

use super::*;
use egui::Key;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let duration = d.app().sequence_length();
    d.command("sequence")?;
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num3, Key::Num0, Key::L])?;
    let revision = d.revision();
    d.command("gag long-answer pause=12f creep=1.5")?;
    d.changed(&revision)?;
    d.settled()?;
    let workspace = d.app().workspace.clone().ok_or("No project")?;
    let group = d.app().selected_beat.clone().ok_or("No gag group")?;
    let node = &workspace.document.nodes()[&group];
    let framed_pause = match &node.kind {
        NodeKind::Sequence { children } => children.first().is_some_and(|child| {
            let beat = &workspace.document.nodes()[child];
            matches!(beat.kind, NodeKind::Hold { .. }) && beat.framing.is_some()
        }),
        _ => false,
    };
    d.check(
        "The Long Answer inserts a framed pause under a group that pins the recipe",
        node.label == "The Long Answer · v1 · pause 12f, creep to 1.500×"
            && framed_pause
            && d.app().sequence_length() == duration + 12
            && d.app().message.as_deref() == Some("Applied The Long Answer."),
        json!({"label":"The Long Answer · v1 · pause 12f, creep to 1.500×","frames":duration + 12}),
        json!({"label":node.label,"framed_pause":framed_pause,"frames":d.app().sequence_length(),"message":d.app().message}),
    )?;
    d.capture("The Long Answer gag")?;
    let applied = d.revision();
    d.key(Key::U)?;
    d.changed(&applied)?;
    d.check(
        "One undo removes the whole gag",
        d.app().sequence_length() == duration,
        json!(duration),
        json!(d.app().sequence_length()),
    )?;

    // Record a pause in macro a, then replay it elsewhere.
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    d.chord(&[Key::Q, Key::A])?;
    let before = d.revision();
    d.chord(&[Key::Comma, Key::H])?;
    d.changed(&before)?;
    d.settled()?;
    d.key(Key::Q)?;
    d.wait_for("Macro a saved", |app| {
        !app.service.is_busy() && !app.macros.is_pending() && !app.macros.recording()
    })?;
    let recorded = d.app().sequence_length();
    d.chord(&[Key::G, Key::G, Key::Num5, Key::Num0, Key::L])?;
    let before = d.revision();
    let shifted = |key, pressed| egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::SHIFT,
    };
    d.events(
        "Run macro a with native @ text",
        vec![
            shifted(Key::Num2, true),
            egui::Event::Text("@".into()),
            shifted(Key::Num2, false),
        ],
    )?;
    d.key(Key::A)?;
    d.changed(&before)?;
    d.settled()?;
    let pauses = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No project")?
        .document
        .nodes()
        .values()
        .filter(|node| matches!(node.kind, NodeKind::Hold { .. }))
        .count();
    d.check(
        "A recorded pause replays as a pause at the new cursor",
        recorded == duration + 15 && d.app().sequence_length() == duration + 30 && pauses == 2,
        json!({"after_recording":duration + 15,"after_replay":duration + 30,"holds":2}),
        json!({"after_recording":recorded,"after_replay":d.app().sequence_length(),"holds":pauses,"message":d.app().message,"error":d.app().error}),
    )?;
    Ok(())
}
