//! Specification §8.1, §8.3 and §7.5: a reverse hiccup (`:reverse`), a
//! ping-pong (`:ping-pong`) and hanging tails (`,t`, `:tail`), each through
//! the production router, semantic project service and store, with one Undo.

use super::*;
use deadpan_core::{HoldAudio, HoldRecipe, HoldVideo, NodeKind, TailEffect};
use egui::Key;

/// The Hold beat that starts at `start` in the current scope.
fn hold_at(d: &Driver<'_>, start: u64) -> Option<HoldRecipe> {
    let app = d.app();
    let row = app.beat_rows.iter().find(|row| row.start == start)?;
    match &app.workspace.as_ref()?.document.nodes().get(&row.id)?.kind {
        NodeKind::Hold { recipe } => Some(recipe.clone()),
        _ => None,
    }
}

fn describe(recipe: Option<&HoldRecipe>) -> Value {
    recipe.map_or(Value::Null, |recipe| {
        json!({
            "frames": recipe.duration.frames(),
            "video": match &recipe.video {
                HoldVideo::Reverse { span, .. } => json!({"reverse": [span.start().ticks, span.end().ticks]}),
                HoldVideo::Freeze { .. } => json!("freeze"),
                HoldVideo::Play { span, .. } => json!({"play": [span.start().ticks, span.end().ticks]}),
                _ => json!("other"),
            },
            "audio": match &recipe.audio {
                HoldAudio::Reverse { source } => json!({"reverse": [source.span.start().ticks, source.span.end().ticks]}),
                HoldAudio::Tail { maximum, effect, .. } => json!({"tail": effect.name(), "rings": maximum.frames()}),
                HoldAudio::Silence => json!("silence"),
                HoldAudio::RoomTone { .. } => json!("room tone"),
                HoldAudio::Tone { frequency_hz, .. } => json!({"tone": frequency_hz}),
            },
        })
    })
}

fn undo(d: &mut Driver<'_>, duration: u64, label: &str) -> Result<(), String> {
    let applied = d.revision();
    d.key(Key::U)?;
    d.changed(&applied)?;
    d.check(
        label,
        d.app().sequence_length() == duration,
        json!(duration),
        json!(d.app().sequence_length()),
    )
}

