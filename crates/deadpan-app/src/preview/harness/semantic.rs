//! Semantic cuts through keyboard input, real commits and composed pictures.

use super::*;
use crate::preview::copied::Content;
use crate::project::semantic::RepeatableCut;
use crate::project::slice::Captured;
use deadpan_core::{FrameRange, ProjectDocument};
use deadpan_store::{AccessMode, ProjectStore};
use egui::{Event, Key, Modifiers};

mod selectors;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 640.0));
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing dot-repeat viewport")?
        .inner_rect = Some(rect);
    d.step("Paint semantic repeat at the minimum viewport", false)?;
    let baseline = document(d)?.clone();
    d.check(
        "Semantic replay begins at the full 120-frame Original baseline",
        d.app().sequence_length() == 120 && !d.app().workspace.as_ref().unwrap().can_undo,
        json!({"frames":120,"undo":false}),
        state(d),
    )?;

    clamped_then_retargeted(d, &baseline)?;
    registers(d, &baseline)?;
    marks(d, &baseline)?;
    guards(d)?;
    unsupported(d, &baseline)?;
    fresh_count(d, &baseline)?;
    selectors::run(d, &baseline)
}

fn clamped_then_retargeted(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 118)?;
    choose(d, Some('a'))?;
    let before = d.revision();
    d.events(
        "Request seven frames from two frames before the end",
        keys(&[Key::Num7, Key::X]),
    )?;
    d.changed(&before)?;
    saved_cut(d, &before, 118, 120, 118, 7, Some('a'))?;
    let first_copy = copied(d, Some('a'))?;
    footer(d, 7)?;
    d.capture("Minimum workspace retains the requested seven-frame repeat after a two-frame cut")?;
    let clamped = document(d)?.clone();
    d.key(Key::Period)?;
    unchanged(
        d,
        &clamped,
        &first_copy,
        "Dot at the terminal cursor refuses without replacing the saved semantic edit",
    )?;
    candidate(
        d,
        7,
        Some('a'),
        "A terminal refusal retains the requested length",
    )?;
    undo(d, baseline)?;
    candidate(
        d,
        7,
        Some('a'),
        "Undo preserves the requested seven-frame edit",
    )?;
    d.check(
        "One Undo restores the full baseline after the clamped cut",
        !d.app().workspace.as_ref().unwrap().can_undo,
        json!({"frames":120,"undo":false}),
        state(d),
    )?;

    at(d, 20)?;
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    saved_cut(d, &before, 20, 27, 113, 7, Some('a'))?;
    let second_copy = copied(d, Some('a'))?;
    d.check(
        "Dot resolves a fresh seven-frame range at the new cursor after Undo",
        second_copy.id() != first_copy.id()
            && second_copy.slice().revision_id() != first_copy.slice().revision_id()
            && d.revision() != first_copy.slice().revision_id().as_str()
            && d.app().presentation.displayed_source_frame() == Some(SourceFrameId(27)),
        json!({"Edit":20,"copied":[20,27],"Original_picture":27,"fresh_capture":true}),
        state(d),
    )?;
    footer(d, 7)?;
    d.capture("Dot at Edit 20 cuts seven frames and displays Original frame 27")?;
    let repeated = document(d)?.clone();
    undo(d, baseline)?;
    let before = d.revision();
    d.key_modified(Key::R, Modifiers::CTRL)?;
    d.changed(&before)?;
    d.check(
        "Redo restores the same authored cut with a fresh revision",
        same_document(document(d)?, &repeated)?
            && document(d)?.revision_id() != repeated.revision_id(),
        json!({"frames":113,"fresh_revision":true}),
        state(d),
    )?;
    candidate(d, 7, Some('a'), "Redo preserves the semantic edit")?;
    undo(d, baseline)
}

