//! Typed command arguments through the production command line: real text
//! input shows the next parameter with its type, unit and default, offers
//! choice values, names an invalid value before Enter and says what the
//! command acts on, from the verb's declarative schema.

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
    d.report.skipped.push("Typing is replayed as egui Text events through the production command line; physical key delivery, IME composition in a native window and VoiceOver speech are not exercised here.".into());
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
    Ok(())
}
