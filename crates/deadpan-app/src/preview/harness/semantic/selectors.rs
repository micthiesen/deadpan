//! Dot resolves the retained cut intent against a fresh native context.

use super::*;
use deadpan_core::{
    RegisterName, RegisterValue, SemanticInstruction, SemanticMotion, SemanticSelector,
};
use deadpan_store::registers::RegisterBank;
use std::num::NonZeroU32;

pub(super) fn run(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    motions_and_registers(d, baseline)?;
    visual(d, baseline)?;
    selected_beat(d, baseline)?;
    recorded(d, baseline)?;
    early_refusals(d, baseline)?;
    delayed(d, baseline)?;
    empty_children(d, baseline)
}

fn backward() -> SemanticSelector {
    SemanticSelector::Motion {
        motion: SemanticMotion::Frames {
            forward: false,
            count: NonZeroU32::new(5).unwrap(),
        },
    }
}

fn idle(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for(
        "Semantic cut receipt and register publication complete",
        |app| !app.service.is_busy() && !app.macros.is_pending() && !app.copied.is_pending(),
    )?;
    d.settled()
}

fn bank(d: &Driver<'_>) -> Result<RegisterBank, String> {
    ProjectStore::open(
        &d.app()
            .workspace
            .as_ref()
            .ok_or("Missing repeat workspace")?
            .path,
        AccessMode::ReadOnly,
    )
    .and_then(|store| store.registers())
    .map_err(|error| error.to_string())
}

fn name(value: Option<char>) -> Result<RegisterName, String> {
    value
        .map_or(Ok(RegisterName::unnamed()), RegisterName::new)
        .map_err(|error| error.to_string())
}

fn selector_candidate(
    d: &mut Driver<'_>,
    selector: SemanticSelector,
    register: Option<char>,
) -> Result<(), String> {
    let expected = RepeatableEdit::Cut(RepeatableCut::Selector(selector));
    d.check(
        "Repeat retains only the requested selector and register at the current saved head",
        d.app().semantic.snapshot().is_some_and(|snapshot| {
            snapshot.head.as_ref() == Some(document(d).unwrap().revision_id())
                && snapshot.error.is_none()
                && snapshot
                    .edit
                    .as_ref()
                    .is_some_and(|edit| edit.operation == expected && edit.register == register)
        }),
        json!({"selector":selector,"register":register}),
        state(d),
    )
}

fn saved(
    d: &mut Driver<'_>,
    before: &str,
    range: [i64; 2],
    frames: u64,
    selector: SemanticSelector,
    register: Option<char>,
) -> Result<(), String> {
    idle(d)?;
    let capture = copied(d, register)?;
    let store = ProjectStore::open(
        &d.app().workspace.as_ref().unwrap().path,
        AccessMode::ReadOnly,
    )
    .map_err(|error| error.to_string())?;
    let durable = store.snapshot().map_err(|error| error.to_string())?;
    let bank = store.registers().map_err(|error| error.to_string())?;
    let exact = |slot| matches!(bank.entries.get(&slot).map(AsRef::as_ref), Some(RegisterValue::Edited { slice }) if slice == capture.slice());
    d.check("Dot publishes one durable cut with exact historical contents and join cursor",
        durable == *document(d)? && d.app().sequence_length() == frames
            && d.app().sequence_cursor == u64::try_from(range[0]).map_err(|error| error.to_string())?
            && capture.slice().range().start() == ProjectFrame(range[0]) && capture.slice().range().end() == ProjectFrame(range[1])
            && capture.slice().revision_id().as_str() == before
            && exact(name(register)?) && exact(RegisterName::unnamed())
            && same_copy(d, None, &capture) && !d.app().edit_range.has_bounds(),
        json!({"range":range,"source_revision":before,"frames":frames,"selector":selector,"register":register}), state(d))?;
    selector_candidate(d, selector, register)
}

fn seed(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 20)?;
    choose(d, Some('a'))?;
    let before = d.revision();
    d.chord(&[Key::D, Key::Num5, Key::H])?;
    d.changed(&before)?;
    saved(d, &before, [15, 20], 115, backward(), Some('a'))?;
    undo(d, baseline)
}

