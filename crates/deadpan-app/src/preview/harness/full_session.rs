//! A complete keyboard-only editorial session (specification §2.1 and §7).
//!
//! Every input is a key event, typed command text or the IME/text companion
//! a physical keyboard delivers; the replay never moves or clicks the pointer.
//! Only OS picker results (⌘N, ⌘I, ⌘E, ⌘O) and the AI model worker are
//! scripted. Each step first checks that its key is painted where the action
//! lives, then performs it and checks the exact document result.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use deadpan_cli::encoded_render::workflow::{WorkflowOutcome, WorkflowStage};
use deadpan_core::{
    Framing, FramingValue, HoldVideo, NodeId, NodeKind, ProjectDocument, RegisterName,
    RegisterValue,
};
use egui::{Event, Key, Modifiers};

use super::*;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push(
        "Keyboard-only session: OS pickers, the AI model worker and audition delivery are scripted; no pointer event is sent. Physical key delivery, OS IME and VoiceOver remain native-only checks.".into(),
    );
    let fixture = d
        .app()
        .feedback
        .original_fixture
        .clone()
        .ok_or("No Original fixture")?;
    start(d, fixture)?;
    navigate(d)?;
    moment(d)?;
    trim(d)?;
    cut_with_motion(d)?;
    let hold = pause(d)?;
    let repeat = repeat_ladder(d)?;
    occurrences(d, &repeat)?;
    follow_target(d)?;
    caption(d)?;
    place_sound(d)?;
    gain_and_mute(d)?;
    undo_redo(d)?;
    macro_and_dot(d)?;
    ai_pause(d, &hold)?;
    let movie = render(d)?;
    close_and_reopen(d, &movie)
}

// ---------------------------------------------------------------- helpers

fn document(d: &Driver<'_>) -> Result<Arc<ProjectDocument>, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.document.clone())
        .ok_or_else(|| "No open project".into())
}

fn frames(d: &Driver<'_>) -> u64 {
    d.app().sequence_length()
}

