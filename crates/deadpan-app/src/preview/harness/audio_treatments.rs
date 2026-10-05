//! Specification §8.3 "Saturation", §6.4 `:gain +6dB` and §7.4 dot-repeat
//! and macros: `:saturate`, typed and relative `:gain`, `.` on another beat
//! and a recorded macro, each through the production router, the semantic
//! project service and the store, with one Undo per edit.

use super::*;
use deadpan_core::{AudioTreatmentStage, AudioTreatments, NodeId};
use egui::{Event, Key, Modifiers};

fn treatments(d: &Driver<'_>, node: &NodeId) -> AudioTreatments {
    d.app()
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.document.nodes().get(node))
        .map(|node| node.audio_treatments.clone())
        .unwrap_or_default()
}

fn drive(treatments: &AudioTreatments) -> Option<i32> {
    treatments
        .saturation()
        .map(|stage| stage.drive().millidecibels())
}

fn trim(treatments: &AudioTreatments) -> Option<i32> {
    treatments
        .clip_gain()
        .map(|clip| clip.trim().millidecibels())
}

fn describe(treatments: &AudioTreatments) -> Value {
    json!({
        "order": treatments.order().iter().map(|stage| format!("{stage:?}")).collect::<Vec<_>>(),
        "trim": trim(treatments),
        "drive": drive(treatments),
    })
}

