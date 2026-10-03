//! Typed Repeat input with genuine SQLite commits and deliberately delayed UI
//! delivery. No authored result or service receipt is fabricated.

use super::*;
use crate::preview::copied::Content;
use crate::project::semantic::RepeatableEdit;
use deadpan_core::{
    ProjectDocument, RegisterName, RegisterValue, SemanticInstruction, SemanticMotion,
    SemanticSelector,
};
use deadpan_store::{AccessMode, ProjectStore, registers::RegisterBank};
use egui::{Event, Key, Modifiers};
use std::num::NonZeroU32;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    idle(d)?;
    let baseline = document(d)?.clone();
    d.check(
        "Repeat operator replay begins with the complete 120-frame Original",
        d.app().sequence_length() == 120 && !d.app().workspace.as_ref().unwrap().can_undo,
        json!({"frames":120,"undo":false}),
        state(d),
    )?;
    selectors(d, &baseline)?;
    dot_and_refusals(d, &baseline)?;
    commands(d, &baseline)?;
    delayed_prefixes(d, &baseline)?;
    recorded(d, &baseline)?;
    d.report.skipped.push("This replay delays delivery of genuine project updates to exercise Repeat prefix ownership. Physical keyboard delivery and OS IME behavior are not exercised.".into());
    Ok(())
}

fn selectors(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    for (input, plays, start, child_frames, total, selector) in [
        (
            vec![Key::R, Key::R],
            2,
            0,
            120,
            240,
            SemanticSelector::SelectedBeat,
        ),
        (
            vec![Key::Num3, Key::R, Key::R],
            3,
            0,
            120,
            360,
            SemanticSelector::SelectedBeat,
        ),
        (vec![Key::R, Key::Num3, Key::L], 2, 20, 3, 123, forward(3)),
        (vec![Key::Num3, Key::R, Key::L], 3, 20, 1, 122, forward(1)),
    ] {
        at(d, 20)?;
        choose_a(d)?;
        let before_bank = bank(d)?;
        let before = d.revision();
        d.events(
            "Repeat with the declared play count and selector distance",
            keys(&input),
        )?;
        d.changed(&before)?;
        idle(d)?;
        wrapped(d, plays, start, child_frames, total)?;
        candidate(d, selector, plays)?;
        d.check(
            "Repeat preserves the complete durable bank and the one-shot a override",
            bank(d)? == before_bank && d.app().copied.selected_override() == Some(Some('a')),
            json!({"bank":"unchanged","override":"a"}),
            state(d),
        )?;
        undo(d, baseline)?;
        d.check(
            "One Undo removes the entire Repeat operation including endpoint splits",
            !d.app().workspace.as_ref().unwrap().can_undo,
            json!("no earlier authored edit"),
            state(d),
        )?;
    }
    at(d, 30)?;
    visual(d, 4, false)?;
    let before = d.revision();
    d.events(
        "Three total plays of the explicit Visual range",
        keys(&[Key::Num3, Key::R]),
    )?;
    d.changed(&before)?;
    idle(d)?;
    wrapped(d, 3, 30, 4, 128)?;
    candidate(d, SemanticSelector::VisualSelection, 3)?;
    d.check(
        "Visual r consumes its selection and selects the resulting Repeat",
        !d.app().edit_range.has_bounds() && d.app().sequence_cursor == 30,
        json!({"selection":null,"Edit":30}),
        state(d),
    )?;
    scenarios::footer_anchored(d, "The Repeat result footer stays above notices")?;
    d.capture("Visual r repeats four frames for three total plays")?;
    undo(d, baseline)
}

