//! Named grouping through production input, semantic receipts and SQLite.

use super::*;
use crate::preview::copied::Content;
use crate::project::semantic::RepeatableEdit;
use deadpan_core::{
    ProjectDocument, RegisterName, RegisterValue, SemanticInstruction, SemanticSelector,
};
use deadpan_store::{AccessMode, ProjectStore, registers::RegisterBank};
use egui::{Event, ImeEvent, Key, Modifiers};
use std::num::NonZeroU32;

mod beat_objects;
mod objects;

const NAME: &str = "l'été 答え \"oui\"";

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    idle(d)?;
    #[cfg(target_os = "macos")]
    {
        let supported = d.harness.ctx.fonts_mut(|fonts| {
            (
                fonts.has_glyphs(&egui::FontId::proportional(13.0), "答え"),
                fonts.has_glyphs(&egui::FontId::monospace(13.0), "答え"),
            )
        });
        let report = d.harness.ctx.data(|data| {
            data.get_temp::<super::super::fonts::Report>(egui::Id::new(
                super::super::fonts::REPORT_KEY,
            ))
        });
        d.check(
            "Both editor font families contain the exact Japanese group-name glyphs",
            supported == (true, true),
            json!([true, true]),
            json!({"supported": supported, "fonts": report}),
        )?;
    }
    let baseline = document(d)?.clone();
    d.check(
        "Group replay starts with the protected 120-frame Original",
        d.app().sequence_length() == 120 && !d.app().workspace.as_ref().unwrap().can_undo,
        json!({"frames":120,"undo":false}),
        state(d),
    )?;
    partial_and_text(d, &baseline)?;
    composite_ranges(d, &baseline)?;
    fresh_dot(d, &baseline)?;
    recorded(d, &baseline)?;
    refusals(d, &baseline)?;
    empty_child(d, &baseline)?;
    objects::run(d, &baseline)?;
    beat_objects::run(d, &baseline)?;
    d.report.skipped.push("Keyboard and IME events are synthetic production event batches. Physical delivery and OS composition remain separate qualification. Composed pixel comparisons run only in visual mode; canonical PCM equivalence is covered by the separate core/audio tests.".into());
    Ok(())
}

