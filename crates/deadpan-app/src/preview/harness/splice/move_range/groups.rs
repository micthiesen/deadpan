//! Boundary reparenting and generic completion after its workspace is visible.

use super::*;
use deadpan_core::{Command, CommandRequest, RevisionId};
use deadpan_store::{AccessMode, ProjectStore};

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let original = document(d)?.clone();
    goto(d, 20)?;
    let revision = d.revision();
    d.key(Key::S)?;
    d.changed(&revision)?;
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .cloned()
        .ok_or("Missing group Move workspace")?;
    let path = workspace.path.clone();
    let children = workspace
        .document
        .children(workspace.document.root())
        .filter(|id| {
            workspace
                .plan
                .node_duration(id)
                .is_some_and(|duration| duration.frames() > 0)
        })
        .cloned()
        .collect::<Vec<_>>();
    if children.len() != 2 {
        return Err("Group Move fixture needs exactly two nonempty root children".into());
    }
    let left = NodeId::new("move-left-group").map_err(|error| error.to_string())?;
    let right = NodeId::new("move-right-group").map_err(|error| error.to_string())?;
    d.app().service.submit(ProjectRequest::Close)?;
    d.wait_for(
        "Release the writer before grouping the Move fixture",
        |app| app.workspace.is_none() && !app.service.is_busy(),
    )?;
    let mut store =
        ProjectStore::open(&path, AccessMode::ReadWrite).map_err(|error| error.to_string())?;
    for (child, group) in children.iter().zip([&left, &right]) {
        let document = store.snapshot().map_err(|error| error.to_string())?;
        let start = document
            .children(document.root())
            .position(|id| id == child)
            .ok_or("Missing Move grouping child")?;
        store
            .commit(&CommandRequest {
                project_id: document.project_id().clone(),
                expected_revision: document.revision_id().clone(),
                new_revision: RevisionId::new(format!("{group}-fixture"))
                    .map_err(|error| error.to_string())?,
                command: Command::Group {
                    parent: document.root().clone(),
                    start,
                    end: start + 1,
                    id: group.clone(),
                    label: format!("{group} group"),
                },
            })
            .map_err(|error| error.to_string())?;
    }
    drop(store);
    d.app().service.submit(ProjectRequest::Open(path))?;
    d.wait_for(
        "Reopen the grouped Move fixture through the service",
        |app| app.workspace.is_some() && !app.service.is_busy(),
    )?;
    d.command("sequence")?;
    goto(d, 30)?;
    d.key(Key::Enter)?;
    d.check(
        "Production Enter opens the source group with its absolute origin",
        d.app().sequence_scope.groups() == [right.clone()] && d.app().scope_start == 20,
        json!({"source_group":right,"origin":20}),
        d.snapshot(),
    )?;
    d.chord(&[Key::G, Key::G, Key::V])?;
    motion(d, 100, true)?;
    d.key(Key::Y)?;
    d.wait_for("Capture the entire right group child through production y", |app| !app.service.is_busy() && !app.copied.is_pending() && matches!(app.copied.content(), Some(Content::Edited(copied)) if copied.slice().range().start() == ProjectFrame(20) && copied.slice().range().end() == ProjectFrame(120)))?;
    let copied = accepted(d)?;
    let saved = document(d)?.clone();
    d.key(Key::Backspace)?;
    goto(d, 5)?;
    d.key(Key::Enter)?;
    d.chord(&[Key::G, Key::G])?;
    motion(d, 20, true)?;
    d.command("splice")?;
    wait_ready(d)?;
    d.key(Key::M)?;
    wait_ready(d)?;
    let exact = prepared(d)?;
    let id = draft(d)?.proposal_for_check().id.clone();
    d.check(
        "Boundary reparenting retains the physical child and leaves its donor group empty",
        exact.parent == left && exact.node == children[1] && exact.range == range(20, 120)?
            && exact.snapshot.document.children(&right).next().is_none()
            && exact.snapshot.document.nodes().contains_key(&right)
            && exact.movement.as_ref().is_some_and(|movement| movement.source_parent == right && movement.destination_before == ProjectFrame(20)),
        json!({"source_parent":right,"destination_parent":left,"range":[20,120],"child":children[1]}), state(d),
    )?;
    d.key(Key::F)?;
    motion(d, 5, true)?;
    wait_picture(d, 25, "Showing proposed edit frame 26")?;
    d.key(Key::B)?;
    wait_picture(d, 25, "Showing sequence frame 26")?;
    d.check(
        "A boundary reparent compares the identical global frame instead of a fictitious gap",
        draft(d)?.cursor == 25 && draft(d)?.proposal_for_check().id == id,
        json!({"before_frame":25,"proposed_frame":25}),
        state(d),
    )?;
    d.key(Key::B)?;
    wait_picture(d, 25, "Showing proposed edit frame 26")?;
    d.capture("Move between groups preserves global timing and shows both group scopes")?;
    d.key(Key::S)?;
    terminal_compare(d, 120)?;

    d.app_mut().feedback.hold_project_updates = true;
    d.key(Key::Enter)?;
    let mut update = receipt::take_update(d, &id)?;
    let result = update
        .committed
        .take()
        .ok_or("Group Move update has no generic receipt")?;
    if result
        .range_selection
        .as_ref()
        .is_none_or(|range| range.parent != left || range.range != exact.range)
    {
        return Err("Group Move receipt lost its complete destination interval".into());
    }
    // Deliver the service's actual workspace and draft acknowledgement, holding
    // only the generic selection receipt until the next service publication.
    d.app_mut().feedback.release_project_update = Some(update);
    d.step(
        "Deliver the cross-parent Move workspace before its generic selection receipt",
        true,
    )?;
    d.check(
        "Workspace-only delivery cannot install an unreceived moved range",
        d.app().splice.is_none()
            && *document(d)? == *exact.snapshot.document
            && d.app().selected_edit_range().is_none(),
        json!("new workspace visible, result selection pending"),
        d.snapshot(),
    )?;
    d.click("Current group beat outline pane")?;
    d.key(Key::Backspace)?;
    d.check(
        "Navigate to root while the Move range receipt is withheld",
        d.app().sequence_scope.groups().is_empty(),
        json!("root scope"),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.app()
        .service
        .submit(ProjectRequest::CaptureEditSlice(edited::capture_request(
            &copied,
        )))?;
    d.app_mut().feedback.hold_project_updates = false;
    d.wait_for(
        "The late matching Move receipt restores the destination scope and range",
        |app| {
            !app.service.is_busy()
                && app.sequence_scope.groups() == [left.clone()]
                && app.selected_edit_range() == Some(exact.range)
        },
    )?;
    d.check("Late generic completion restores scope even when its workspace revision was already visible", d.revision() == revision && d.app().scope_start == 0 && d.app().scope_end == 120, json!({"same_revision":revision,"scope":[left],"range":[20,120]}), d.snapshot())?;
    committed(d, &exact)?;
    d.key(Key::Backspace)?;
    d.app()
        .service
        .submit(ProjectRequest::CaptureEditSlice(edited::capture_request(
            &copied,
        )))?;
    d.wait_for(
        "Republish the already-consumed cross-parent Move receipt",
        |app| !app.service.is_busy(),
    )?;
    d.settled()?;
    d.check(
        "A duplicate cross-parent receipt cannot pull navigation back into the destination group",
        d.app().sequence_scope.groups().is_empty() && d.app().selected_edit_range().is_none(),
        json!("manual root scope retained"),
        d.snapshot(),
    )?;
    undo(d, &saved)?;
    for _ in 0..3 {
        let revision = d.revision();
        d.key(Key::U)?;
        d.changed(&revision)?;
    }
    let mut actual = serde_json::to_value(document(d)?).map_err(|error| error.to_string())?;
    actual["revision_id"] = json!(original.revision_id());
    d.check(
        "Move replay removes its grouped fixture and restores every original authored field",
        actual == serde_json::to_value(&original).map_err(|error| error.to_string())?,
        json!("exact entry document with a fresh revision"),
        d.snapshot(),
    )
}