fn motions_and_registers(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    seed(d, baseline)?;
    at(d, 30)?;
    let original_cursor = d.app().source_cursor;
    let selection = d.app().selected_beat.clone();
    let snapshot = d.app().semantic.snapshot().cloned();
    let before = document(d)?.clone();
    d.command("register b")?;
    d.chord(&[Key::Y, Key::Num2, Key::L])?;
    idle(d)?;
    d.check("A motion yank saves a different range while preserving the complete repeat candidate and both cursors",
        d.app().semantic.snapshot() == snapshot.as_ref() && document(d)? == &before
            && d.app().sequence_cursor == 30 && d.app().source_cursor == original_cursor && d.app().selected_beat == selection
            && copied(d, Some('b'))?.slice().range() == FrameRange::new(ProjectFrame(30), ProjectFrame(32)).map_err(|error| error.to_string())?,
        json!({"copy":[30,32],"repeat":"backward 5 frames to a","Edit":30}), state(d))?;
    let retained_b = copied(d, Some('b'))?;
    at(d, 40)?;
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    saved(d, &before, [35, 40], 115, backward(), Some('a'))?;
    d.check(
        "Dot re-resolves the requested direction and count after navigation and copy",
        same_copy(d, Some('b'), &retained_b)
            && d.app().presentation.displayed_source_frame() == Some(SourceFrameId(40)),
        json!({"cut":[35,40],"Original_picture":40,"yank_retained":true}),
        state(d),
    )?;
    let paint = scenarios::text_paint_visibility(d, "repeat cut 5f backward");
    d.check(
        "The compact footer paints the retained selector direction and requested count",
        !paint.is_empty() && paint.iter().all(|part| part["fully_visible"] == true),
        json!("repeat cut 5f backward"),
        json!(paint),
    )?;
    scenarios::footer_anchored(d, "The typed-repeat footer remains above the notice panel")?;
    d.capture("Dot repeats a backward five-frame cut at its new location")?;
    undo(d, baseline)?;

    at(d, 0)?;
    choose(d, Some('b'))?;
    let before = document(d)?.clone();
    let old_bank = bank(d)?;
    let candidate = d.app().semantic.snapshot().cloned();
    d.key(Key::Period)?;
    idle(d)?;
    d.check(
        "A failed backward repeat consumes its override without changing any saved state",
        document(d)? == &before
            && bank(d)? == old_bank
            && d.app().semantic.snapshot() == candidate.as_ref()
            && d.app().copied.selected_override().is_none()
            && d.app().sequence_cursor == 0
            && d.app().error.is_some(),
        json!({"unchanged":true,"override_consumed":true}),
        state(d),
    )?;
    at(d, 50)?;
    choose(d, Some('b'))?;
    let retained_a = copied(d, Some('a'))?;
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    saved(d, &before, [45, 50], 115, backward(), Some('b'))?;
    d.check(
        "A named override becomes the repeat destination while a remains unchanged",
        same_copy(d, Some('a'), &retained_a),
        json!("a retained; b saved"),
        state(d),
    )?;
    let retained_b = copied(d, Some('b'))?;
    undo(d, baseline)?;
    at(d, 60)?;
    choose(d, None)?;
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    saved(d, &before, [55, 60], 115, backward(), None)?;
    d.check(
        "An explicit unnamed repeat preserves both named slots",
        same_copy(d, Some('a'), &retained_a) && same_copy(d, Some('b'), &retained_b),
        json!("unnamed only"),
        state(d),
    )?;
    undo(d, baseline)
}