fn partial_and_text(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    let pixels = pictures(d)?;
    d.check(
        "Pixel checkpoints refer to three distinct Original frames",
        pixels.iter().map(|(source, _)| *source).collect::<Vec<_>>() == [20, 23, 27],
        json!([20, 23, 27]),
        json!(pixels),
    )?;
    if d.options.mode == RunMode::Visual
        && pixels
            .iter()
            .filter_map(|(_, hash)| hash.as_deref())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            < 2
    {
        d.report.skipped.push("The decoded fixture has identical composed hashes at Edit 20, 23 and 27. Exact displayed Original frame identities still distinguish the checkpoints; pixel equality alone does not witness temporal variation.".into());
    }
    at(d, 20)?;
    visual(d, 7, false)?;
    choose_a(d)?;
    let original_bank = bank(d)?;
    let before = d.revision();
    let input = vec![
        key_event(Key::Comma, Modifiers::NONE, true),
        Event::Text(",".into()),
        key_event(Key::Comma, Modifiers::NONE, false),
        key_event(Key::G, Modifiers::NONE, true),
        Event::Text("g".into()),
        key_event(Key::G, Modifiers::NONE, false),
        Event::Text("\"l'été ".into()),
    ];
    d.events(
        "One batch opens ,g, consumes only its companion g and types quoted Unicode",
        input,
    )?;
    d.check(
        "The Group opener preserves its same-batch native text suffix",
        d.app().command_open && d.app().command == "group name=\"l'été " && d.revision() == before,
        json!("group name=\"l'été "),
        state(d),
    )?;
    // Native composition starts after the field receives focus. A batch with
    // any IME event belongs to composition and intentionally blocks shortcuts.
    let mut input = vec![Event::Ime(ImeEvent::Preedit {
        text: "答え".into(),
        active_range_chars: Some(0..2),
    })];
    input.extend(keys(&[Key::Enter]));
    d.events("Group name composition owns Enter", input)?;
    d.check(
        "Preedit keeps the captured Group field and authored revision",
        d.app().command_open && d.app().ime_composing && d.revision() == before,
        json!("composition owns Enter"),
        state(d),
    )?;
    let mut input = vec![
        Event::Ime(ImeEvent::Commit("答え".into())),
        Event::Text(" \\\"oui\\\"\"".into()),
    ];
    input.extend(keys(&[Key::Enter]));
    d.events(
        "Group name commit preserves escaped text and owns its same-batch Enter",
        input,
    )?;
    let expected_command = format!(
        "group name={}",
        serde_json::to_string(NAME).map_err(|error| error.to_string())?
    );
    d.check(
        "IME completion keeps the Group field open with exact JSON text and no authored edit",
        d.app().command_open
            && !d.app().ime_composing
            && d.app().command == expected_command
            && d.revision() == before,
        json!({"command":expected_command,"revision":before}),
        state(d),
    )?;
    painted(d, "Name the selection")?;
    d.key(Key::Enter)?;
    d.changed(&before)?;
    idle(d)?;
    let group = grouped(d, NAME, 20, 7)?;
    let grouped_document = document(d)?.clone();
    candidate_group(d, SemanticSelector::VisualSelection, NAME)?;
    preserved(d, &original_bank)?;
    compare_pictures(
        d,
        &pixels,
        "Grouping a partial Source preserves composed pixels across both seams",
    )?;
    at(d, 20)?;
    for (width, height) in [(1280.0, 820.0), (960.0, 640.0)] {
        resize(d, width, height)?;
        painted(d, NAME)?;
        if scenarios::text_paint_visibility(d, "name group").is_empty() {
            painted(d, "all editor keys")?;
        } else {
            painted(d, "name group")?;
            painted(d, ",g")?;
        }
        scenarios::footer_anchored(d, "Group controls stay above notices")?;
        d.key(Key::Enter)?;
        d.settled()?;
        d.check(
            "Enter opens the selected ordinary group without changing the document",
            d.app().sequence_scope.groups() == [group.clone()]
                && d.app().scope_start == 20
                && d.app().scope_end == 27
                && document(d)? == &grouped_document,
            json!({"group":group,"range":[20,27]}),
            state(d),
        )?;
        painted(d, NAME)?;
        if scenarios::text_paint_visibility(d, "parent").is_empty() {
            painted(d, "all editor keys")?;
        } else {
            painted(d, "parent")?;
        }
        d.capture("Named group and its breadcrumb at the current viewport")?;
        d.key(Key::Backspace)?;
        d.settled()?;
        d.check(
            "Backspace returns to the same selected group",
            d.app().sequence_scope.groups().is_empty()
                && d.app().selected_beat.as_ref() == Some(&group),
            json!(group),
            state(d),
        )?;
        help(d)?;
        // Select a again for the next independent structural-edit witness.
        choose_a(d)?;
    }
    resize(d, 1280.0, 820.0)?;
    undo(d, baseline)?;
    d.check(
        "One Undo removes the group and both endpoint splits",
        !d.app().workspace.as_ref().unwrap().can_undo,
        json!("baseline"),
        state(d),
    )?;
    redo(d, &grouped_document)?;
    at(d, 20)?;
    let before = d.revision();
    d.command("ungroup")?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Ungroup promotes the partial child and preserves total duration",
        !document(d)?.nodes().contains_key(&group)
            && d.app().sequence_length() == 120
            && !d.app().edit_range.has_bounds(),
        json!({"removed":group,"frames":120}),
        state(d),
    )?;
    preserved(d, &original_bank)?;
    compare_pictures(
        d,
        &pixels,
        "Ungroup preserves composed pictures at the same Edit coordinates",
    )?;
    undo(d, &grouped_document)?;
    undo(d, baseline)
}

fn composite_ranges(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    for setup in ["repeat 2", "retime 0.5 pitch=preserve"] {
        at(d, 0)?;
        let before = d.revision();
        d.command(setup)?;
        d.changed(&before)?;
        idle(d)?;
        let composite = document(d)?.clone();
        let length = d.app().sequence_length();
        let pixels = pictures(d)?;
        at(d, 20)?;
        visual(d, 7, true)?;
        let before = d.revision();
        d.command("group name=\"composite excerpt\"")?;
        d.changed(&before)?;
        grouped(d, "composite excerpt", 20, 7)?;
        d.check(
            "Grouping a partial Repeat or Retime retains its full duration",
            d.app().sequence_length() == length,
            json!({"setup":setup,"frames":length}),
            state(d),
        )?;
        compare_pictures(
            d,
            &pixels,
            "Composite endpoint grouping preserves actual composed pictures",
        )?;
        undo(d, &composite)?;
        undo(d, baseline)?;
    }
    Ok(())
}

