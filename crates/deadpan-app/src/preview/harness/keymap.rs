//! Startup-injected keymaps exercised through the production input router.

use super::*;
use deadpan_core::{HoldAudio, NodeKind, ProjectDocument};
use egui::{Event, Key, Modifiers};
use egui_kittest::kittest::Queryable as _;

const VALID: &[u8] = br#"{
  "version":1,"key_mode":"logical","bindings":[
    {"action":"frame.next","keys":[["a","h"]]},
    {"action":"repeat.operator","keys":[["b"]]},
    {"action":"hold","keys":[["e","b"]]},
    {"action":"command","keys":[["e","c"],[":"],["F2"]]},
    {"action":"search","keys":[["e","s"],["/"]]},
    {"action":"help","keys":[["?"],[";"]]},
    {"action":"cut.frames","keys":[["z"]]},
    {"action":"trim","keys":[["e","t"]]}
  ]
}"#;

// A valid first override precedes the malformed declaration. No part may install.
const INVALID: &[u8] = br#"{
  "version":1,"key_mode":"logical","bindings":[
    {"action":"frame.previous","keys":[["a"]]},
    {"action":"does.not.exist","keys":[["b"]]}
  ]
}"#;

const LONG_TRANSPORT: &[u8] = br#"{
  "version":1,"key_mode":"logical","bindings":[
    {"action":"playback","keys":[["F7","F8","F9","F10","F11","F12"],["Space"]]},
    {"action":"audition","keys":[["F15","F16","F17","F18","F19","F20"],["Shift+Space"]]}
  ]
}"#;

pub(super) fn startup(name: &str, documents: &Path) -> Result<crate::keymap::Startup, String> {
    let bytes = match name {
        "keymap" => VALID,
        "keymap-error" => INVALID,
        "original-layout-long" => LONG_TRANSPORT,
        _ => return Ok(crate::keymap::Startup::shipped()),
    };
    // `documents` belongs to this replay's exclusive temporary/retained root.
    // Native user settings are never consulted, even before this injection.
    std::fs::create_dir_all(documents).map_err(|error| error.to_string())?;
    let path = documents
        .canonicalize()
        .map_err(|error| error.to_string())?
        .join("keymap.json");
    std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
    Ok(crate::keymap::Startup::from_file(
        crate::keymap_file::read_from(path),
    ))
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.check(
        "The private keymap is installed before the first editor input",
        !d.app().keymap_error
            && d.app().keymap_status().starts_with("Custom editor keys")
            && d.app().keymap_status().contains("Documents/keymap.json"),
        json!("custom logical keys from the private scenario file"),
        json!(d.app().keymap_status()),
    )?;
    let baseline = document(d)?.clone();
    minimum_size(d)?;
    d.command("help")?;
    d.step("Measure remapped Repeat help", false)?;
    let example = "3bah repeats one frame three times, b3ah repeats three frames twice";
    for _ in 0..128 {
        let paint = scenarios::text_paint_visibility(d, example);
        if !paint.is_empty() && paint.iter().all(|part| part["fully_visible"] == true) {
            break;
        }
        d.key(Key::J)?;
    }
    painted(d, example)?;
    d.capture("Repeat examples use the configured operator and motion")?;
    d.key(Key::Escape)?;
    held_motion(d)?;
    custom_hold(d, &baseline)?;
    field_entry(d)?;
    native_ownership(d)?;
    trim_absence(d)?;
    d.check(
        "Keymap navigation, text and refused actions preserve authored state",
        same_document(document(d)?, &baseline)? && !d.app().workspace.as_ref().unwrap().can_undo,
        json!("initial document with a fresh Undo revision"),
        d.snapshot(),
    )
}