fn visual(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    for finished in [false, true] {
        if finished {
            at(d, 20)?;
            choose(d, Some('a'))?;
            let before = d.revision();
            d.chord(&[Key::Num5, Key::X])?;
            d.changed(&before)?;
            saved_cut(d, &before, 20, 25, 115, 5, Some('a'))?;
            undo(d, baseline)?;
        } else {
            seed(d, baseline)?;
        }
        at(d, 60)?;
        d.key(Key::V)?;
        d.chord(&[Key::Num3, Key::H])?;
        if finished {
            d.key(Key::V)?;
        }
        let before = d.revision();
        d.key(Key::Period)?;
        d.changed(&before)?;
        saved(
            d,
            &before,
            [57, 60],
            117,
            SemanticSelector::VisualSelection,
            Some('a'),
        )?;
        undo(d, baseline)?;
        at(d, 40)?;
        let old = document(d)?.clone();
        let old_bank = bank(d)?;
        let candidate = d.app().semantic.snapshot().cloned();
        d.key(Key::Period)?;
        idle(d)?;
        d.check("A saved Visual selector refuses absent current selection without reusing the old range length",
            document(d)? == &old && bank(d)? == old_bank && d.app().semantic.snapshot() == candidate.as_ref()
                && d.app().sequence_cursor == 40
                && d.app().error.as_deref() == Some("Select a new Visual range before repeating this cut."),
            json!({"former_selection":[57,60],"current_selection":null,"no_edit":true}), state(d))?;
        d.key(Key::V)?;
        if finished {
            d.key(Key::V)?;
        }
        let selection = d.app().edit_range.clone();
        choose(d, Some('b'))?;
        d.key(Key::Period)?;
        idle(d)?;
        d.check("An explicit empty Visual repeat preserves its orientation and saved candidate while consuming the override",
            document(d)? == &old && bank(d)? == old_bank && d.app().semantic.snapshot() == candidate.as_ref()
                && d.app().edit_range == selection && d.app().copied.selected_override().is_none()
                && d.app().error.as_deref() == Some("The Edit selection is empty. Move a boundary before repeating the cut."),
            json!({"finished":finished,"empty_selection_preserved":true,"all_saved_state_unchanged":true}), state(d))?;
        d.key(Key::Escape)?;
    }
    // Both ordinary Visual entrypoints must install explicit Visual intent.
    for command in [false, true] {
        at(d, 25)?;
        d.key(Key::V)?;
        d.chord(&[Key::Num4, Key::L])?;
        let before = d.revision();
        if command {
            d.command("delete")?;
        } else {
            d.key(Key::D)?;
        }
        d.changed(&before)?;
        saved(
            d,
            &before,
            [25, 29],
            116,
            SemanticSelector::VisualSelection,
            None,
        )?;
        undo(d, baseline)?;
        at(d, 70)?;
        d.key(Key::V)?;
        d.chord(&[Key::Num2, Key::L])?;
        let before = d.revision();
        d.key(Key::Period)?;
        d.changed(&before)?;
        saved(
            d,
            &before,
            [70, 72],
            118,
            SemanticSelector::VisualSelection,
            None,
        )?;
        undo(d, baseline)?;
    }
    Ok(())
}

fn selected_beat(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 30)?;
    let before = d.revision();
    d.key(Key::S)?;
    d.changed(&before)?;
    let split = document(d)?.clone();
    at(d, 10)?;
    let selected = d.app().selected_beat.clone();
    let before = d.revision();
    d.chord(&[Key::D, Key::D])?;
    d.changed(&before)?;
    saved(
        d,
        &before,
        [0, 30],
        90,
        SemanticSelector::SelectedBeat,
        None,
    )?;
    undo(d, &split)?;
    at(d, 70)?;
    let next = d.app().selected_beat.clone();
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    saved(
        d,
        &before,
        [30, 120],
        30,
        SemanticSelector::SelectedBeat,
        None,
    )?;
    d.check("Whole-beat dot uses the newly selected child's exact identity and length",
        next != selected && matches!(copied(d, None)?.slice().selection(), deadpan_core::SliceCaptureSelection::Child { node } if Some(node) == next.as_ref()),
        json!({"first_child":selected,"repeated_child":next,"copied":[30,120]}), state(d))?;
    undo(d, &split)?;
    undo(d, baseline)
}

