//! Cursor-relative range intent and retained disjoint geometry through native input.
use super::*;
use deadpan_core::{RegisterName, RegisterValue, SemanticInstruction};
use deadpan_store::registers::RegisterBank;

fn idle(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Range-set service and bank receipts finish", |app| {
        !app.service.is_busy() && !app.macros.is_pending() && !app.copied.is_pending()
    })?;
    d.settled()
}
fn bank(d: &Driver<'_>) -> Result<RegisterBank, String> {
    ProjectStore::open(
        &d.app()
            .workspace
            .as_ref()
            .ok_or("Missing range workspace")?
            .path,
        AccessMode::ReadOnly,
    )
    .and_then(|store| store.registers())
    .map_err(|error| error.to_string())
}
fn select(d: &mut Driver<'_>) -> Result<(), String> {
    d.command(r#"select ranges=[{"start":20,"end":24},{"start":-8,"end":-4},{"start":2,"end":6},{"start":3,"end":5}]"#)?;
    idle(d)
}
fn ranges(d: &Driver<'_>) -> Result<Vec<(i64, i64)>, String> {
    Ok(d.app()
        .selected_range_set()?
        .ok_or("Missing selected ranges")?
        .ranges
        .iter()
        .map(|range| (range.start().0, range.end().0))
        .collect())
}
fn union(d: &Driver<'_>) -> Result<bool, String> {
    let saved = bank(d)?;
    Ok(
        matches!(saved.entries.get(&RegisterName::unnamed()).map(AsRef::as_ref), Some(RegisterValue::EditedSet {set}) if set.components().len()==3 && set.duration().frames()==12),
    )
}
fn copied_ranges(d: &Driver<'_>) -> Result<Vec<(i64, i64)>, String> {
    let saved = bank(d)?;
    let Some(RegisterValue::EditedSet { set }) = saved
        .entries
        .get(&RegisterName::unnamed())
        .map(AsRef::as_ref)
    else {
        return Err("Missing copied range set".into());
    };
    set.components()
        .iter()
        .map(|slice| match slice.selection() {
            deadpan_core::SliceCaptureSelection::Range { range } => {
                Ok((range.start().0, range.end().0))
            }
            _ => Err("Copied component is not a range".into()),
        })
        .collect()
}

pub(in crate::preview::harness) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 640.0));
    d.harness.input_mut().screen_rect = Some(rect);
    d.harness
        .input_mut()
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing range viewport")?
        .inner_rect = Some(rect);
    d.step("Paint range selectors at minimum viewport", false)?;
    let baseline = document(d)?.clone();
    at(d, 10)?;
    d.key(Key::Y)?;
    d.key(Key::Y)?;
    idle(d)?;
    d.check(
        "Ordinary beat copy prepares a non-union Place slice source",
        matches!(
            bank(d)?
                .entries
                .get(&RegisterName::unnamed())
                .map(AsRef::as_ref),
            Some(RegisterValue::Edited { .. })
        ) && document(d)? == &baseline,
        json!({"copied_type":"edited","authored_write":false}),
        state(d),
    )?;
    let initial_bank = bank(d)?;
    select(d)?;
    d.check(
        "Relative pairs resolve once, merge overlaps, and retain three separate ranges",
        ranges(d)? == [(2, 6), (12, 16), (30, 34)]
            && d.app().sequence_cursor == 34
            && d.app().selected_edit_range().is_none()
            && document(d)? == &baseline
            && bank(d)? == initial_bank,
        json!({"ranges":[[2,6],[12,16],[30,34]],"selected_frames":12,"cursor":34,"hull":false}),
        state(d),
    )?;
    let description = d
        .app()
        .inspector_description()
        .ok_or("Missing range inspector")?;
    d.check(
        "Range inspector is read-only and lists all exact half-open ranges",
        description.kind == "Range set"
            && description.parameter.is_none()
            && description
                .fields
                .iter()
                .filter(|(label, _)| *label == "Selected range")
                .count()
                == 3
            && description
                .fields
                .iter()
                .any(|(_, value)| value.contains("[30..34)")),
        json!({"kind":"Range set","ranges":3,"parameter":null}),
        json!(description.fields),
    )?;
    d.capture("Disjoint ranges show separate highlights and complete range inspector")?;
    let selection = d.app().capture_visual_selection()?;
    d.command("splice")?;
    idle(d)?;
    d.check(
        "Place slice rejects range authority even with an ordinary copied source",
        d.app().splice.is_none()
            && document(d)? == &baseline
            && bank(d)? == initial_bank
            && d.app().capture_visual_selection()? == selection
            && d.app()
                .error
                .as_ref()
                .is_some_and(|error| error.contains("Clear the selection set")),
        json!({"ordinary_source":true,"selection_set_refused":true,"no_write":true}),
        state(d),
    )?;
    d.key(Key::L)?;
    idle(d)?;
    d.check(
        "Cursor motion preserves finished absolute geometry",
        d.app().sequence_cursor == 35 && d.app().capture_visual_selection()? == selection,
        json!({"cursor":35,"ranges":[[2,6],[12,16],[30,34]]}),
        state(d),
    )?;
    d.key(Key::Y)?;
    idle(d)?;
    d.check(
        "Yank stores one twelve-frame union without editing its gaps",
        union(d)? && document(d)? == &baseline,
        json!({"union_components":3,"frames":12}),
        state(d),
    )?;
    let copied_bank = bank(d)?;
    for command in [
        "duplicate",
        "group",
        "wrap-repeat 2",
        "gain -3",
        "generate-ai",
        "framing-save x",
        "splice",
    ] {
        d.command(command)?;
        idle(d)?;
        d.check(
            "Unsupported range-set consumers refuse without document or register writes",
            document(d)? == &baseline
                && bank(d)? == copied_bank
                && d.app().capture_visual_selection()? == selection
                && d.app().error.is_some(),
            json!({"command":command,"no_write":true}),
            state(d),
        )?;
    }
    d.key_modified(Key::Space, egui::Modifiers::SHIFT)?;
    idle(d)?;
    d.check(
        "Loop refuses the range set without falling back to the navigation beat",
        d.snapshot()["playback"].is_null()
            && document(d)? == &baseline
            && bank(d)? == copied_bank
            && d.app().error.is_some(),
        json!({"loop_refused":true}),
        state(d),
    )?;
    d.key(Key::P)?;
    idle(d)?;
    d.check(
        "Active-set replacement refuses without using an enclosing range",
        document(d)? == &baseline && bank(d)? == copied_bank && d.app().error.is_some(),
        json!({"replacement_refused":true}),
        state(d),
    )?;
    d.key(Key::Escape)?;
    idle(d)?;
    at(d, 10)?;
    d.command("record w")?;
    select(d)?;
    d.key(Key::Y)?;
    idle(d)?;
    let before = d.revision();
    d.key(Key::D)?;
    d.changed(&before)?;
    idle(d)?;
    d.key(Key::Q)?;
    idle(d)?;
    let saved = bank(d)?;
    d.check("Recorded select retains original offset intent while copy and cut consume the current Visual set",
        d.app().sequence_length()==108 && union(d)? && matches!(saved.entries.get(&RegisterName::new('w').unwrap()).map(AsRef::as_ref),Some(RegisterValue::Macro {program}) if matches!(program.instructions(),[SemanticInstruction::SelectRanges {ranges},SemanticInstruction::YankSelection {..},SemanticInstruction::CutSelection {..}] if ranges.iter().map(|r|(r.start.0,r.end.0)).collect::<Vec<_>>()==[(20,24),(-8,-4),(2,6),(3,5)])),
        json!({"macro":"w","remaining_frames":108,"one_atomic_cut":true}),state(d))?;
    undo(d, &baseline)?;
    idle(d)?;
    let restored = document(d)?.clone();
    let bank_before_dot = bank(d)?;
    d.key(Key::Period)?;
    idle(d)?;
    d.check(
        "Dot after a Visual union cut requires a new selection",
        document(d)? == &restored && bank(d)? == bank_before_dot && d.app().error.is_some(),
        json!({"old_endpoints_reused":false}),
        state(d),
    )?;
    at(d, 20)?;
    select(d)?;
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Dot consumes a new range set at its new cursor",
        d.app().sequence_length() == 108
            && union(d)?
            && copied_ranges(d)? == [(12, 16), (22, 26), (40, 44)],
        json!({"new_ranges":[[12,16],[22,26],[40,44]],"frames":108}),
        state(d),
    )?;
    undo(d, &restored)?;
    idle(d)?;
    let restored = document(d)?.clone();
    at(d, 20)?;
    d.command("macro w")?;
    idle(d)?;
    d.check(
        "Named call resolves recorded offsets afresh",
        d.app().sequence_length() == 108
            && union(d)?
            && copied_ranges(d)? == [(12, 16), (22, 26), (40, 44)],
        json!({"macro":"w","entry_cursor":20,"new_ranges":[[12,16],[22,26],[40,44]],"frames":108}),
        state(d),
    )?;
    let called = document(d)?.clone();
    at(d, 60)?;
    let before = d.revision();
    d.key(Key::P)?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Seam paste inserts all components with one Undo",
        d.app().sequence_length() == 120,
        json!({"frames":120,"pasted":12}),
        state(d),
    )?;
    undo(d, &called)?;
    idle(d)?;
    undo(d, &restored)?;
    idle(d)?;
    // Delayed completion owns the exact entry cursor and absence of a selection.
    d.key(Key::Escape)?;
    idle(d)?;
    at(d, 10)?;
    let capture = d.app().capture_macro_target();
    let prior = document(d)?.clone();
    let prior_bank = bank(d)?;
    d.key(Key::L)?;
    d.app_mut().select_explicit_ranges(
        Some(capture),
        vec![deadpan_core::SemanticRangeOffsets {
            start: deadpan_core::SemanticFrameOffset(2),
            end: deadpan_core::SemanticFrameOffset(6),
        }],
    );
    idle(d)?;
    d.check(
        "Delayed range command cannot retarget after cursor navigation",
        document(d)? == &prior
            && bank(d)? == prior_bank
            && d.app().capture_visual_selection()?.is_none()
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("context changed")),
        json!({"entry_cursor":10,"current_cursor":11,"no_write":true}),
        state(d),
    )?;
    at(d, 10)?;
    let capture = d.app().capture_macro_target();
    d.key(Key::V)?;
    idle(d)?;
    let new_selection = d.app().capture_visual_selection()?;
    d.app_mut().select_explicit_ranges(
        Some(capture),
        vec![deadpan_core::SemanticRangeOffsets {
            start: deadpan_core::SemanticFrameOffset(2),
            end: deadpan_core::SemanticFrameOffset(6),
        }],
    );
    idle(d)?;
    d.check(
        "Delayed range command retains captured selection absence",
        document(d)? == &prior
            && bank(d)? == prior_bank
            && d.app().capture_visual_selection()? == new_selection
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("context changed")),
        json!({"absent_at_entry":true,"new_visual_unchanged":true}),
        state(d),
    )?;
    d.key(Key::Escape)?;
    idle(d)?;
    d.check(
        "Range replay restores full protected Original",
        same_document(document(d)?, &baseline)? && d.app().sequence_length() == 120,
        json!({"frames":120}),
        state(d),
    )?;
    Ok(())
}