fn fresh_dot(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 0)?;
    choose_a(d)?;
    let saved_bank = bank(d)?;
    let before = d.revision();
    d.command("group name=\"outer\"")?;
    d.changed(&before)?;
    let first = grouped(d, "outer", 0, 120)?;
    let once = document(d)?.clone();
    visual(d, 1, false)?;
    let selection = d.app().edit_range.clone();
    d.command("ungroup")?;
    unchanged(
        d,
        &once,
        &saved_bank,
        "Ungroup refuses a nonempty Visual range even with a valid Sequence selected",
    )?;
    d.check(
        "Visual Ungroup refusal preserves selection and register intent",
        d.app().edit_range == selection && d.app().error.is_some(),
        json!("retained selection"),
        state(d),
    )?;
    preserved(d, &saved_bank)?;
    d.key(Key::Escape)?;
    choose_a(d)?;
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    let second = grouped(d, "outer", 0, 120)?;
    let twice = document(d)?.clone();
    d.check(
        "Group dot resolves the newly selected group and retains the label",
        document(d)?.children(&second).cloned().collect::<Vec<_>>() == [first.clone()],
        json!({"outer":second,"child":first}),
        state(d),
    )?;
    preserved(d, &saved_bank)?;
    let before = d.revision();
    d.command("ungroup")?;
    d.changed(&before)?;
    idle(d)?;
    let promoted = document(d)?.clone();
    d.check(
        "Ungroup selects the promoted inner group",
        d.app().selected_beat.as_ref() == Some(&first),
        json!(first),
        state(d),
    )?;
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Ungroup dot resolves that fresh explicit Sequence instead of the removed wrapper",
        !document(d)?.nodes().contains_key(&first)
            && !document(d)?.nodes().contains_key(&second)
            && d.app().sequence_length() == 120,
        json!("both wrappers removed"),
        state(d),
    )?;
    preserved(d, &saved_bank)?;
    undo(d, &promoted)?;
    undo(d, &twice)?;
    undo(d, &once)?;
    undo(d, baseline)?;

    at(d, 0)?;
    let before = d.revision();
    d.command("group name=\"fresh range\"")?;
    d.changed(&before)?;
    undo(d, baseline)?;
    at(d, 50)?;
    visual(d, 4, true)?;
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    grouped(d, "fresh range", 50, 4)?;
    candidate_group(d, SemanticSelector::VisualSelection, "fresh range")?;
    preserved(d, &saved_bank)?;
    undo(d, baseline)?;
    at(d, 60)?;
    let before = document(d)?.clone();
    d.key(Key::Period)?;
    unchanged(
        d,
        &before,
        &saved_bank,
        "A saved Visual Group refuses dot without a fresh range",
    )?;
    d.check(
        "Missing Visual Group preserves its candidate and reports refusal",
        d.app().error.is_some(),
        json!("missing Visual selection"),
        state(d),
    )?;
    preserved(d, &saved_bank)
}