fn registers(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    let retained_a = copied(d, Some('a'))?;
    at(d, 30)?;
    choose(d, Some('b'))?;
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    saved_cut(d, &before, 30, 37, 113, 7, Some('b'))?;
    d.check(
        "An explicit b register overrides the saved destination without changing a",
        same_copy(d, Some('a'), &retained_a) && d.app().copied.selected_override().is_none(),
        json!({"destination":"b","a_retained":true,"one_shot_consumed":true}),
        state(d),
    )?;
    undo(d, baseline)?;
    at(d, 40)?;
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    saved_cut(d, &before, 40, 47, 113, 7, Some('b'))?;
    let retained_b = copied(d, Some('b'))?;
    undo(d, baseline)?;

    at(d, 50)?;
    choose(d, None)?;
    d.check(
        "Two quotes explicitly select the unnamed destination",
        d.app().copied.selected_override() == Some(None),
        json!("explicit unnamed override"),
        state(d),
    )?;
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    saved_cut(d, &before, 50, 57, 113, 7, None)?;
    d.check(
        "An explicit unnamed repeat preserves both named historical copies",
        same_copy(d, Some('a'), &retained_a)
            && same_copy(d, Some('b'), &retained_b)
            && d.app().copied.selected_override().is_none(),
        json!({"destination":"unnamed","a_retained":true,"b_retained":true}),
        state(d),
    )?;
    undo(d, baseline)
}

fn marks(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 45)?;
    let before = d.revision();
    d.events(
        "Save mark z without replacing the semantic cut",
        keys(&[Key::M, Key::Z]),
    )?;
    d.changed(&before)?;
    candidate(d, 7, None, "A saved mark preserves semantic repetition")?;
    let marked = document(d)?.clone();
    at(d, 65)?;
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    saved_cut(d, &before, 65, 72, 113, 7, None)?;
    undo(d, &marked)?;
    undo(d, baseline)?;
    candidate(
        d,
        7,
        None,
        "Undoing the cut and mark retains the last semantic operation",
    )
}

fn guards(d: &mut Driver<'_>) -> Result<(), String> {
    at(d, 20)?;
    let saved = document(d)?.clone();
    let accepted = copied(d, None)?;
    for count in ["0", "1", "4294967296"] {
        let mut input = count.bytes().map(digit_key).collect::<Vec<_>>();
        input.push(Key::Period);
        d.events("A count cannot scale or execute dot-repeat", keys(&input))?;
        unchanged(d, &saved, &accepted, "Counted dot makes no edit")?;
    }
    for modifiers in [
        Modifiers::SHIFT,
        Modifiers::ALT,
        Modifiers::CTRL,
        Modifiers::COMMAND,
    ] {
        d.key_modified(Key::Period, modifiers)?;
        unchanged(d, &saved, &accepted, "Modified Period makes no edit")?;
    }
    for finished in [false, true] {
        at(d, 20)?;
        d.key(Key::V)?;
        if finished {
            d.key(Key::V)?;
        }
        let selection = d.app().edit_range.clone();
        d.key(Key::Period)?;
        unchanged(
            d,
            &saved,
            &accepted,
            "Dot refuses an explicit empty Visual selection",
        )?;
        d.check(
            "A refused dot retains its active or finished Visual selection",
            d.app().edit_range == selection,
            json!({"finished":finished,"nonempty":false,"selection_retained":true}),
            state(d),
        )?;
    }
    at(d, 20)?;
    d.command("source")?;
    d.key(Key::Period)?;
    unchanged(
        d,
        &saved,
        &accepted,
        "Original cannot repeat against the retained Edit cursor",
    )?;
    d.command("sequence")?;
    focus_pane(d, Pane::Sources)?;
    d.key(Key::Period)?;
    unchanged(
        d,
        &saved,
        &accepted,
        "Sources focus cannot repeat against the retained Edit cursor",
    )?;
    d.command("sounds")?;
    d.key(Key::Period)?;
    unchanged(
        d,
        &saved,
        &accepted,
        "Placed sounds cannot repeat a picture cut",
    )?;

    at(d, 20)?;
    d.events(
        "Opening Keys blocks a later dot in the same event batch",
        keys(&[Key::Questionmark, Key::Period]),
    )?;
    d.check(
        "Keys owns the input batch",
        d.app().help_open,
        json!(true),
        state(d),
    )?;
    unchanged(
        d,
        &saved,
        &accepted,
        "A newly opened modal prevents dot-repeat",
    )?;
    d.key(Key::Escape)?;
    focus_pane(d, Pane::Sequence)?;
    d.key(Key::Colon)?;
    let mut input = vec![Event::Text(".".into())];
    input.extend(keys(&[Key::Period]));
    d.events("Native command text owns the period", input)?;
    d.check(
        "Period stays literal command text",
        d.app().command_open && d.app().command == ".",
        json!("."),
        state(d),
    )?;
    unchanged(d, &saved, &accepted, "Command text does not repeat an edit")?;
    d.key(Key::Escape)?;
    for (ime, composing) in [
        (
            egui::ImeEvent::Preedit {
                text: ".".into(),
                active_range_chars: Some(0..1),
            },
            true,
        ),
        (egui::ImeEvent::Commit(".".into()), false),
    ] {
        let mut input = vec![Event::Ime(ime)];
        input.extend(keys(&[Key::Period]));
        d.events("IME owns Period for the complete native batch", input)?;
        d.check(
            "Dot follows composition ownership",
            d.app().ime_composing == composing,
            json!(composing),
            state(d),
        )?;
        unchanged(
            d,
            &saved,
            &accepted,
            "IME preedit and commit do not leak dot-repeat",
        )?;
    }
    candidate(
        d,
        7,
        None,
        "Refused input never replaces the successful semantic edit",
    )
}

