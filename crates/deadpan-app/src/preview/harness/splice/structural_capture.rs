//! Empty groups stay editable without inventing source pictures or audio.
use super::*;
use deadpan_core::{
    BeatNode, Command, CommandRequest, NodeKind, RevisionId, SliceCaptureSelection, Subtree,
};
use deadpan_store::{AccessMode, ProjectStore};
use std::collections::BTreeMap;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    whole_child_move_toggle(d)?;
    let original = document(d)?.clone();
    let path = d
        .app()
        .workspace
        .as_ref()
        .ok_or("Missing empty-group fixture")?
        .path
        .clone();
    d.app().service.submit(ProjectRequest::Close)?;
    d.wait_for("Release the writer before installing empty groups", |app| {
        app.workspace.is_none() && !app.service.is_busy()
    })?;
    let mut store =
        ProjectStore::open(&path, AccessMode::ReadWrite).map_err(|error| error.to_string())?;
    let middle = node("structural-notes")?;
    for (slot, name, label) in [
        (0, "structural-left", "Left empty group"),
        (1, "structural-notes", "Notes"),
        (2, "structural-right", "Right empty group"),
    ] {
        let before = store.snapshot().map_err(|error| error.to_string())?;
        let root = node(name)?;
        let mut nodes = BTreeMap::new();
        let children = if root == middle {
            let nested = node("structural-nested-notes")?;
            nodes.insert(nested.clone(), BeatNode::sequence("Nested notes", vec![]));
            vec![nested]
        } else {
            vec![]
        };
        nodes.insert(root.clone(), BeatNode::sequence(label, children));
        store
            .commit(&CommandRequest {
                project_id: before.project_id().clone(),
                expected_revision: before.revision_id().clone(),
                new_revision: RevisionId::new(format!("{name}-fixture"))
                    .map_err(|error| error.to_string())?,
                command: Command::Insert {
                    parent: before.root().clone(),
                    index: slot,
                    subtree: Subtree {
                        root,
                        nodes,
                        overrides: BTreeMap::new(),
                        gap_overrides: BTreeMap::new(),
                    },
                },
            })
            .map_err(|error| error.to_string())?;
    }
    drop(store);
    d.app().service.submit(ProjectRequest::Open(path))?;
    d.wait_for(
        "Open three adjacent empty groups beside the qualified Original",
        |app| app.workspace.is_some() && !app.service.is_busy(),
    )?;
    d.command("sequence")?;
    edited::goto(d, 0)?;
    select_child(d, &middle)?;
    let saved = document(d)?.clone();
    d.key(Key::Y)?;
    d.wait_for("Capture the exact middle empty child through production y", |app| {
        !app.service.is_busy() && !app.copied.is_pending()
            && matches!(app.copied.content(), Some(crate::preview::copied::Content::Edited(copied))
                if copied.slice().selection() == &(SliceCaptureSelection::Child { node: middle.clone() }))
    })?;
    let copied = edited::accepted(d)?;
    let displayed = d.app().presentation.displayed_label();
    let picture = d.app().presentation.displayed_source_frame();
    d.command("splice")?;
    wait_ready(d)?;
    for text in [
        "Empty group ‘Notes’",
        "0 frames · structure only",
        "No included pictures or audio.",
    ] {
        paint_text(d, text)?;
    }
    d.check(
        "Empty source has a truthful structural card and exact selected sibling slot",
        draft(d)?.proposal_for_check().destination == Destination::Slot(1)
            && prepared(d)?.empty_slot == Some(1)
            && draft(d)?.empty_endpoints_for_check()
            && !has_endpoint_widgets(d)
            && d.app().presentation.displayed_label() == displayed,
        json!({"slot":1,"frames":0,"endpoints":false}),
        state(d),
    )?;
    let proposal = draft(d)?.proposal_for_check().id.clone();
    for keys in [
        &[Key::I, Key::L][..],
        &[Key::O, Key::H],
        &[Key::M],
        &[Key::R],
        &[Key::Space],
    ] {
        d.chord(keys)?;
        d.check(
            "Empty source controls cannot refine, move, replace or audition nonexistent media",
            draft(d)?.proposal_for_check().id == proposal
                && draft(d)?.empty_endpoints_for_check()
                && d.app().splice_picture_work(None).is_none()
                && d.app().transport.is_none()
                && d.app().presentation.displayed_source_frame() == picture
                && d.app().presentation.displayed_label() == displayed
                && *document(d)? == saved,
            json!({"same_proposal":true,"source_work":false,"same_destination_picture":true}),
            state(d),
        )?;
    }
    d.key(Key::D)?;
    for slot in [2, 3] {
        d.key(Key::J)?;
        wait_ready(d)?;
        d.check(
            "Next seam retains distinct same-time child slots",
            draft(d)?.proposal_for_check().destination == Destination::Slot(slot)
                && prepared(d)?.empty_slot == Some(slot)
                && prepared(d)?.range == edited::range(0, 0)?,
            json!({"slot":slot,"boundary":0}),
            state(d),
        )?;
    }
    let before_blocked_step = draft(d)?.proposal_for_check().id.clone();
    d.key(Key::L)?;
    d.check(
        "Frame motion cannot turn an empty copy into an interior insertion",
        draft(d)?.proposal_for_check().id == before_blocked_step
            && draft(d)?.proposal_for_check().destination == Destination::Slot(3),
        json!({"slot":3,"unchanged":true}),
        state(d),
    )?;
    structural_layout(d, false)?;
    edited::cancel(d)?;
    d.check(
        "Cancel preserves the accepted empty group and all saved authored state",
        *document(d)? == saved
            && Arc::ptr_eq(&edited::accepted(d)?, &copied)
            && d.app().presentation.displayed_label() == displayed,
        json!("saved state, copied child and destination caption retained"),
        d.snapshot(),
    )?;

    d.command("splice")?;
    wait_ready(d)?;
    d.key(Key::J)?;
    wait_ready(d)?;
    let exact = prepared(d)?;
    let revision = d.revision();
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    d.check(
        "One zero-time commit selects the exact imported child without an empty Visual range",
        d.app().splice.is_none()
            && *document(d)? == *exact.snapshot.document
            && d.app().selected_beat.as_ref() == Some(&exact.node)
            && d.app().selected_edit_range().is_none()
            && d.app().sequence_cursor == 0
            && d.app().sequence_length() == 120
            && document(d)?.children(document(d)?.root()).nth(2) == Some(&exact.node),
        json!({"slot":2,"node":exact.node,"duration":120,"visual":false}),
        d.snapshot(),
    )?;
    edited::undo(d, &saved)?;
    empty_destination(d, &saved, &middle)?;

    // Restore the three fixture insertions. Copy history remains independent.
    for _ in 0..3 {
        let revision = d.revision();
        d.key(Key::U)?;
        d.changed(&revision)?;
    }
    let mut actual = serde_json::to_value(document(d)?).map_err(|error| error.to_string())?;
    actual["revision_id"] = json!(original.revision_id());
    d.check(
        "Empty-group replay restores every original authored field",
        actual == serde_json::to_value(original).map_err(|error| error.to_string())?,
        json!("fixture fully undone"),
        d.snapshot(),
    )
}