fn recorded(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 0)?;
    choose_a(d)?;
    let before_bank = bank(d)?;
    d.chord(&[Key::Q, Key::Z])?;
    let before = d.revision();
    d.command("group name=\"recorded group\"")?;
    d.changed(&before)?;
    idle(d)?;
    let grouped_document = document(d)?.clone();
    d.key(Key::L)?;
    let before = d.revision();
    d.command("ungroup")?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Recording appends Group, relative motion and Ungroup only after successful actions",
        d.app().macros.recording_name() == Some('z') && d.app().macros.instruction_count() == 3,
        json!({"recording":"z","instructions":3}),
        state(d),
    )?;
    preserved(d, &before_bank)?;
    d.key(Key::Q)?;
    d.wait_for("Group macro is durably saved", |app| {
        !app.service.is_busy()
            && !app.macros.recording()
            && !app.macros.is_pending()
            && app
                .copied
                .entries()
                .any(|(name, value)| name == 'z' && matches!(value, Content::Macro(_)))
    })?;
    idle(d)?;
    let saved_bank = bank(d)?;
    let expected = [
        SemanticInstruction::Group {
            selector: SemanticSelector::SelectedBeat,
            label: "recorded group".into(),
        },
        SemanticInstruction::MoveFrames {
            forward: true,
            count: NonZeroU32::new(1).unwrap(),
        },
        SemanticInstruction::Ungroup,
    ];
    d.check("The named Macro stores structural intent and keeps the unnamed content and register override",
        matches!(saved_bank.entries.get(&RegisterName::new('z').map_err(|error| error.to_string())?).map(AsRef::as_ref), Some(RegisterValue::Macro { program }) if program.instructions() == expected)
            && saved_bank.entries.get(&RegisterName::unnamed()) == before_bank.entries.get(&RegisterName::unnamed())
            && d.app().copied.selected_override() == Some(Some('a')),
        json!(expected), state(d))?;
    undo(d, &grouped_document)?;
    undo(d, baseline)?;
    at(d, 0)?;
    let before_run = document(d)?.clone();
    let before = d.revision();
    let mut input = keys(&[Key::Num2]);
    input.extend([
        key_event(Key::Num2, Modifiers::SHIFT, true),
        Event::Text("@".into()),
        key_event(Key::Num2, Modifiers::SHIFT, false),
    ]);
    input.extend(keys(&[Key::Z]));
    d.events(
        "Run Group/motion/Ungroup twice through paired logical @ input",
        input,
    )?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Counted macro replay returns to the original explicit child without accumulating groups",
        document(d)?.nodes() == before_run.nodes()
            && d.app().sequence_length() == 120
            && d.app().sequence_cursor == 0
            && d.app().selected_beat.as_ref() == before_run.children(before_run.root()).next(),
        json!({"frames":120,"Edit":0,"wrappers":0}),
        state(d),
    )?;
    preserved(d, &saved_bank)?;
    undo(d, &before_run)?;
    d.check(
        "One Undo reverses the entire counted macro and keeps its durable program",
        !d.app().workspace.as_ref().unwrap().can_undo && bank(d)? == saved_bank,
        json!("one history transaction, saved macro retained"),
        state(d),
    )
}

fn refusals(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 0)?;
    choose_a(d)?;
    let saved_bank = bank(d)?;
    for text in [
        "group name=unquoted".to_owned(),
        "group name=\"one\" name=\"two\"".to_owned(),
        format!("group name=\"{}\"", "é".repeat(513)),
    ] {
        let before = document(d)?.clone();
        d.command(&text)?;
        unchanged(
            d,
            &before,
            &saved_bank,
            "Malformed, duplicate and oversized group names make no write",
        )?;
        d.check(
            "Invalid names report a visible error",
            d.app().error.is_some(),
            json!("error"),
            state(d),
        )?;
        preserved(d, &saved_bank)?;
    }
    for finished in [false, true] {
        at(d, 20)?;
        visual(d, 0, finished)?;
        choose_a(d)?;
        let selection = d.app().edit_range.clone();
        for text in ["group name=\"empty\"", "ungroup"] {
            let before = document(d)?.clone();
            d.command(text)?;
            unchanged(
                d,
                &before,
                &saved_bank,
                "An explicit empty Visual range refuses Group and Ungroup",
            )?;
            d.check(
                "Refusal preserves the complete empty selection and pending register",
                d.app().edit_range == selection
                    && d.app().copied.selected_override() == Some(Some('a'))
                    && d.app().error.is_some(),
                json!({"finished":finished,"command":text,"selection":"unchanged"}),
                state(d),
            )?;
        }
    }
    at(d, 0)?;
    choose_a(d)?;
    let before = d.revision();
    d.command("group name=\"stale\"")?;
    d.changed(&before)?;
    idle(d)?;
    d.chord(&[Key::Comma, Key::G])?;
    let before = d.revision();
    d.app().service.submit(ProjectRequest::Undo {
        expected_revision: document(d)?.revision_id().clone(),
    })?;
    d.changed(&before)?;
    let restored = document(d)?.clone();
    submit_text(d, "\"must not retarget\"")?;
    unchanged(
        d,
        &restored,
        &saved_bank,
        "Group entry cannot retarget after a real Undo changes its revision",
    )?;
    d.check(
        "Stale Group entry reports the captured context change",
        same_document(document(d)?, baseline)?
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("context changed")),
        json!("captured revision refused"),
        state(d),
    )?;
    preserved(d, &saved_bank)?;

    at(d, 0)?;
    let before = d.revision();
    d.chord(&[Key::D, Key::D])?;
    d.changed(&before)?;
    idle(d)?;
    choose_a(d)?;
    let empty_bank = bank(d)?;
    d.check(
        "Deleting the only child creates genuine target absence",
        d.app().selected_beat.is_none()
            && document(d)?.children(document(d)?.root()).next().is_none(),
        json!("no child"),
        state(d),
    )?;
    d.chord(&[Key::Comma, Key::G])?;
    let before = d.revision();
    d.app().service.submit(ProjectRequest::Undo {
        expected_revision: document(d)?.revision_id().clone(),
    })?;
    d.changed(&before)?;
    let restored = document(d)?.clone();
    submit_text(d, "\"absent at entry\"")?;
    unchanged(
        d,
        &restored,
        &empty_bank,
        "Captured target absence cannot use a beat supplied by a later Undo",
    )?;
    d.check(
        "Absence remains refused after the Original returns",
        same_document(document(d)?, baseline)? && d.app().error.is_some(),
        json!("baseline restored without grouping"),
        state(d),
    )?;
    preserved(d, &empty_bank)
}

