//! Beat object ownership through the real key router, service and register bank.

use super::*;
use deadpan_core::{
    SemanticObjectSelection, SemanticTextObject, SemanticVisualSelection, SliceAttachments,
};

pub(super) fn run(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 20)?;
    let before = d.revision();
    d.key(Key::S)?;
    d.changed(&before)?;
    let split_once = document(d)?.clone();
    at(d, 40)?;
    let before = d.revision();
    d.key(Key::S)?;
    d.changed(&before)?;
    let split_twice = document(d)?.clone();
    at(d, 20)?;
    let before = d.revision();
    d.command("caption \"Beat object\" delay=1f")?;
    d.changed(&before)?;
    let captioned = document(d)?.clone();
    let before = d.revision();
    d.chord(&[Key::M, Key::B])?;
    d.changed(&before)?;
    let attached = document(d)?.clone();
    let beat = d
        .app()
        .selected_beat
        .clone()
        .ok_or("Beat object fixture has no selected beat")?;
    let tail = attached
        .children(attached.root())
        .last()
        .cloned()
        .ok_or("Missing final beat")?;
    for (prefix, attachments) in [
        (Key::I, SliceAttachments::Excluded),
        (Key::A, SliceAttachments::Owned),
    ] {
        let before_bank = bank(d)?;
        d.chord(&[Key::Y, prefix, Key::B])?;
        idle(d)?;
        let saved = bank(d)?;
        let value = saved
            .entries
            .get(&RegisterName::unnamed())
            .ok_or("Missing beat object copy")?;
        let RegisterValue::Edited { slice } = value.as_ref() else {
            return Err("Beat object copy is not editable".into());
        };
        d.check(
            "yib and yab keep identical picture bounds and distinct owned attachments",
            slice.attachments() == attachments && slice.range() == range(20, 40)
                && saved.version == before_bank.version + 1 && document(d)? == &attached
                && d.app().sequence_cursor == 20 && d.app().selected_beat.as_ref() == Some(&beat),
            json!({"range":[20,40],"attachments":attachments,"cursor":20}),
            json!({"range":slice.range(),"attachments":slice.attachments(),"cursor":d.app().sequence_cursor}),
        )?;
    }
    d.chord(&[Key::V, Key::I, Key::B, Key::V])?;
    d.settled()?;
    let expected = SemanticVisualSelection::Object {
        selection: SemanticObjectSelection {
            kind: SemanticTextObject::InnerBeat,
            group: beat.clone(),
        },
        extending: false,
    };
    d.check(
        "vib retains exact beat identity and names its attachment choice",
        d.app().capture_visual_selection()? == Some(expected.clone())
            && d.app()
                .edit_range_label()
                .is_some_and(|label| label.starts_with("Beat without attachments")),
        json!(expected),
        json!(d.app().capture_visual_selection()?),
    )?;
    d.key(Key::Escape)?;
    at(d, 20)?;
    let before = d.revision();
    d.chord(&[Key::Num3, Key::R, Key::I, Key::B])?;
    d.changed(&before)?;
    idle(d)?;
    let repeated = document(d)?.clone();
    let wrapper = d
        .app()
        .selected_beat
        .clone()
        .ok_or("rib has no Repeat selected")?;
    let expected_dot = RepeatableEdit::Repeat {
        selector: SemanticSelector::TextObject {
            object: SemanticTextObject::InnerBeat,
        },
        plays: NonZeroU32::new(3).unwrap(),
        escalation: None,
    };
    d.check("3rib creates three total plays, keeps captions and marks once, and saves an unresolved beat selector for dot",
        repeated.duration().map_err(|e| e.to_string())?.frames() == 160
            && repeated.overrides().get(&wrapper).is_some_and(|entries| entries.len() == 1)
            && repeated.nodes().values().filter(|node| !node.captions.is_empty()).count() == 1
            && repeated.marks().len() == attached.marks().len()
            && d.app().semantic.snapshot().is_some_and(|snapshot| snapshot.edit.as_ref().is_some_and(|edit| edit.operation == expected_dot)),
        json!({"frames":160,"caption_hosts":1,"marks":attached.marks().len(),"selector":"inner_beat"}), state(d))?;
    at(d, 80)?;
    d.check(
        "Dot retarget fixture selects the independent final beat",
        d.app().selected_beat.as_ref() == Some(&tail),
        json!(tail),
        state(d),
    )?;
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Dot resolves rib against the new current beat and keeps the first Repeat intact",
        d.app().sequence_length() == 320
            && document(d)?.nodes().get(&wrapper) == repeated.nodes().get(&wrapper)
            && document(d)?.marks() == repeated.marks(),
        json!({"frames":320,"original_repeat_retained":true}),
        state(d),
    )?;
    undo(d, &repeated)?;
    undo(d, &attached)?;
    at(d, 20)?;
    let restored = document(d)?.clone();
    recorded(d, &restored, &tail)?;
    undo(d, &captioned)?;
    undo(d, &split_twice)?;
    undo(d, &split_once)?;
    undo(d, baseline)
}

fn recorded(d: &mut Driver<'_>, attached: &ProjectDocument, tail: &NodeId) -> Result<(), String> {
    d.chord(&[Key::Q, Key::M])?;
    d.chord(&[Key::Y, Key::I, Key::B])?;
    idle(d)?;
    d.chord(&[Key::V, Key::A, Key::B, Key::V])?;
    d.key(Key::Q)?;
    idle(d)?;
    let expected = [
        SemanticInstruction::Yank {
            selector: SemanticSelector::TextObject {
                object: SemanticTextObject::InnerBeat,
            },
            register: RegisterName::unnamed(),
        },
        SemanticInstruction::BeginSelection,
        SemanticInstruction::SelectObject {
            object: SemanticTextObject::AroundBeat,
        },
        SemanticInstruction::FinishSelection,
    ];
    let saved = bank(d)?;
    d.check("Macro records ib and ab ownership without old beat identities", matches!(saved.entries.get(&RegisterName::new('m').map_err(|e| e.to_string())?).map(AsRef::as_ref), Some(RegisterValue::Macro { program }) if program.instructions() == expected), json!(expected), state(d))?;
    d.key(Key::Escape)?;
    at(d, 40)?;
    d.events(
        "Replay beat objects on a different beat",
        vec![
            key_event(Key::Num2, Modifiers::SHIFT, true),
            Event::Text("@".into()),
            key_event(Key::Num2, Modifiers::SHIFT, false),
            key_event(Key::M, Modifiers::NONE, true),
            key_event(Key::M, Modifiers::NONE, false),
        ],
    )?;
    idle(d)?;
    let selected = SemanticVisualSelection::Object {
        selection: SemanticObjectSelection {
            kind: SemanticTextObject::AroundBeat,
            group: tail.clone(),
        },
        extending: false,
    };
    d.check(
        "Beat object macro resolves the new beat and preserves authored history",
        document(d)? == attached
            && d.app().capture_visual_selection()? == Some(selected.clone())
            && d.app().sequence_cursor == 120,
        json!(selected),
        state(d),
    )?;
    d.key(Key::Escape)
}

fn range(start: i64, end: i64) -> deadpan_core::FrameRange {
    deadpan_core::FrameRange::new(
        deadpan_core::ProjectFrame(start),
        deadpan_core::ProjectFrame(end),
    )
    .unwrap()
}
