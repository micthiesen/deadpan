//! Record and replay real copy/paste operations with exact native selection.

use super::*;

pub(super) fn run(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    edited(d, baseline)?;
    original(d, baseline)?;
    whole_original(d, baseline)
}

fn whole_original(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 20)?;
    let before_bank = bank(d)?;
    d.command("record z")?;
    let before = d.revision();
    d.chord(&[Key::Comma, Key::I])?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Whole-Original reuse records one semantic instruction and leaves copies unchanged",
        d.app().sequence_length() == 240
            && d.app().macros.instruction_count() == 1
            && bank(d)? == before_bank,
        json!({"frames":240,"instructions":1,"bank_unchanged":true}),
        state(d),
    )?;
    d.command("record-stop")?;
    wait_saved(d, 'z')?;
    d.check(
        "Whole-Original macro retains asset identity without a moment register",
        program(d, 'z').is_some_and(|program| {
            matches!(
                program.instructions(),
                [SemanticInstruction::InsertOriginal { .. }]
            )
        }),
        json!(["insert_original"]),
        state(d),
    )?;
    undo(d, baseline)?;
    at(d, 20)?;
    let before = d.revision();
    execute(d, 'z', Some(2))?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Counted whole-Original macro inserts two full sources in one Undo",
        d.app().sequence_length() == 360,
        json!({"frames":360}),
        state(d),
    )?;
    undo(d, baseline)?;
    at(d, 20)?;
    d.command("source")?;
    d.key(Key::Escape)?;
    let captured = d.app().capture_original_insert_target()?;
    d.check(
        "Original-pane reuse captures its own current view and focus",
        captured.matches(d.app()),
        json!(true),
        state(d),
    )?;
    let pane = d.app().pane;
    d.app_mut().pane = if pane == Pane::Inspector {
        Pane::Viewer
    } else {
        Pane::Inspector
    };
    let changed_pane_refused = !captured.matches(d.app());
    d.app_mut().pane = pane;
    d.app_mut().sequence_cursor += 1;
    let changed_cursor_refused = !captured.matches(d.app());
    d.app_mut().sequence_cursor -= 1;
    d.app_mut().view = View::Sequence;
    let changed_view_refused = !captured.matches(d.app());
    d.app_mut().view = View::Source;
    d.app_mut().pane = Pane::Sources;
    let focus_capture = d.app().capture_original_insert_target()?;
    let selected_sound = d.app().selected_sound.clone();
    d.app_mut().selected_sound =
        Some(deadpan_core::AssetId::new("focus-witness").map_err(|error| error.to_string())?);
    let changed_focus_refused = !focus_capture.matches(d.app());
    d.app_mut().selected_sound = selected_sound;
    d.app_mut().pane = pane;
    d.check(
        "A captured Original reuse refuses later pane, view or Edit cursor changes",
        changed_pane_refused
            && changed_cursor_refused
            && changed_view_refused
            && changed_focus_refused
            && captured.matches(d.app()),
        json!({"pane":true,"view":true,"cursor":true,"sound_focus":true}),
        state(d),
    )?;
    let original_cursor = d.app().source_cursor;
    let before = d.revision();
    d.chord(&[Key::Comma, Key::I])?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Whole-Original insertion succeeds from the Original pane without losing its cursor",
        d.app().sequence_length() == 240 && d.app().source_cursor == original_cursor,
        json!({"frames":240,"Original":original_cursor}),
        state(d),
    )?;
    at(d, 20)?;
    undo(d, baseline)
}