/// Every painted text galley in the last frame, in paint order.
fn painted(d: &Driver<'_>) -> Vec<String> {
    d.harness
        .output()
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

/// Discoverability: one of `needles` (the key as taught) must be fully
/// painted on screen before the step uses it. Recorded as a check so a
/// regression in teaching fails the session.
fn taught(d: &mut Driver<'_>, step: &str, needles: &[&str]) -> Result<(), String> {
    d.step(&format!("Paint before {step}"), false)?;
    let visible = needles.iter().find(|needle| {
        let paint = scenarios::text_paint_visibility(d, needle);
        !paint.is_empty() && paint.iter().any(|item| item["fully_visible"] == true)
    });
    let texts = painted(d);
    d.check(
        &format!("Discoverable: {step} is taught on screen before use"),
        visible.is_some(),
        json!({"any_of":needles}),
        json!({"visible":visible,"painted":texts}),
    )
}

fn stroke(key: Key, modifiers: Modifiers, text: Option<&str>) -> Vec<Event> {
    let mut events = vec![key_event(key, modifiers, true)];
    if let Some(text) = text {
        events.push(Event::Text(text.into()));
    }
    events.push(key_event(key, modifiers, false));
    events
}

/// Printable keys deliver their text companion as on macOS.
fn typed(d: &mut Driver<'_>, label: &str, keys: &[(Key, Modifiers, &str)]) -> Result<(), String> {
    let events = keys
        .iter()
        .flat_map(|(key, modifiers, text)| stroke(*key, *modifiers, Some(text)))
        .collect();
    d.events(label, events)
}

fn plain(d: &mut Driver<'_>, keys: &str) -> Result<(), String> {
    let mut events = Vec::new();
    for character in keys.chars() {
        let name = character.to_ascii_uppercase().to_string();
        let key = match character {
            ',' => Key::Comma,
            '.' => Key::Period,
            '+' => Key::Plus,
            '[' => Key::OpenBracket,
            ']' => Key::CloseBracket,
            _ => Key::from_name(&name).ok_or_else(|| format!("No key for {character}"))?,
        };
        let modifiers = if character.is_ascii_uppercase() {
            Modifiers::SHIFT
        } else {
            Modifiers::NONE
        };
        events.extend(stroke(key, modifiers, Some(&character.to_string())));
    }
    d.events(&format!("Keys {keys}"), events)
}

fn focus_edit(d: &mut Driver<'_>) -> Result<(), String> {
    if d.app().view != View::Sequence {
        d.command("sequence")?;
        d.settled()?;
    }
    super::transcript::focus_your_edit(d)
}

/// Select a beat at the current depth with `j`/`k` only.
fn select(d: &mut Driver<'_>, target: &NodeId) -> Result<(), String> {
    for _ in 0..64 {
        if d.app().selected_beat.as_ref() == Some(target) {
            return d.settled();
        }
        let rows = &d.app().beat_rows;
        let index = rows
            .iter()
            .position(|row| &row.id == target)
            .ok_or("The beat is not visible at this depth")?;
        let current = d
            .app()
            .selected_beat
            .as_ref()
            .and_then(|selected| rows.iter().position(|row| &row.id == selected))
            .unwrap_or(0);
        d.key(if index > current { Key::J } else { Key::K })?;
    }
    Err("Could not select the beat with j/k".into())
}

fn children(d: &Driver<'_>) -> Result<Vec<NodeId>, String> {
    let document = document(d)?;
    Ok(document.children(document.root()).cloned().collect())
}

fn root_kinds(d: &Driver<'_>) -> Result<Vec<(String, u64)>, String> {
    Ok(d.app()
        .beat_rows
        .iter()
        .map(|row| (row.kind.to_string(), row.frames))
        .collect())
}

fn edit(
    d: &mut Driver<'_>,
    input: impl FnOnce(&mut Driver<'_>) -> Result<(), String>,
) -> Result<String, String> {
    let before = d.revision();
    input(d)?;
    d.changed(&before)?;
    Ok(d.revision())
}

// ------------------------------------------------------------------ steps

/// Start screen → ⌘N → the full Original is the initial edit.
fn start(d: &mut Driver<'_>, fixture: PathBuf) -> Result<(), String> {
    d.capture("Start screen")?;
    taught(d, "choose a video", &["⌘N"])?;
    d.app_mut().dialogs = Dialogs::scripted(vec![(DialogKind::CreateProject, Some(fixture))]);
    d.key_modified(Key::N, Modifiers::COMMAND)?;
    d.wait_for("Original initialized from ⌘N", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|w| matches!(w.single_source, Some(SingleSourceState::Ready { .. })))
            && app.presentation.has_displayed()
            && !app.presentation.loading()
    })?;
    d.settled()?;
    let document = document(d)?;
    d.check(
        "⌘N creates a project whose edit is the whole unedited Original",
        d.app().beat_rows.len() == 1
            && frames(d) == 120
            && !d.app().workspace.as_ref().is_some_and(|w| w.can_undo),
        json!({"beats":1,"frames":120,"undo":false}),
        json!({"beats":d.app().beat_rows.len(),"frames":frames(d),"root":document.root()}),
    )?;
    d.capture("Project created from the keyboard")
}

/// Original and Your edit keep distinct cursors.
fn navigate(d: &mut Driver<'_>) -> Result<(), String> {
    taught(d, "switch to the Original", &[":source"])?;
    d.command("source")?;
    d.settled()?;
    plain(d, "gg")?;
    typed(
        d,
        "Count and motion in the Original",
        &[
            (Key::Num1, Modifiers::NONE, "1"),
            (Key::Num0, Modifiers::NONE, "0"),
            (Key::L, Modifiers::NONE, "l"),
        ],
    )?;
    d.settled()?;
    let original = d.app().source_cursor;
    taught(d, "switch to Your edit", &[":sequence"])?;
    focus_edit(d)?;
    plain(d, "G")?;
    d.settled()?;
    d.check(
        "Original and Your edit retain separate cursors",
        original == 10
            && d.app().view == View::Sequence
            && d.app().sequence_cursor + 1 >= frames(d)
            && d.app().source_cursor == 10,
        json!({"original":10,"edit":"end"}),
        json!({"original":d.app().source_cursor,"edit":d.app().sequence_cursor,"frames":frames(d)}),
    )?;
    plain(d, "gg")?;
    d.settled()?;
    d.capture("Navigated both clocks")
}

