//! Native buffered EQ, keyboard-only Apply, captured direct commands and undo.
use super::*;
use deadpan_core::{AudioTreatments, EqualizerRecipe, GainDb, NodeId};
use egui::{Event, Key, Modifiers};

fn stroke(key: Key, modifiers: Modifiers, text: &str) -> Vec<Event> {
    vec![
        key_event(key, modifiers, true),
        Event::Text(text.into()),
        key_event(key, modifiers, false),
    ]
}

const COMMAND: &str = "eq bass=3dB middle=-6dB treble=1.5dB low-cut=80Hz high-cut=12000Hz";
fn selected(d: &Driver<'_>) -> Result<NodeId, String> {
    d.app().selected_beat.clone().ok_or("No EQ beat".into())
}
fn recipe(d: &Driver<'_>, owner: &NodeId) -> AudioTreatments {
    d.app().workspace.as_ref().unwrap().document.nodes()[owner]
        .audio_treatments
        .clone()
}
fn edit(d: &mut Driver<'_>, command: &str) -> Result<(), String> {
    let before = d.revision();
    d.command(command)?;
    d.changed(&before)?;
    d.settled()
}
fn undo(d: &mut Driver<'_>) -> Result<(), String> {
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    d.settled()
}
fn prepared(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Latest EQ draft is prepared", |app| {
        app.gain
            .as_ref()
            .is_some_and(|draft| draft.prepared_snapshot().is_some())
    })
}
fn focus(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    for _ in 0..40 {
        if d.harness.root().children_recursive().any(|node| {
            let access = node.accesskit_node();
            access.is_focused() && access.label().as_deref() == Some(label) && !access.is_disabled()
        }) {
            return Ok(());
        }
        d.key(Key::Tab)?;
    }
    Err(format!("Tab could not reach {label}"))
}
fn field(d: &mut Driver<'_>, label: &str, value: &str) -> Result<(), String> {
    focus(d, label)?;
    d.key_modified(Key::A, Modifiers::COMMAND)?;
    d.events("Type native EQ value", vec![Event::Text(value.into())])
}
fn activate(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    focus(d, label)?;
    d.key(Key::Enter)
}
fn resize(d: &mut Driver<'_>, width: f32, height: f32) -> Result<(), String> {
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
    let input = d.harness.input_mut();
    input.screen_rect = Some(viewport);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing EQ viewport")?
        .inner_rect = Some(viewport);
    d.step("Resize EQ workspace", true)
}
fn editor(d: &Driver<'_>) -> Value {
    let state = d.snapshot();
    json!({"source":state["source_cursor"],"edit":state["sequence_cursor"],"selected":state["selected_beat"],"frames":state["duration"]})
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("Offscreen EQ checks native fields, production input, immutable writer proposals and saved recipes. It does not open a physical audio device or establish listening quality; shared PCM and emitted export have separate qualification.".into());
    d.command("sequence")?;
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num4, Key::Num0, Key::L])?;
    let before = d.revision();
    d.key(Key::S)?;
    d.changed(&before)?;
    d.settled()?;
    let owner = selected(d)?;
    edit(d, "saturate 6dB")?;
    edit(d, "gain -3dB")?;
    let initial = recipe(d, &owner);
    let clocks = editor(d);
    let base = d.revision();
    d.command("eq")?;
    prepared(d)?;
    d.check(
        "EQ names its unsaved preview and skips the unused gain waveform",
        d.app().unsaved_previews() == ["EQ"]
            && d.app()
                .gain
                .as_ref()
                .is_some_and(|draft| draft.waveform_state().0.is_none())
            && scenarios::text_paint_visibility(d, "EQ DRAFT · UNSAVED")
                .iter()
                .any(|item| item["fully_visible"] == true)
            && scenarios::text_paint_visibility(d, "EQ OWNER")
                .iter()
                .any(|item| item["fully_visible"] == true),
        json!("EQ preview; no waveform request"),
        d.snapshot(),
    )?;
    d.check(
        "Opening EQ preserves authored state and both clocks",
        d.revision() == base && recipe(d, &owner) == initial && editor(d) == clocks,
        json!(base),
        d.snapshot(),
    )?;
    field(d, "Bass · dB", "3")?;
    field(d, "Middle · dB", "-6")?;
    field(d, "Treble · dB", "1.5")?;
    field(d, "Low cut · Hz / off", "80")?;
    field(d, "High cut · Hz / off", "12000")?;
    d.key(Key::Enter)?;
    d.check(
        "Enter in an EQ text field cannot commit buffered values",
        d.revision() == base && d.rect("Apply  Enter").is_err() && recipe(d, &owner) == initial,
        json!("buffered fields; Apply disabled"),
        d.snapshot(),
    )?;
    activate(d, "Set EQ")?;
    prepared(d)?;
    let proposed = d
        .app()
        .gain
        .as_ref()
        .and_then(|draft| draft.prepared_snapshot())
        .ok_or("Missing EQ proposal")?;
    let expected = proposed.document.nodes()[&owner].audio_treatments.clone();
    d.check(
        "Prepared EQ preserves gain, saturation and serialized stage order",
        expected
            .equalizer()
            .is_some_and(|eq| eq.gains().map(|g| g.millidecibels()) == [3000, -6000, 1500])
            && expected.clip_gain() == initial.clip_gain()
            && expected.saturation() == initial.saturation()
            && expected.order()
                == [
                    deadpan_core::AudioTreatmentStage::ClipGain,
                    deadpan_core::AudioTreatmentStage::Equalizer,
                    deadpan_core::AudioTreatmentStage::Saturation,
                ]
            && d.revision() == base,
        json!(["ClipGain", "Equalizer", "Saturation"]),
        json!(expected),
    )?;
    d.capture("EQ draft at default size")?;
    resize(d, 960.0, 640.0)?;
    for label in [
        "Bass · dB",
        "Middle · dB",
        "Treble · dB",
        "Low cut · Hz / off",
        "High cut · Hz / off",
        "Set EQ",
        "Remove EQ",
        "Before",
        "Draft",
        "Apply  Enter",
        "Cancel  Esc",
    ] {
        focus(d, label)?;
        let paint = scenarios::text_paint_visibility(d, label);
        d.check(
            "EQ control remains keyboard reachable at minimum size",
            d.rect(label).is_ok() && paint.iter().any(|item| item["fully_visible"] == true),
            json!(label),
            json!(paint),
        )?;
    }
    focus(d, "EQ draft keyboard focus")?;
    d.capture("EQ draft at minimum size")?;
    field(d, "Low cut · Hz / off", "15000")?;
    activate(d, "Set EQ")?;
    d.check(
        "Crossed cutoffs refuse without replacing the valid draft",
        d.revision() == base
            && d.rect("Apply  Enter").is_err()
            && d.app()
                .gain
                .as_ref()
                .and_then(|draft| draft.prepared_snapshot())
                .is_some_and(|snapshot| {
                    snapshot.document.nodes()[&owner].audio_treatments == expected
                }),
        json!("invalid passband; previous draft retained"),
        d.snapshot(),
    )?;
    activate(d, "Reset fields")?;
    prepared(d)?;
    activate(d, "Before")?;
    d.check(
        "Before and Draft comparison retain the committed revision",
        d.revision() == base,
        json!(base),
        d.snapshot(),
    )?;
    activate(d, "Draft")?;
    activate(d, "Apply  Enter")?;
    d.changed(&base)?;
    d.settled()?;
    d.check(
        "Keyboard Apply commits one EQ edit and preserves both cursors",
        recipe(d, &owner) == expected && editor(d) == clocks && d.app().gain.is_none(),
        json!(expected),
        d.snapshot(),
    )?;
    d.capture("Saved EQ at minimum size")?;
    undo(d)?;
    d.check(
        "One Undo restores the complete pre-EQ recipe",
        recipe(d, &owner) == initial,
        json!(initial),
        json!(recipe(d, &owner)),
    )?;
    resize(d, 1440.0, 900.0)?;
    d.command("eq")?;
    prepared(d)?;
    field(d, "Middle · dB", "9")?;
    activate(d, "Set EQ")?;
    prepared(d)?;
    d.key(Key::Escape)?;
    d.settled()?;
    d.check(
        "Escape discards EQ without changing the committed recipe",
        recipe(d, &owner) == initial && d.app().gain.is_none(),
        json!(initial),
        d.snapshot(),
    )?;
    edit(d, COMMAND)?;
    let with_eq = recipe(d, &owner);
    edit(d, "eq off")?;
    d.check(
        "EQ off removes only EQ",
        recipe(d, &owner) == initial,
        json!(initial),
        json!(recipe(d, &owner)),
    )?;
    undo(d)?;
    d.check(
        "Undo EQ removal restores the complete recipe",
        recipe(d, &owner) == with_eq,
        json!(with_eq),
        json!(recipe(d, &owner)),
    )?;
    // Re-establish the repeatable recipe after the removal command.
    edit(d, "eq off")?;
    edit(d, COMMAND)?;
    d.key(Key::K)?;
    d.settled()?;
    let left = selected(d)?;
    let before = d.revision();
    d.events("Dot repeats EQ", stroke(Key::Period, Modifiers::NONE, "."))?;
    d.changed(&before)?;
    d.settled()?;
    d.check(
        "Dot repeats the complete EQ on a different beat",
        left != owner && recipe(d, &left).equalizer() == expected.equalizer(),
        json!(expected.equalizer()),
        json!(recipe(d, &left)),
    )?;
    undo(d)?;
    undo(d)?;
    d.key(Key::Q)?;
    d.key(Key::E)?;
    edit(d, COMMAND)?;
    d.key(Key::Q)?;
    d.wait_for("EQ macro saved", |app| {
        !app.service.is_busy() && !app.macros.recording() && !app.macros.is_pending()
    })?;
    d.settled()?;
    d.key(Key::J)?;
    d.settled()?;
    let before = d.revision();
    let mut events = stroke(Key::Num2, Modifiers::SHIFT, "@");
    events.extend(stroke(Key::E, Modifiers::NONE, "e"));
    d.events("Replay EQ macro", events)?;
    d.changed(&before)?;
    d.settled()?;
    d.check(
        "Macro replays EQ as one undoable transaction",
        selected(d)? == owner && recipe(d, &owner).equalizer() == expected.equalizer(),
        json!(expected.equalizer()),
        json!(recipe(d, &owner)),
    )?;
    undo(d)?;
    let capture = d.app().capture_gain_target();
    let macro_capture = d.app().capture_macro_target();
    edit(d, "gain -6dB")?;
    let before = d.revision();
    let chosen =
        Some(EqualizerRecipe::new([GainDb::UNITY; 3], None, None).map_err(|e| e.to_string())?);
    d.app_mut()
        .equalizer_command(Some(capture), Some(macro_capture), chosen);
    d.settled()?;
    d.check(
        "A stale captured EQ command cannot cross a revision",
        d.revision() == before && d.app().error.is_some(),
        json!(before),
        d.snapshot(),
    )?;
    d.app_mut().equalizer_command(None, None, chosen);
    d.settled()?;
    d.check(
        "Absent EQ capture cannot use a later selected beat",
        d.revision() == before && d.app().error.is_some(),
        json!(before),
        d.snapshot(),
    )?;
    let before = d.revision();
    d.command("eq bass=25dB middle=0dB treble=0dB")?;
    d.settled()?;
    d.check(
        "Invalid typed EQ changes no history",
        d.revision() == before && d.app().error.is_some(),
        json!(before),
        d.snapshot(),
    )?;
    edit(d, "wrap-repeat 3")?;
    let repeat = selected(d)?;
    d.key(Key::Enter)?;
    d.command("scope play 2")?;
    d.settled()?;
    let original = recipe(d, &owner);
    edit(d, COMMAND)?;
    let isolated = d.app().scoped_target()?.ok_or("Missing EQ play")?;
    d.check(
        "EQ on one Repeat play isolates only that occurrence",
        isolated.target.node != owner
            && recipe(d, &isolated.target.node).equalizer() == expected.equalizer()
            && recipe(d, &owner) == original
            && d.app().workspace.as_ref().is_some_and(|w| {
                w.document
                    .overrides()
                    .get(&repeat)
                    .is_some_and(|p| p.len() == 1)
            }),
        json!("one isolated EQ play"),
        d.snapshot(),
    )?;
    undo(d)?;
    d.check(
        "Undo removes the EQ occurrence isolation",
        recipe(d, &owner) == original
            && d.app()
                .workspace
                .as_ref()
                .is_some_and(|w| w.document.overrides().is_empty()),
        json!("original shared play"),
        d.snapshot(),
    )
}
