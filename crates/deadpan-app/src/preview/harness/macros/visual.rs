//! Visible range intent, typed register replacement and one-Undo replay.

use super::*;
use deadpan_core::SemanticVisualSelection;

pub(super) fn run(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    empty_paste(d, baseline)?;
    edited_replacement(d, baseline)?;
    reverse_cut(d, baseline)?;
    original_replacement(d, baseline)?;
    existing_selection(d, baseline)?;
    selection_motions(d, baseline)?;
    delayed_selection(d, baseline)
}

fn selection(d: &Driver<'_>) -> Result<Option<SemanticVisualSelection>, String> {
    d.app().edit_range.semantic()
}

fn visual(anchor: i64, head: i64, extending: bool) -> Option<SemanticVisualSelection> {
    Some(SemanticVisualSelection::Time {
        anchor: ProjectFrame(anchor),
        head: ProjectFrame(head),
        extending,
    })
}

fn motion(d: &mut Driver<'_>, count: u64, key: Key) -> Result<(), String> {
    let mut events = count_keys(count)?;
    events.extend(keys(&[key]));
    d.events(
        "Move the visible range endpoint through native keys",
        events,
    )
}

fn empty_paste(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 20)?;
    let before_bank = bank(d)?;
    let before_history = history(d);
    for finished in [false, true] {
        d.key(Key::V)?;
        if finished {
            d.key(Key::V)?;
        }
        for action in ["p", "P", "paste", "paste-before"] {
            d.command("register r")?;
            match action {
                "p" => d.key(Key::P)?,
                "P" => d.key_modified(Key::P, Modifiers::SHIFT)?,
                command => d.command(command)?,
            }
            idle(d)?;
            d.check(
                "An empty Visual selection refuses paste without falling back to beat placement",
                same_document(document(d)?, baseline)?
                    && bank(d)? == before_bank
                    && history(d) == before_history
                    && selection(d)? == visual(20, 20, !finished)
                    && d.app()
                        .error
                        .as_deref()
                        .is_some_and(|error| error.contains("empty")),
                json!({"action":action,"finished":finished,"unchanged":true}),
                state(d),
            )?;
        }
        d.key(Key::Escape)?;
    }
    Ok(())
}

fn edited_replacement(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 20)?;
    let before_revision = d.revision();
    let before_history = history(d);
    let original_cursor = d.app().source_cursor;
    d.command("record s")?;
    d.key(Key::V)?;
    motion(d, 4, Key::L)?;
    d.command("register t")?;
    d.key(Key::Y)?;
    idle(d)?;
    d.check(
        "Recording Visual yank saves an exact range and finishes its visible selection without an edit",
        d.revision() == before_revision
            && history(d) == before_history
            && d.app().sequence_cursor == 24
            && d.app().source_cursor == original_cursor
            && selection(d)? == visual(20, 24, false)
            && d.app().macros.instruction_count() == 3
            && matches!(bank(d)?.entries.get(&register('t')?).map(AsRef::as_ref),
                Some(RegisterValue::Edited { slice }) if slice.range().start() == ProjectFrame(20)
                    && slice.range().end() == ProjectFrame(24)),
        json!({"selection":[20,24],"extending":false,"instructions":3,"history_unchanged":true}),
        state(d),
    )?;
    d.capture("A recorded Visual copy retains its visible four-frame selection")?;
    d.key(Key::Escape)?;
    d.check(
        "Escape records clearing the selection and keeps the macro draft active",
        selection(d)?.is_none()
            && d.app().macros.recording_name() == Some('s')
            && d.app().macros.instruction_count() == 4
            && d.app().sequence_cursor == 24,
        json!({"selection":null,"recording":"s","instructions":4}),
        state(d),
    )?;
    motion(d, 6, Key::L)?;
    d.key(Key::V)?;
    motion(d, 2, Key::L)?;
    d.key(Key::V)?;
    d.command("register t")?;
    let before = d.revision();
    d.key(Key::P)?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Recorded Visual paste replaces two frames with the four-frame copy and selects its new root",
        d.app().sequence_length() == 122
            && d.app().sequence_cursor == 30
            && d.app().selected_beat.is_some()
            && selection(d)?.is_none()
            && d.app().presentation.displayed_source_frame() == Some(SourceFrameId(20))
            && d.app().macros.instruction_count() == 9,
        json!({"frames":122,"Edit":30,"Original_picture":20,"instructions":9}),
        state(d),
    )?;
    d.capture("A recorded range replacement shows the inserted slice and its destination")?;
    d.key(Key::Q)?;
    wait_saved(d, 's')?;
    d.check(
        "The saved Visual workflow contains explicit selection, copy, clear and replacement intent",
        program(d, 's').is_some_and(|program| {
            matches!(
                program.instructions(),
                [
                    SemanticInstruction::BeginSelection,
                    SemanticInstruction::MoveFrames { .. },
                    SemanticInstruction::YankSelection { .. },
                    SemanticInstruction::ClearSelection,
                    SemanticInstruction::MoveFrames { .. },
                    SemanticInstruction::BeginSelection,
                    SemanticInstruction::MoveFrames { .. },
                    SemanticInstruction::FinishSelection,
                    SemanticInstruction::ReplaceSelection { .. },
                ]
            )
        }),
        json!({"instructions":9,"semantic_selection":true}),
        state(d),
    )?;
    undo(d, baseline)?;
    at(d, 40)?;
    let before = d.revision();
    execute(d, 's', Some(2))?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Counted Visual replay resolves both selections against the staged edit and commits once",
        d.app().sequence_length() == 124
            && d.app().sequence_cursor == 60
            && d.app().presentation.displayed_source_frame() == Some(SourceFrameId(40))
            && selection(d)?.is_none(),
        json!({"frames":124,"Edit":60,"Original_picture":40,"selection":null}),
        state(d),
    )?;
    d.capture("Two Visual copy and replacement repetitions form one undoable edit")?;
    undo(d, baseline)
}

