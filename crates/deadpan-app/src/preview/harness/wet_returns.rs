//! Real command routing and durable sends; PCM is checked in deadpan-audio.
use super::*;
use deadpan_core::{GainDb, NodeId, TailEffect};
use egui::{Event, Key, Modifiers};

fn selected(d: &Driver<'_>) -> Result<NodeId, String> {
    d.app()
        .selected_beat
        .clone()
        .ok_or("No selected beat".into())
}
fn level(d: &Driver<'_>, node: &NodeId, effect: TailEffect) -> Option<i32> {
    d.app()
        .workspace
        .as_ref()?
        .document
        .nodes()
        .get(node)?
        .audio_treatments
        .wet_sends()
        .level(effect)
        .map(GainDb::millidecibels)
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
fn stroke(key: Key, modifiers: Modifiers, text: &str) -> Vec<Event> {
    vec![
        key_event(key, modifiers, true),
        Event::Text(text.into()),
        key_event(key, modifiers, false),
    ]
}

fn visible(d: &Driver<'_>, text: &str) -> bool {
    scenarios::text_paint_visibility(d, text)
        .iter()
        .any(|paint| paint["fully_visible"] == true)
}

fn inspector_top(d: &mut Driver<'_>, heading: &str) -> Result<(), String> {
    let point = d.rect(heading)?.center() + egui::vec2(0.0, 120.0);
    d.events(
        "Scroll inspector to top",
        vec![
            Event::PointerMoved(point),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, 2000.0),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::NONE,
            },
        ],
    )?;
    for _ in 0..8 {
        d.step("Settle inspector scroll", false)?;
    }
    Ok(())
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("This offscreen replay verifies commands, persistence and painting. It does not start an audio device; decoded PCM and export checks establish the sound.".into());
    d.command("sequence")?;
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num4, Key::Num0, Key::L])?;
    let before = d.revision();
    d.key(Key::S)?;
    d.changed(&before)?;
    d.settled()?;
    let right = selected(d)?;
    let duration = d.app().sequence_length();
    edit(d, "wet-send delay -6dB")?;
    edit(d, "wet-send reverb -3dB")?;
    d.check(
        "Separate send levels keep picture time and other sends",
        level(d, &right, TailEffect::Delay) == Some(-6000)
            && level(d, &right, TailEffect::Reverb) == Some(-3000)
            && d.app().sequence_length() == duration,
        json!({"delay":-6000,"reverb":-3000,"duration":duration}),
        d.snapshot(),
    )?;
    inspector_top(d, "Selected beat inspector pane")?;
    d.step("Paint continuous beat returns", true)?;
    scenarios::reveal_inspector_button(d, "delay return: -6dB  :wet-send")?;
    d.check(
        "The inspector visibly teaches the delay return and its command",
        visible(d, "delay return: -6dB"),
        json!("delay return: -6dB"),
        d.widgets(),
    )?;
    d.capture("Continuous beat returns")?;
    undo(d)?;
    d.check(
        "Undo removes only the latest send",
        level(d, &right, TailEffect::Reverb).is_none()
            && level(d, &right, TailEffect::Delay) == Some(-6000),
        json!("delay only"),
        d.snapshot(),
    )?;
    edit(d, "wet-send delay off")?;
    undo(d)?;
    edit(d, "wet-send reverb -3dB")?;
    d.key(Key::K)?;
    d.settled()?;
    let left = selected(d)?;
    let before = d.revision();
    d.events(
        "Repeat send with dot",
        stroke(Key::Period, Modifiers::NONE, "."),
    )?;
    d.changed(&before)?;
    d.settled()?;
    d.check(
        "Dot applies the captured send to another beat",
        left != right && level(d, &left, TailEffect::Reverb) == Some(-3000),
        json!(-3000),
        d.snapshot(),
    )?;
    undo(d)?;
    d.key(Key::Q)?;
    d.key(Key::A)?;
    edit(d, "wet-send delay -9dB")?;
    d.key(Key::Q)?;
    d.wait_for("Save send macro", |app| {
        !app.service.is_busy() && !app.macros.recording() && !app.macros.is_pending()
    })?;
    d.key(Key::J)?;
    d.settled()?;
    let before = d.revision();
    let mut events = stroke(Key::Num2, Modifiers::SHIFT, "@");
    events.extend(stroke(Key::A, Modifiers::NONE, "a"));
    d.events("Replay send macro", events)?;
    d.changed(&before)?;
    d.settled()?;
    d.check(
        "Macro changes the selected send as one edit",
        selected(d)? == right && level(d, &right, TailEffect::Delay) == Some(-9000),
        json!(-9000),
        d.snapshot(),
    )?;
    undo(d)?;
    d.check(
        "Undo restores the previous send",
        level(d, &right, TailEffect::Delay) == Some(-6000),
        json!(-6000),
        d.snapshot(),
    )?;

    let capture = d.app().capture_gain_target();
    let macro_capture = d.app().capture_macro_target();
    edit(d, "gain -3dB")?;
    let before = d.revision();
    d.app_mut().tail_send_command(
        Some(capture),
        Some(macro_capture),
        TailEffect::Delay,
        Some(GainDb::UNITY),
        true,
    );
    d.settled()?;
    d.check(
        "A stale send capture cannot cross a revision",
        d.revision() == before && d.app().error.is_some(),
        json!(before),
        d.snapshot(),
    )?;
    d.app_mut()
        .tail_send_command(None, None, TailEffect::Delay, Some(GainDb::UNITY), true);
    d.settled()?;
    d.check(
        "An absent send capture cannot use a later target",
        d.revision() == before && d.app().error.is_some(),
        json!(before),
        d.snapshot(),
    )?;
    undo(d)?;

    edit(d, "wrap-repeat 3")?;
    d.key(Key::Enter)?;
    d.command("scope play 2")?;
    d.settled()?;
    edit(d, "wet-send delay -12dB")?;
    let isolated = d.app().scoped_target()?.ok_or("No isolated play")?;
    d.check(
        "A selected play isolates its send and preserves the base",
        level(d, &isolated.target.node, TailEffect::Delay) == Some(-12000)
            && level(d, &right, TailEffect::Delay) == Some(-6000),
        json!({"play2":-12000,"base":-6000}),
        d.snapshot(),
    )?;
    undo(d)?;
    d.command("scope all")?;
    d.key(Key::Backspace)?;
    undo(d)?;

    let before = d.revision();
    d.command("wet-send delay 24.001dB")?;
    d.check(
        "Invalid return level cannot commit",
        d.revision() == before && d.app().error.is_some(),
        json!(before),
        d.snapshot(),
    )?;
    let saved = level(d, &right, TailEffect::Delay);
    edit(d, "gain -4dB")?;
    d.check(
        "Gain edits preserve continuous returns",
        level(d, &right, TailEffect::Delay) == saved,
        json!(saved),
        d.snapshot(),
    )?;
    undo(d)?;
    d.check(
        "Returns never add picture time",
        d.app().sequence_length() == duration,
        json!(duration),
        d.snapshot(),
    )?;
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 640.0));
    let input = d.harness.input_mut();
    input.screen_rect = Some(viewport);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing return viewport")?
        .inner_rect = Some(viewport);
    d.step("Resize return inspector", true)?;
    inspector_top(d, "Selected beat inspector pane")?;
    scenarios::reveal_inspector_button(d, "delay return: -6dB  :wet-send")?;
    d.check(
        "Return control remains readable at minimum size",
        visible(d, "delay return: -6dB"),
        json!("readable continuous delay control"),
        d.widgets(),
    )?;
    d.capture("Continuous returns at minimum size")?;
    super::tail_sends::placed_sound(d, true)?;
    Ok(())
}