/// `v`, motion, `y` in the Original; `p` in Your edit.
fn moment(d: &mut Driver<'_>) -> Result<(), String> {
    d.command("source")?;
    d.settled()?;
    taught(d, "select a moment", &["Select moment  v", "v select"])?;
    plain(d, "v")?;
    typed(
        d,
        "Extend the moment",
        &[
            (Key::Num1, Modifiers::NONE, "1"),
            (Key::Num4, Modifiers::NONE, "4"),
            (Key::L, Modifiers::NONE, "l"),
        ],
    )?;
    d.settled()?;
    taught(d, "copy the moment", &["Copy moment  y", "y copy"])?;
    plain(d, "y")?;
    d.wait_for("Original copy saved", |app| {
        !app.service.is_busy() && !app.copied.is_pending()
    })?;
    d.check(
        "v14ly copies the half-open Original moment [10,24)",
        d.app()
            .copied
            .original()
            .is_some_and(|copy| copy.ordinals == (10..24)),
        json!([10, 24]),
        d.snapshot(),
    )?;
    focus_edit(d)?;
    taught(d, "paste after", &["Paste after  p", "p paste"])?;
    edit(d, |d| plain(d, "p"))?;
    d.check(
        "p pastes the moment after the selected beat as one edit",
        frames(d) == 134 && d.app().beat_rows.len() == 2,
        json!({"frames":134,"beats":2}),
        json!({"frames":frames(d),"rows":root_kinds(d)?}),
    )?;
    d.capture("Moment pasted")
}

/// `d` + count + motion cuts exactly that range into the unnamed register.
fn cut_with_motion(d: &mut Driver<'_>) -> Result<(), String> {
    plain(d, "gg")?;
    typed(
        d,
        "Move to 30",
        &[
            (Key::Num3, Modifiers::NONE, "3"),
            (Key::Num0, Modifiers::NONE, "0"),
            (Key::L, Modifiers::NONE, "l"),
        ],
    )?;
    d.settled()?;
    taught(d, "cut with a motion", &["+ motion: cut / copy"])?;
    edit(d, |d| {
        typed(
            d,
            "d5l",
            &[
                (Key::D, Modifiers::NONE, "d"),
                (Key::Num5, Modifiers::NONE, "5"),
                (Key::L, Modifiers::NONE, "l"),
            ],
        )
    })?;
    d.settled()?;
    d.check(
        "d5l cuts five frames at the cursor and keeps the join",
        frames(d) == 128 && d.app().sequence_cursor == 30,
        json!({"frames":128,"cursor":30}),
        json!({"frames":frames(d),"cursor":d.app().sequence_cursor,"rows":root_kinds(d)?}),
    )?;
    d.capture("Cut five frames with d5l")
}

/// `,h` inserts a silent half-second pause at the cursor.
fn pause(d: &mut Driver<'_>) -> Result<NodeId, String> {
    let before = frames(d);
    taught(d, "insert a pause", &[",h"])?;
    edit(d, |d| plain(d, ",h"))?;
    d.settled()?;
    let document = document(d)?;
    let hold = d
        .app()
        .selected_beat
        .clone()
        .filter(|node| matches!(document.nodes()[node].kind, NodeKind::Hold { .. }))
        .ok_or("The new pause is not selected")?;
    let rate = document.presentation_basis().frame_rate;
    let added = frames(d) - before;
    d.check(
        ",h inserts one silent pause whose length is the added time",
        d.app()
            .beat_rows
            .iter()
            .any(|row| row.id == hold && row.frames == added)
            && added > 0,
        json!({"added_equals_pause":true}),
        json!({"added":added,"rows":root_kinds(d)?,"rate":format!("{rate:?}")}),
    )?;
    d.capture("Pause inserted with ,h")?;
    Ok(hold)
}

/// `:repeat 3 gap=6f gap-step=-2f` on the pasted moment.
fn repeat_ladder(d: &mut Driver<'_>) -> Result<NodeId, String> {
    let last = children(d)?.last().cloned().ok_or("No beats")?;
    select(d, &last)?;
    let before = frames(d);
    let body = d
        .app()
        .beat_rows
        .iter()
        .find(|row| row.id == last)
        .map_or(0, |row| row.frames);
    taught(d, "repeat", &["r repeat", "rr", "Repeat"])?;
    edit(d, |d| d.command("repeat 3 gap=6f gap-step=-2f"))?;
    d.settled()?;
    let document = document(d)?;
    let repeat = d.app().selected_beat.clone().ok_or("No Repeat selected")?;
    let ladder = matches!(
        &document.nodes()[&repeat].kind,
        NodeKind::Repeat { iterations, gap: Some(gap), .. }
            if iterations.len() == 3 && gap.duration.frames() == 6
    ) && document
        .gap_overrides()
        .get(&repeat)
        .is_some_and(|gaps| gaps.len() == 1);
    d.check(
        ":repeat 3 gap=6f gap-step=-2f makes three plays with 6 and 4 frame gaps",
        ladder && frames(d) == before + 2 * body + 10,
        json!({"plays":3,"gaps":[6,4],"frames":before + 2 * body + 10}),
        json!({"frames":frames(d),"rows":root_kinds(d)?}),
    )?;
    d.capture("Repeat with a gap ladder")?;
    Ok(repeat)
}

