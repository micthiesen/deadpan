//! Place slice records the semantic operation it actually shows.

use super::*;
use crate::preview::copied::Content;
use crate::project::splice::Source;
use deadpan_core::{RegisterName, SemanticInstruction as I};

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 640.0));
    let input = d.harness.input_mut();
    input.screen_rect = Some(viewport);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing Place slice recording viewport")?
        .inner_rect = Some(viewport);
    d.step("Record Place slice at the minimum workspace size", true)?;
    d.command("sequence")?;
    d.key(Key::Escape)?;
    let baseline = document(d)?.clone();
    d.command("source")?;
    d.key(Key::Escape)?;
    d.chord(&[
        Key::G,
        Key::G,
        Key::Num1,
        Key::Num0,
        Key::L,
        Key::V,
        Key::Num1,
        Key::Num4,
        Key::L,
    ])?;
    d.command("register w")?;
    d.key(Key::Y)?;
    d.wait_for("Save the exact Original range in named register w", |app| {
        !app.service.is_busy()
            && !app.copied.is_pending()
            && app.copied.entries().any(|(name, value)| {
                name == 'w' && matches!(value, Content::Original(copy) if copy.ordinals == (10..24))
            })
    })?;
    d.command("sequence")?;
    insertion(d, &baseline)?;
    replacement(d, &baseline)?;
    edited(d, &baseline)?;
    d.check(
        "Place slice recording restores the complete fixture while retaining saved programs",
        d.app().sequence_length() == 120
            && program(d, 'a').is_some()
            && program(d, 'b').is_some()
            && program(d, 'c').is_some(),
        json!({"frames":120,"macros":["a","b","c"]}),
        d.snapshot(),
    )
}

fn insertion(d: &mut Driver<'_>, baseline: &deadpan_core::ProjectDocument) -> Result<(), String> {
    edited::goto(d, 20)?;
    d.command("register w")?;
    let copied_source = source(d, 'w')?;
    let selected = d
        .app()
        .selected_beat
        .as_ref()
        .ok_or("No selected insertion child")?;
    let before_slot = document(d)?
        .children(document(d)?.root())
        .position(|child| child == selected)
        .ok_or("Selected insertion child is absent from its parent")?;
    let revision = d.revision();
    d.chord(&[Key::Q, Key::A])?;
    d.command("splice")?;
    wait_ready(d)?;
    d.key(Key::Enter)?;
    refused(
        d,
        &revision,
        "An interior preview cannot masquerade as a semantic seam paste",
    )?;
    d.key(Key::K)?;
    wait_ready(d)?;
    d.chord(&[Key::I, Key::L])?;
    wait_ready(d)?;
    d.key(Key::Enter)?;
    refused(
        d,
        &revision,
        "Refining the copied In refuses recording without changing the register",
    )?;
    d.check(
        "A refused refined placement retains the immutable named source",
        source(d, 'w')? == copied_source,
        json!("named register w unchanged"),
        state(d),
    )?;
    d.capture("A refined Place slice shows its explicit recording restriction")?;
    d.key(Key::H)?;
    wait_ready(d)?;
    d.key(Key::D)?;
    wait_picture(d, 10, "Showing proposed edit frame 1")?;
    let preview = prepared(d)?;
    d.check(
        "Restoring the register endpoints recovers the exact captured before slot",
        draft(d)?.proposal_for_check().destination == Destination::Slot(before_slot)
            && draft(d)?.proposal_for_check().source == copied_source,
        json!({"slot":before_slot,"source":"w","range":[10,24]}),
        state(d),
    )?;
    paint_text(
        d,
        "Apply records this register placement as one semantic action and one Undo.",
    )?;
    d.capture("Eligible Place slice explains its recorded semantic Apply")?;
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Recorded Place slice commits once through semantic Apply with fresh identities",
        d.app().sequence_length() == 134
            && d.app().sequence_cursor == 0
            && d.app().presentation.displayed_source_frame() == Some(SourceFrameId(10))
            && d.app().selected_beat.as_ref() != Some(&preview.node)
            && d.app().macros.recording_name() == Some('a')
            && d.app().macros.instruction_count() == 1
            && source(d, 'w')? == copied_source,
        json!({"frames":134,"cursor":0,"Original_picture":10,"instructions":1}),
        d.snapshot(),
    )?;
    d.capture("Place slice recorded as one named semantic paste")?;
    save(d, 'a')?;
    let expected = vec![I::Paste {
        register: RegisterName::new('w').unwrap(),
        before: true,
    }];
    d.check(
        "The saved program retains the named before-paste intent",
        program(d, 'a') == Some(expected.clone()),
        json!(expected),
        json!(program(d, 'a')),
    )?;
    edited::undo(d, baseline)?;
    edited::goto(d, 60)?;
    let revision = d.revision();
    d.key(Key::S)?;
    d.changed(&revision)?;
    let split = document(d)?.clone();
    edited::goto(d, 75)?;
    let revision = d.revision();
    super::super::macros::execute(d, 'a', None)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Replaying the recorded paste resolves the new selected child rather than its old preview slot",
        d.app().sequence_length() == 134 && d.app().sequence_cursor == 60
            && d.app().presentation.displayed_source_frame() == Some(SourceFrameId(10))
            && source(d, 'w')? == copied_source,
        json!({"frames":134,"cursor":60,"Original_picture":10}),
        d.snapshot(),
    )?;
    edited::undo(d, &split)?;
    edited::undo(d, baseline)
}