fn dot_and_refusals(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 20)?;
    let before = d.revision();
    d.events(
        "Retain a three-frame Repeat selector for dot",
        keys(&[Key::R, Key::Num3, Key::L]),
    )?;
    d.changed(&before)?;
    idle(d)?;
    undo(d, baseline)?;
    at(d, 40)?;
    choose_a(d)?;
    let before_bank = bank(d)?;
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    idle(d)?;
    wrapped(d, 2, 40, 3, 123)?;
    candidate(d, forward(3), 2)?;
    d.check(
        "Dot resolves the retained motion at Edit 40 and preserves register intent",
        d.app().sequence_cursor == 40
            && d.app().presentation.displayed_source_frame() == Some(SourceFrameId(40))
            && bank(d)? == before_bank
            && d.app().copied.selected_override() == Some(Some('a')),
        json!({"repeat_range":[40,43],"Original_picture":40,"override":"a"}),
        state(d),
    )?;
    undo(d, baseline)?;

    at(d, 50)?;
    visual(d, 5, true)?;
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    idle(d)?;
    wrapped(d, 2, 50, 5, 125)?;
    candidate(d, SemanticSelector::VisualSelection, 2)?;
    d.check(
        "Current finished Visual range overrides the retained three-frame selector",
        d.app().sequence_cursor == 50 && !d.app().edit_range.has_bounds(),
        json!({"repeat_range":[50,55],"selection":null}),
        state(d),
    )?;
    undo(d, baseline)?;

    at(d, 70)?;
    choose_a(d)?;
    let before = document(d)?.clone();
    let before_bank = bank(d)?;
    let candidate_before = d.app().semantic.snapshot().cloned();
    d.key(Key::Period)?;
    unchanged(
        d,
        &before,
        &before_bank,
        "A retained Visual selector cannot repeat without a current selection",
    )?;
    d.check(
        "Missing Visual dot retains the saved selector and one-shot register",
        d.app().semantic.snapshot() == candidate_before.as_ref()
            && d.app().copied.selected_override() == Some(Some('a'))
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Select a new Visual range")),
        json!({"candidate":"unchanged","override":"a","refusal":"missing Visual range"}),
        state(d),
    )?;

    for finished in [false, true] {
        at(d, 70)?;
        visual(d, 0, finished)?;
        choose_a(d)?;
        let selection = d.app().edit_range.clone();
        for key in [Key::R, Key::Period] {
            let before = document(d)?.clone();
            let before_bank = bank(d)?;
            d.key(key)?;
            unchanged(
                d,
                &before,
                &before_bank,
                "An explicit empty Visual Repeat refuses instead of wrapping the selected beat",
            )?;
            d.check("Empty Repeat preserves the exact active or finished endpoints and register override",
                d.app().edit_range == selection && d.app().copied.selected_override() == Some(Some('a')) && d.app().error.is_some(),
                json!({"finished":finished,"override":"a","selection":"unchanged"}), state(d))?;
        }
    }
    at(d, 20)?;
    for input in [
        vec![Key::Num3, Key::R, Key::Num2, Key::L],
        vec![Key::Num3, Key::R, Key::Num3, Key::L],
        vec![Key::Num0, Key::R, Key::R],
        vec![Key::R, Key::Num0, Key::L],
    ] {
        let before = document(d)?.clone();
        let before_bank = bank(d)?;
        let selected = d.app().selected_beat.clone();
        d.events(
            "Reject conflicting or zero Repeat counts as a complete command",
            keys(&input),
        )?;
        unchanged(
            d,
            &before,
            &before_bank,
            "Invalid Repeat counts make no authored or register write",
        )?;
        d.check(
            "Invalid suffix consumption preserves the cursor and exact selected child",
            d.app().sequence_cursor == 20
                && d.app().selected_beat == selected
                && d.app().bindings.pending().is_empty(),
            json!({"Edit":20,"selected":selected,"prefix":""}),
            state(d),
        )?;
    }
    Ok(())
}