fn node(name: &str) -> Result<NodeId, String> {
    NodeId::new(name).map_err(|error| error.to_string())
}

fn whole_child_move_toggle(d: &mut Driver<'_>) -> Result<(), String> {
    d.command("sequence")?;
    edited::goto(d, 0)?;
    let saved = document(d)?.clone();
    let source = saved
        .children(saved.root())
        .find(|node| matches!(saved.nodes()[*node].kind, NodeKind::Source { .. }))
        .cloned()
        .ok_or("Whole-child Move fixture has no Source")?;
    let source_slot = saved
        .children(saved.root())
        .position(|node| node == &source)
        .ok_or("Whole-child Move fixture lost its Source slot")?;
    select_child(d, &source)?;
    d.key(Key::Y)?;
    d.wait_for("Copy a positive whole child for refinement and Move recovery", |app| {
        !app.service.is_busy() && !app.copied.is_pending()
            && matches!(app.copied.content(), Some(crate::preview::copied::Content::Edited(copied))
                if copied.slice().selection() == &(SliceCaptureSelection::Child { node: source.clone() }))
    })?;
    let copied = edited::accepted(d)?;
    d.command("splice")?;
    wait_ready(d)?;
    d.key(Key::D)?;
    let Destination::Slot(current_slot) = draft(d)?.proposal_for_check().destination else {
        return Err("Whole-child Move fixture did not open at a Sequence seam".into());
    };
    // Empty siblings can share the Source's frame-zero boundary. Only its
    // exact original slot makes restoration of the whole range a no-op.
    for _ in 0..source_slot.abs_diff(current_slot) {
        d.key(if current_slot < source_slot {
            Key::J
        } else {
            Key::K
        })?;
    }
    wait_ready(d)?;
    d.check(
        "Move recovery retains the Source's exact sibling slot despite same-time empty seams",
        draft(d)?.proposal_for_check().destination == Destination::Slot(source_slot),
        json!({"source_slot":source_slot,"boundary":copied.slice().range().start().0}),
        state(d),
    )?;
    d.chord(&[Key::I, Key::L])?;
    wait_ready(d)?;
    d.key(Key::M)?;
    wait_ready(d)?;
    d.check(
        "A locally refined whole-child copy can enter a valid range Move",
        draft(d)?.proposal_for_check().operation == crate::project::splice::Operation::Move
            && prepared(d)?.movement.is_some(),
        json!({"operation":"Move","refined_source":true}),
        state(d),
    )?;
    d.chord(&[Key::I, Key::H])?;
    d.wait_for(
        "Restoring the exact child extent rejects a no-op Move at its current position",
        |app| {
            !app.service.is_busy()
                && app.splice.as_ref().is_some_and(|draft| {
                    draft
                        .error_for_check()
                        .is_some_and(|error| error == "Already at this position; no change")
                })
        },
    )?;
    d.check(
        "A no-op whole-child Move keeps Copy instead enabled",
        !draft(d)?.ready_for_check()
            && d.rect("Copy instead · m").is_ok()
            && draft(d)?.proposal_for_check().source.boundaries()? == (0..120),
        json!({"copy_enabled":true,"bounds":[0,120]}),
        state(d),
    )?;
    d.key(Key::M)?;
    wait_ready(d)?;
    d.check(
        "m exits the rejected Move and prepares the original whole-child Copy",
        draft(d)?.proposal_for_check().operation == crate::project::splice::Operation::Copy
            && prepared(d)?.movement.is_none()
            && prepared(d)?.range.duration() == copied.slice().duration()
            && *document(d)? == saved
            && Arc::ptr_eq(&edited::accepted(d)?, &copied),
        json!({"operation":"Copy","ready":true,"saved_unchanged":true}),
        state(d),
    )?;
    edited::cancel(d)?;
    d.check(
        "Cancel after Move recovery preserves the whole child and accepted copy",
        *document(d)? == saved && Arc::ptr_eq(&edited::accepted(d)?, &copied),
        json!("history and register unchanged"),
        d.snapshot(),
    )
}