fn reverse_cut(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 30)?;
    d.command("record n")?;
    d.key(Key::V)?;
    motion(d, 3, Key::H)?;
    let before = d.revision();
    d.key(Key::D)?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Recorded d cuts a backward Visual selection at its normalized join",
        d.app().sequence_length() == 117
            && d.app().sequence_cursor == 27
            && selection(d)?.is_none()
            && d.app().macros.instruction_count() == 3,
        json!({"frames":117,"Edit":27,"instructions":3}),
        state(d),
    )?;
    d.key(Key::Q)?;
    wait_saved(d, 'n')?;
    undo(d, baseline)?;
    at(d, 60)?;
    let before = d.revision();
    execute(d, 'n', Some(2))?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Counted backward selections cut six frames and retain the final historical slice",
        d.app().sequence_length() == 114
            && d.app().sequence_cursor == 54
            && matches!(bank(d)?.entries.get(&RegisterName::unnamed()).map(AsRef::as_ref),
                Some(RegisterValue::Edited { slice }) if slice.range().start() == ProjectFrame(54)
                    && slice.range().end() == ProjectFrame(57)),
        json!({"frames":114,"Edit":54,"last_copy":[54,57]}),
        state(d),
    )?;
    undo(d, baseline)
}

fn original_replacement(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 20)?;
    d.command("record i")?;
    d.key(Key::V)?;
    motion(d, 2, Key::L)?;
    d.command("register r")?;
    let before = d.revision();
    d.key_modified(Key::P, Modifiers::SHIFT)?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Visual P records the same replacement intent for a qualified Original register",
        d.app().sequence_length() == 121
            && d.app().sequence_cursor == 20
            && d.app().presentation.displayed_source_frame() == Some(SourceFrameId(10))
            && selection(d)?.is_none()
            && d.app().macros.instruction_count() == 3,
        json!({"frames":121,"Edit":20,"Original_picture":10,"instructions":3}),
        state(d),
    )?;
    d.key(Key::Q)?;
    wait_saved(d, 'i')?;
    undo(d, baseline)?;
    at(d, 40)?;
    let before_bank = bank(d)?;
    let before = d.revision();
    execute(d, 'i', Some(2))?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Two Original replacements share one edit and preserve the frozen input bank",
        d.app().sequence_length() == 122
            && d.app().sequence_cursor == 40
            && d.app().presentation.displayed_source_frame() == Some(SourceFrameId(10))
            && bank(d)? == before_bank,
        json!({"frames":122,"Edit":40,"Original_picture":10,"bank_unchanged":true}),
        state(d),
    )?;
    d.capture("Counted Original replacement keeps the inserted picture and Edit cursor aligned")?;
    undo(d, baseline)
}

fn existing_selection(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 60)?;
    d.key(Key::V)?;
    motion(d, 4, Key::H)?;
    d.key(Key::V)?;
    d.command("record j")?;
    d.command("register t")?;
    d.key(Key::Y)?;
    idle(d)?;
    d.command("record-stop")?;
    wait_saved(d, 'j')?;
    d.check(
        "Recording can start from an existing finished backward selection",
        selection(d)? == visual(60, 56, false)
            && program(d, 'j').is_some_and(|program| {
                matches!(
                    program.instructions(),
                    [SemanticInstruction::YankSelection { .. }]
                )
            }),
        json!({"anchor":60,"head":56,"extending":false,"instructions":1}),
        state(d),
    )?;
    at(d, 80)?;
    d.key(Key::V)?;
    motion(d, 3, Key::H)?;
    let before_history = history(d);
    let before_revision = d.revision();
    execute(d, 'j', None)?;
    idle(d)?;
    d.check(
        "A selection-dependent macro uses the invocation range and returns its oriented finished selection",
        selection(d)? == visual(80, 77, false)
            && d.app().sequence_cursor == 77
            && history(d) == before_history && d.revision() == before_revision
            && same_document(document(d)?, baseline)?,
        json!({"anchor":80,"head":77,"extending":false,"Edit":77,"history_unchanged":true}),
        state(d),
    )?;
    d.key(Key::Escape)?;
    let before_bank = bank(d)?;
    execute(d, 'j', None)?;
    idle(d)?;
    d.check(
        "A selection-dependent macro refuses absent selection without copying the selected beat",
        bank(d)? == before_bank
            && d.revision() == before_revision
            && selection(d)?.is_none()
            && d.app().error.is_some(),
        json!({"selection":null,"bank_unchanged":true,"refused":true}),
        state(d),
    )
}