fn enter(d: &mut Driver<'_>, label: &str, text: &[&str]) -> Result<(), String> {
    let mut events = Vec::new();
    for piece in text {
        match *piece {
            "\u{8}" => {
                events.push(key_event(Key::Backspace, egui::Modifiers::NONE, true));
                events.push(key_event(Key::Backspace, egui::Modifiers::NONE, false));
            }
            text => events.push(egui::Event::Text(text.into())),
        }
    }
    events.push(key_event(Key::Enter, egui::Modifiers::NONE, true));
    events.push(key_event(Key::Enter, egui::Modifiers::NONE, false));
    d.events(label, events)
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.command("sequence")?;
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num3, Key::Num0, Key::L])?;
    d.settled()?;
    let duration = d.app().sequence_length();

    // A reverse hiccup: the eight frames before the cursor, backwards.
    let before = d.revision();
    d.command("reverse 8f")?;
    d.changed(&before)?;
    d.settled()?;
    let reverse = hold_at(d, 30);
    let span = match reverse.as_ref().map(|recipe| &recipe.video) {
        Some(HoldVideo::Reverse { span, .. }) => Some((span.start().ticks, span.end().ticks)),
        _ => None,
    };
    d.check(
        ":reverse 8f inserts an 8-frame pause playing the 8 frames before it backwards, with their sound reversed",
        reverse.as_ref().is_some_and(|recipe| {
            recipe.duration.frames() == 8 && matches!(recipe.audio, HoldAudio::Reverse { .. })
        }) && span.is_some()
            && d.app().sequence_length() == duration + 8
            && d.app().error.is_none(),
        json!({"frames":8,"video":"reverse","audio":"reverse","total":duration + 8}),
        json!({"hold":describe(reverse.as_ref()),"total":d.app().sequence_length(),"error":d.app().error,"message":d.app().message}),
    )?;
    d.step("Paint the reverse in the inspector", true)?;
    let inspector = scenarios::text_paint_visibility(d, "Reversed");
    d.check(
        "The inspector names the reversed picture and sound",
        !inspector.is_empty(),
        json!("Reversed"),
        json!(inspector),
    )?;
    d.capture("Reverse hiccup")?;
    undo(d, duration, "One undo removes the reverse")?;

    // A ping-pong over the same twelve frames does not show the turning
    // picture twice: it is one frame shorter and ends its span one picture
    // earlier than a reverse of the same length.
    d.chord(&[Key::G, Key::G, Key::Num3, Key::Num0, Key::L])?;
    let before = d.revision();
    d.command("reverse 12f")?;
    d.changed(&before)?;
    let plain = hold_at(d, 30);
    undo(d, duration, "One undo removes the 12-frame reverse")?;
    d.chord(&[Key::G, Key::G, Key::Num3, Key::Num0, Key::L])?;
    let before = d.revision();
    d.command("ping-pong 12f")?;
    d.changed(&before)?;
    d.settled()?;
    let bounce = hold_at(d, 30);
    let ends = |recipe: Option<&HoldRecipe>| match recipe.map(|recipe| &recipe.video) {
        Some(HoldVideo::Reverse { span, .. }) => Some((span.start().ticks, span.end().ticks)),
        _ => None,
    };
    let (plain_span, bounce_span) = (ends(plain.as_ref()), ends(bounce.as_ref()));
    d.check(
        ":ping-pong 12f bounces back over the 12 frames without repeating the turning picture",
        bounce
            .as_ref()
            .is_some_and(|recipe| recipe.duration.frames() == 11)
            && plain_span
                .zip(bounce_span)
                .is_some_and(|(plain, bounce)| plain.0 == bounce.0 && bounce.1 < plain.1)
            && d.app().sequence_length() == duration + 11,
        json!({"frames":11,"same_start_shorter_end":true,"total":duration + 11}),
        json!({"reverse":describe(plain.as_ref()),"ping_pong":describe(bounce.as_ref()),"total":d.app().sequence_length(),"error":d.app().error}),
    )?;
    d.capture("Ping-pong")?;
    undo(d, duration, "One undo removes the ping-pong")?;

    // ,t without a selected pause proposes a new one-second reverb tail.
    d.chord(&[Key::G, Key::G, Key::Num3, Key::Num0, Key::L])?;
    d.chord(&[Key::Comma, Key::T])?;
    d.step("Paint the tail command", true)?;
    d.check(
        ",t opens :tail with its length ready to change",
        d.app().command_open && d.app().command == "tail 1s effect=reverb",
        json!("tail 1s effect=reverb"),
        json!({"command":d.app().command,"open":d.app().command_open}),
    )?;
    d.capture("Tail command from ,t")?;
    let before = d.revision();
    enter(d, "Apply the proposed tail pause", &[])?;
    d.changed(&before)?;
    d.settled()?;
    let tail = hold_at(d, 30);
    let added = d.app().sequence_length().saturating_sub(duration);
    d.check(
        "Enter inserts a freeze pause at the cursor whose reverb tail rings for its whole length",
        tail.as_ref().is_some_and(|recipe| {
            matches!(recipe.video, HoldVideo::Freeze { .. })
                && matches!(
                    recipe.audio,
                    HoldAudio::Tail { maximum, effect: TailEffect::Reverb, .. }
                        if maximum == recipe.duration
                )
                && recipe.duration.frames() as u64 == added
        }) && added > 0,
        json!({"video":"freeze","audio":{"tail":"reverb","rings":"whole pause"}}),
        json!({"hold":describe(tail.as_ref()),"added":added,"error":d.app().error}),
    )?;
    d.capture("Tail pause")?;
    undo(d, duration, "One undo removes the tail pause")?;

    // ,h then ,t: the selected pause gets the tail of what precedes it,
    // here switched to the echo, with no change in timing.
    d.chord(&[Key::G, Key::G, Key::Num3, Key::Num0, Key::L])?;
    let before = d.revision();
    d.chord(&[Key::Comma, Key::H])?;
    d.changed(&before)?;
    d.settled()?;
    let paused = d.app().sequence_length();
    let frames = hold_at(d, 30).map_or(0, |recipe| recipe.duration.frames());
    d.chord(&[Key::Comma, Key::T])?;
    d.step("Paint the tail command for the pause", true)?;
    let proposed = format!("tail {frames}f effect=reverb");
    d.check(
        ",t on a selected pause proposes a tail ringing for the whole pause",
        d.app().command_open && d.app().command == proposed,
        json!(proposed),
        json!({"command":d.app().command,"open":d.app().command_open}),
    )?;
    let before = d.revision();
    enter(
        d,
        "Switch the effect to the echo and apply",
        &[
            "\u{8}", "\u{8}", "\u{8}", "\u{8}", "\u{8}", "\u{8}", "delay",
        ],
    )?;
    d.changed(&before)?;
    d.settled()?;
    let echoed = hold_at(d, 30);
    d.check(
        "The selected pause now carries the delay tail with its timing unchanged",
        echoed.as_ref().is_some_and(|recipe| {
            matches!(
                recipe.audio,
                HoldAudio::Tail { maximum, effect: TailEffect::Delay, .. }
                    if maximum == recipe.duration
            )
        }) && d.app().sequence_length() == paused,
        json!({"audio":{"tail":"delay","rings":frames},"total":paused}),
        json!({"hold":describe(echoed.as_ref()),"total":d.app().sequence_length(),"error":d.app().error}),
    )?;
    d.step("Paint the tail in the inspector", true)?;
    let inspector = scenarios::text_paint_visibility(d, &format!("Delay tail {frames} f"));
    let live = scenarios::text_paint_visibility(d, "Live 2 s before");
    d.check(
        "The inspector shows the effect, how long it rings and its live input",
        !inspector.is_empty() && !live.is_empty(),
        json!([format!("Delay tail {frames} f"), "Live 2 s before"]),
        json!({"sound":inspector,"input":live}),
    )?;
    d.capture("Delay tail on a pause")?;
    undo(d, paused, "One undo restores the silent pause")?;
    undo(d, duration, "A second undo removes the pause")?;

    // :lift a Visual range: the cut content goes to the register and the
    // same time comes back as a silent black pause.
    d.chord(&[Key::G, Key::G, Key::Num3, Key::Num0, Key::L, Key::V])?;
    d.chord(&[Key::Num5, Key::L])?;
    let before = d.revision();
    d.command("lift")?;
    d.changed(&before)?;
    d.settled()?;
    let lifted = hold_at(d, 30);
    d.check(
        ":lift replaces the Visual range with a silent black pause of the same length",
        lifted.as_ref().is_some_and(|recipe| {
            recipe.duration.frames() == 5
                && matches!(recipe.video, HoldVideo::Background)
                && matches!(recipe.audio, HoldAudio::Silence)
        }) && d.app().sequence_length() == duration
            && d.app().copied.selected_content().is_some(),
        json!({"frames":5,"video":"black","audio":"silence","total":duration}),
        json!({"hold":describe(lifted.as_ref()),"total":d.app().sequence_length(),"error":d.app().error,"message":d.app().message}),
    )?;
    d.capture("Lifted range")?;
    let applied = d.revision();
    d.key(Key::U)?;
    d.changed(&applied)?;
    d.settled()?;
    d.check(
        "One undo restores the lifted content",
        hold_at(d, 30).is_none() && d.app().sequence_length() == duration,
        json!({"pause_at_30":false,"total":duration}),
        json!({"pause_at_30":hold_at(d, 30).is_some(),"total":d.app().sequence_length()}),
    )?;

    // ,b over a Visual range: the pictures keep playing over a tone.
    d.chord(&[Key::G, Key::G, Key::Num3, Key::Num0, Key::L, Key::V])?;
    d.chord(&[Key::Num5, Key::L])?;
    let before = d.revision();
    d.chord(&[Key::Comma, Key::B])?;
    d.changed(&before)?;
    d.settled()?;
    let bleeped = hold_at(d, 30);
    d.check(
        ",b bleeps the Visual range: same length, the same pictures played forward, a 1 kHz tone instead of the sound",
        bleeped.as_ref().is_some_and(|recipe| {
            recipe.duration.frames() == 5
                && matches!(recipe.video, HoldVideo::Play { .. })
                && matches!(recipe.audio, HoldAudio::Tone { frequency_hz: 1_000, .. })
        }) && d.app().sequence_length() == duration,
        json!({"frames":5,"video":"play","audio":{"tone":1000},"total":duration}),
        json!({"hold":describe(bleeped.as_ref()),"total":d.app().sequence_length(),"error":d.app().error,"message":d.app().message}),
    )?;
    d.step("Paint the bleep in the inspector", true)?;
    let inspector = scenarios::text_paint_visibility(d, "Bleep 1000 Hz -10 dB");
    d.check(
        "The inspector names the tone",
        !inspector.is_empty(),
        json!("Bleep 1000 Hz -10 dB"),
        json!(inspector),
    )?;
    d.capture("Bleep")?;
    let applied = d.revision();
    d.key(Key::U)?;
    d.changed(&applied)?;
    d.settled()?;
    d.check(
        "One undo restores the bleeped sound",
        hold_at(d, 30).is_none() && d.app().sequence_length() == duration,
        json!({"pause_at_30":false}),
        json!({"pause_at_30":hold_at(d, 30).is_some()}),
    )?;
    Ok(())
}
