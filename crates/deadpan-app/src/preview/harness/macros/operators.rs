//! Typed operator motions through the production router, store and renderer.

use super::*;
use deadpan_core::{SemanticMotion, SemanticSelector};

pub(super) fn run(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    ranges(d, baseline)?;
    beat_motions(d, baseline)?;
    recorded(d, baseline)?;
    refused(d, baseline)?;
    escaped_pending(d, baseline)?;
    refresh_failure_absence(d, baseline)
}

fn copied_range(d: &Driver<'_>, name: char, start: i64, end: i64) -> Result<bool, String> {
    Ok(
        matches!(bank(d)?.entries.get(&register(name)?).map(AsRef::as_ref),
        Some(RegisterValue::Edited { slice }) if slice.range().start() == ProjectFrame(start)
            && slice.range().end() == ProjectFrame(end)),
    )
}

fn ranges(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 20)?;
    let old_history = history(d);
    let old_bank = bank(d)?;
    let selected = d.app().selected_beat.clone();
    d.command("register v")?;
    d.key(Key::Y)?;
    d.check(
        "The first y waits visibly without copying or moving either clock",
        d.app().bindings.operator_pending()
            && bank(d)? == old_bank
            && d.app().sequence_cursor == 20
            && history(d) == old_history,
        json!({"pending":"y","Edit":20,"writes":0}),
        state(d),
    )?;
    d.capture("A pending yank teaches motion keys while keeping the destination picture visible")?;
    d.chord(&[Key::Num5, Key::L])?;
    idle(d)?;
    d.check(
        "y5l copies the next five frames without moving the cursor, beat or history",
        copied_range(d, 'v', 20, 25)?
            && d.app().sequence_cursor == 20
            && d.app().selected_beat == selected
            && history(d) == old_history
            && same_document(document(d)?, baseline)?,
        json!({"copied":[20,25],"Edit":20,"history_unchanged":true}),
        state(d),
    )?;
    d.command("register v")?;
    let before = d.revision();
    d.chord(&[Key::D, Key::Num5, Key::H])?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "d5h cuts the preceding range once and displays the exact joined frame",
        copied_range(d, 'v', 15, 20)?
            && d.app().sequence_length() == 115
            && d.app().sequence_cursor == 15
            && d.app().presentation.displayed_source_frame() == Some(SourceFrameId(20)),
        json!({"cut":[15,20],"frames":115,"Edit":15,"Original_picture":20}),
        state(d),
    )?;
    d.capture("A backward motion cut displays its join and the copied slice")?;
    undo(d, baseline)?;
    at(d, 20)?;
    d.command("register v")?;
    d.key(Key::Y)?;
    d.key_modified(Key::G, Modifiers::SHIFT)?;
    idle(d)?;
    d.check(
        "yG copies to the exclusive group end while preserving the cursor",
        copied_range(d, 'v', 20, 120)? && d.app().sequence_cursor == 20,
        json!({"copied":[20,120],"Edit":20}),
        state(d),
    )?;
    let before = d.revision();
    d.chord(&[Key::D, Key::G, Key::G])?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "dgg resolves to the group start and cuts one half-open interval",
        d.app().sequence_length() == 100
            && d.app().sequence_cursor == 0
            && d.app().presentation.displayed_source_frame() == Some(SourceFrameId(20)),
        json!({"cut":[0,20],"frames":100,"Original_picture":20}),
        state(d),
    )?;
    undo(d, baseline)?;
    at(d, 20)?;
    let before = d.revision();
    d.chord(&[Key::D, Key::D])?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "dd cuts the explicit whole beat independently of its interior cursor",
        d.app().sequence_length() == 0 && d.app().selected_beat.is_none(),
        json!({"frames":0,"selected":null}),
        state(d),
    )?;
    undo(d, baseline)
}