fn selection_motions(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 20)?;
    let before_history = history(d);
    let before_revision = d.revision();
    d.command("record m")?;
    d.key(Key::V)?;
    d.key(Key::J)?;
    d.key_modified(Key::G, Modifiers::SHIFT)?;
    d.key(Key::K)?;
    d.chord(&[Key::G, Key::G])?;
    d.key(Key::V)?;
    d.key(Key::Q)?;
    wait_saved(d, 'm')?;
    d.check(
        "Beat and group-boundary motions record their semantic intent while extending a selection",
        selection(d)? == visual(20, 0, false)
            && d.app().sequence_cursor == 0
            && d.revision() == before_revision
            && history(d) == before_history
            && program(d, 'm').is_some_and(|program| {
                matches!(
                    program.instructions(),
                    [
                        SemanticInstruction::BeginSelection,
                        SemanticInstruction::MoveBeats { forward: true, .. },
                        SemanticInstruction::MoveScope { end: true },
                        SemanticInstruction::MoveBeats { forward: false, .. },
                        SemanticInstruction::MoveScope { end: false },
                        SemanticInstruction::FinishSelection,
                    ]
                )
            }),
        json!({"anchor":20,"head":0,"extending":false,"instructions":6,"history_unchanged":true}),
        state(d),
    )?;
    at(d, 50)?;
    let before_bank = bank(d)?;
    execute(d, 'm', None)?;
    idle(d)?;
    d.check(
        "A selection-only replay returns oriented endpoints without saving history or registers",
        selection(d)? == visual(50, 0, false)
            && d.app().sequence_cursor == 0
            && bank(d)? == before_bank && history(d) == before_history
            && d.revision() == before_revision && same_document(document(d)?, baseline)?,
        json!({"anchor":50,"head":0,"extending":false,"bank_unchanged":true,"history_unchanged":true}),
        state(d),
    )?;
    d.capture("A selection-only macro visibly returns its backward range without a timeline edit")?;
    d.key(Key::Escape)
}

fn delayed_selection(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 60)?;
    d.key(Key::V)?;
    motion(d, 4, Key::H)?;
    let entry_selection = selection(d)?;
    let revision = d.revision();
    let before_bank = bank(d)?;
    d.app_mut().feedback.hold_project_updates = true;
    execute(d, 'j', None)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    let update = loop {
        if let Some(update) = d.app().service.take_update()
            && update.saved_macro.as_ref().is_some_and(|receipt| {
                receipt.id.revision.as_str() == revision
                    && receipt.bank_version > before_bank.version
            })
        {
            break update;
        }
        if Instant::now() >= deadline {
            return Err("The deferred Visual copy receipt did not arrive".into());
        }
        d.step(
            "Withhold delivery while the genuine Visual copy saves",
            false,
        )?;
        d.wake
            .wait_until((Instant::now() + Duration::from_millis(16)).min(deadline));
    };
    // Change only selection state, then return it to the original state. The
    // genuine completion must publish its bank but cannot reclaim ownership.
    d.app_mut()
        .restore_macro_visual_selection(visual(60, 56, false))?;
    d.app_mut().reconcile_macro_recording();
    d.app_mut()
        .restore_macro_visual_selection(entry_selection.clone())?;
    d.app_mut().reconcile_macro_recording();
    d.app_mut().feedback.release_project_update = Some(update);
    d.app_mut().feedback.hold_project_updates = false;
    d.step(
        "Release the saved copy after independent Visual state changed",
        false,
    )?;
    idle(d)?;
    d.check(
        "Delayed bank-only completion cannot finish a selection whose ownership was relinquished",
        selection(d)? == entry_selection
            && d.app().sequence_cursor == 56 && d.revision() == revision
            && bank(d)?.version == before_bank.version + 1
            && same_document(document(d)?, baseline)?,
        json!({"anchor":60,"head":56,"extending":true,"bank_saved":true,"injection":"delivery timing and selection observations only"}),
        state(d),
    )?;
    d.key(Key::Escape)
}