fn play(d: &Driver<'_>) -> Result<(Option<u32>, NodeId), String> {
    let app = d.app();
    let workspace = app.workspace.as_ref().ok_or("No project")?;
    let state = app.scoped.as_ref().ok_or("Repeat contents are not open")?;
    Ok((
        state
            .repeat_choice(workspace)?
            .and_then(|choice| choice.one_based),
        state.selected_target().node.clone(),
    ))
}

/// Enter, `]r`/`[r`, `j` and Backspace walk the plays and the owned gap.
fn occurrences(d: &mut Driver<'_>, repeat: &NodeId) -> Result<(), String> {
    let saved = d.revision();
    taught(d, "open the Repeat", &["open group"])?;
    taught(d, "open a play from the Repeat", &["open a play"])?;
    d.key(Key::Enter)?;
    d.settled()?;
    let body = play(d)?.1;
    d.check(
        "Enter opens the Repeat at All plays on its shared body",
        play(d)?.0.is_none() && body != *repeat,
        json!({"play":null}),
        json!(format!("{:?}", play(d)?)),
    )?;
    taught(d, "next play", &["Next  ]r"])?;
    taught(d, "Repeat play keys in the footer", &["Repeat play"])?;
    plain(d, "]r")?;
    d.settled()?;
    let first = play(d)?.0;
    plain(d, "]r")?;
    d.settled()?;
    let second = play(d)?.0;
    plain(d, "j")?;
    d.settled()?;
    let gap = play(d)?.1;
    let document = document(d)?;
    let owned_gap = document
        .gap_overrides()
        .get(repeat)
        .is_some_and(|gaps| gaps.iter().any(|(_, node)| *node == gap));
    d.capture("Play 2 with its owned gap selected")?;
    plain(d, "3[r")?;
    d.settled()?;
    let all = play(d)?.0;
    d.check(
        "]r steps to play 1 and play 2, j reaches play 2's own gap, and a counted [r returns to All plays",
        first == Some(1) && second == Some(2) && owned_gap && all.is_none() && d.revision() == saved,
        json!({"plays":[1, 2],"gap":"owned by play 2","back":"all","revision":saved}),
        json!({"plays":[first, second],"owned_gap":owned_gap,"back":all,"revision":d.revision()}),
    )?;
    d.key(Key::Backspace)?;
    d.settled()?;
    d.check(
        "Backspace leaves the Repeat with its beat still selected and no edit",
        d.app().scoped.is_none()
            && d.app().selected_beat.as_ref() == Some(repeat)
            && d.revision() == saved,
        json!({"scoped":false,"selected":"Repeat"}),
        d.snapshot(),
    )?;
    // On the selected Repeat a count names the play: 3]r is play 3, 2[r the
    // second play from the end.
    let mut opened = Vec::new();
    for keys in ["3]r", "2[r", "[r"] {
        plain(d, keys)?;
        d.settled()?;
        opened.push(play(d)?.0);
        d.key(Key::Backspace)?;
        d.settled()?;
    }
    d.check(
        "Counted ]r/[r on a selected Repeat open play N or the Nth play from the end",
        opened == [Some(3), Some(2), Some(3)] && d.revision() == saved,
        json!([3, 2, 3]),
        json!(opened),
    )
}