fn edited(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 20)?;
    let before_history = history(d);
    let before_revision = d.revision();
    let before_bank = bank(d)?.version;
    let selected = d.app().selected_beat.clone();
    let original_cursor = d.app().source_cursor;
    d.command("record e")?;
    d.command("register f")?;
    d.chord(&[Key::Y, Key::Y])?;
    idle(d)?;
    let copied = bank(d)?;
    d.check(
        "Recording a beat yank saves the named and unnamed copy without a timeline revision or cursor change",
        d.revision() == before_revision
            && history(d) == before_history
            && copied.version == before_bank + 1
            && copied.entries.get(&register('f')?) == copied.entries.get(&RegisterName::unnamed())
            && matches!(copied.entries.get(&register('f')?).map(AsRef::as_ref),
                Some(RegisterValue::Edited { slice }) if matches!(slice.selection(),
                    deadpan_core::SliceCaptureSelection::Child { node } if Some(node) == selected.as_ref()))
            && d.app().sequence_cursor == 20
            && d.app().source_cursor == original_cursor
            && d.app().selected_beat == selected
            && d.app().macros.instruction_count() == 1,
        json!({"revision":before_revision,"Edit":20,"history":before_history,"instructions":1}),
        state(d),
    )?;
    let copied_document = document(d)?.clone();
    d.check(
        "The recorded-paste refusal fixture uses an empty register",
        !copied.entries.contains_key(&register('w')?),
        json!({"register":"w","empty":true}),
        state(d),
    )?;
    d.command("register w")?;
    d.key(Key::P)?;
    idle(d)?;
    d.check(
        "An empty named register refuses a recorded paste without fallback or a recorded instruction",
        document(d)? == &copied_document
            && bank(d)? == copied
            && d.app().selected_beat == selected
            && d.app().sequence_cursor == 20
            && d.app().macros.instruction_count() == 1
            && d.app().macros.recording_name() == Some('e')
            && d.app().error.as_deref().is_some_and(|error| error.contains("Register w is empty")),
        json!({"register":"w","unchanged":true,"instructions":1,"recording":"e"}),
        state(d),
    )?;
    d.command("register f")?;
    let before = d.revision();
    d.key(Key::P)?;
    d.changed(&before)?;
    idle(d)?;
    let after_first = document(d)?.clone();
    d.check(
        "Recorded p inserts the selected register after the selected beat and selects its fresh root",
        d.app().sequence_length() == 240
            && d.app().sequence_cursor == 120
            && d.app().selected_beat != selected
            && d.app().presentation.displayed_source_frame() == Some(SourceFrameId(0))
            && d.app().macros.instruction_count() == 2,
        json!({"frames":240,"Edit":120,"Original_picture":0,"instructions":2}),
        state(d),
    )?;
    d.command("register f")?;
    let before = d.revision();
    d.key_modified(Key::P, Modifiers::SHIFT)?;
    d.changed(&before)?;
    idle(d)?;
    recording_paint(d, 'e', 3)?;
    d.check(
        "Recorded P inserts before the selected pasted beat while keeping the insertion boundary",
        d.app().sequence_length() == 360 && d.app().sequence_cursor == 120,
        json!({"frames":360,"Edit":120}),
        state(d),
    )?;
    d.capture(
        "Recording a beat copy and both paste directions shows the selected inserted footage",
    )?;
    d.key(Key::Q)?;
    wait_saved(d, 'e')?;
    d.check(
        "The saved reuse macro contains relative typed yank and paste intent",
        program(d, 'e').is_some_and(|program| {
            matches!(program.instructions(), [
            SemanticInstruction::Yank { selector: deadpan_core::SemanticSelector::SelectedBeat, register: yank },
            SemanticInstruction::Paste { register: after, before: false },
            SemanticInstruction::Paste { register: before, before: true },
        ] if yank.as_char() == 'f' && after == yank && before == yank)
        }),
        json!(["yank selected beat to f", "paste f after", "paste f before"]),
        state(d),
    )?;
    undo(d, &after_first)?;
    undo(d, baseline)?;
    at(d, 17)?;
    let before = d.revision();
    execute(d, 'e', Some(2))?;
    d.changed(&before)?;
    idle(d)?;
    let repeated = document(d)?.clone();
    d.check(
        "A counted reuse macro yanks the staged selected beat and pastes four independent copies in one edit",
        d.app().sequence_length() == 600
            && d.app().sequence_cursor == 240
            && d.app().presentation.displayed_source_frame() == Some(SourceFrameId(0))
            && d.app().selected_beat.is_some(),
        json!({"frames":600,"Edit":240,"Original_picture":0}),
        state(d),
    )?;
    d.capture("Two macro repetitions retain editable copied groups and one Undo")?;
    undo(d, baseline)?;
    let before = d.revision();
    d.key_modified(Key::R, Modifiers::CTRL)?;
    d.changed(&before)?;
    d.check(
        "Redo restores every copied structure with a fresh outer revision",
        same_document(document(d)?, &repeated)? && d.revision() != repeated.revision_id().as_str(),
        json!({"complete_document_restored":true,"fresh_revision":true}),
        state(d),
    )?;
    undo(d, baseline)
}

fn original(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    d.command("source")?;
    d.key(Key::Escape)?;
    d.chord(&[Key::G, Key::G])?;
    d.events(
        "Navigate to the Original slice in point",
        keys(&[Key::Num1, Key::Num0, Key::L]),
    )?;
    d.key(Key::V)?;
    d.events(
        "Select three exact Original frames",
        keys(&[Key::Num3, Key::L]),
    )?;
    d.command("register r")?;
    d.key(Key::Y)?;
    idle(d)?;
    d.check(
        "Original register r retains exactly the selected measured ordinals",
        matches!(bank(d)?.entries.get(&register('r')?).map(AsRef::as_ref),
            Some(RegisterValue::Original { ordinals, .. }) if *ordinals == (10..13)),
        json!({"Original":[10,13]}),
        state(d),
    )?;
    at(d, 20)?;
    d.command("record o")?;
    d.command("register r")?;
    let before = d.revision();
    d.key(Key::P)?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Recording an Original paste uses its qualified source range and selects the inserted picture",
        d.app().sequence_length() == 123
            && d.app().sequence_cursor == 120
            && d.app().presentation.displayed_source_frame() == Some(SourceFrameId(10))
            && d.app().macros.instruction_count() == 1,
        json!({"frames":123,"Edit":120,"Original_picture":10,"instructions":1}),
        state(d),
    )?;
    d.command("record-stop")?;
    wait_saved(d, 'o')?;
    undo(d, baseline)?;
    at(d, 20)?;
    let before_bank = bank(d)?;
    let before = d.revision();
    execute(d, 'o', Some(3))?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Three Original pastes follow each newly selected copy and do not rewrite the source register bank",
        d.app().sequence_length() == 129
            && d.app().sequence_cursor == 126
            && d.app().presentation.displayed_source_frame() == Some(SourceFrameId(10))
            && bank(d)? == before_bank,
        json!({"frames":129,"Edit":126,"Original_picture":10,"bank_unchanged":true}),
        state(d),
    )?;
    d.capture("A counted macro reuses a measured Original slice as three editable beats")?;
    undo(d, baseline)
}