fn unsupported(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 10)?;
    let before = d.revision();
    d.key(Key::S)?;
    d.changed(&before)?;
    let split = document(d)?.clone();
    let accepted = copied(d, None)?;
    d.check(
        "A committed Split clears the unsupported last edit",
        d.app()
            .semantic
            .snapshot()
            .is_some_and(|snapshot| snapshot.edit.is_none()),
        json!("no repeatable edit"),
        state(d),
    )?;
    d.key(Key::Period)?;
    unchanged(
        d,
        &split,
        &accepted,
        "Dot cannot silently repeat the older frame cut after Split",
    )?;
    undo(d, baseline)?;
    let restored = document(d)?.clone();
    d.key(Key::Period)?;
    unchanged(
        d,
        &restored,
        &accepted,
        "Undo cannot revive the invalidated older edit",
    )?;
    d.check(
        "Undo does not restore an older semantic candidate",
        d.app()
            .semantic
            .snapshot()
            .is_some_and(|snapshot| snapshot.edit.is_none()),
        json!("no repeatable edit"),
        state(d),
    )
}

fn fresh_count(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 20)?;
    let before = d.revision();
    d.command("delete-frames 3f")?;
    d.changed(&before)?;
    saved_cut(d, &before, 20, 23, 117, 3, None)?;
    let first = document(d)?.clone();
    at(d, 30)?;
    let before = d.revision();
    d.events(
        "Press and hold dot for the new three-frame edit",
        vec![key_event(Key::Period, Modifiers::NONE, true)],
    )?;
    d.changed(&before)?;
    saved_cut(d, &before, 30, 33, 114, 3, None)?;
    let repeated = document(d)?.clone();
    let accepted = copied(d, None)?;
    d.events(
        "A held Period cannot repeat again after the writer becomes idle",
        vec![Event::Key {
            key: Key::Period,
            physical_key: None,
            pressed: true,
            repeat: true,
            modifiers: Modifiers::NONE,
        }],
    )?;
    d.events(
        "Release Period",
        vec![key_event(Key::Period, Modifiers::NONE, false)],
    )?;
    unchanged(
        d,
        &repeated,
        &accepted,
        "Held dot produces only one saved edit",
    )?;
    footer(d, 3)?;
    d.capture("The new command count replaces the old edit and held dot saves once")?;
    undo(d, &first)?;
    undo(d, baseline)?;
    d.check(
        "Each semantic execution is one Undo and the baseline is restored",
        !d.app().workspace.as_ref().unwrap().can_undo,
        json!({"frames":120,"undo":false}),
        state(d),
    )
}

