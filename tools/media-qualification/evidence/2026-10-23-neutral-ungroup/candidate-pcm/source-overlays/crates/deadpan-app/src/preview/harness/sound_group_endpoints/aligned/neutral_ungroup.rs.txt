//! Production Ungroup preserves later child edits and retained sound history.
use super::*;

pub(super) fn exercise(
    d: &mut Driver<'_>,
    grouped: &ProjectDocument,
    top: &NodeId,
    group: &NodeId,
    owner: &NodeId,
) -> Result<(), String> {
    let bank = reader(d)?.registers().map_err(|e| e.to_string())?;
    d.key(Key::Enter)?;
    d.settled()?;
    d.check(
        "Enter selects the Source inside the introduced neutral group",
        d.app().sequence_scope.groups() == [top.clone(), group.clone()]
            && d.app().selected_beat.as_ref() == Some(owner),
        json!({"scope":[top,group],"selected":owner}),
        d.snapshot(),
    )?;
    command(d, "gain -3")?;
    let before = document(d)?.clone();
    d.check(
        "Later Source gain changes without altering any retained sound journal",
        before.nodes()[owner] != grouped.nodes()[owner]
            && before.nodes()[group] == grouped.nodes()[group]
            && before.audio_bindings() == grouped.audio_bindings()
            && before.beat_sounds() == grouped.beat_sounds(),
        json!({"child_gain":"changed","group":"neutral","sound_history":"unchanged"}),
        d.snapshot(),
    )?;
    d.key(Key::Backspace)?;
    d.settled()?;
    let pixels = checkpoints(d)?;
    scope_at(d, 1)?;
    command(d, "ungroup")?;
    let after = document(d)?.clone();
    let mut expected = serde_json::to_value(&before).map_err(|e| e.to_string())?;
    expected["revision_id"] = json!(after.revision_id());
    expected["nodes"]
        .as_object_mut()
        .ok_or("No nodes")?
        .remove(group.as_str());
    let mut promoted = before.nodes()[top].clone();
    let NodeKind::Sequence { children } = &mut promoted.kind else {
        return Err("Top is no longer a Sequence".into());
    };
    let slot = children
        .iter()
        .position(|child| child == group)
        .ok_or("No G in Top")?;
    children[slot] = owner.clone();
    expected["nodes"][top.as_str()] = serde_json::to_value(promoted).map_err(|e| e.to_string())?;
    if let Some(lineage) = expected
        .get_mut("audio_lineage")
        .and_then(serde_json::Value::as_object_mut)
    {
        lineage.remove(group.as_str());
    }
    for events in expected["audio_bindings"]["sound_clocks"]
        .as_object_mut()
        .ok_or("No clocks")?
        .values_mut()
    {
        for journal in events.as_object_mut().ok_or("No events")?.values_mut() {
            for reference in journal["clocks"].as_array_mut().ok_or("No references")? {
                let groups = reference["introduced_groups"]
                    .as_array_mut()
                    .ok_or("No introduced groups")?;
                groups.retain(|item| item.as_str() != Some(group.as_str()));
                if groups.is_empty() {
                    reference
                        .as_object_mut()
                        .ok_or("No reference")?
                        .remove("introduced_groups");
                }
            }
        }
    }
    let expected = ProjectDocument::from_json(&expected.to_string()).map_err(|e| e.to_string())?;
    d.check(
        "Production Ungroup promotes the Source and preserves every later edit and sound clock",
        after == expected
            && d.app().sequence_scope.groups() == [top.clone()]
            && d.app().selected_beat.as_ref() == Some(owner)
            && d.app().sequence_cursor == 1
            && d.app().source_cursor == ORIGINAL_CURSOR
            && reader(d)?.registers().map_err(|e| e.to_string())? == bank,
        json!({"removed":group,"selected":owner,"Edit":1,"Original":ORIGINAL_CURSOR,"bank":"unchanged"}),
        d.snapshot(),
    )?;
    compare_checkpoints(d, &pixels)?;
    undo(d, &before)?;
    redo(d, &after)?;
    let saved = document(d)?.clone();
    let path = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No workspace")?
        .path
        .clone();
    d.app().service.submit(ProjectRequest::Close)?;
    d.wait_for("Release neutral Ungroup writer before reopening", |app| {
        app.workspace.is_none() && !app.service.is_busy()
    })?;
    open(d, &path, ORIGINAL_CURSOR)?;
    d.check(
        "Native reopen retains the exact ungrouped document, media and register bank",
        document(d)? == &saved
            && reader(d)?.registers().map_err(|e| e.to_string())? == bank
            && reader(d)?.validate().is_ok(),
        json!({"document":"exact durable state","bank":"unchanged"}),
        d.snapshot(),
    )?;
    compare_checkpoints(d, &pixels)?;
    undo(d, &before)?;
    redo(d, &after)?;
    undo(d, &before)?;
    undo(d, grouped)?;
    // Resume the existing EQ/Split/Move replay from the original grouped state.
    at(d, 1)?;
    d.key(Key::Enter)?;
    d.settled()?;
    scope_at(d, 1)?;
    d.check(
        "Neutral Ungroup trial restores the original selected wrapper for subsequent edits",
        equivalent(document(d)?, grouped)?
            && d.app().sequence_scope.groups() == [top.clone()]
            && d.app().selected_beat.as_ref() == Some(group)
            && d.app().source_cursor == ORIGINAL_CURSOR,
        json!({"selected":group,"scope":[top],"Original":ORIGINAL_CURSOR}),
        d.snapshot(),
    )
}

fn checkpoints(d: &mut Driver<'_>) -> Result<Vec<(u64, u64, Option<String>)>, String> {
    let mut result = Vec::new();
    for (frame, expected) in [(1, 0), (61, 60), (120, 119)] {
        scope_at(d, frame)?;
        let actual = d
            .app()
            .presentation
            .displayed_source_frame()
            .ok_or("No exact picture")?
            .0;
        d.check(
            "Neutral Ungroup trial displays the exact Original checkpoint",
            actual == expected,
            json!({"Edit":frame,"Original":expected}),
            d.snapshot(),
        )?;
        result.push((
            frame,
            actual,
            super::super::super::group_pixels::fingerprint(d)?,
        ));
    }
    Ok(result)
}

fn compare_checkpoints(
    d: &mut Driver<'_>,
    expected: &[(u64, u64, Option<String>)],
) -> Result<(), String> {
    let actual = checkpoints(d)?;
    d.check(
        "Neutral Ungroup preserves exact submitted pictures at Source endpoints and midpoint",
        actual == expected
            && (d.options.mode != RunMode::Visual
                || actual.iter().all(|(_, _, hash)| hash.is_some())),
        json!(expected),
        json!(actual),
    )
}