fn empty_child(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 0)?;
    let before = d.revision();
    d.command("group name=\"empty child\"")?;
    d.changed(&before)?;
    let empty = grouped(d, "empty child", 0, 120)?;
    let full = document(d)?.clone();
    d.key(Key::Enter)?;
    let before = d.revision();
    d.chord(&[Key::D, Key::D])?;
    d.changed(&before)?;
    idle(d)?;
    d.key(Key::Backspace)?;
    d.settled()?;
    let empty_document = document(d)?.clone();
    d.check(
        "The root explicitly selects the retained zero-duration Sequence",
        d.app().selected_beat.as_ref() == Some(&empty)
            && document(d)?.children(&empty).next().is_none(),
        json!(empty),
        state(d),
    )?;
    choose_a(d)?;
    let saved_bank = bank(d)?;
    let before = d.revision();
    d.command("group name=\"empty wrapper\"")?;
    d.changed(&before)?;
    let wrapper = grouped(d, "empty wrapper", 0, 0)?;
    let wrapped = document(d)?.clone();
    d.check(
        "Grouping an empty child wraps its exact identity without cursor fallback",
        document(d)?.children(&wrapper).cloned().collect::<Vec<_>>() == [empty.clone()],
        json!({"wrapper":wrapper,"child":empty}),
        state(d),
    )?;
    let before = d.revision();
    d.command("ungroup")?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Ungroup selects the promoted empty child and keeps zero duration",
        d.app().selected_beat.as_ref() == Some(&empty) && d.app().sequence_length() == 0,
        json!({"selected":empty,"frames":0}),
        state(d),
    )?;
    preserved(d, &saved_bank)?;
    undo(d, &wrapped)?;
    undo(d, &empty_document)?;
    undo(d, &full)?;
    undo(d, baseline)
}

fn grouped(d: &mut Driver<'_>, label: &str, start: u64, frames: i64) -> Result<NodeId, String> {
    idle(d)?;
    let selected = d
        .app()
        .selected_beat
        .clone()
        .ok_or("Group result has no selected node")?;
    let document = document(d)?;
    let direct = document
        .children(document.root())
        .any(|node| node == &selected);
    let node = document
        .nodes()
        .get(&selected)
        .ok_or("Selected Group is absent")?;
    let valid = direct
        && node.label == label
        && matches!(node.kind, NodeKind::Sequence { .. })
        && document
            .node_duration(&selected)
            .map_err(|error| error.to_string())?
            .frames()
            == frames
        && d.app().sequence_cursor == start
        && !d.app().edit_range.has_bounds();
    d.check(
        "Group selects the exact named Sequence and clears Visual state",
        valid,
        json!({"label":label,"Edit":start,"frames":frames}),
        state(d),
    )?;
    Ok(selected)
}