fn selected(d: &Driver<'_>) -> Result<NodeId, String> {
    d.app()
        .selected_beat
        .clone()
        .ok_or_else(|| "No beat is selected".to_owned())
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

fn stroke(key: Key, modifiers: Modifiers, text: Option<&str>) -> Vec<Event> {
    let mut events = vec![key_event(key, modifiers, true)];
    if let Some(text) = text {
        events.push(Event::Text(text.into()));
    }
    events.push(key_event(key, modifiers, false));
    events
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("Saturation is checked as authored recipes and their inspector text; the soft-clipped PCM is verified by the deadpan-audio stage tests and the release preview/export fixture, not by listening.".into());
    d.command("sequence")?;
    super::transcript::focus_your_edit(d)?;
    // Two beats: split the Original at frame 40.
    d.chord(&[Key::G, Key::G, Key::Num4, Key::Num0, Key::L])?;
    let before = d.revision();
    d.key(Key::S)?;
    d.changed(&before)?;
    d.settled()?;
    let right = selected(d)?;

    edit(d, "saturate 12dB")?;
    let driven = treatments(d, &right);
    d.check(
        ":saturate 12dB adds a 12 dB saturation stage to the selected beat without changing time",
        drive(&driven) == Some(12_000)
            && driven.order() == [AudioTreatmentStage::Saturation]
            && d.app().sequence_length() == 120
            && d.app().error.is_none(),
        json!({"order":["Saturation"],"drive":12000,"frames":120}),
        json!({"recipe":describe(&driven),"frames":d.app().sequence_length(),"error":d.app().error,"message":d.app().message}),
    )?;
    d.step("Paint the saturation in the inspector", true)?;
    let painted = scenarios::text_paint_visibility(d, "12 dB drive");
    d.check(
        "The inspector lists the drive",
        !painted.is_empty(),
        json!("12 dB drive"),
        json!(painted),
    )?;
    d.capture("Saturation on the selected beat")?;

    // Typed and relative gain keep the saturation and its order.
    edit(d, "gain +6dB")?;
    edit(d, "gain +=3dB")?;
    edit(d, "gain -=1.5dB")?;
    let gained = treatments(d, &right);
    d.check(
        ":gain +6dB sets the trim, +=3dB and -=1.5dB change it, and clip gain goes before saturation",
        trim(&gained) == Some(7_500)
            && drive(&gained) == Some(12_000)
            && gained.order() == [AudioTreatmentStage::ClipGain, AudioTreatmentStage::Saturation],
        json!({"trim":7500,"drive":12000,"order":["ClipGain","Saturation"]}),
        describe(&gained),
    )?;
    undo(d)?;
    undo(d)?;
    undo(d)?;
    undo(d)?;
    d.check(
        "Four undos remove the gain changes and the saturation one at a time",
        treatments(d, &right).is_empty(),
        json!({}),
        describe(&treatments(d, &right)),
    )?;

    // Dot repeats the saturation on the other beat.
    edit(d, "saturate 9dB")?;
    d.key(Key::K)?;
    d.settled()?;
    let left = selected(d)?;
    let before = d.revision();
    d.events(
        "Dot repeats the saturation",
        stroke(Key::Period, Modifiers::NONE, Some(".")),
    )?;
    d.changed(&before)?;
    d.settled()?;
    d.check(
        ". applies the same 9 dB saturation to the newly selected beat",
        left != right
            && drive(&treatments(d, &left)) == Some(9_000)
            && drive(&treatments(d, &right)) == Some(9_000),
        json!({"left":9000,"right":9000}),
        json!({"left":describe(&treatments(d, &left)),"right":describe(&treatments(d, &right)),"error":d.app().error}),
    )?;
    undo(d)?;
    undo(d)?;

    // A recorded macro replays the gain step and saturation on another beat.
    d.key(Key::Q)?;
    d.key(Key::A)?;
    let before = d.revision();
    d.key(Key::Plus)?;
    d.changed(&before)?;
    d.settled()?;
    edit(d, "saturate 6dB")?;
    d.key(Key::Q)?;
    d.wait_for("The macro is saved", |app| {
        !app.service.is_busy() && !app.macros.recording() && !app.macros.is_pending()
    })?;
    d.settled()?;
    let recorded = d.app().copied.entries().any(|(slot, value)| {
        slot == 'a' && matches!(value, crate::preview::copied::Content::Macro(_))
    });
    d.check(
        "Recording keeps + and :saturate as edits of the recorded beat",
        recorded
            && trim(&treatments(d, &left)) == Some(3_000)
            && drive(&treatments(d, &left)) == Some(6_000),
        json!({"macro":"a","trim":3000,"drive":6000}),
        json!({"macro":recorded,"recipe":describe(&treatments(d, &left)),"error":d.app().error}),
    )?;
    d.key(Key::J)?;
    d.settled()?;
    let before = d.revision();
    let mut events = stroke(Key::Num2, Modifiers::SHIFT, Some("@"));
    events.extend(stroke(Key::A, Modifiers::NONE, Some("a")));
    d.events("Run macro a", events)?;
    d.changed(&before)?;
    d.settled()?;
    d.check(
        "@a applies the recorded +3 dB and 6 dB saturation to the other beat as one edit",
        selected(d)? == right
            && trim(&treatments(d, &right)) == Some(3_000)
            && drive(&treatments(d, &right)) == Some(6_000),
        json!({"trim":3000,"drive":6000}),
        json!({"recipe":describe(&treatments(d, &right)),"error":d.app().error}),
    )?;
    undo(d)?;
    d.check(
        "One undo removes the whole macro run",
        treatments(d, &right).is_empty(),
        json!({}),
        describe(&treatments(d, &right)),
    )?;
    pitch(d)
}

fn pitch_of(d: &Driver<'_>) -> Option<(deadpan_core::PitchPolicy, i64, i64)> {
    let node = d.app().selected_beat.clone()?;
    match &d
        .app()
        .workspace
        .as_ref()?
        .document
        .nodes()
        .get(&node)?
        .kind
    {
        deadpan_core::NodeKind::Retime {
            pitch,
            duration,
            mapping,
            purpose: deadpan_core::RetimePurpose::Edit,
            ..
        } => Some((*pitch, duration.frames(), mapping.duration().frames())),
        _ => None,
    }
}

/// `:pitch` shifts whole semitones without changing duration, on a
/// unity-speed stage for a plain beat and in place on a speed change.
fn pitch(d: &mut Driver<'_>) -> Result<(), String> {
    use deadpan_core::PitchPolicy;
    let length = d.app().sequence_length();
    edit(d, "pitch +5st")?;
    let shifted = pitch_of(d);
    d.check(
        ":pitch +5st wraps the selected beat in a unity-speed stage shifted five semitones",
        matches!(shifted, Some((PitchPolicy::Shift { semitones: 5 }, a, b)) if a == b)
            && d.app().sequence_length() == length,
        json!({"pitch":"+5st","speed":1,"frames":length}),
        json!({"retime":format!("{shifted:?}"),"frames":d.app().sequence_length(),"error":d.app().error}),
    )?;
    d.step("Paint the shifted beat's inspector", true)?;
    let painted = scenarios::text_paint_visibility(d, "Shifted +5 semitones");
    d.check(
        "The inspector names the shift",
        !painted.is_empty(),
        json!("Shifted +5 semitones"),
        json!(painted),
    )?;
    d.capture("Pitch shift on the selected beat")?;
    edit(d, "pitch -3st")?;
    d.check(
        ":pitch -3st changes the existing stage in place",
        matches!(pitch_of(d), Some((PitchPolicy::Shift { semitones: -3 }, a, b)) if a == b),
        json!("-3st"),
        json!(format!("{:?}", pitch_of(d))),
    )?;
    edit(d, "pitch 0")?;
    d.check(
        ":pitch 0 returns the stage to plain pitch preservation at the same speed",
        matches!(pitch_of(d), Some((PitchPolicy::Preserve, a, b)) if a == b),
        json!("preserve"),
        json!(format!("{:?}", pitch_of(d))),
    )?;
    undo(d)?;
    undo(d)?;
    undo(d)?;
    d.check(
        "Three undos remove the stage",
        pitch_of(d).is_none() && d.app().sequence_length() == length,
        json!(null),
        json!(format!("{:?}", pitch_of(d))),
    )
}