/// Camera `,f`, a new target with `n`, then `,z` follows it.
fn follow_target(d: &mut Driver<'_>) -> Result<(), String> {
    let first = children(d)?.first().cloned().ok_or("No beats")?;
    select(d, &first)?;
    plain(d, "gg")?;
    d.settled()?;
    taught(d, "Camera", &[",f"])?;
    plain(d, ",f")?;
    d.wait_for("Camera opens", |app| app.camera.is_some())?;
    d.settled()?;
    taught(
        d,
        "new target rectangle",
        &["n new", "n  new", "New target"],
    )?;
    plain(d, "n")?;
    typed(
        d,
        "Move the target right",
        &[
            (Key::Num8, Modifiers::NONE, "8"),
            (Key::L, Modifiers::NONE, "l"),
        ],
    )?;
    d.key(Key::Enter)?;
    d.wait_for("Target saved", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| !workspace.document.targets().is_empty())
            && app.camera.as_ref().is_some_and(|camera| {
                let state = camera.harness_state();
                state["saving"] == false && state["rebasing"] == false
            })
    })?;
    d.key(Key::Escape)?;
    d.settled()?;
    taught(d, "punch in on the target", &["punch in / creep"])?;
    edit(d, |d| plain(d, ",z"))?;
    d.settled()?;
    let document = document(d)?;
    let framing = document.nodes()[&first].framing.clone();
    d.check(
        ",z punches in to 1.35x following the saved target",
        matches!(&framing, Some(Framing { value: FramingValue::Follow { scale, .. }, .. })
            if *scale == deadpan_core::ExactRatio::new(27, 20).unwrap()),
        json!("Follow target-1 at 1.35"),
        json!(format!("{framing:?}")),
    )?;
    d.capture("Punch-in follows the target")
}

fn caption(d: &mut Driver<'_>) -> Result<(), String> {
    let node = d.app().selected_beat.clone().ok_or("No beat")?;
    let before = frames(d);
    // Typing the start of a command lists the matching commands.
    d.key(Key::Colon)?;
    d.events("Type cap", vec![Event::Text("cap".into())])?;
    taught(d, "caption", &[":caption TEXT"])?;
    d.capture("Typed :cap lists the caption command")?;
    edit(d, |d| {
        d.events(
            "Finish the caption command",
            vec![
                Event::Text("tion Hello there at=top".into()),
                key_event(Key::Enter, Modifiers::NONE, true),
                key_event(Key::Enter, Modifiers::NONE, false),
            ],
        )
    })?;
    d.settled()?;
    let document = document(d)?;
    let (host, _) = deadpan_core::cutaway_host(&document, &node).ok_or("No caption host")?;
    let captions = &document.nodes()[&host].captions;
    d.check(
        ":caption puts the text on the selected beat without changing time",
        captions.len() == 1 && captions[0].text == "Hello there" && frames(d) == before,
        json!({"captions":["Hello there"],"frames":before}),
        json!({"captions":captions.iter().map(|c| c.text.clone()).collect::<Vec<_>>(),"frames":frames(d)}),
    )?;
    d.capture("Caption on the first beat")
}

fn place_sound(d: &mut Driver<'_>) -> Result<(), String> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav")
        .canonicalize()
        .map_err(|error| error.to_string())?;
    taught(d, "add a sound", &["⌘I"])?;
    d.command("sound-channels stereo")?;
    d.app_mut().dialogs = Dialogs::scripted(vec![(DialogKind::ImportSound, Some(fixture))]);
    d.key_modified(Key::I, Modifiers::COMMAND)?;
    d.wait_for("Sound is qualified in the real catalog", |app| {
        app.sound_rows.len() == 1 && !app.service.is_busy() && !app.importing()
    })?;
    d.settled()?;
    let nodes = document(d)?.nodes().clone();
    // The retained Edit cursor is the placement point.
    focus_edit(d)?;
    plain(d, "gg")?;
    typed(
        d,
        "Move to 6",
        &[
            (Key::Num6, Modifiers::NONE, "6"),
            (Key::L, Modifiers::NONE, "l"),
        ],
    )?;
    d.settled()?;
    // Choose the catalog sound from the keyboard: Tab to the rail, then j.
    for _ in 0..6 {
        if d.app().pane == Pane::Sources {
            break;
        }
        d.key(Key::Tab)?;
    }
    for _ in 0..4 {
        if d.app().selected_sound.is_some() {
            break;
        }
        plain(d, "j")?;
    }
    d.settled()?;
    d.check(
        "Tab and j select the new catalog sound and keep the Edit cursor",
        d.app().selected_sound.as_ref() == Some(&d.app().sound_rows[0].0)
            && d.app().sequence_cursor == 6,
        json!({"sound":"selected","edit_cursor":6}),
        d.snapshot(),
    )?;
    taught(d, "place the sound", &[",s"])?;
    edit(d, |d| plain(d, ",s"))?;
    d.settled()?;
    let document = document(d)?;
    d.check(
        ",s places the catalog sound at the Edit cursor without adding picture time",
        document.sounds().len() == 1 && document.nodes() == &nodes,
        json!({"sounds":1,"picture_unchanged":true}),
        json!({"sounds":document.sounds().len(),"error":d.app().error}),
    )?;
    d.capture("Sound placed with ,s")
}