fn beat_motions(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 20)?;
    let before = d.revision();
    d.key(Key::S)?;
    d.changed(&before)?;
    idle(d)?;
    let split_once = document(d)?.clone();
    at(d, 50)?;
    let before = d.revision();
    d.key(Key::S)?;
    d.changed(&before)?;
    idle(d)?;
    let split_twice = document(d)?.clone();
    at(d, 25)?;
    d.command("register v")?;
    d.chord(&[Key::Y, Key::J])?;
    idle(d)?;
    d.check(
        "yj resolves the next direct beat boundary without moving selection",
        copied_range(d, 'v', 25, 50)? && d.app().sequence_cursor == 25,
        json!({"copied":[25,50],"Edit":25}),
        state(d),
    )?;
    d.command("register v")?;
    d.chord(&[Key::Y, Key::K])?;
    idle(d)?;
    d.check(
        "yk resolves the previous direct beat boundary",
        copied_range(d, 'v', 0, 25)? && d.app().sequence_cursor == 25,
        json!({"copied":[0,25],"Edit":25}),
        state(d),
    )?;
    let before = d.revision();
    d.chord(&[Key::D, Key::J])?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "dj commits the motion range and preserves the exact suffix picture",
        d.app().sequence_length() == 95
            && d.app().sequence_cursor == 25
            && d.app().presentation.displayed_source_frame() == Some(SourceFrameId(50)),
        json!({"frames":95,"Edit":25,"Original_picture":50}),
        state(d),
    )?;
    undo(d, &split_twice)?;
    undo(d, &split_once)?;
    undo(d, baseline)
}

fn recorded(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 20)?;
    d.command("record z")?;
    d.command("register v")?;
    d.chord(&[Key::Y, Key::Num3, Key::L])?;
    idle(d)?;
    d.command("register v")?;
    let before = d.revision();
    d.chord(&[Key::Num2, Key::D, Key::L])?;
    d.changed(&before)?;
    idle(d)?;
    d.key(Key::Q)?;
    wait_saved(d, 'z')?;
    d.check("Recording stores requested operator selectors rather than absolute range endpoints",
        program(d, 'z').is_some_and(|program| matches!(program.instructions(), [
            SemanticInstruction::Yank { selector: SemanticSelector::Motion { motion: SemanticMotion::Frames { forward: true, count: a } }, .. },
            SemanticInstruction::Cut { selector: SemanticSelector::Motion { motion: SemanticMotion::Frames { forward: true, count: b } }, .. },
        ] if a.get() == 3 && b.get() == 2)),
        json!(["y3l","2dl"]), state(d))?;
    undo(d, baseline)?;
    at(d, 40)?;
    let before = d.revision();
    execute(d, 'z', Some(2))?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Counted operator macros resolve each new staged range and commit one Undo",
        d.app().sequence_length() == 116
            && d.app().sequence_cursor == 40
            && d.app().presentation.displayed_source_frame() == Some(SourceFrameId(44)),
        json!({"frames":116,"Edit":40,"Original_picture":44}),
        state(d),
    )?;
    d.capture("Counted operator macros retain their requested motion and one Undo")?;
    undo(d, baseline)
}

fn refused(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 0)?;
    let old_bank = bank(d)?;
    let old_history = history(d);
    for input in [
        vec![Key::Y, Key::H],
        vec![Key::Num3, Key::D, Key::Num2, Key::L],
    ] {
        d.command("register v")?;
        d.events(
            "Refuse an empty range or conflicting operator counts",
            keys(&input),
        )?;
        idle(d)?;
        d.check(
            "A refused operator leaves all document, bank and history state unchanged",
            same_document(document(d)?, baseline)?
                && bank(d)? == old_bank
                && history(d) == old_history
                && d.app().error.is_some()
                && d.app().sequence_cursor == 0
                && d.app().copied.selected().is_none(),
            json!({"unchanged":true,"consumed_register":true}),
            state(d),
        )?;
    }
    at(d, 20)?;
    d.command("register v")?;
    d.key(Key::Y)?;
    d.app_mut().sequence_cursor = 21;
    d.events("Observe a changed pending-operator cursor", Vec::new())?;
    d.app_mut().sequence_cursor = 20;
    d.events(
        "Return the cursor without reviving the captured operator",
        Vec::new(),
    )?;
    d.key(Key::L)?;
    idle(d)?;
    d.check(
        "A pending operator cannot revive after its cursor changes and returns",
        same_document(document(d)?, baseline)?
            && bank(d)? == old_bank
            && history(d) == old_history
            && d.app().sequence_cursor == 20
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("context changed")),
        json!({"unchanged":true,"stale_prefix_refused":true}),
        state(d),
    )
}