fn saved_cut(
    d: &mut Driver<'_>,
    before: &str,
    start: i64,
    end: i64,
    frames: u64,
    requested: u32,
    register: Option<char>,
) -> Result<(), String> {
    let copied = copied(d, register)?;
    let current = document(d)?;
    let durable = ProjectStore::open(
        &d.app().workspace.as_ref().unwrap().path,
        AccessMode::ReadOnly,
    )
    .and_then(|store| store.snapshot())
    .map_err(|error| error.to_string())?;
    let expected = FrameRange::new(ProjectFrame(start), ProjectFrame(end))
        .map_err(|error| error.to_string())?;
    d.check(
        "The saved cut, current cursor and exact historical register agree",
        durable == *current
            && d.app().sequence_length() == frames
            && d.app().sequence_cursor == u64::try_from(start).map_err(|error| error.to_string())?
            && copied.slice().range() == expected
            && copied.slice().revision_id().as_str() == before
            && copied.slice().duration().frames() == end - start
            && same_copy(d, None, &copied)
            && !d.app().copied.is_pending(),
        json!({"source_revision":before,"range":[start,end],"duration":frames,"register":register,"durable":true}),
        state(d),
    )?;
    candidate(
        d,
        requested,
        register,
        "The receipt retains semantic length and destination",
    )
}

fn candidate(
    d: &mut Driver<'_>,
    count: u32,
    register: Option<char>,
    label: &str,
) -> Result<(), String> {
    let revision = document(d)?.revision_id();
    let matches = d.app().semantic.snapshot().is_some_and(|snapshot| {
        snapshot.head.as_ref() == Some(revision)
            && snapshot
                .edit
                .as_ref()
                .is_some_and(|edit| matches!(&edit.operation, RepeatableCut::Frames(operation) if operation.count() == count) && edit.register == register)
            && snapshot.error.is_none()
    });
    d.check(
        label,
        matches,
        json!({"count":count,"register":register}),
        state(d),
    )
}

fn footer(d: &mut Driver<'_>, count: u32) -> Result<(), String> {
    let label = format!("repeat cut {count}f");
    let paint = scenarios::text_paint_visibility(d, &label);
    let mut key = scenarios::text_paint_visibility(d, ".");
    key.retain(|part| part["text"] == ".");
    d.check(
        "Dot and its exact requested count are fully painted at 960 by 640",
        !paint.is_empty()
            && !key.is_empty()
            && paint
                .iter()
                .chain(&key)
                .all(|part| part["fully_visible"] == true),
        json!({"key":".","label":label,"viewport":[960,640]}),
        json!({"key_paint":key,"label_paint":paint}),
    )?;
    scenarios::footer_anchored(d, "The repeat footer stays above the notice panel")
}

fn at(d: &mut Driver<'_>, cursor: u64) -> Result<(), String> {
    d.command("sequence")?;
    d.key(Key::Escape)?;
    focus_pane(d, Pane::Sequence)?;
    let mut input = vec![Key::G, Key::G];
    if cursor > 0 {
        input.extend(cursor.to_string().bytes().map(digit_key));
        input.push(Key::L);
    }
    d.events(
        "Navigate to the new Edit cursor with gg and counted l",
        keys(&input),
    )?;
    d.settled()?;
    d.check(
        "Keyboard navigation reaches the exact Edit boundary",
        d.app().sequence_cursor == cursor,
        json!(cursor),
        state(d),
    )
}

