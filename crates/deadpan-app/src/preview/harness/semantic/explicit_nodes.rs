//! Exact authored identities survive native selection, recording and replacement.
use super::*;
use deadpan_core::{
    RegisterName, RegisterValue, SemanticInstruction, SemanticSelector, SemanticVisualSelection,
};
use deadpan_store::registers::RegisterBank;

fn idle(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Explicit-node receipts finish", |app| {
        !app.service.is_busy() && !app.macros.is_pending() && !app.copied.is_pending()
    })?;
    d.settled()
}
fn bank(d: &Driver<'_>) -> Result<RegisterBank, String> {
    ProjectStore::open(
        &d.app()
            .workspace
            .as_ref()
            .ok_or("Missing node workspace")?
            .path,
        AccessMode::ReadOnly,
    )
    .and_then(|store| store.registers())
    .map_err(|error| error.to_string())
}
fn select(d: &mut Driver<'_>, node: &NodeId) -> Result<(), String> {
    d.command(&format!("select node={}", node.as_str()))?;
    idle(d)
}
fn selected(d: &Driver<'_>, node: &NodeId, start: i64, end: i64) -> Result<bool, String> {
    Ok(d.app().capture_visual_selection()?
        == Some(SemanticVisualSelection::Node { node: node.clone() })
        && d.app().selected_edit_range()
            == Some(
                FrameRange::new(ProjectFrame(start), ProjectFrame(end))
                    .map_err(|e| e.to_string())?,
            ))
}
pub(in crate::preview::harness) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 640.0));
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing node viewport")?
        .inner_rect = Some(rect);
    d.step(
        "Paint explicit node selectors at the minimum workspace",
        false,
    )?;
    let baseline = document(d)?.clone();
    at(d, 20)?;
    d.key(Key::V)?;
    d.events(
        "Select twenty frames to group",
        keys(&[Key::Num2, Key::Num0, Key::L, Key::V]),
    )?;
    let before = d.revision();
    d.command("group name=\"Node scope\"")?;
    d.changed(&before)?;
    idle(d)?;
    let grouped = document(d)?.clone();
    let group = d.app().selected_beat.clone().ok_or("Missing group")?;
    let source = grouped
        .children(&group)
        .next()
        .cloned()
        .ok_or("Missing nested source")?;
    let root = grouped.root().clone();

    at(d, 80)?;
    d.key(Key::V)?;
    d.events(
        "Make an unrelated finished Visual range",
        keys(&[Key::Num5, Key::L, Key::V]),
    )?;
    let old_bank = bank(d)?;
    select(d, &source)?;
    d.check("Exact descendant selection replaces unrelated Visual intent without writing history or bank",
        document(d)? == &grouped && bank(d)? == old_bank && selected(d, &source, 20, 40)?
            && d.app().sequence_cursor == 40 && d.app().selected_beat.as_ref() == Some(&group)
            && d.app().sequence_scope.groups().is_empty(),
        json!({"node":source,"scope":root,"range":[20,40],"history_write":false}), state(d))?;
    let label = d.app().edit_range_label().ok_or("Missing node label")?;
    d.check(
        "Node status exposes its label, exact identity, scope and whole-node geometry",
        label.contains(source.as_str())
            && label.contains(root.as_str())
            && label.contains("[20..40)")
            && label.contains("whole node retained"),
        json!({"id":source,"scope":root,"frames":20}),
        json!(label),
    )?;
    d.capture("Explicit descendant selection shows exact identity and captured scope")?;
    d.key(Key::H)?;
    idle(d)?;
    d.check(
        "Independent motion retains the complete selected node",
        selected(d, &source, 20, 40)? && d.app().sequence_cursor == 39,
        json!({"node":source,"cursor":39}),
        state(d),
    )?;
    d.command("record u")?;
    select(d, &source)?;
    d.key(Key::Y)?;
    idle(d)?;
    let before = d.revision();
    d.command("duplicate")?;
    d.changed(&before)?;
    idle(d)?;
    d.key(Key::Q)?;
    idle(d)?;
    let saved = bank(d)?;
    d.check(
        "Recorded selection, Yank and Duplicate preserve the explicit authored identity",
        d.app().sequence_length() == 140
            && matches!(saved.entries.get(&RegisterName::new('u').unwrap()).map(AsRef::as_ref),
                Some(RegisterValue::Macro {program}) if matches!(program.instructions(), [
                    SemanticInstruction::SelectNode {node:first},
                    SemanticInstruction::Yank {selector:SemanticSelector::Node {node:second},..},
                    SemanticInstruction::Duplicate {selector:SemanticSelector::Node {node:third}}
                ] if first == &source && second == &source && third == &source)),
        json!({"node":source,"macro":"u","frames":140}),
        state(d),
    )?;
    undo(d, &grouped)?;
    idle(d)?;
    at(d, 80)?;
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Dot resolves the stored descendant ID from a different cursor",
        d.app().sequence_length() == 140
            && d.app()
                .semantic
                .snapshot()
                .and_then(|snapshot| snapshot.edit.as_ref())
                .is_some_and(|last| {
                    matches!(&last.operation, RepeatableEdit::Duplicate {
                    selector:SemanticSelector::Node {node}} if node == &source)
                }),
        json!({"node":source,"cursor_at_invocation":80,"frames":140}),
        state(d),
    )?;
    undo(d, &grouped)?;
    idle(d)?;
    at(d, 70)?;
    let before = d.revision();
    d.command("macro u")?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Named call retains the current node and complete copied subtree",
        d.app().sequence_length() == 140
            && copied(d, None)?.slice().selection()
                == &(deadpan_core::SliceCaptureSelection::Child {
                    node: source.clone(),
                }),
        json!({"node":source,"macro":"u","frames":140}),
        state(d),
    )?;
    undo(d, &grouped)?;
    idle(d)?;

    select(d, &source)?;
    let before_splice = document(d)?.clone();
    let before_splice_bank = bank(d)?;
    d.command("splice")?;
    idle(d)?;
    d.check(
        "Place slice refuses an explicit node destination instead of flattening it into time",
        document(d)? == &before_splice
            && bank(d)? == before_splice_bank
            && d.app().splice.is_none()
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("p/P"))
            && selected(d, &source, 20, 40)?,
        json!({"node":source,"use":"p/P","no_edit":true}),
        state(d),
    )?;
    let before = d.revision();
    d.key(Key::P)?;
    d.changed(&before)?;
    idle(d)?;
    d.check("Fast paste replaces the nested complete node at its own parent without changing navigation scope",
        d.app().sequence_length() == 120 && !document(d)?.nodes().contains_key(&source)
            && document(d)?.children(&group).count() == 1 && d.app().sequence_scope.groups().is_empty(),
        json!({"replaced_node":source,"parent":group,"frames":120}), state(d))?;
    undo(d, &grouped)?;
    idle(d)?;

    select(d, &source)?;
    let before = d.revision();
    d.command("delete")?;
    d.changed(&before)?;
    idle(d)?;
    let empty = document(d)?.clone();
    d.check(
        "Cut removes exactly the explicit descendant while retaining its empty parent",
        d.app().sequence_length() == 100
            && !empty.nodes().contains_key(&source)
            && empty.children(&group).next().is_none(),
        json!({"cut_node":source,"empty_parent":group,"frames":100}),
        state(d),
    )?;
    at(d, 70)?;
    let before_bank = bank(d)?;
    d.command("macro u")?;
    idle(d)?;
    d.check("Missing explicit ID refuses a named call without selected-beat fallback or bank publication",
        document(d)? == &empty && bank(d)? == before_bank && d.app().error.is_some(),
        json!({"missing":source,"no_write":true}), state(d))?;

    select(d, &group)?;
    let empty_label = d
        .app()
        .edit_range_label()
        .ok_or("Missing empty node label")?;
    d.check(
        "Empty node selection remains a distinct authored identity at a zero-width boundary",
        selected(d, &group, 20, 20)?
            && empty_label.contains("empty node retained")
            && d.app().edit_selection() == navigation::EditSelection::Object,
        json!({"empty_node":group,"range":[20,20]}),
        json!(empty_label),
    )?;
    d.capture("Empty node remains selectable by exact identity")?;
    d.key(Key::Y)?;
    idle(d)?;
    d.check(
        "Copy retains the complete empty structural node",
        copied(d, None)?.slice().selection()
            == &(deadpan_core::SliceCaptureSelection::Child {
                node: group.clone(),
            })
            && copied(d, None)?.slice().range().duration().frames() == 0,
        json!({"empty_node":group,"frames":0}),
        state(d),
    )?;
    let before = d.revision();
    d.key(Key::P)?;
    d.changed(&before)?;
    idle(d)?;
    d.check("Fast paste replaces a zero-duration node by identity rather than inserting at an equal-time seam",
        !document(d)?.nodes().contains_key(&group) && d.app().sequence_length() == 100
            && document(d)?.children(document(d)?.root()).count() == empty.children(empty.root()).count(),
        json!({"replaced_empty_node":group,"frames":100,"same_child_count":true}), state(d))?;
    undo(d, &empty)?;
    idle(d)?;
    select(d, &group)?;
    let before_bank = bank(d)?;
    let before_repeat = document(d)?.clone();
    d.command("wrap-repeat 2")?;
    idle(d)?;
    d.check(
        "Empty Repeat refuses without replacing the empty node or register bank",
        document(d)? == &before_repeat && bank(d)? == before_bank && d.app().error.is_some(),
        json!({"empty_node":group,"no_write":true}),
        state(d),
    )?;
    select(d, &group)?;
    let before = d.revision();
    d.command("delete")?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Cut removes an empty identity with zero duration change",
        !document(d)?.nodes().contains_key(&group) && d.app().sequence_length() == 100,
        json!({"removed_empty_node":group,"frames":100}),
        state(d),
    )?;
    undo(d, &empty)?;
    idle(d)?;
    select(d, &group)?;
    let before_bank = bank(d)?;
    let before_delayed = document(d)?.clone();
    let before_selection = d.app().capture_visual_selection()?;
    d.key(Key::Colon)?;
    d.app_mut().command = format!("select node={}", group.as_str());
    d.app_mut().sequence_cursor = 21;
    d.app_mut().request_picture(false);
    d.key(Key::Enter)?;
    idle(d)?;
    d.check(
        "Delayed node selection refuses a changed command context atomically",
        document(d)? == &before_delayed
            && bank(d)? == before_bank
            && d.app().capture_visual_selection()? == before_selection
            && d.app().sequence_cursor == 21
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("context changed")),
        json!({"node":group,"captured_cursor":20,"current_cursor":21}),
        state(d),
    )?;
    undo(d, &grouped)?;
    undo(d, &baseline)
}