fn candidate_group(
    d: &mut Driver<'_>,
    selector: SemanticSelector,
    label: &str,
) -> Result<(), String> {
    let expected = RepeatableEdit::Group {
        selector,
        label: label.into(),
    };
    d.check(
        "Dot retains explicit Group selector and name at the committed revision",
        d.app().semantic.snapshot().is_some_and(|snapshot| {
            snapshot.head.as_ref() == Some(document(d).unwrap().revision_id())
                && snapshot
                    .edit
                    .as_ref()
                    .is_some_and(|edit| edit.operation == expected && edit.register.is_none())
        }),
        json!({"selector":selector,"label":label}),
        state(d),
    )
}

fn pictures(d: &mut Driver<'_>) -> Result<Vec<(u64, Option<String>)>, String> {
    let mut pictures = Vec::new();
    for frame in [20, 23, 27] {
        at(d, frame)?;
        let source = d
            .app()
            .presentation
            .displayed_source_frame()
            .ok_or("Group checkpoint has no displayed Original frame")?
            .0;
        pictures.push((source, super::group_pixels::fingerprint(d)?));
    }
    Ok(pictures)
}

fn compare_pictures(
    d: &mut Driver<'_>,
    expected: &[(u64, Option<String>)],
    label: &str,
) -> Result<(), String> {
    let actual = pictures(d)?;
    let visual = d.options.mode == RunMode::Visual;
    d.check(
        label,
        actual == expected && (!visual || actual.iter().all(|(_, hash)| hash.is_some())),
        json!({"Edit":[20,23,27],"hashes":expected,"visual_readback":visual}),
        json!(actual),
    )
}

fn submit_text(d: &mut Driver<'_>, text: &str) -> Result<(), String> {
    let mut events = vec![Event::Text(text.into())];
    events.extend(keys(&[Key::Enter]));
    d.events("Submit the captured Group name", events)
}

fn visual(d: &mut Driver<'_>, count: u64, finished: bool) -> Result<(), String> {
    d.key(Key::V)?;
    if count > 0 {
        motion(d, count)?;
    }
    if finished {
        d.key(Key::V)?;
    }
    d.settled()
}

fn choose_a(d: &mut Driver<'_>) -> Result<(), String> {
    d.key_modified(Key::Quote, Modifiers::SHIFT)?;
    d.key(Key::A)
}

fn at(d: &mut Driver<'_>, cursor: u64) -> Result<(), String> {
    d.command("sequence")?;
    if d.app().edit_range.has_bounds() {
        d.key(Key::Escape)?;
    }
    for _ in 0..6 {
        if d.app().pane == Pane::Sequence
            && d.harness
                .ctx
                .memory(|memory| memory.has_focus(pane_id(Pane::Sequence)))
        {
            break;
        }
        d.key(Key::Tab)?;
    }
    d.chord(&[Key::G, Key::G])?;
    if cursor > 0 {
        motion(d, cursor)?;
    }
    d.settled()?;
    d.check(
        "Group input has Sequence focus at the intended Edit frame",
        d.app().sequence_cursor == cursor && d.app().pane == Pane::Sequence,
        json!({"Edit":cursor,"pane":"Sequence"}),
        state(d),
    )
}

fn motion(d: &mut Driver<'_>, count: u64) -> Result<(), String> {
    let mut input = count
        .to_string()
        .chars()
        .map(|digit| Key::from_name(&digit.to_string()).ok_or("Invalid motion digit".to_owned()))
        .collect::<Result<Vec<_>, _>>()?;
    input.push(Key::L);
    d.events("Move by an exact frame count", keys(&input))
}

fn idle(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Group or macro receipt is fully admitted", |app| {
        !app.service.is_busy()
            && !app.macros.is_pending()
            && !app.copied.is_pending()
            && !app.repeat_queue.active()
    })?;
    d.settled()
}

fn undo(d: &mut Driver<'_>, expected: &ProjectDocument) -> Result<(), String> {
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    restored(
        d,
        expected,
        "One Undo restores the complete authored document",
    )
}

fn redo(d: &mut Driver<'_>, expected: &ProjectDocument) -> Result<(), String> {
    let before = d.revision();
    d.key_modified(Key::R, Modifiers::CTRL)?;
    d.changed(&before)?;
    restored(d, expected, "One Redo restores the complete Group document")
}