fn empty_destination(
    d: &mut Driver<'_>,
    saved: &deadpan_core::ProjectDocument,
    middle: &NodeId,
) -> Result<(), String> {
    let source = saved
        .children(saved.root())
        .find(|node| matches!(saved.nodes()[*node].kind, NodeKind::Source { .. }))
        .cloned()
        .ok_or("Empty destination fixture has no Original child")?;
    select_child(d, &source)?;
    let revision = d.revision();
    d.chord(&[Key::D, Key::D])?;
    d.changed(&revision)?;
    let empty = document(d)?.clone();
    select_child(d, middle)?;
    d.key(Key::Y)?;
    d.wait_for("Copy an empty child in a completely zero-time edit", |app| {
        !app.service.is_busy() && !app.copied.is_pending()
            && matches!(app.copied.content(), Some(crate::preview::copied::Content::Edited(copied))
                if copied.slice().revision_id() == empty.revision_id()
                    && copied.slice().selection() == &(SliceCaptureSelection::Child { node: middle.clone() }))
    })?;
    d.command("splice")?;
    wait_ready(d)?;
    paint_text(d, "Empty edit · no pictures or audio")?;
    d.check(
        "A zero-time destination stays empty and enables structural placement without media work",
        prepared(d)?.plan.duration().frames() == 0
            && prepared(d)?.empty_slot == Some(1)
            && d.app().splice_picture_work(None).is_none()
            && draft(d)?.empty_endpoints_for_check()
            && !has_endpoint_widgets(d)
            && d.rect(APPLY).is_ok(),
        json!({"frames":0,"picture_work":false,"structural_apply":true}),
        state(d),
    )?;
    d.key(Key::Space)?;
    d.check(
        "Space on an entirely empty destination never starts a zero-length audition",
        d.app().transport.is_none()
            && draft(d)?
                .error_for_check()
                .is_some_and(|error| error.contains("destination edit is empty"))
            && *document(d)? == empty,
        json!({"transport":false,"saved_unchanged":true}),
        state(d),
    )?;
    structural_layout(d, true)?;
    let exact = prepared(d)?;
    let revision = d.revision();
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    d.check(
        "An empty project receives one selectable zero-time group and no Visual range",
        *document(d)? == *exact.snapshot.document
            && d.app().sequence_length() == 0
            && d.app().selected_beat.as_ref() == Some(&exact.node)
            && d.app().selected_edit_range().is_none(),
        json!({"frames":0,"node":exact.node,"visual":false}),
        d.snapshot(),
    )?;
    edited::undo(d, &empty)?;
    edited::undo(d, saved)
}