fn minimum_size(d: &mut Driver<'_>) -> Result<(), String> {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 640.0));
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing keymap replay viewport")?
        .inner_rect = Some(rect);
    d.step("Custom key hints at the minimum native window size", true)?;
    for label in ["eb", "pause", "ec", "z"] {
        painted(d, label)?;
    }
    d.events(
        "Custom shared prefix with its native text echo",
        stroke(Key::E, "e"),
    )?;
    d.check(
        "An arbitrary shared prefix is visible and teaches its actual continuations",
        d.app().bindings.pending() == "e" && !d.app().command_open,
        json!("e"),
        d.snapshot(),
    )?;
    // Independent literals, not labels produced by the map under test.
    for label in ["b pause", "c command", "s search", "t Trim"] {
        painted(d, label)?;
    }
    d.capture("Custom e prefix and exact continuations at 960 by 640")?;
    d.key(Key::Escape)
}

fn held_motion(d: &mut Driver<'_>) -> Result<(), String> {
    d.chord(&[Key::G, Key::G])?;
    let revision = d.revision();
    d.events("Hold shipped h at the start boundary", vec![down(Key::H)])?;
    d.key(Key::A)?;
    d.events(
        "A held h cannot complete the newly entered ah path",
        vec![down(Key::H)],
    )?;
    d.check(
        "Held root motion cannot become a custom prefix continuation",
        d.app().sequence_cursor == 0
            && d.app().bindings.pending() == "a"
            && d.revision() == revision,
        json!({"Edit":0,"pending":"a"}),
        d.snapshot(),
    )?;
    painted(d, "h move by frames")?;
    d.events(
        "Release then explicitly complete ah",
        vec![up(Key::H), down(Key::H)],
    )?;
    d.check(
        "Explicit ah moves forward once",
        d.app().sequence_cursor == 1,
        json!(1),
        d.snapshot(),
    )?;
    d.events(
        "Repeat the resolved ah motion while h remains down",
        vec![down(Key::H)],
    )?;
    d.check(
        "Held custom terminal repeats its resolved forward action, not root h",
        d.app().sequence_cursor == 2 && d.app().bindings.pending().is_empty(),
        json!({"Edit":2,"pending":""}),
        d.snapshot(),
    )?;
    d.events("Release custom motion terminal", vec![up(Key::H)])?;
    d.chord(&[Key::Num3, Key::A])?;
    d.events("Counted custom forward motion", vec![down(Key::H)])?;
    d.check(
        "Three ah advances exactly three frames",
        d.app().sequence_cursor == 5,
        json!(5),
        d.snapshot(),
    )?;
    d.events("Count is consumed before native repeat", vec![down(Key::H)])?;
    d.check(
        "Repeated counted motion advances one unit",
        d.app().sequence_cursor == 6,
        json!(6),
        d.snapshot(),
    )?;
    d.events(
        "Release and press root h in the same native batch",
        vec![up(Key::H), down(Key::H), up(Key::H)],
    )?;
    d.check(
        "Release clears the resolved-motion latch",
        d.app().sequence_cursor == 5,
        json!(5),
        d.snapshot(),
    )?;
    d.key(Key::A)?;
    d.events(
        "Latch custom forward h before native focus loss",
        vec![down(Key::H)],
    )?;
    d.events(
        "Native focus loss replaces the missing key release",
        vec![Event::WindowFocused(false)],
    )?;
    d.events(
        "Return focus then freshly press root h",
        vec![Event::WindowFocused(true), down(Key::H), up(Key::H)],
    )?;
    d.check(
        "Window focus loss clears held motion without altering history",
        d.app().sequence_cursor == 5 && d.revision() == revision,
        json!({"Edit":5,"revision":revision}),
        d.snapshot(),
    )?;
    d.settled()
}