fn escaped_pending(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 20)?;
    let revision = d.revision();
    d.app_mut().feedback.hold_project_updates = true;
    d.chord(&[Key::D, Key::Num5, Key::H])?;
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
            return Err("The deferred operator cut receipt did not arrive".into());
        }
        d.step(
            "Withhold delivery of a genuine non-recording operator cut",
            false,
        )?;
        d.wake
            .wait_until((Instant::now() + Duration::from_millis(16)).min(deadline));
    };
    d.key(Key::Escape)?;
    d.app_mut().feedback.release_project_update = Some(update);
    d.app_mut().feedback.hold_project_updates = false;
    d.step(
        "Release the saved operator after Escape relinquished its cursor",
        false,
    )?;
    idle(d)?;
    d.check(
        "Escape cannot cancel a queued edit, but its late receipt cannot reclaim the cursor",
        d.app().sequence_length() == 115
            && d.app().sequence_cursor == 20
            && !d.app().macros.recording()
            && d.app()
                .message
                .as_deref()
                .is_some_and(|message| message.contains("not applied")),
        json!({"frames":115,"Edit":20,"queued_edit_saved":true,"cursor_not_reclaimed":true}),
        state(d),
    )?;
    undo(d, baseline)
}

fn refresh_failure_absence(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 20)?;
    let revision = d.revision();
    let old_history = history(d);
    let old_version = bank(d)?.version;
    d.app_mut().selected_beat = None;
    d.app_mut().feedback.hold_project_updates = true;
    d.chord(&[Key::Y, Key::Num3, Key::L])?;
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut update = loop {
        if let Some(update) = d.app().service.take_update()
            && update.saved_macro.as_ref().is_some_and(|receipt| {
                receipt.id.revision.as_str() == revision && receipt.bank_version > old_version
            })
        {
            break update;
        }
        if Instant::now() >= deadline {
            return Err("The deferred operator copy receipt did not arrive".into());
        }
        d.step(
            "Withhold a genuine copy receipt with no selected child",
            false,
        )?;
        d.wake
            .wait_until((Instant::now() + Duration::from_millis(16)).min(deadline));
    };
    // The copy and bank are real. Inject only the refresh failure observation;
    // service-level tests separately exercise the actual failed refresh path.
    let receipt = update.saved_macro.as_mut().ok_or("Missing copy receipt")?;
    let crate::project::macros::Outcome::Applied { refresh_error, .. } = &mut receipt.outcome
    else {
        return Err("The copy did not use the shared Apply path".into());
    };
    *refresh_error = Some("Copy saved, but preview refresh failed. Reopen the project.".into());
    if let Some(response) = &mut update.macros {
        response.result = Ok(receipt.clone());
    }
    d.app_mut().feedback.release_project_update = Some(update);
    d.app_mut().feedback.hold_project_updates = false;
    d.step(
        "Deliver the saved copy with an injected refresh failure",
        false,
    )?;
    idle(d)?;
    d.check("A saved refresh failure preserves captured absence instead of inferring a beat",
        d.app().selected_beat.is_none() && d.app().sequence_cursor == 20
            && d.revision() == revision && bank(d)?.version == old_version + 1
            && history(d) == old_history && same_document(document(d)?, baseline)?
            && d.app().message.as_deref().is_some_and(|message| message.contains("Reopen")),
        json!({"selected":null,"bank_saved":true,"history_unchanged":true,"injection":"refresh failure observation only"}), state(d))
}