fn recorded(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    seed(d, baseline)?;
    at(d, 50)?;
    d.command("record z")?;
    choose(d, Some('b'))?;
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    saved(d, &before, [45, 50], 115, backward(), Some('b'))?;
    d.check(
        "A recorded dot appends one successful cut instruction",
        d.app().macros.recording() && d.app().macros.instruction_count() == 1,
        json!(1),
        state(d),
    )?;
    d.key(Key::Q)?;
    idle(d)?;
    let bank = bank(d)?;
    let expected = SemanticInstruction::Cut {
        selector: backward(),
        register: name(Some('b'))?,
    };
    d.check("The saved macro contains the effective semantic cut rather than a dynamic dot or absolute range",
        matches!(bank.entries.get(&name(Some('z'))?).map(AsRef::as_ref), Some(RegisterValue::Macro { program }) if program.instructions() == [expected]),
        json!({"selector":backward(),"register":"b","instructions":1}), state(d))?;
    undo(d, baseline)?;
    at(d, 80)?;
    let before = d.revision();
    d.command("macro z")?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "The recorded dot replays at a new cursor in one transaction",
        d.app().sequence_length() == 115
            && d.app().sequence_cursor == 75
            && copied(d, Some('b'))?.slice().range()
                == FrameRange::new(ProjectFrame(75), ProjectFrame(80))
                    .map_err(|error| error.to_string())?,
        json!({"cut":[75,80],"frames":115}),
        state(d),
    )?;
    undo(d, baseline)
}

fn early_refusals(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 80)?;
    choose(d, Some('a'))?;
    let retained_a = copied(d, Some('a'))?;
    let visible = document(d)?.clone();
    let revision = d.revision();
    d.app_mut().feedback.hold_project_updates = true;
    d.command("macro z")?;
    let deadline = Instant::now() + Duration::from_secs(15);
    let update = loop {
        if let Some(update) = d.app().service.take_update()
            && update.saved_macro.as_ref().is_some_and(|receipt| {
                receipt.id.revision.as_str() == revision && receipt.committed().is_some()
            })
        {
            break update;
        }
        if Instant::now() >= deadline {
            return Err("The queued macro Run receipt did not arrive for the dot refusal".into());
        }
        d.step(
            "Withhold the genuine Run receipt before refusing a newer dot",
            false,
        )?;
        d.wake
            .wait_until((Instant::now() + Duration::from_millis(16)).min(deadline));
    };
    let committed = update
        .workspace
        .as_ref()
        .ok_or("Macro Run did not publish its workspace")?
        .document
        .clone();
    let committed_bank = bank(d)?;
    let cursor = d.app().sequence_cursor;
    let selected = d.app().selected_beat.clone();
    d.check(
        "The one-shot a override is still armed while the genuine Run receipt awaits delivery",
        d.app().macros.is_pending() && d.app().copied.selected_override() == Some(Some('a')),
        json!({"macro_pending":true,"override":"a"}),
        state(d),
    )?;
    d.key(Key::Period)?;
    d.check("Dot refused behind a pending Run consumes only its one-shot override",
        d.app().copied.selected_override().is_none() && d.app().macros.is_pending()
            && document(d)? == &visible && bank(d)? == committed_bank
            && d.app().sequence_cursor == cursor && d.app().selected_beat == selected
            && same_copy(d, Some('a'), &retained_a)
            && d.app().error.as_deref().is_some_and(|error| error.contains("pending macro")),
        json!({"override_consumed":true,"original_receipt_pending":true,"UI_unchanged":true,"bank_unchanged":true}), state(d))?;
    d.app_mut().feedback.release_project_update = Some(update);
    d.app_mut().feedback.hold_project_updates = false;
    d.step(
        "Deliver the exact Run receipt retained across the refused dot",
        false,
    )?;
    idle(d)?;
    d.check(
        "The original Run receipt still installs its exact document, b copy and cursor once",
        document(d)? == committed.as_ref()
            && bank(d)? == committed_bank
            && !d.app().macros.is_pending()
            && d.app().sequence_cursor == 75
            && same_copy(d, Some('a'), &retained_a)
            && copied(d, Some('b'))?.slice().range()
                == FrameRange::new(ProjectFrame(75), ProjectFrame(80))
                    .map_err(|error| error.to_string())?
            && d.app().copied.selected_override().is_none(),
        json!({"frames":115,"Edit":75,"destination":"b","a_retained":true}),
        state(d),
    )?;
    undo(d, baseline)?;

    seed(d, baseline)?;
    at(d, 20)?;
    d.command("record v")?;
    let motions: Vec<_> = (0..deadpan_core::MAX_SEMANTIC_PROGRAM_INSTRUCTIONS)
        .map(|index| if index % 2 == 0 { Key::H } else { Key::L })
        .collect();
    d.events(
        "Fill a recording with 1024 ordinary keyboard frame motions",
        keys(&motions),
    )?;
    idle(d)?;
    choose(d, Some('a'))?;
    let before = document(d)?.clone();
    let old_bank = bank(d)?;
    let candidate = d.app().semantic.snapshot().cloned();
    let selected = d.app().selected_beat.clone();
    d.check(
        "The production recording reaches its exact instruction limit with an armed override",
        d.app().macros.recording_name() == Some('v')
            && d.app().macros.instruction_count() == 1024
            && d.app().sequence_cursor == 20
            && d.app().copied.selected_override() == Some(Some('a')),
        json!({"recording":"v","instructions":1024,"override":"a"}),
        state(d),
    )?;
    d.key(Key::Period)?;
    idle(d)?;
    d.check("Dot refused at the recording limit consumes its override while preserving draft and saved state",
        d.app().copied.selected_override().is_none() && d.app().macros.recording_name() == Some('v')
            && d.app().macros.instruction_count() == 1024 && !d.app().macros.is_pending()
            && document(d)? == &before && bank(d)? == old_bank && d.app().semantic.snapshot() == candidate.as_ref()
            && d.app().sequence_cursor == 20 && d.app().selected_beat == selected
            && d.app().error.as_deref().is_some_and(|error| error.contains("1024 instructions")),
        json!({"override_consumed":true,"instructions":1024,"no_edit":true,"candidate_preserved":true}), state(d))?;
    d.key(Key::Q)?;
    idle(d)?;
    d.check("Saving the full draft proves the refused dot did not append or replace an instruction",
        matches!(bank(d)?.entries.get(&name(Some('v'))?).map(AsRef::as_ref), Some(RegisterValue::Macro { program })
            if program.instructions().len() == 1024 && program.instructions().iter().enumerate().all(|(index, instruction)|
                matches!(instruction, SemanticInstruction::MoveFrames { forward, count } if *forward == (index % 2 == 1) && count.get() == 1)))
            && document(d)? == &before && d.app().semantic.snapshot() == candidate.as_ref(),
        json!({"macro":"v","instructions":"1024 alternating single-frame motions","dot_recorded":false}), state(d))
}