fn custom_hold(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    d.chord(&[Key::G, Key::G])?;
    let before = d.revision();
    let mut events = stroke(Key::E, "e");
    events.extend(stroke(Key::B, "b"));
    d.events("Insert one Hold through the configured eb path", events)?;
    d.changed(&before)?;
    let frames = navigation::duration::DurationInput::half_seconds(1)
        .resolve(baseline.presentation_basis().frame_rate)?
        .frames();
    let holds: Vec<_> = document(d)?
        .nodes()
        .values()
        .filter_map(|node| match &node.kind {
            NodeKind::Hold { recipe } => Some(recipe),
            _ => None,
        })
        .collect();
    d.check(
        "Custom eb creates exactly one half-second silent Hold and one history entry",
        holds.len() == 1
            && holds[0].duration.frames() == frames
            && holds[0].audio == HoldAudio::Silence
            && document(d)?
                .duration()
                .map_err(|error| error.to_string())?
                .frames()
                == baseline
                    .duration()
                    .map_err(|error| error.to_string())?
                    .frames()
                    + frames
            && d.app().workspace.as_ref().unwrap().can_undo
            && !d.app().workspace.as_ref().unwrap().can_redo,
        json!({"holds":1,"frames":frames}),
        d.snapshot(),
    )?;
    let saved = d.revision();
    d.key(Key::U)?;
    d.changed(&saved)?;
    d.check(
        "One shipped Undo exactly restores the document after custom Hold",
        same_document(document(d)?, baseline)?
            && !d.app().workspace.as_ref().unwrap().can_undo
            && d.app().workspace.as_ref().unwrap().can_redo,
        json!("all authored fields restored, fresh revision, no earlier Undo"),
        d.snapshot(),
    )
}