fn gain_of(d: &Driver<'_>, node: &NodeId) -> Result<(i32, bool), String> {
    let document = document(d)?;
    Ok(document.nodes()[node]
        .audio_treatments
        .clip_gain()
        .map_or((0, false), |gain| {
            (gain.trim().millidecibels(), gain.muted())
        }))
}

fn gain_and_mute(d: &mut Driver<'_>) -> Result<(), String> {
    focus_edit(d)?;
    let target = children(d)?.get(2).cloned().ok_or("No third beat")?;
    select(d, &target)?;
    taught(d, "raise gain", &["+3 dB  +", "+ / -"])?;
    edit(d, |d| plain(d, "+"))?;
    d.settled()?;
    d.check(
        "+ raises the selected beat by exactly 3 dB",
        gain_of(d, &target)? == (3000, false),
        json!([3000, false]),
        json!(gain_of(d, &target)?),
    )?;
    taught(d, "mute", &["Mute  ,m"])?;
    edit(d, |d| plain(d, ",m"))?;
    d.settled()?;
    d.check(
        ",m mutes the whole beat and keeps its gain",
        gain_of(d, &target)? == (3000, true),
        json!([3000, true]),
        json!(gain_of(d, &target)?),
    )?;
    d.capture("Gain and mute")
}

fn undo_redo(d: &mut Driver<'_>) -> Result<(), String> {
    let muted = document(d)?;
    taught(d, "undo", &["u undo", "Undo"])?;
    edit(d, |d| plain(d, "u"))?;
    d.settled()?;
    let unmuted = document(d)?;
    d.check(
        "u undoes only the mute",
        unmuted.nodes() != muted.nodes() && unmuted.duration().ok() == muted.duration().ok(),
        json!("mute removed"),
        json!({"frames":frames(d)}),
    )?;
    edit(d, |d| {
        d.key_modified(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT)
    })?;
    d.settled()?;
    d.check(
        "⌘⇧Z redoes the mute exactly",
        document(d)?.nodes() == muted.nodes(),
        json!("redo restores the muted document"),
        json!({"can_redo":d.app().workspace.as_ref().map(|w| w.can_redo)}),
    )
}

fn macro_and_dot(d: &mut Driver<'_>) -> Result<(), String> {
    focus_edit(d)?;
    plain(d, "gg")?;
    typed(
        d,
        "Move to 60",
        &[
            (Key::Num6, Modifiers::NONE, "6"),
            (Key::Num0, Modifiers::NONE, "0"),
            (Key::L, Modifiers::NONE, "l"),
        ],
    )?;
    d.settled()?;
    let before = frames(d);
    taught(d, "record a macro", &["+ letter: record / run macro"])?;
    plain(d, "qa")?;
    d.check(
        "qa starts recording macro a",
        d.app().macros.recording_name() == Some('a'),
        json!("recording a"),
        d.snapshot(),
    )?;
    typed(
        d,
        "Record 2l",
        &[
            (Key::Num2, Modifiers::NONE, "2"),
            (Key::L, Modifiers::NONE, "l"),
        ],
    )?;
    d.settled()?;
    edit(d, |d| plain(d, "x"))?;
    d.wait_for("Recorded cut acknowledged", |app| {
        !app.service.is_busy() && !app.macros.is_pending()
    })?;
    plain(d, "q")?;
    d.wait_for("Macro a saved", |app| {
        !app.service.is_busy()
            && !app.macros.recording()
            && !app.macros.is_pending()
            && app.copied.entries().any(|(slot, value)| {
                slot == 'a' && matches!(value, crate::preview::copied::Content::Macro(_))
            })
    })?;
    d.settled()?;
    let recorded = frames(d);
    taught(d, "run a macro", &["+ letter: record / run macro"])?;
    let before_run = d.revision();
    d.events(
        "@a",
        [
            stroke(Key::Num2, Modifiers::SHIFT, Some("@")),
            stroke(Key::A, Modifiers::NONE, Some("a")),
        ]
        .concat(),
    )?;
    d.changed(&before_run)?;
    let ran = frames(d);
    // A named run is not a dot target; an ordinary cut is.
    d.check(
        "After @a, . teaches no repeat target",
        scenarios::text_paint_visibility(d, "repeat cut").is_empty(),
        json!("no . hint"),
        json!(painted(d)),
    )?;
    edit(d, |d| plain(d, "x"))?;
    d.settled()?;
    let cut = frames(d);
    taught(d, "repeat the last edit", &["repeat cut"])?;
    edit(d, |d| {
        d.events(".", stroke(Key::Period, Modifiers::NONE, Some(".")))
    })?;
    d.settled()?;
    d.check(
        "Recording, @a, x and . each cut exactly one frame",
        recorded == before - 1 && ran == recorded - 1 && cut == ran - 1 && frames(d) == cut - 1,
        json!({"recorded":before - 1,"ran":before - 2,"x":before - 3,"dot":before - 4}),
        json!({"recorded":recorded,"ran":ran,"x":cut,"dot":frames(d)}),
    )?;
    d.capture("Macro and dot")
}