fn delayed(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 60)?;
    let before = d.revision();
    d.key(Key::S)?;
    d.changed(&before)?;
    let split = document(d)?.clone();
    seed(d, &split)?;
    at(d, 80)?;
    let retained_child = d.app().selected_beat.clone();
    at(d, 20)?;
    let revision = d.revision();
    let old_bank = bank(d)?;
    d.app_mut().feedback.hold_project_updates = true;
    d.key(Key::Period)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    let update = loop {
        if let Some(update) = d.app().service.take_update()
            && update.saved_macro.as_ref().is_some_and(|receipt| {
                receipt.id.revision.as_str() == revision && receipt.committed().is_some()
            })
        {
            break update;
        }
        if Instant::now() >= deadline {
            return Err("The genuine deferred dot receipt did not arrive".into());
        }
        d.step(
            "Withhold only the real dot receipt while its transaction commits",
            false,
        )?;
        d.wake
            .wait_until((Instant::now() + Duration::from_millis(16)).min(deadline));
    };
    d.app_mut().sequence_cursor = 80;
    d.app_mut().selected_beat = retained_child.clone();
    d.app_mut().reconcile_macro_recording();
    d.app_mut().sequence_cursor = 20;
    d.app_mut().reconcile_macro_recording();
    d.app_mut().feedback.release_project_update = Some(update);
    d.app_mut().feedback.hold_project_updates = false;
    d.step(
        "Release dot after pointer observations permanently relinquished its context",
        false,
    )?;
    idle(d)?;
    let current = ProjectStore::open(
        &d.app().workspace.as_ref().unwrap().path,
        AccessMode::ReadOnly,
    )
    .and_then(|store| store.snapshot())
    .map_err(|error| error.to_string())?;
    d.check("A late dot installs the committed document and copy without reclaiming the cursor or selected beat",
        current == *document(d)? && d.app().sequence_length() == 115 && d.app().sequence_cursor == 20
            && d.app().selected_beat == retained_child && !d.app().edit_range.has_bounds()
            && bank(d)?.version == old_bank.version + 1
            && copied(d, Some('a'))?.slice().range() == FrameRange::new(ProjectFrame(15), ProjectFrame(20)).map_err(|error| error.to_string())?,
        json!({"cut":[15,20],"Edit":20,"selected":retained_child,"saved":true}), state(d))?;
    selector_candidate(d, backward(), Some('a'))?;
    undo(d, &split)?;
    undo(d, baseline)
}