fn field_entry(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    let mut events = stroke(Key::E, "e");
    events.extend(stroke(Key::C, "c"));
    events.extend([
        Event::Text(":".into()),
        Event::Text("/".into()),
        Event::Text("é".into()),
    ]);
    d.events(
        "Custom Command preserves later colon, slash and Unicode text",
        events,
    )?;
    command_text(d, ":/é")?;
    d.key(Key::Escape)?;

    let mut events = stroke(Key::E, "e");
    events.extend(stroke(Key::C, "c"));
    events.extend([
        Event::Text("source".into()),
        down(Key::Enter),
        up(Key::Enter),
    ]);
    d.events(
        "Open ec, type the full command and submit in one native batch",
        events,
    )?;
    d.check(
        "Same-frame final text is processed before Command Enter",
        !d.app().command_open && d.app().view == View::Source && d.revision() == revision,
        json!("Original view, no history edit"),
        d.snapshot(),
    )?;
    d.settled()?;
    d.events(
        "Nonprintable F2 opener has no companion Text",
        vec![
            down(Key::F2),
            Event::Text("sequence".into()),
            up(Key::F2),
            down(Key::Enter),
            up(Key::Enter),
        ],
    )?;
    d.check(
        "F2 retains unpaired text immediately after its keydown",
        !d.app().command_open && d.app().view == View::Sequence && d.revision() == revision,
        json!("Edit view, no history edit"),
        d.snapshot(),
    )?;

    let mut events = stroke(Key::E, "e");
    events.extend([down(Key::C), Event::Text("c".into())]);
    d.events("Keep the custom Command opener physically held", events)?;
    command_text(d, "")?;
    d.events(
        "Held opener repeat cannot leak its text into the command",
        vec![down(Key::C), Event::Text("c".into())],
    )?;
    command_text(d, "")?;
    d.events(
        "Release then type c as native field input",
        vec![
            up(Key::C),
            down(Key::C),
            Event::Text("c".into()),
            up(Key::C),
        ],
    )?;
    command_text(d, "c")?;
    d.key(Key::Escape)?;

    d.events(
        "Open Command with shifted physical Semicolon",
        vec![
            Event::Key {
                key: Key::Colon,
                physical_key: Some(Key::Semicolon),
                modifiers: Modifiers::SHIFT,
                pressed: true,
                repeat: false,
            },
            Event::Text(":".into()),
        ],
    )?;
    command_text(d, "")?;
    d.events(
        "Release Shift while the physical opener remains held",
        vec![down(Key::Semicolon), Event::Text(";".into())],
    )?;
    command_text(d, "")?;
    d.events(
        "Release the physical opener then type native Semicolon",
        vec![
            up(Key::Semicolon),
            down(Key::Semicolon),
            Event::Text(";".into()),
            up(Key::Semicolon),
            up(Key::Colon),
        ],
    )?;
    command_text(d, ";")?;
    d.key(Key::Escape)?;

    d.events(
        "Keep shifted physical Semicolon held while opening another Command",
        vec![
            Event::Key {
                key: Key::Colon,
                physical_key: Some(Key::Semicolon),
                modifiers: Modifiers::SHIFT,
                pressed: true,
                repeat: false,
            },
            Event::Text(":".into()),
        ],
    )?;
    d.events(
        "Submit Command before releasing its physical opener",
        vec![
            Event::Text("source".into()),
            down(Key::Enter),
            up(Key::Enter),
        ],
    )?;
    d.check(
        "A held opener does not prevent a different submit key",
        !d.app().command_open && d.app().view == View::Source,
        json!("Original after Command closes"),
        d.snapshot(),
    )?;
    d.events(
        "Release Shift on the next frame while physical Semicolon remains held",
        vec![down(Key::Semicolon), Event::Text(";".into())],
    )?;
    d.check(
        "The held opener cannot become the editor Semicolon Help binding after submit",
        !d.app().help_open && !d.app().command_open && d.revision() == revision,
        json!("no editor action before the opener's key-up"),
        d.snapshot(),
    )?;
    d.events(
        "Release both reported logical identities of the physical opener",
        vec![up(Key::Semicolon), up(Key::Colon)],
    )?;
    d.command("sequence")?;
    d.settled()?;

    let mut events = stroke(Key::E, "e");
    events.extend(stroke(Key::C, "c"));
    events.extend([
        Event::Text("source".into()),
        down(Key::Enter),
        up(Key::Enter),
        Event::Text("post-submit text".into()),
    ]);
    // The later command must run in the context produced by the first one.
    events.extend(stroke(Key::E, "e"));
    events.extend(stroke(Key::C, "c"));
    events.extend([
        Event::Text("sequence".into()),
        down(Key::Enter),
        up(Key::Enter),
    ]);
    d.events("Two ordered commands in one native input batch", events)?;
    d.check(
        "Text after Enter cannot alter the command being submitted",
        !d.app().command_open
            && d.app().command == "source"
            && d.app().view == View::Source
            && d.revision() == revision,
        json!("source submitted before the remaining input"),
        d.snapshot(),
    )?;
    d.step("The next outer frame owns the post-submit suffix", false)?;
    d.check(
        "Post-submit keys keep order and run in the resulting context",
        !d.app().command_open
            && d.app().command == "sequence"
            && d.app().view == View::Sequence
            && d.revision() == revision,
        json!("sequence submitted by the retained suffix"),
        d.snapshot(),
    )?;
    d.settled()?;

    let mut events = stroke(Key::E, "e");
    events.extend(stroke(Key::S, "s"));
    events.extend([
        Event::Text("/".into()),
        Event::Text(":".into()),
        Event::Text("é".into()),
    ]);
    d.events("Custom Search keeps later punctuation and Unicode", events)?;
    d.check(
        "Custom Search removes only its editor prefix text",
        d.app().source_search == "/:é"
            && d.harness
                .ctx
                .memory(|memory| memory.has_focus(egui::Id::new(SEARCH_ID)))
            && d.revision() == revision,
        json!("/:é"),
        json!({"search":d.app().source_search,"state":d.snapshot()}),
    )?;
    d.key_modified(Key::A, Modifiers::COMMAND)?;
    d.events(
        "Replace search through native text editing",
        vec![
            Event::Text(String::new()),
            down(Key::Backspace),
            up(Key::Backspace),
        ],
    )?;
    d.key(Key::Escape)?;
    d.events(
        "Search cancellation separates later text from the closing field",
        vec![
            down(Key::Slash),
            Event::Text("/".into()),
            up(Key::Slash),
            Event::Text("before escape".into()),
            down(Key::Escape),
            up(Key::Escape),
            Event::Text("after escape".into()),
        ],
    )?;
    d.check(
        "Text after Escape cannot change the closing Search field",
        d.app().source_search == "before escape",
        json!("before escape"),
        json!(&d.app().source_search),
    )?;
    d.step(
        "Post-cancel text reaches the editor without changing Search",
        false,
    )?;
    d.check(
        "Post-cancel text does not return to the closed Search field",
        d.app().source_search == "before escape" && d.revision() == revision,
        json!("before escape, unchanged revision"),
        d.snapshot(),
    )?;
    d.command("sequence")?;
    d.settled()?;
    let keys = d.rect("Keys  ?")?;
    d.check(
        "Short custom keys keep the header compact after repeated field transitions",
        keys.bottom() <= 60.0,
        json!("header action bottom at most 60 points"),
        json!({"keys_rect":format!("{keys:?}")}),
    )
}