fn trim(d: &mut Driver<'_>) -> Result<(), String> {
    focus_edit(d)?;
    let document = document(d)?;
    let target = children(d)?
        .into_iter()
        .rev()
        .find(|node| {
            matches!(
                document.nodes()[node].kind,
                NodeKind::Source { .. } | NodeKind::Retime { .. }
            ) && document.nodes()[node].framing.is_none()
                && document.nodes()[node].audio_treatments.is_empty()
        })
        .ok_or("No plain Source beat for Trim")?;
    select(d, &target)?;
    let before = frames(d);
    taught(d, "Trim", &[",v"])?;
    plain(d, ",v")?;
    d.wait_for("Trim opens", |app| app.trim.is_some())?;
    plain(d, "l")?;
    d.wait_for("Trim pair submitted", |app| {
        !app.service.is_busy()
            && app.trim.as_ref().is_some_and(|draft| {
                draft.prepared_for_check().is_some()
                    && draft
                        .identity_for_check()
                        .is_some_and(|identity| app.junction_pictures.ready_for_apply(identity))
            })
    })?;
    d.step("Paint Trim controls", true)?;
    taught(d, "apply Trim", &["Apply  Enter"])?;
    edit(d, |d| d.key(Key::Enter))?;
    d.wait_for("Trim closed", |app| app.trim.is_none())?;
    d.settled()?;
    d.check(
        "Trim In +1 ripples one frame out of the beat",
        frames(d) == before - 1,
        json!(before - 1),
        json!(frames(d)),
    )?;
    d.capture("Trimmed In by one frame")
}

fn ai_pause(d: &mut Driver<'_>, hold: &NodeId) -> Result<(), String> {
    if let Err(reason) = crate::project::generation::synthetic_tools() {
        d.report.skipped.push(format!(
            "AI pause step skipped: the synthetic Ready worker's tools are unavailable: {reason}"
        ));
        return Ok(());
    }
    focus_edit(d)?;
    select(d, hold)?;
    taught(d, "AI pictures", &[",a"])?;
    plain(d, ",a")?;
    d.wait_for("AI variant Ready", |app| {
        app.ai.job().is_some_and(|job| !job.running()) && app.ai.variant_count() >= 1
    })?;
    d.step("Variant listed", true)?;
    taught(d, "preview AI", &[",x"])?;
    plain(d, ",x")?;
    d.wait_for("Candidate previewed", |app| {
        app.presentation.displayed_candidate()
            && !app.presentation.loading()
            && !app.presentation.needs_render()
    })?;
    d.capture("AI candidate preview")?;
    taught(d, "accept AI", &[":accept-ai"])?;
    edit(d, |d| d.command("accept-ai"))?;
    d.settled()?;
    let document = document(d)?;
    d.check(
        ":accept-ai replaces only the pause's picture provider",
        matches!(&document.nodes()[hold].kind, NodeKind::Hold { recipe } if matches!(recipe.video, HoldVideo::Generated { .. })),
        json!("Generated"),
        json!(format!("{:?}", document.nodes()[hold].kind)),
    )?;
    d.capture("AI pause accepted")
}

