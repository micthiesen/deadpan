//! Typed command arguments through the production command line: real text
//! input shows the next parameter with its type, unit and default, offers
//! choice values, names an invalid value before Enter and says what the
//! command acts on, from the verb's declarative schema. Tab completes the
//! verb, a parameter name and enum values, cycling with repeated Tab,
//! Shift+Tab, Down and Up, and a parameter conflict is named before Enter.

use egui::{Key, Modifiers};

use super::*;

fn text(value: &str) -> egui::Event {
    egui::Event::Text(value.into())
}

fn painted(d: &Driver<'_>, needle: &str) -> Vec<Value> {
    super::scenarios::text_paint_visibility(d, needle)
}

fn fully_visible(paints: &[Value]) -> bool {
    !paints.is_empty() && paints.iter().all(|paint| paint["fully_visible"] == true)
}

/// The accessible text of the first completion row: every row, untruncated.
fn spoken(d: &Driver<'_>) -> Option<String> {
    d.harness
        .root()
        .children_recursive()
        .take(4096)
        .flat_map(|node| {
            // egui exposes a label widget's text as its value.
            let access = node.accesskit_node();
            [access.label(), access.value()]
        })
        .flatten()
        .find(|label| label.contains("Next: ") && label.contains("Acts on "))
}

fn clear(d: &mut Driver<'_>) -> Result<(), String> {
    for _ in 0..d.app().command.chars().count() {
        d.key(Key::Backspace)?;
    }
    Ok(())
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("Typing is replayed as egui Text events through the production command line; physical key delivery, IME composition in a native window (unit tests cover Tab refusal while composing) and VoiceOver speech are not exercised here.".into());
    super::transcript::focus_your_edit(d)?;
    d.settled()?;
    let revision = d.revision();

    d.key(Key::Colon)?;
    d.wait_for("The command line opens", |app| app.command_open)?;
    d.events("Type :repeat 3 and a space", vec![text("repeat 3 ")])?;
    d.step("Repeat parameters paint", true)?;
    let gap = painted(d, "gap=duration (keeps)");
    let pitch = painted(d, "pitch=preserve|tape (keeps)");
    let target = painted(d, "acts on ");
    d.check(
        "After :repeat 3 the line lists every parameter with its type and default, and the target",
        d.app().command == "repeat 3 "
            && fully_visible(&gap)
            && !pitch.is_empty()
            && fully_visible(&target)
            && d.revision() == revision,
        json!({"gap":"gap=duration (keeps)","pitch":"pitch=preserve|tape (keeps)","target":"acts on …"}),
        json!({"command":d.app().command,"gap":gap,"pitch":pitch,"target":target}),
    )?;
    let accessible = spoken(d);
    d.check(
        "The completion is one accessible text with every parameter, the target and the usage",
        accessible.as_deref().is_some_and(|label| {
            label.contains("extend=hold (refuses overflow)")
                && label.contains("Usage: :repeat [PLAYS] [gap=duration]")
        }),
        json!("Next … Choices … extend=hold … Acts on … Usage: :repeat [PLAYS] …"),
        json!(accessible),
    )?;
    d.capture("Repeat parameters with defaults")?;

    clear(d)?;
    d.events("Type :ramp 1 2 pitch=", vec![text("ramp 1 2 pitch=")])?;
    d.step("Pitch values paint", true)?;
    let values = painted(d, "pitch=preserve  pitch=tape");
    let next = painted(d, "next: pitch= keep pitch");
    d.check(
        "After pitch= the line offers the enum values and explains the parameter",
        d.app().command == "ramp 1 2 pitch="
            && fully_visible(&values)
            && !next.is_empty()
            && d.revision() == revision,
        json!({"values":"pitch=preserve  pitch=tape","next":"next: pitch= keep pitch …"}),
        json!({"command":d.app().command,"values":values,"next":next}),
    )?;
    d.capture("Ramp pitch values")?;

    d.events("Type a value that is not a choice", vec![text("q ")])?;
    d.step("Problem paints", true)?;
    let problem = painted(d, "pitch=q is not one of preserve, tape");
    d.check(
        "An invalid value is named with its parameter before Enter",
        d.app().command == "ramp 1 2 pitch=q "
            && fully_visible(&problem)
            && d.revision() == revision,
        json!({"problem":"pitch=q is not one of preserve, tape"}),
        json!({"command":d.app().command,"problem":problem}),
    )?;
    d.capture("Invalid parameter value named inline")?;

    clear(d)?;
    d.events(
        "Type an unknown parameter",
        vec![text("hold 12f colour=red ")],
    )?;
    d.step("Unknown parameter paints", true)?;
    let unknown = painted(
        d,
        "colour= is not a parameter of :hold; it takes video, audio",
    );
    d.check(
        "An unknown parameter is named with the ones the verb takes",
        fully_visible(&unknown) && d.revision() == revision,
        json!({"problem":"colour= is not a parameter of :hold; it takes video, audio"}),
        json!({"command":d.app().command,"problem":unknown}),
    )?;
    d.capture("Unknown parameter named inline")?;

    // Tab completion: verb, parameter name, then enum values.
    clear(d)?;
    d.events("Type rep", vec![text("rep")])?;
    d.step("Matching verbs paint", true)?;
    let marked = painted(d, "▸:repeat");
    d.check(
        "While a verb is typed, the verb Tab inserts is marked first",
        fully_visible(&marked),
        json!("▸:repeat …"),
        json!({"command":d.app().command,"marked":marked}),
    )?;
    d.key(Key::Tab)?;
    d.step("Tab completes the verb", true)?;
    d.check(
        ":rep then Tab completes the verb and a space",
        d.app().command == "repeat " && d.app().command_open,
        json!("repeat "),
        json!(d.app().command),
    )?;
    d.events("Type 3 g", vec![text("3 g")])?;
    d.key(Key::Tab)?;
    d.step("Tab completes the parameter name", true)?;
    d.check(
        ":repeat 3 g then Tab inserts gap=",
        d.app().command == "repeat 3 gap=",
        json!("repeat 3 gap="),
        json!(d.app().command),
    )?;
    d.events("Type a gap", vec![text("6f")])?;
    d.check(
        "Typing after a completion edits the field normally",
        d.app().command == "repeat 3 gap=6f",
        json!("repeat 3 gap=6f"),
        json!(d.app().command),
    )?;

    clear(d)?;
    d.events("Type :ramp 1 2 pitch=", vec![text("ramp 1 2 pitch=")])?;
    d.key(Key::Tab)?;
    d.step("Tab inserts the first value", true)?;
    let first = d.app().command.clone();
    d.key(Key::Tab)?;
    d.step("Tab cycles to the next value", true)?;
    let second = d.app().command.clone();
    let cycled = painted(d, "pitch=preserve  ▸pitch=tape");
    d.check(
        "Repeated Tab cycles enum values and marks the current one",
        first == "ramp 1 2 pitch=preserve "
            && second == "ramp 1 2 pitch=tape "
            && fully_visible(&cycled),
        json!({"first":"ramp 1 2 pitch=preserve ","second":"ramp 1 2 pitch=tape ","row":"pitch=preserve  ▸pitch=tape"}),
        json!({"first":first,"second":second,"row":cycled}),
    )?;
    d.capture("Tab cycling enum values")?;
    d.key_modified(Key::Tab, Modifiers::SHIFT)?;
    d.step("Shift+Tab goes back", true)?;
    let back = d.app().command.clone();
    d.key(Key::ArrowDown)?;
    d.step("Down moves forward", true)?;
    let down = d.app().command.clone();
    d.key(Key::ArrowUp)?;
    d.step("Up moves back", true)?;
    let up = d.app().command.clone();
    d.check(
        "Shift+Tab and Up go back, Down goes forward, all in the open field",
        back == "ramp 1 2 pitch=preserve "
            && down == "ramp 1 2 pitch=tape "
            && up == "ramp 1 2 pitch=preserve "
            && d.app().command_open
            && d.revision() == revision,
        json!({"back":"…preserve ","down":"…tape ","up":"…preserve "}),
        json!({"back":back,"down":down,"up":up,"open":d.app().command_open}),
    )?;

    clear(d)?;
    d.events(
        "Type a role repeat with a gap",
        vec![text("repeat 3 gap=6f role=audio ")],
    )?;
    d.step("Conflict paints", true)?;
    let conflict = painted(
        d,
        "role= cannot be combined with gap=: a role repeat adds no gaps or per-play changes",
    );
    d.check(
        "Mixing role= with escalation parameters is named before Enter",
        fully_visible(&conflict) && d.revision() == revision,
        json!("role= cannot be combined with gap=: …"),
        json!({"command":d.app().command,"problem":conflict}),
    )?;
    d.capture("Parameter conflict named inline")?;

    d.events(
        "Escape closes the command line",
        vec![egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    )?;
    d.wait_for("Command entry closes", |app| !app.command_open)?;
    d.check(
        "Typing and cancelling arguments edits nothing",
        d.revision() == revision,
        json!(revision),
        json!(d.revision()),
    )?;
    let pane = d.app().pane;
    d.key(Key::Tab)?;
    d.step("Tab outside the command line", true)?;
    d.check(
        "Outside the command line Tab still moves to the next pane",
        d.app().pane != pane && !d.app().command_open,
        json!(format!("not {pane:?}")),
        json!(format!("{:?}", d.app().pane)),
    )?;
    Ok(())
}