fn structural_layout(d: &mut Driver<'_>, empty_destination: bool) -> Result<(), String> {
    for (width, height) in [(960.0, 640.0), (1280.0, 820.0)] {
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
        let input = d.harness.input_mut();
        input.screen_rect = Some(viewport);
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .ok_or("Missing structural placement viewport")?
            .inner_rect = Some(viewport);
        d.step(
            "Paint empty-group placement at the exact requested viewport",
            true,
        )?;
        for label in [
            "Empty group ‘Notes’",
            "0 frames · structure only",
            "No included pictures or audio.",
            APPLY,
            CANCEL,
        ] {
            paint_text(d, label)?;
        }
        if empty_destination {
            paint_text(d, "Empty edit · no pictures or audio")?;
        } else {
            viewer_painted(d)?;
        }
        d.capture(&format!(
            "Empty group placement with {} destination at {width} by {height}",
            if empty_destination {
                "empty"
            } else {
                "retained picture"
            }
        ))?;
    }
    Ok(())
}

fn has_endpoint_widgets(d: &Driver<'_>) -> bool {
    d.harness.root().children_recursive().any(|node| {
        let access = node.accesskit_node();
        let endpoint =
            |text: &str| text.starts_with("First included") || text.starts_with("Last included");
        access.label().as_deref().is_some_and(endpoint)
            || access.value().as_deref().is_some_and(endpoint)
    })
}

fn select_child(d: &mut Driver<'_>, wanted: &NodeId) -> Result<(), String> {
    let children: Vec<_> = document(d)?
        .children(document(d)?.root())
        .cloned()
        .collect();
    let target = children
        .iter()
        .position(|node| node == wanted)
        .ok_or("Missing empty group")?;
    for _ in 0..children.len() {
        if d.app().selected_beat.as_ref() == Some(wanted) {
            return d.settled();
        }
        let current = d
            .app()
            .selected_beat
            .as_ref()
            .and_then(|node| children.iter().position(|child| child == node))
            .unwrap_or(0);
        d.key(if current > target { Key::K } else { Key::J })?;
    }
    Err("Production beat navigation did not reach the middle empty group".into())
}