fn empty_children(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    use deadpan_core::{BeatNode, Command, CommandRequest, NodeId, RevisionId, Subtree};
    use std::collections::BTreeMap;
    let path = d
        .app()
        .workspace
        .as_ref()
        .ok_or("Missing empty-repeat fixture")?
        .path
        .clone();
    d.app().service.submit(ProjectRequest::Close)?;
    d.wait_for(
        "Release the writer before adding adjacent empty beat fixtures",
        |app| app.workspace.is_none() && !app.service.is_busy(),
    )?;
    let mut store =
        ProjectStore::open(&path, AccessMode::ReadWrite).map_err(|error| error.to_string())?;
    let mut ids = Vec::new();
    for index in 0..2 {
        let before = store.snapshot().map_err(|error| error.to_string())?;
        let id = NodeId::new(format!("dot-empty-{index}")).map_err(|error| error.to_string())?;
        store
            .commit(&CommandRequest {
                project_id: before.project_id().clone(),
                expected_revision: before.revision_id().clone(),
                new_revision: RevisionId::new(format!("dot-empty-insert-{index}"))
                    .map_err(|error| error.to_string())?,
                command: Command::Insert {
                    parent: before.root().clone(),
                    index,
                    subtree: Subtree {
                        root: id.clone(),
                        nodes: BTreeMap::from([(
                            id.clone(),
                            BeatNode::sequence(format!("Dot empty {index}"), vec![]),
                        )]),
                        overrides: BTreeMap::new(),
                        gap_overrides: BTreeMap::new(),
                    },
                },
            })
            .map_err(|error| error.to_string())?;
        ids.push(id);
    }
    let fixture = store.snapshot().map_err(|error| error.to_string())?;
    drop(store);
    d.app().service.submit(ProjectRequest::Open(path.clone()))?;
    d.wait_for("Open the real adjacent-empty-beat repeat fixture", |app| {
        !app.service.is_busy()
            && app
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.path == path)
    })?;
    at(d, 0)?;
    d.chord(&[Key::K, Key::K])?;
    d.check(
        "Beat navigation selects the first empty sibling at the shared boundary",
        d.app().selected_beat.as_ref() == ids.first() && d.app().sequence_cursor == 0,
        json!(ids[0]),
        state(d),
    )?;
    let before = d.revision();
    d.command("record w")?;
    d.command("delete")?;
    d.changed(&before)?;
    saved(
        d,
        &before,
        [0, 0],
        120,
        SemanticSelector::SelectedBeat,
        None,
    )?;
    d.key(Key::Q)?;
    idle(d)?;
    d.check("Recording :delete on an empty child retains an explicit selected-beat cut",
        matches!(bank(d)?.entries.get(&name(Some('w'))?).map(AsRef::as_ref), Some(RegisterValue::Macro { program })
            if matches!(program.instructions(), [SemanticInstruction::Cut { selector: SemanticSelector::SelectedBeat, register }] if *register == RegisterName::unnamed()))
            && d.app().selected_beat.as_ref() == ids.get(1),
        json!({"macro":"w","selector":"selected_beat","next_empty":ids[1]}), state(d))?;
    let first_cut = document(d)?.clone();
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    saved(
        d,
        &before,
        [0, 0],
        120,
        SemanticSelector::SelectedBeat,
        None,
    )?;
    d.check("Dot removes the next explicit empty sibling without stealing the nonempty child at the same boundary",
        !document(d)?.nodes().contains_key(&ids[0]) && !document(d)?.nodes().contains_key(&ids[1])
            && matches!(copied(d, None)?.slice().selection(), deadpan_core::SliceCaptureSelection::Child { node } if node == &ids[1])
            && same_document(document(d)?, baseline)?,
        json!({"removed":ids,"frames":120,"copied_duration":0}), state(d))?;
    undo(d, &first_cut)?;
    undo(d, &fixture)
}