fn restored(d: &mut Driver<'_>, expected: &ProjectDocument, label: &str) -> Result<(), String> {
    idle(d)?;
    d.check(
        label,
        same_document(document(d)?, expected)?
            && document(d)?.revision_id() != expected.revision_id()
            && store(d)?.snapshot().map_err(|error| error.to_string())? == *document(d)?,
        json!(expected.revision_id()),
        state(d),
    )
}

fn unchanged(
    d: &mut Driver<'_>,
    expected: &ProjectDocument,
    expected_bank: &RegisterBank,
    label: &str,
) -> Result<(), String> {
    idle(d)?;
    d.check(
        label,
        document(d)? == expected
            && bank(d)? == *expected_bank
            && store(d)?.snapshot().map_err(|error| error.to_string())? == *expected,
        json!({"revision":expected.revision_id(),"bank_version":expected_bank.version}),
        state(d),
    )
}

fn preserved(d: &mut Driver<'_>, expected: &RegisterBank) -> Result<(), String> {
    d.check(
        "Structural grouping preserves the durable bank and pending register a",
        bank(d)? == *expected
            && d.app().copied.bank_version() == Some(expected.version)
            && d.app().copied.selected_override() == Some(Some('a')),
        json!({"bank_version":expected.version,"override":"a"}),
        state(d),
    )
}

fn document<'a>(d: &'a Driver<'_>) -> Result<&'a ProjectDocument, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.document.as_ref())
        .ok_or_else(|| "Group replay has no workspace".into())
}

fn same_document(actual: &ProjectDocument, expected: &ProjectDocument) -> Result<bool, String> {
    let mut actual = serde_json::to_value(actual).map_err(|error| error.to_string())?;
    actual["revision_id"] = json!(expected.revision_id());
    Ok(actual == serde_json::to_value(expected).map_err(|error| error.to_string())?)
}

fn store(d: &Driver<'_>) -> Result<ProjectStore, String> {
    ProjectStore::open(
        &d.app()
            .workspace
            .as_ref()
            .ok_or("Group replay has no workspace")?
            .path,
        AccessMode::ReadOnly,
    )
    .map_err(|error| error.to_string())
}

fn bank(d: &Driver<'_>) -> Result<RegisterBank, String> {
    store(d)?.registers().map_err(|error| error.to_string())
}

fn state(d: &Driver<'_>) -> Value {
    let mut value = d.snapshot();
    value["group"] = json!({"override":d.app().copied.selected_override(),"candidate":format!("{:?}",d.app().semantic.snapshot()),"recording":d.app().macros.recording_name(),"instructions":d.app().macros.instruction_count()});
    value
}

fn resize(d: &mut Driver<'_>, width: f32, height: f32) -> Result<(), String> {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing Group viewport")?
        .inner_rect = Some(rect);
    d.step(&format!("Group controls at {width} by {height}"), true)
}

fn painted(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    let paint = scenarios::text_paint_visibility(d, label);
    d.check(
        &format!("Group label is fully painted: {label}"),
        !paint.is_empty() && paint.iter().all(|part| part["fully_visible"] == true),
        json!("inside viewport and clip"),
        json!(paint),
    )
}

fn help(d: &mut Driver<'_>) -> Result<(), String> {
    d.key(Key::Questionmark)?;
    d.key(Key::Home)?;
    for needle in [":group name=", "custom audio edges"] {
        for _ in 0..32 {
            let paint = scenarios::text_paint_visibility(d, needle);
            if paint.iter().any(|part| part["fully_visible"] == true) {
                break;
            }
            let key = paint.first().map_or(Key::PageDown, |part| {
                if part["bounds"][1].as_f64() < part["clip"][1].as_f64() {
                    Key::K
                } else {
                    Key::J
                }
            });
            d.key(key)?;
        }
        let paint = scenarios::text_paint_visibility(d, needle);
        d.check(
            "Group help paints the named entry and safe Ungroup restrictions",
            d.app().help_open && paint.iter().any(|part| part["fully_visible"] == true),
            json!(needle),
            json!(paint),
        )?;
    }
    d.capture("Group and safe Ungroup help at the current viewport")?;
    d.key(Key::Escape)
}

fn keys(input: &[Key]) -> Vec<Event> {
    input
        .iter()
        .flat_map(|key| {
            [
                key_event(*key, Modifiers::NONE, true),
                key_event(*key, Modifiers::NONE, false),
            ]
        })
        .collect()
}