fn render(d: &mut Driver<'_>) -> Result<PathBuf, String> {
    let output = d.options.output.join("full-session-render");
    std::fs::create_dir(&output).map_err(|error| error.to_string())?;
    let movie = output
        .canonicalize()
        .map_err(|error| error.to_string())?
        .join("session.mp4");
    focus_edit(d)?;
    taught(d, "render", &["⌘E"])?;
    let revision = d.revision();
    d.app_mut().dialogs = Dialogs::scripted(vec![(DialogKind::Render, Some(movie.clone()))]);
    d.key_modified(Key::E, Modifiers::COMMAND)?;
    super::render::wait_terminal(d)?;
    let workflow = d
        .app()
        .render_job
        .as_ref()
        .and_then(|update| update.workflow.clone())
        .ok_or("No render workflow")?;
    d.check(
        "⌘E renders the committed revision with the automatic policy and publishes it",
        workflow.status.outcome == Some(WorkflowOutcome::Published)
            && workflow.status.stage == WorkflowStage::Finished
            && workflow.revision.as_str() == revision
            && workflow
                .status
                .receipt
                .as_ref()
                .is_some_and(|receipt| receipt.movie == movie),
        json!({"outcome":"Published","revision":revision}),
        json!(format!("{:?}", workflow.status)),
    )?;
    d.capture("Render published")?;
    let package = d.app().workspace.as_ref().ok_or("No project")?.path.clone();
    let (request, _) = deadpan_cli::export_verification::cli::parse(&[
        package.to_str().ok_or("Path")?,
        "--movie",
        movie.to_str().ok_or("Path")?,
        "--revision",
        &revision,
        "--every",
        "12",
    ])
    .map_err(|error| error.to_string())?;
    let report = deadpan_cli::export_verification::verify(
        &request,
        &AtomicBool::new(false),
        Instant::now() + Duration::from_secs(600),
    )
    .map_err(|error| error.to_string())?;
    d.check(
        "verify-export independently passes on the published movie",
        report.passed,
        json!({"passed":true}),
        json!({"passed":report.passed,"failures":format!("{:?}", report.failures)}),
    )?;
    d.key(Key::Escape)?;
    d.step("Render status closed", true)?;
    Ok(movie)
}

fn close_and_reopen(d: &mut Driver<'_>, _movie: &Path) -> Result<(), String> {
    focus_edit(d)?;
    let saved = document(d)?;
    let path = d.app().workspace.as_ref().ok_or("No project")?.path.clone();
    // An unfinished recording refuses :close (the recorder rejects the
    // command before the close readiness check, which unit tests cover).
    plain(d, "qb")?;
    d.command("close")?;
    d.step("Refused close", true)?;
    d.check(
        ":close refuses while a macro is being recorded and keeps the project open",
        d.app().workspace.is_some()
            && d.app().macros.recording()
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("macro")),
        json!("refused with the recording named"),
        json!({"error":d.app().error,"open":d.app().workspace.is_some()}),
    )?;
    d.key(Key::Escape)?;
    d.wait_for("Recording cancelled", |app| {
        !app.macros.recording() && !app.macros.is_pending()
    })?;
    d.key(Key::Colon)?;
    d.events("Type clo", vec![Event::Text("clo".into())])?;
    taught(d, "close the project", &[":close  close this project"])?;
    d.events(
        "Finish :close",
        vec![
            Event::Text("se".into()),
            key_event(Key::Enter, Modifiers::NONE, true),
            key_event(Key::Enter, Modifiers::NONE, false),
        ],
    )?;
    d.wait_for("Project closed", |app| {
        app.workspace.is_none() && !app.service.is_busy()
    })?;
    d.step("Start screen after close", true)?;
    taught(d, "open a project", &["⌘O"])?;
    d.app_mut().dialogs = Dialogs::scripted(vec![(DialogKind::OpenProject, Some(path))]);
    d.key_modified(Key::O, Modifiers::COMMAND)?;
    d.wait_for("Project reopened", |app| {
        app.workspace.is_some() && !app.service.is_busy()
    })?;
    d.settled()?;
    let reopened = document(d)?;
    let bank = deadpan_store::ProjectStore::open(
        &d.app().workspace.as_ref().ok_or("No project")?.path,
        deadpan_store::AccessMode::ReadOnly,
    )
    .and_then(|store| store.registers())
    .map_err(|error| error.to_string())?;
    let macro_saved = bank
        .entries
        .get(&RegisterName::new('a').map_err(|error| error.to_string())?)
        .is_some_and(|value| matches!(value.as_ref(), RegisterValue::Macro { .. }));
    d.check(
        "Reopening restores the exact saved revision, document and macro register",
        reopened.revision_id() == saved.revision_id()
            && reopened.nodes() == saved.nodes()
            && reopened.sounds() == saved.sounds()
            && macro_saved,
        json!({"revision":saved.revision_id(),"macro_a":true}),
        json!({"revision":reopened.revision_id(),"macro_a":macro_saved}),
    )?;
    d.capture("Reopened session")
}