fn focus_pane(d: &mut Driver<'_>, pane: Pane) -> Result<(), String> {
    for _ in 0..6 {
        if d.app().pane == pane
            && d.harness
                .ctx
                .memory(|memory| memory.has_focus(pane_id(pane)))
        {
            return Ok(());
        }
        d.key(Key::Tab)?;
    }
    Err(format!(
        "Keyboard focus could not reach {pane:?}: {}",
        state(d)
    ))
}

fn choose(d: &mut Driver<'_>, name: Option<char>) -> Result<(), String> {
    d.key_modified(Key::Quote, Modifiers::SHIFT)?;
    match name {
        Some('a') => d.key(Key::A),
        Some('b') => d.key(Key::B),
        None => d.key_modified(Key::Quote, Modifiers::SHIFT),
        Some(_) => Err("Semantic replay only names registers a and b".into()),
    }
}

fn undo(d: &mut Driver<'_>, expected: &ProjectDocument) -> Result<(), String> {
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    d.check(
        "One Undo restores the complete authored snapshot with a fresh revision",
        same_document(document(d)?, expected)?,
        json!(expected.revision_id()),
        state(d),
    )
}

fn unchanged(
    d: &mut Driver<'_>,
    expected: &ProjectDocument,
    accepted: &Arc<Captured>,
    label: &str,
) -> Result<(), String> {
    d.settled()?;
    d.check(
        label,
        document(d)? == expected && same_copy(d, None, accepted) && !d.app().copied.is_pending(),
        json!("unchanged revision and historical copy"),
        state(d),
    )
}

fn copied(d: &Driver<'_>, name: Option<char>) -> Result<Arc<Captured>, String> {
    match d
        .app()
        .copied
        .entries()
        .find(|(slot, _)| *slot == name.unwrap_or('"'))
        .map(|(_, value)| value)
    {
        Some(Content::Edited(copied)) => Ok(copied.clone()),
        _ => Err(format!(
            "Register {} has no edited cut",
            name.unwrap_or('"')
        )),
    }
}

fn same_copy(d: &Driver<'_>, name: Option<char>, expected: &Arc<Captured>) -> bool {
    copied(d, name)
        .is_ok_and(|actual| actual.id() == expected.id() && actual.slice() == expected.slice())
}

fn document<'a>(d: &'a Driver<'_>) -> Result<&'a ProjectDocument, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.document.as_ref())
        .ok_or_else(|| "Dot replay has no workspace".into())
}

fn same_document(actual: &ProjectDocument, expected: &ProjectDocument) -> Result<bool, String> {
    let mut actual = serde_json::to_value(actual).map_err(|error| error.to_string())?;
    actual["revision_id"] = json!(expected.revision_id());
    Ok(actual == serde_json::to_value(expected).map_err(|error| error.to_string())?)
}

fn state(d: &Driver<'_>) -> Value {
    let mut state = d.snapshot();
    state["semantic_repeat"] = d.app().semantic.snapshot().map_or(Value::Null, |snapshot| json!({
        "session":snapshot.session,"version":snapshot.version,"head":snapshot.head,"error":snapshot.error,
        "edit":snapshot.edit.as_ref().map(|edit| json!({"operation":format!("{:?}",edit.operation),"register":edit.register})),
    }));
    state
}

fn digit_key(digit: u8) -> Key {
    [
        Key::Num0,
        Key::Num1,
        Key::Num2,
        Key::Num3,
        Key::Num4,
        Key::Num5,
        Key::Num6,
        Key::Num7,
        Key::Num8,
        Key::Num9,
    ][usize::from(digit - b'0')]
}

fn keys(keys: &[Key]) -> Vec<Event> {
    keys.iter()
        .flat_map(|key| {
            [
                key_event(*key, Modifiers::NONE, true),
                key_event(*key, Modifiers::NONE, false),
            ]
        })
        .collect()
}
