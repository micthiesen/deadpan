//! Real command routing and durable sends; PCM is checked in deadpan-audio.
use super::*;
use crate::navigation::duration::DurationInput;
use deadpan_core::{GainDb, HoldAudio, NodeId, NodeKind, TailEffect, TailInput};
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
        .tail_sends()
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
    edit(d, "tail-send delay -6dB")?;
    edit(d, "tail-send reverb -3dB")?;
    d.check(
        "Separate send levels keep picture time and other sends",
        level(d, &right, TailEffect::Delay) == Some(-6000)
            && level(d, &right, TailEffect::Reverb) == Some(-3000)
            && d.app().sequence_length() == duration,
        json!({"delay":-6000,"reverb":-3000,"duration":duration}),
        d.snapshot(),
    )?;
    d.step("Paint selected beat sends", true)?;
    scenarios::reveal_inspector_button(d, "delay: -6dB  :tail-send")?;
    d.check(
        "The inspector teaches both send levels and the tail command",
        visible(d, "delay: -6dB"),
        json!("delay: -6dB"),
        d.widgets(),
    )?;
    d.capture("Selected beat sends")?;
    undo(d)?;
    d.check(
        "Undo removes only the latest send",
        level(d, &right, TailEffect::Reverb).is_none()
            && level(d, &right, TailEffect::Delay) == Some(-6000),
        json!("delay only"),
        d.snapshot(),
    )?;
    edit(d, "tail-send delay off")?;
    undo(d)?;
    edit(d, "tail-send reverb -3dB")?;
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
    edit(d, "tail-send delay -9dB")?;
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
    let tail_capture = d.app().capture_macro_target();
    edit(d, "gain -3dB")?;
    let before = d.revision();
    d.app_mut().tail_send_command(
        Some(capture),
        Some(macro_capture),
        TailEffect::Delay,
        Some(GainDb::UNITY),
        false,
    );
    d.settled()?;
    d.check(
        "A stale send capture cannot cross a revision",
        d.revision() == before && d.app().error.is_some(),
        json!(before),
        d.snapshot(),
    )?;
    d.app_mut()
        .tail_send_command(None, None, TailEffect::Delay, Some(GainDb::UNITY), false);
    d.settled()?;
    d.check(
        "An absent send capture cannot use a later target",
        d.revision() == before && d.app().error.is_some(),
        json!(before),
        d.snapshot(),
    )?;
    d.app_mut().apply_tail(
        tail_capture,
        Some(DurationInput::parse("12f")?),
        TailEffect::Delay,
        TailInput::Sends,
    );
    d.settled()?;
    d.check(
        "A stale tail capture cannot insert time into a newer revision",
        d.revision() == before && d.app().error.is_some(),
        json!(before),
        d.snapshot(),
    )?;
    undo(d)?;

    edit(d, "wrap-repeat 3")?;
    d.key(Key::Enter)?;
    d.command("scope play 2")?;
    d.settled()?;
    edit(d, "tail-send delay -12dB")?;
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

    d.chord(&[Key::G, Key::G, Key::Num6, Key::Num0, Key::L])?;
    edit(d, "tail 12f effect=delay input=sends")?;
    let tail = selected(d)?;
    let is_send_tail = d.app().workspace.as_ref().and_then(|w| w.document.nodes().get(&tail)).is_some_and(|node| matches!(&node.kind, NodeKind::Hold { recipe } if matches!(recipe.audio, HoldAudio::Tail { input: TailInput::Sends, effect: TailEffect::Delay, .. })));
    d.check(
        "The inserted pause uses selected delay sends",
        is_send_tail && d.app().sequence_length() == duration + 12,
        json!("12-frame selected-send delay"),
        d.snapshot(),
    )?;
    inspector_top(d, "Selected beat inspector pane")?;
    d.capture("Selected sends into a tail pause")?;
    d.check(
        "The tail inspector identifies selected input",
        visible(d, "Selected sends"),
        json!("Selected sends"),
        d.widgets(),
    )?;
    d.chord(&[Key::Comma, Key::T])?;
    d.check(
        "Reopening a tail preserves its input and effect",
        d.app().command.contains("input=sends") && d.app().command.contains("effect=delay"),
        json!("input=sends effect=delay"),
        json!(d.app().command),
    )?;
    d.key(Key::Escape)?;
    undo(d)?;

    placed_sound(d, false)
}