fn native_ownership(d: &mut Driver<'_>) -> Result<(), String> {
    let before = document(d)?.clone();
    let cursor = d.app().sequence_cursor;
    d.harness.get_by_label("Keys  ?").focus();
    d.step("Native Keys button receives accessibility focus", false)?;
    d.check(
        "A real native control owns focus before custom cut",
        native_control_focused(&d.harness.ctx),
        json!(true),
        d.snapshot(),
    )?;
    d.key(Key::Z)?;
    d.check(
        "Custom cut key respects native control focus",
        *document(d)? == before && d.app().sequence_cursor == cursor && !d.app().service.is_busy(),
        json!("no cut or queued edit"),
        d.snapshot(),
    )?;
    d.command("sequence")?;
    d.key(Key::Colon)?;
    let mut events = stroke(Key::E, "e");
    events.extend(stroke(Key::B, "b"));
    events.extend(stroke(Key::Z, "z"));
    d.events(
        "Custom Hold and cut strings belong to the native command field",
        events,
    )?;
    command_text(d, "ebz")?;
    d.check(
        "Native field text cannot author edits",
        *document(d)? == before,
        json!("unchanged"),
        d.snapshot(),
    )?;
    d.key_modified(Key::A, Modifiers::COMMAND)?;
    d.events(
        "IME preedit owns custom editor keys and Enter",
        vec![
            Event::Ime(egui::ImeEvent::Preedit {
                text: "source".into(),
                active_range_chars: Some(0..6),
            }),
            down(Key::Enter),
            up(Key::Enter),
        ],
    )?;
    d.check(
        "IME preedit retains Command and authored state",
        d.app().command_open && d.app().ime_composing && *document(d)? == before,
        json!("composition owns Enter"),
        d.snapshot(),
    )?;
    d.events(
        "IME commit owns its same-frame Enter",
        vec![
            Event::Ime(egui::ImeEvent::Commit("source".into())),
            down(Key::Enter),
            up(Key::Enter),
        ],
    )?;
    d.check(
        "IME commit cannot submit in the same batch",
        d.app().command_open && !d.app().ime_composing && *document(d)? == before,
        json!("Command remains open"),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    d.command("sequence")
}

fn trim_absence(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    d.command("source")?;
    d.key(Key::E)?;
    d.check(
        "Shared custom e prefix captures unavailable Trim target immediately",
        d.app()
            .trim_prefix_target
            .as_ref()
            .is_some_and(Result::is_err)
            && d.app().bindings.pending() == "e",
        json!("Original target absence captured at e"),
        d.snapshot(),
    )?;
    d.key(Key::T)?;
    d.check(
        "Custom et preserves captured Original refusal",
        d.app().trim.is_none()
            && d.revision() == revision
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Your edit")),
        json!("no Trim draft or edit"),
        d.snapshot(),
    )?;
    d.command("sequence")?;
    d.settled()
}

pub(super) fn error(d: &mut Driver<'_>) -> Result<(), String> {
    let baseline = document(d)?.clone();
    d.check(
        "Malformed keymap is rejected as a whole at startup",
        d.app().keymap_error
            && d.app().keymap_status().contains("does.not.exist")
            && d.app()
                .keymap_status()
                .contains("All shipped editor keys remain active")
            && d.app().keymap_status().contains("Documents/keymap.json"),
        json!("persistent file diagnostic and shipped fallback"),
        json!(d.app().keymap_status()),
    )?;
    d.chord(&[Key::G, Key::G, Key::Num3, Key::L])?;
    d.key(Key::A)?;
    d.check(
        "Earlier valid override was not partially installed",
        d.app().sequence_cursor == 3 && d.app().bindings.pending().is_empty(),
        json!(3),
        d.snapshot(),
    )?;
    d.key(Key::H)?;
    d.check(
        "Shipped h remains active after keymap rejection",
        d.app().sequence_cursor == 2,
        json!(2),
        d.snapshot(),
    )?;
    painted(d, "Keymap error")?;
    d.click("Keymap error")?;
    d.check(
        "Persistent header error opens contextual Help",
        d.app().help_open,
        json!(true),
        d.snapshot(),
    )?;
    // egui measures a newly opened Window before its first painted frame.
    // Retain that transition, then inspect the complete diagnostic's paint.
    d.step("Keymap error reference initial measurement", true)?;
    painted(d, "does.not.exist")?;
    d.capture("Rejected keymap detail remains visible after ordinary navigation")?;
    d.key(Key::Escape)?;
    let before = d.revision();
    d.chord(&[Key::Comma, Key::H])?;
    d.changed(&before)?;
    painted(d, "Keymap error")?;
    let saved = d.revision();
    d.key(Key::U)?;
    d.changed(&saved)?;
    d.check(
        "Shipped Hold and Undo stay atomic while the keymap error persists",
        same_document(document(d)?, &baseline)?
            && d.app().keymap_error
            && !d.app().workspace.as_ref().unwrap().can_undo,
        json!("baseline restored and keymap error retained"),
        d.snapshot(),
    )?;
    painted(d, "Keymap error")
}

fn painted(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    let mut paint = scenarios::text_paint_visibility(d, label);
    if label.chars().count() == 1 {
        // A single keycap must not pass because its letter appears in prose.
        paint.retain(|part| part["text"] == label);
    }
    d.check(
        "Configured key guidance is actually painted within its clip and viewport",
        !paint.is_empty() && paint.iter().all(|part| part["fully_visible"] == true),
        json!(label),
        json!(paint),
    )
}

fn command_text(d: &mut Driver<'_>, expected: &str) -> Result<(), String> {
    d.check(
        "Command retains exactly the native field text after custom activation",
        d.app().command_open && d.app().command == expected,
        json!(expected),
        json!({"command":d.app().command,"state":d.snapshot()}),
    )
}

fn document<'a>(d: &'a Driver<'_>) -> Result<&'a ProjectDocument, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.document.as_ref())
        .ok_or_else(|| "Keymap replay has no document".into())
}

fn same_document(actual: &ProjectDocument, expected: &ProjectDocument) -> Result<bool, String> {
    let mut actual = serde_json::to_value(actual).map_err(|error| error.to_string())?;
    actual["revision_id"] = json!(expected.revision_id());
    Ok(actual == serde_json::to_value(expected).map_err(|error| error.to_string())?)
}

fn down(key: Key) -> Event {
    key_event(key, Modifiers::NONE, true)
}
fn up(key: Key) -> Event {
    key_event(key, Modifiers::NONE, false)
}
fn stroke(key: Key, text: &str) -> Vec<Event> {
    vec![down(key), Event::Text(text.into()), up(key)]
}