fn replacement(d: &mut Driver<'_>, baseline: &deadpan_core::ProjectDocument) -> Result<(), String> {
    edited::select(d, 30, 40)?;
    d.key(Key::V)?;
    d.command("register w")?;
    let copied_source = source(d, 'w')?;
    d.chord(&[Key::Q, Key::B])?;
    let revision = d.revision();
    d.command("splice")?;
    wait_ready(d)?;
    d.key(Key::R)?;
    wait_ready(d)?;
    d.check(
        "The recorded replacement previews the exact captured range",
        draft(d)?.proposal_for_check().destination
            == Destination::Replace {
                range: edited::range(30, 40)?,
            },
        json!({"removed":[30,40],"inserted":[30,44]}),
        state(d),
    )?;
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Place slice records one replacement with its real duration change",
        d.app().sequence_length() == 124
            && d.app().sequence_cursor == 30
            && d.app().macros.instruction_count() == 1
            && source(d, 'w')? == copied_source,
        json!({"frames":124,"cursor":30,"instructions":1}),
        d.snapshot(),
    )?;
    save(d, 'b')?;
    let expected = vec![I::ReplaceSelection {
        register: RegisterName::new('w').unwrap(),
    }];
    d.check(
        "Place slice stores replacement intent without its old interval",
        program(d, 'b') == Some(expected.clone()),
        json!(expected),
        json!(program(d, 'b')),
    )?;
    edited::undo(d, baseline)?;
    edited::goto(d, 50)?;
    d.key(Key::V)?;
    edited::motion(d, 20, false)?;
    d.key(Key::V)?;
    let revision = d.revision();
    super::super::macros::execute(d, 'b', None)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Replacement replay uses the new backward twenty-frame selection",
        d.app().sequence_length() == 114
            && d.app().sequence_cursor == 30
            && d.app().presentation.displayed_source_frame() == Some(SourceFrameId(10))
            && source(d, 'w')? == copied_source,
        json!({"frames":114,"removed":[30,50],"inserted":[30,44]}),
        d.snapshot(),
    )?;
    d.capture("Recorded Place slice replaces the new invocation range")?;
    edited::undo(d, baseline)
}