pub(super) fn placed_sound(d: &mut Driver<'_>, wet: bool) -> Result<(), String> {
    d.command("sting")?;
    d.wait_for("Qualify synthesized sting", |app| {
        app.sound_rows.len() == 1 && !app.service.is_busy() && !app.importing()
    })?;
    let catalog = d.app().sound_rows[0].clone();
    d.chord(&[Key::G, Key::G])?;
    // At minimum height the catalog is below the sidebar's current scroll.
    // Use native pane/list navigation rather than clicking an offscreen rect.
    for _ in 0..6 {
        if d.app().pane == Pane::Sources {
            break;
        }
        d.key(Key::Tab)?;
    }
    d.key(Key::J)?;
    d.step("Select sting", true)?;
    d.check(
        "Keyboard navigation selects the catalog sound before placement",
        d.app().pane == Pane::Sources && d.app().selected_sound.as_ref() == Some(&catalog.0),
        json!(catalog.0),
        d.snapshot(),
    )?;
    edit(d, "sound-place")?;
    d.command("sounds")?;
    let sound = d.app().selected_event.clone().ok_or("No placed sound")?;
    let base = d.app().workspace.as_ref().unwrap().document.sounds()[&sound].clone();
    let command = if wet {
        "sound-wet-send"
    } else {
        "sound-tail-send"
    };
    edit(d, &format!("{command} reverb -6dB"))?;
    let actual = d.app().workspace.as_ref().unwrap().document.sounds()[&sound].clone();
    let mut expected = base.clone();
    let sends = if wet {
        &mut expected.wet_sends
    } else {
        &mut expected.tail_sends
    };
    *sends = sends.with_level(TailEffect::Reverb, Some(GainDb::new(-6000).unwrap()));
    d.check(
        "A placed sound send preserves its complete placement and gain",
        actual == expected,
        json!(expected),
        json!(actual),
    )?;
    inspector_top(d, "Selected sound inspector pane")?;
    let label = if wet {
        "reverb return: -6dB"
    } else {
        "reverb send: -6dB"
    };
    scenarios::reveal_inspector_button(d, &format!("{label}  :{command}"))?;
    if wet {
        scenarios::reveal_inspector_button(d, "delay return: off  :sound-wet-send")?;
        d.check(
            "Both placed sound return levels are readable at minimum size after scrolling",
            visible(d, label) && visible(d, "delay return: off"),
            json!([label, "delay return: off"]),
            d.widgets(),
        )?;
    }
    d.capture(if wet {
        "Placed sound continuous returns"
    } else {
        "Placed sound sends"
    })?;
    d.check(
        "The placed sound inspector visibly teaches its saved send",
        visible(d, label),
        json!(label),
        d.widgets(),
    )?;
    if wet {
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 820.0));
        let input = d.harness.input_mut();
        input.screen_rect = Some(viewport);
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .ok_or("Missing sound return viewport")?
            .inner_rect = Some(viewport);
        d.step("Resize placed sound returns to default size", true)?;
        scenarios::reveal_inspector_button(d, &format!("{label}  :{command}"))?;
        d.capture("Placed sound continuous returns at default size")?;
        d.check(
            "The placed sound return remains readable at default size",
            visible(d, label),
            json!(label),
            d.widgets(),
        )?;
    }
    edit(d, "sound-gain -3")?;
    d.check(
        "Sound gain edits retain sends",
        {
            let event = &d.app().workspace.as_ref().unwrap().document.sounds()[&sound];
            event.tail_sends == expected.tail_sends && event.wet_sends == expected.wet_sends
        },
        json!({"tail": expected.tail_sends, "wet":expected.wet_sends}),
        d.snapshot(),
    )?;
    undo(d)?;
    undo(d)?;
    d.check(
        "Undo restores the complete original sound event",
        d.app().workspace.as_ref().unwrap().document.sounds()[&sound] == base,
        json!(base),
        d.snapshot(),
    )?;
    Ok(())
}