fn commands(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 30)?;
    visual(d, 4, true)?;
    let before = d.revision();
    d.command("wrap-repeat 3")?;
    d.changed(&before)?;
    idle(d)?;
    wrapped(d, 3, 30, 4, 128)?;
    undo(d, baseline)?;

    at(d, 0)?;
    let before = d.revision();
    d.events(
        "Wrap the whole selected beat twice",
        keys(&[Key::R, Key::R]),
    )?;
    d.changed(&before)?;
    let twice = document(d)?.clone();
    let inner = d
        .app()
        .selected_beat
        .clone()
        .ok_or("No Repeat for setter")?;
    let original_child = repeat_child(d, &inner)?.clone();
    let before = d.revision();
    d.command("repeat 3")?;
    d.changed(&before)?;
    wrapped(d, 3, 0, 120, 360)?;
    d.check(
        ":repeat changes the selected Repeat count without creating an outer wrapper",
        d.app().selected_beat.as_ref() == Some(&inner)
            && repeat_child(d, &inner)? == &original_child
            && document(d)?.nodes().len() == twice.nodes().len(),
        json!({"selected":inner,"child":original_child,"node_count":twice.nodes().len()}),
        state(d),
    )?;
    let set = document(d)?.clone();
    let before = d.revision();
    d.command("wrap-repeat 2")?;
    d.changed(&before)?;
    wrapped(d, 2, 0, 360, 720)?;
    let outer = d.app().selected_beat.as_ref().ok_or("No outer Repeat")?;
    d.check(
        ":wrap-repeat always creates an outer wrapper around the existing Repeat",
        outer != &inner && repeat_child(d, outer)? == &inner,
        json!({"outer":"new","child":inner}),
        state(d),
    )?;
    undo(d, &set)?;
    undo(d, &twice)?;

    // The field remains open while an independent genuine Undo lands.
    d.key(Key::Colon)?;
    d.events(
        "Type wrapper command against the currently selected Repeat",
        vec![Event::Text("wrap-repeat 2".into())],
    )?;
    let before = d.revision();
    d.app().service.submit(ProjectRequest::Undo {
        expected_revision: document(d)?.revision_id().clone(),
    })?;
    d.changed(&before)?;
    let restored = document(d)?.clone();
    let restored_bank = bank(d)?;
    d.key(Key::Enter)?;
    unchanged(
        d,
        &restored,
        &restored_bank,
        "A delayed wrapper command cannot use the new child supplied by Undo",
    )?;
    d.check(
        "Stale command entry reports its captured context changed",
        same_document(document(d)?, baseline)?
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("context changed")),
        json!("captured revision refused"),
        state(d),
    )?;

    at(d, 0)?;
    let before = d.revision();
    d.events(
        "Delete the only beat to create actual target absence",
        keys(&[Key::D, Key::D]),
    )?;
    d.changed(&before)?;
    idle(d)?;
    let empty = document(d)?.clone();
    let empty_bank = bank(d)?;
    d.command("wrap-repeat 2")?;
    unchanged(
        d,
        &empty,
        &empty_bank,
        "Wrapper command refuses the absent selected child",
    )?;
    d.check(
        "The empty Sequence retains no fabricated Repeat target",
        d.app().selected_beat.is_none() && d.app().error.is_some(),
        json!("no selected beat"),
        state(d),
    )?;
    d.key(Key::Colon)?;
    d.events(
        "Capture another wrapper command while the Sequence is empty",
        vec![Event::Text("wrap-repeat 2".into())],
    )?;
    let before = d.revision();
    d.app().service.submit(ProjectRequest::Undo {
        expected_revision: document(d)?.revision_id().clone(),
    })?;
    d.changed(&before)?;
    let restored = document(d)?.clone();
    d.key(Key::Enter)?;
    unchanged(
        d,
        &restored,
        &empty_bank,
        "Command-entry absence cannot become the beat restored by a later Undo",
    )?;
    d.check(
        "Absent captured target remains refused after the Original beat returns",
        same_document(document(d)?, baseline)? && d.app().error.is_some(),
        json!("baseline restored, no Repeat"),
        state(d),
    )
}

fn delayed_prefixes(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    for terminal in [Key::R, Key::L] {
        at(d, 0)?;
        let before_bank = bank(d)?;
        let before = d.revision();
        d.app_mut().feedback.hold_project_updates = true;
        d.events(
            "Submit a real whole-beat wrap while withholding its UI receipt",
            keys(&[Key::R, Key::R]),
        )?;
        d.key(Key::R)?;
        d.wait_for(
            "The writer finishes before the next Repeat selector arrives",
            |app| !app.service.is_busy(),
        )?;
        d.check(
            "The pending prefix still belongs to the original visible revision",
            d.revision() == before
                && d.app().bindings.pending() == "r"
                && d.app().repeat_prefix_target.is_some(),
            json!({"revision":before,"prefix":"r"}),
            state(d),
        )?;
        d.app_mut().feedback.hold_project_updates = false;
        d.changed(&before)?;
        let first = document(d)?.clone();
        let first_wrapper = d
            .app()
            .selected_beat
            .clone()
            .ok_or("No completed wrapper")?;
        d.check(
            "A validated own wrapper receipt preserves the pending r",
            d.app().bindings.pending() == "r" && d.app().sequence_length() == 240,
            json!({"prefix":"r","frames":240}),
            state(d),
        )?;
        let before = d.revision();
        d.key(terminal)?;
        if terminal == Key::R {
            d.changed(&before)?;
            wrapped(d, 2, 0, 240, 480)?;
            let selected = d
                .app()
                .selected_beat
                .as_ref()
                .ok_or("No continued wrapper")?;
            d.check(
                "Only the explicit rr terminal may continue onto the validated new wrapper",
                repeat_child(d, selected)? == &first_wrapper && bank(d)? == before_bank,
                json!({"child":first_wrapper,"bank":"unchanged"}),
                state(d),
            )?;
            undo(d, &first)?;
        } else {
            unchanged(
                d,
                &first,
                &before_bank,
                "The same retained r followed by l refuses its stale original motion capture",
            )?;
            d.check(
                "A stale motion cannot become a fresh root l or consume the wrapper selection",
                d.app().sequence_cursor == 0
                    && d.app().selected_beat.as_ref() == Some(&first_wrapper)
                    && d.app()
                        .error
                        .as_deref()
                        .is_some_and(|error| error.contains("context changed")),
                json!({"Edit":0,"selected":first_wrapper,"refusal":"stale context"}),
                state(d),
            )?;
        }
        undo(d, baseline)?;
    }
    Ok(())
}