fn edited(d: &mut Driver<'_>, baseline: &deadpan_core::ProjectDocument) -> Result<(), String> {
    edited::select(d, 0, 12)?;
    d.command("register v")?;
    d.key(Key::Y)?;
    d.wait_for("Save the exact linked Edit range in register v", |app| {
        !app.service.is_busy() && !app.copied.is_pending()
            && app.copied.entries().any(|(name, value)| name == 'v'
                && matches!(value, Content::Edited(copy) if copy.slice().range().start() == ProjectFrame(0)
                    && copy.slice().range().end() == ProjectFrame(12)))
    })?;
    edited::goto(d, 40)?;
    d.command("register v")?;
    let copied_source = source(d, 'v')?;
    let revision = d.revision();
    d.chord(&[Key::Q, Key::C])?;
    d.command("splice")?;
    wait_ready(d)?;
    d.chord(&[Key::D, Key::J, Key::M])?;
    wait_ready(d)?;
    d.check(
        "The Move refusal fixture has a valid actual Move proposal",
        draft(d)?.proposal_for_check().operation == crate::project::splice::Operation::Move,
        json!("Move"),
        state(d),
    )?;
    d.key(Key::Enter)?;
    refused(
        d,
        &revision,
        "Move remains explicitly outside the recorded paste vocabulary",
    )?;
    d.capture("Move placement explains its recording restriction")?;
    d.key(Key::M)?;
    wait_ready(d)?;
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Returning to Copy records the unchanged owned Edited source",
        d.app().sequence_length() == 132
            && d.app().sequence_cursor == 120
            && d.app().macros.instruction_count() == 1
            && source(d, 'v')? == copied_source,
        json!({"frames":132,"cursor":120,"instructions":1,"register":"v"}),
        d.snapshot(),
    )?;
    save(d, 'c')?;
    let expected = vec![I::Paste {
        register: RegisterName::new('v').unwrap(),
        before: false,
    }];
    d.check(
        "Edited placement saves the exact named after-paste intent",
        program(d, 'c') == Some(expected.clone()),
        json!(expected),
        json!(program(d, 'c')),
    )?;
    edited::undo(d, baseline)
}

fn source(d: &Driver<'_>, register: char) -> Result<Source, String> {
    d.app()
        .copied
        .entries()
        .find(|(name, _)| *name == register)
        .ok_or_else(|| format!("Register {register} missing"))?
        .1
        .source()
}

fn program(d: &Driver<'_>, name: char) -> Option<Vec<I>> {
    d.app()
        .copied
        .entries()
        .find_map(|(slot, value)| match value {
            Content::Macro(program) if slot == name => Some(program.instructions().to_vec()),
            _ => None,
        })
}

fn idle(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for(
        "The recorded placement and proposal abandonment complete",
        |app| {
            !app.service.is_busy()
                && !app.macros.is_pending()
                && app.splice.is_none()
                && app.splice_abandon.is_none()
        },
    )?;
    d.settled()
}

fn save(d: &mut Driver<'_>, name: char) -> Result<(), String> {
    d.key(Key::Q)?;
    d.wait_for("The recorded placement program is saved", |app| {
        !app.service.is_busy()
            && !app.macros.recording()
            && !app.macros.is_pending()
            && app
                .copied
                .entries()
                .any(|(slot, value)| slot == name && matches!(value, Content::Macro(_)))
    })?;
    d.settled()
}

fn refused(d: &mut Driver<'_>, revision: &str, name: &str) -> Result<(), String> {
    d.step("Settle the explicit recording refusal", false)?;
    let error = draft(d)?
        .error_for_check()
        .ok_or("Missing recording refusal")?
        .to_owned();
    d.check(
        name,
        d.revision() == revision
            && d.app().macros.recording()
            && d.app().macros.instruction_count() == 0
            && !error.is_empty(),
        json!({"history":"unchanged","instructions":0,"draft":"retained"}),
        json!({"error":error,"state":state(d)}),
    )?;
    paint_text(d, &error)
}