fn recorded(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 0)?;
    let before_bank = bank(d)?;
    d.events("Start named recording z", keys(&[Key::Q, Key::Z]))?;
    let before = d.revision();
    d.events(
        "Record one selected-beat Repeat through the shared semantic planner",
        keys(&[Key::R, Key::R]),
    )?;
    d.changed(&before)?;
    idle(d)?;
    wrapped(d, 2, 0, 120, 240)?;
    d.check(
        "The durable Repeat receipt appends exactly one recording instruction",
        d.app().macros.recording_name() == Some('z') && d.app().macros.instruction_count() == 1,
        json!({"recording":"z","instructions":1}),
        state(d),
    )?;
    d.key(Key::Q)?;
    d.wait_for("Saved Macro z is published to the native bank", |app| {
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
    let expected = SemanticInstruction::Repeat {
        selector: SemanticSelector::SelectedBeat,
        plays: NonZeroU32::new(2).unwrap(),
    };
    d.check("The named Macro durably stores Repeat intent while preserving the unnamed contents",
        matches!(saved_bank.entries.get(&RegisterName::new('z').map_err(|error| error.to_string())?).map(AsRef::as_ref), Some(RegisterValue::Macro { program }) if program.instructions() == [expected])
            && saved_bank.entries.get(&RegisterName::unnamed()) == before_bank.entries.get(&RegisterName::unnamed()),
        json!({"body":"Repeat selected beat, 2 total plays","unnamed":"unchanged"}), state(d))?;
    undo(d, baseline)?;
    at(d, 0)?;
    let before = d.revision();
    let mut input = keys(&[Key::Num2]);
    input.push(key_event(Key::Num2, Modifiers::SHIFT, true));
    input.push(Event::Text("@".into()));
    input.push(key_event(Key::Num2, Modifiers::SHIFT, false));
    input.extend(keys(&[Key::Z]));
    d.events(
        "Execute saved Macro z twice through paired logical @ text",
        input,
    )?;
    d.changed(&before)?;
    idle(d)?;
    wrapped(d, 2, 0, 240, 480)?;
    let outer = d
        .app()
        .selected_beat
        .as_ref()
        .ok_or("No macro Repeat result")?;
    let inner = repeat_child(d, outer)?;
    d.check("Counted macro execution authors two nested Repeats without changing its saved program or copied bank",
        matches!(document(d)?.nodes().get(inner).map(|node| &node.kind), Some(NodeKind::Repeat { iterations, gap: None, .. }) if iterations.len() == 2)
            && bank(d)? == saved_bank,
        json!({"frames":480,"nested_plays":[2,2],"bank":"unchanged"}), state(d))?;
    d.capture("Counted Macro replay produces two nested Repeats in one transaction")?;
    undo(d, baseline)?;
    d.check(
        "One Undo restores the entire counted macro result, retaining the durable program",
        !d.app().workspace.as_ref().unwrap().can_undo && bank(d)? == saved_bank,
        json!({"frames":120,"undo":false,"macro":"retained"}),
        state(d),
    )
}

fn forward(count: u32) -> SemanticSelector {
    SemanticSelector::Motion {
        motion: SemanticMotion::Frames {
            forward: true,
            count: NonZeroU32::new(count).unwrap(),
        },
    }
}

fn candidate(d: &mut Driver<'_>, selector: SemanticSelector, plays: u32) -> Result<(), String> {
    let expected = RepeatableEdit::Repeat {
        selector,
        plays: NonZeroU32::new(plays).unwrap(),
    };
    d.check(
        "Saved repetition retains the requested selector and total plays at the current revision",
        d.app().semantic.snapshot().is_some_and(|snapshot| {
            snapshot.head.as_ref() == Some(document(d).unwrap().revision_id())
                && snapshot.error.is_none()
                && snapshot
                    .edit
                    .as_ref()
                    .is_some_and(|edit| edit.operation == expected && edit.register.is_none())
        }),
        json!({"selector":selector,"plays":plays}),
        state(d),
    )
}

fn wrapped(
    d: &mut Driver<'_>,
    plays: u32,
    start: i64,
    child_frames: i64,
    total: u64,
) -> Result<(), String> {
    idle(d)?;
    let selected = d
        .app()
        .selected_beat
        .as_ref()
        .ok_or("Repeat result has no selected child")?;
    let child = repeat_child(d, selected)?;
    let document = document(d)?;
    let scope = d
        .app()
        .sequence_scope
        .resolve(d.app().workspace.as_ref().unwrap())?;
    let mut actual_start = 0;
    let mut direct = false;
    for sibling in scope.children {
        if sibling == selected {
            direct = true;
            break;
        }
        actual_start += document
            .node_duration(sibling)
            .map_err(|error| error.to_string())?
            .frames();
    }
    let child_duration = document
        .node_duration(child)
        .map_err(|error| error.to_string())?
        .frames();
    let valid = matches!(&document.nodes()[selected].kind, NodeKind::Repeat { iterations, gap: None, .. } if iterations.len() == plays);
    let actual = json!({"selected":selected,"start":actual_start,"child_frames":child_duration,"kind":document.nodes()[selected].kind,"frames":d.app().sequence_length()});
    let durable_matches = store(d)?.snapshot().map_err(|error| error.to_string())? == *document;
    d.check(
        "The selected ordinary child is an exact gapless Repeat of the resolved interval",
        direct
            && valid
            && actual_start == start
            && child_duration == child_frames
            && d.app().sequence_length() == total
            && durable_matches,
        json!({"start":start,"child_frames":child_frames,"plays":plays,"frames":total}),
        actual,
    )
}

fn repeat_child<'a>(
    d: &'a Driver<'_>,
    id: &deadpan_core::NodeId,
) -> Result<&'a deadpan_core::NodeId, String> {
    match document(d)?.nodes().get(id).map(|node| &node.kind) {
        Some(NodeKind::Repeat { child, .. }) => Ok(child),
        _ => Err(format!("Expected a Repeat at {id:?}")),
    }
}

fn visual(d: &mut Driver<'_>, frames: u64, finished: bool) -> Result<(), String> {
    d.key(Key::V)?;
    if frames > 0 {
        let mut input = count_keys(frames)?;
        input.extend(keys(&[Key::L]));
        d.events("Extend the explicit Visual range", input)?;
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
    d.key(Key::Escape)?;
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
    let mut input = keys(&[Key::G, Key::G]);
    if cursor > 0 {
        input.extend(count_keys(cursor)?);
        input.extend(keys(&[Key::L]));
    }
    d.events("Navigate to the explicit Repeat input context", input)?;
    d.settled()?;
    d.check(
        "Repeat input begins at the requested Edit cursor with Sequence focus",
        d.app().sequence_cursor == cursor && d.app().pane == Pane::Sequence,
        json!({"Edit":cursor,"pane":"Sequence"}),
        state(d),
    )
}

fn idle(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Repeat or macro receipt is fully admitted", |app| {
        !app.service.is_busy()
            && !app.repeat_queue.active()
            && !app.macros.is_pending()
            && !app.copied.is_pending()
    })?;
    d.settled()
}

fn undo(d: &mut Driver<'_>, expected: &ProjectDocument) -> Result<(), String> {
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "One Undo restores every authored field with a fresh revision",
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

fn document<'a>(d: &'a Driver<'_>) -> Result<&'a ProjectDocument, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.document.as_ref())
        .ok_or_else(|| "Repeat replay has no workspace".into())
}

fn same_document(actual: &ProjectDocument, expected: &ProjectDocument) -> Result<bool, String> {
    let mut actual = serde_json::to_value(actual).map_err(|error| error.to_string())?;
    actual["revision_id"] = json!(expected.revision_id());
    Ok(actual == serde_json::to_value(expected).map_err(|error| error.to_string())?)
}

fn bank(d: &Driver<'_>) -> Result<RegisterBank, String> {
    store(d)?.registers().map_err(|error| error.to_string())
}

fn store(d: &Driver<'_>) -> Result<ProjectStore, String> {
    ProjectStore::open(
        &d.app()
            .workspace
            .as_ref()
            .ok_or("Repeat bank has no workspace")?
            .path,
        AccessMode::ReadOnly,
    )
    .map_err(|error| error.to_string())
}

fn state(d: &Driver<'_>) -> Value {
    let mut state = d.snapshot();
    state["repeat"] = json!({"pending":d.app().bindings.pending(),"override":d.app().copied.selected_override(),"candidate":format!("{:?}",d.app().semantic.snapshot()),"recording":d.app().macros.recording_name(),"instructions":d.app().macros.instruction_count()});
    state
}

fn count_keys(count: u64) -> Result<Vec<Event>, String> {
    count
        .to_string()
        .chars()
        .map(|digit| {
            Key::from_name(&digit.to_string())
                .ok_or_else(|| "Invalid Repeat count digit".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|input| keys(&input))
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
