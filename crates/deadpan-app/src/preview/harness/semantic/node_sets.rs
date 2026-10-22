//! Native unions retain separate structural membership through copy, cut and paste.
use super::*;
use deadpan_core::{
    RegisterName, RegisterValue, SemanticInstruction, SemanticSelector, SemanticVisualSelection,
};
use deadpan_store::registers::RegisterBank;

fn idle(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Node-set service and bank receipts finish", |app| {
        !app.service.is_busy() && !app.macros.is_pending() && !app.copied.is_pending()
    })?;
    d.settled()
}
fn bank(d: &Driver<'_>) -> Result<RegisterBank, String> {
    ProjectStore::open(
        &d.app()
            .workspace
            .as_ref()
            .ok_or("Missing set workspace")?
            .path,
        AccessMode::ReadOnly,
    )
    .and_then(|store| store.registers())
    .map_err(|error| error.to_string())
}
fn generation_state(d: &Driver<'_>) -> Result<(usize, usize, usize, Option<u64>), String> {
    let store = ProjectStore::open(
        &d.app()
            .workspace
            .as_ref()
            .ok_or("Missing AI regression workspace")?
            .path,
        AccessMode::ReadOnly,
    )
    .map_err(|error| error.to_string())?;
    Ok((
        store
            .current_generation_requests()
            .map_err(|error| error.to_string())?
            .len(),
        store
            .generation_preparations(None, 256)
            .map_err(|error| error.to_string())?
            .len(),
        store
            .generation_intents(None, 256)
            .map_err(|error| error.to_string())?
            .len(),
        d.app().ai.job().map(|job| job.ticket),
    ))
}

fn command(nodes: &[NodeId]) -> Result<String, String> {
    serde_json::to_string(nodes)
        .map(|json| format!("select nodes={json}"))
        .map_err(|error| error.to_string())
}
fn select(d: &mut Driver<'_>, nodes: &[NodeId]) -> Result<(), String> {
    d.command(&command(nodes)?)?;
    idle(d)
}
fn range(d: &mut Driver<'_>, start: u64, frames: u64) -> Result<(), String> {
    at(d, start)?;
    d.key(Key::V)?;
    let mut input = frames
        .to_string()
        .bytes()
        .map(digit_key)
        .collect::<Vec<_>>();
    input.extend([Key::L, Key::V]);
    d.events("Choose exact fixture range", keys(&input))?;
    d.settled()
}
fn group(d: &mut Driver<'_>, label: &str) -> Result<NodeId, String> {
    let before = d.revision();
    d.command(&format!(
        "group name={}",
        serde_json::to_string(label).map_err(|error| error.to_string())?
    ))?;
    d.changed(&before)?;
    idle(d)?;
    d.app()
        .selected_beat
        .clone()
        .ok_or("Missing fixture group".into())
}
fn union(d: &Driver<'_>, expected: &[NodeId], frames: i64) -> Result<bool, String> {
    let saved = bank(d)?;
    let Some(RegisterValue::EditedSet { set }) = saved
        .entries
        .get(&RegisterName::unnamed())
        .map(AsRef::as_ref)
    else {
        return Ok(false);
    };
    let members = set
        .components()
        .iter()
        .filter_map(|slice| match slice.selection() {
            deadpan_core::SliceCaptureSelection::Child { node } => Some(node.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    Ok(members == expected
        && set.duration().frames() == frames
        && matches!(d.app().copied.content(), Some(Content::EditedSet(runtime)) if runtime == set))
}
pub(in crate::preview::harness) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 640.0));
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing set viewport")?
        .inner_rect = Some(rect);
    d.step("Paint node unions at minimum workspace", false)?;
    let baseline = document(d)?.clone();

    // Native fixture: two empty siblings at Edit 0, two positive descendants in
    // different ordinary parents, and an unselected source gap between them.
    at(d, 0)?;
    let outer = group(d, "Set scope")?;
    d.key(Key::Enter)?;
    idle(d)?;
    range(d, 0, 10)?;
    let empty_a = group(d, "Empty member")?;
    d.key(Key::Enter)?;
    idle(d)?;
    let before = d.revision();
    d.chord(&[Key::D, Key::D])?;
    d.changed(&before)?;
    idle(d)?;
    d.key(Key::Backspace)?;
    idle(d)?;
    d.command(&format!("select node={}", empty_a.as_str()))?;
    idle(d)?;
    let before_nodes = document(d)?
        .nodes()
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let before = d.revision();
    d.command("duplicate")?;
    d.changed(&before)?;
    idle(d)?;
    let empty_b = document(d)?
        .nodes()
        .iter()
        .find_map(|(id, node)| {
            (!before_nodes.contains(id)
                && matches!(&node.kind,NodeKind::Sequence {children} if children.is_empty()))
            .then(|| id.clone())
        })
        .ok_or("Empty duplicate has no distinct identity")?;
    // Ordinary Duplicate imports a neutral containing Sequence. Ungroup that
    // selected wrapper so both empty authored members are actual siblings.
    let before = d.revision();
    d.command("ungroup")?;
    d.changed(&before)?;
    idle(d)?;
    range(d, 10, 10)?;
    let first_parent = group(d, "First selected owner")?;
    let first = document(d)?
        .children(&first_parent)
        .next()
        .cloned()
        .ok_or("Missing first member")?;
    range(d, 40, 10)?;
    let second_parent = group(d, "Second selected owner")?;
    let second = document(d)?
        .children(&second_parent)
        .next()
        .cloned()
        .ok_or("Missing second member")?;
    d.key(Key::Backspace)?;
    idle(d)?;
    let fixture = document(d)?.clone();
    let requested = vec![
        second.clone(),
        empty_b.clone(),
        first.clone(),
        empty_a.clone(),
    ];
    let canonical = vec![
        empty_a.clone(),
        empty_b.clone(),
        first.clone(),
        second.clone(),
    ];
    let mut selected_subtrees = std::collections::BTreeSet::new();
    let mut pending = canonical.clone();
    while let Some(node) = pending.pop() {
        if selected_subtrees.insert(node.clone()) {
            pending.extend(fixture.children(&node).cloned());
        }
    }
    let unaffected = fixture
        .nodes()
        .iter()
        .filter(|(id, _)| !selected_subtrees.contains(id))
        .map(|(id, node)| (id.clone(), node.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    d.check("Native set fixture has separate parents, distinct equal-time empties and an unselected middle",
        d.app().sequence_length()==110 && d.app().sequence_scope.groups().is_empty()
            && first_parent != second_parent && empty_a != empty_b
            && fixture.children(&outer).any(|id|id==&empty_a)
            && fixture.children(&outer).any(|id|id==&empty_b),
        json!({"frames":110,"members":requested,"canonical":canonical,"ranges":[[0,0],[0,0],[10,20],[40,50]]}),state(d))?;
    at(d, 80)?;
    let old_bank = bank(d)?;
    select(d, &requested)?;
    let target = d
        .app()
        .selected_node_set()?
        .ok_or("Missing resolved node set")?;
    d.check(
        "Noncanonical IDs resolve into separate canonical members without authorizing the hull",
        document(d)? == &fixture
            && bank(d)? == old_bank
            && d.app().selected_edit_range().is_none()
            && d.app().capture_visual_selection()?
                == Some(SemanticVisualSelection::Nodes {
                    nodes: requested.clone(),
                })
            && target
                .members
                .iter()
                .map(|member| member.node.clone())
                .collect::<Vec<_>>()
                == canonical
            && target.total_duration.frames() == 20
            && target
                .members
                .iter()
                .map(|member| (member.range.start().0, member.range.end().0))
                .collect::<Vec<_>>()
                == [(0, 0), (0, 0), (10, 20), (40, 50)],
        json!({"nodes":requested,"selected_frames":20,"hull_authority":false}),
        state(d),
    )?;
    let description = d
        .app()
        .inspector_description()
        .ok_or("Missing node-set inspector")?;
    d.check(
        "Inspector lists every exact member, including both empty identities",
        description.kind == "Node set"
            && description.parameter.is_none()
            && canonical.iter().all(|id| {
                description
                    .fields
                    .iter()
                    .any(|(_, value)| value.contains(id.as_str()))
            })
            && description
                .fields
                .iter()
                .filter(|(label, _)| *label == "Selected member")
                .count()
                == 4,
        json!({"members":canonical,"parameter":null}),
        json!(description.fields),
    )?;
    d.capture("Four-member union shows separate highlights and empty structural members")?;
    let detail = d.app().node_set_details().ok_or("Missing set details")?[0].clone();
    let paint = scenarios::text_paint_visibility(d, &detail);
    d.check(
        "The first inspector member paints its complete identity and range without elision",
        !paint.is_empty()
            && paint
                .iter()
                .all(|part| part["fully_visible"] == true && part["elided"] == false),
        json!({"member":detail,"complete_paint":true}),
        json!(paint),
    )?;
    let before_loop_bank = bank(d)?;
    let before_loop_selection = d.app().capture_visual_selection()?;
    d.key_modified(Key::Space, egui::Modifiers::SHIFT)?;
    idle(d)?;
    d.check(
        "Loop selection refuses a node set without auditioning its navigation beat or gaps",
        document(d)? == &fixture
            && bank(d)? == before_loop_bank
            && d.app().capture_visual_selection()? == before_loop_selection
            && d.snapshot()["playback"].is_null()
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Looping a node set")),
        json!({"no_loop":true,"no_write":true}),
        state(d),
    )?;
    let before_selection = d.app().capture_visual_selection()?;
    d.key(Key::H)?;
    idle(d)?;
    d.check(
        "Finished node set survives independent cursor motion",
        d.app().capture_visual_selection()? == before_selection
            && d.app().selected_edit_range().is_none(),
        json!({"selection_retained":true,"hull_authority":false}),
        state(d),
    )?;
    for refused in [
        "group name=\"No hull\"",
        "duplicate",
        "wrap-repeat 2",
        "select role=audio",
        "gain -3",
        "splice",
    ] {
        let before_bank = bank(d)?;
        d.command(refused)?;
        idle(d)?;
        d.check(
            "Unsupported union consumer refuses without changing history, bank or exact selection",
            document(d)? == &fixture
                && bank(d)? == before_bank
                && d.app().capture_visual_selection()? == before_selection
                && d.app().error.is_some(),
            json!({"command":refused,"no_write":true}),
            state(d),
        )?;
    }
    d.key(Key::Escape)?;
    at(d, 11)?;
    d.key(Key::Enter)?;
    idle(d)?;
    at(d, 11)?;
    d.key(Key::Enter)?;
    idle(d)?;
    let scoped_bank = bank(d)?;
    let scoped_selection = d.app().capture_visual_selection()?;
    select(d, &requested)?;
    d.check(
        "An explicit set refuses members outside the captured inner scope",
        document(d)? == &fixture
            && bank(d)? == scoped_bank
            && d.app().capture_visual_selection()? == scoped_selection
            && d.app().error.is_some(),
        json!({"scope":first_parent,"outside_members":requested,"no_write":true}),
        state(d),
    )?;
    d.key(Key::Backspace)?;
    d.key(Key::Backspace)?;
    idle(d)?;
    select(d, &requested)?;
    d.key(Key::Y)?;
    idle(d)?;
    d.check(
        "Yank publishes one union value in canonical structural order without changing history",
        document(d)? == &fixture && union(d, &canonical, 20)?,
        json!({"canonical":canonical,"frames":20,"value_type":"edited_set"}),
        state(d),
    )?;
    let before_bank = bank(d)?;
    d.command("framing-save a")?;
    idle(d)?;
    d.check(
        "Framing preset save refuses a set without saving one navigation beat",
        document(d)? == &fixture
            && bank(d)? == before_bank
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Clear the set")),
        json!({"no_preset_write":true}),
        state(d),
    )?;
    d.key(Key::P)?;
    idle(d)?;
    d.check(
        "Active-set replacement refuses instead of converting the union into an enclosing range",
        document(d)? == &fixture
            && bank(d)? == before_bank
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Clear the set")),
        json!({"no_replacement":true}),
        state(d),
    )?;

    d.command("record u")?;
    select(d, &requested)?;
    d.key(Key::Y)?;
    idle(d)?;
    let before = d.revision();
    d.key(Key::D)?;
    d.changed(&before)?;
    idle(d)?;
    d.key(Key::Q)?;
    idle(d)?;
    let cut = document(d)?.clone();
    let stored = bank(d)?;
    d.check(
        "Recorded union cut removes only its separate members in one authored transaction",
        d.app().sequence_length() == 90
            && canonical.iter().all(|id| !cut.nodes().contains_key(id))
            && unaffected
                .iter()
                .filter(|(id, _)| {
                    *id != fixture.root()
                        && *id != &outer
                        && *id != &first_parent
                        && *id != &second_parent
                })
                .all(|(id, node)| cut.nodes().get(id) == Some(node))
            && union(d, &canonical, 20)?
            && matches!(stored.entries.get(&RegisterName::new('u').unwrap()).map(AsRef::as_ref),
                Some(RegisterValue::Macro {program}) if matches!(program.instructions(),[
                    SemanticInstruction::SelectNodes {nodes:a},
                    SemanticInstruction::Yank {selector:SemanticSelector::Nodes {nodes:b},..},
                    SemanticInstruction::Cut {selector:SemanticSelector::Nodes {nodes:c},..}
                ] if a==&requested && b==&requested && c==&requested)),
        json!({"removed":canonical,"remaining_frames":90,"macro":"u"}),
        state(d),
    )?;
    // Seam Paste is currently an unsupported dot transition and clears the
    // candidate. Qualify Cut dot before that independent operation.
    undo(d, &fixture)?;
    idle(d)?;
    // Undo verified the authored fixture but intentionally created a fresh head.
    let fixture = document(d)?.clone();
    at(d, 80)?;
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    idle(d)?;
    d.check("Dot re-resolves the exact ID list from another cursor after Undo",
        d.app().sequence_length()==90 && union(d,&canonical,20)?
            && d.app().semantic.snapshot().and_then(|snapshot|snapshot.edit.as_ref()).is_some_and(|last|
                matches!(&last.operation,RepeatableEdit::Cut(RepeatableCut::Selector(SemanticSelector::Nodes {nodes})) if nodes==&requested)),
        json!({"nodes":requested,"frames":90}),state(d))?;
    let cut = document(d)?.clone();
    at(d, 70)?;
    let before = d.revision();
    d.key(Key::P)?;
    d.changed(&before)?;
    idle(d)?;
    let pasted = document(d)?;
    d.check(
        "After clearing selection, seam paste inserts the complete editable union in one operation",
        d.app().sequence_length() == 110
            && canonical.iter().all(|id| !pasted.nodes().contains_key(id))
            && union(d, &canonical, 20)?,
        json!({"pasted_frames":20,"frames":110,"historical_ids_remapped":true}),
        state(d),
    )?;
    undo(d, &cut)?;
    idle(d)?;
    let restored_cut = document(d)?.clone();
    let before_bank = bank(d)?;
    d.command("macro u")?;
    idle(d)?;
    d.check(
        "A named union call refuses deleted IDs without beat fallback or partial bank writes",
        document(d)? == &restored_cut && bank(d)? == before_bank && d.app().error.is_some(),
        json!({"missing":requested,"no_write":true}),
        state(d),
    )?;
    undo(d, &fixture)?;
    idle(d)?;
    // Undo verified the authored fixture but intentionally created a fresh head.
    let fixture = document(d)?.clone();
    at(d, 75)?;
    let before = d.revision();
    d.command("macro u")?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Named call re-resolves the complete union from current staged identities",
        d.app().sequence_length() == 90 && union(d, &canonical, 20)?,
        json!({"macro":"u","frames":90}),
        state(d),
    )?;
    undo(d, &fixture)?;
    idle(d)?;
    // Undo verified the authored fixture but intentionally created a fresh head.
    let fixture = document(d)?.clone();
    select(d, &requested)?;
    let before_bank = bank(d)?;
    let before_selection = d.app().capture_visual_selection()?;
    let prior = d.app().sequence_cursor;
    d.key(Key::Colon)?;
    d.app_mut().command = command(&requested)?;
    d.app_mut().sequence_cursor = prior + 1;
    d.app_mut().request_picture(false);
    d.key(Key::Enter)?;
    idle(d)?;
    d.check(
        "Delayed set selection retains entry absence and context without late retargeting",
        document(d)? == &fixture
            && bank(d)? == before_bank
            && d.app().capture_visual_selection()? == before_selection
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("context changed")),
        json!({"entry_cursor":prior,"changed_cursor":prior+1,"no_write":true}),
        state(d),
    )?;
    // A real eligible Hold distinguishes set refusal from an already-ineligible
    // Source. Retain its pre-set AI target to exercise direct UI dispatch too.
    let prior_cursor = d.app().sequence_cursor;
    let prior_selected = d.app().selected_beat.clone();
    let prior_scope = d.app().sequence_scope.clone();
    let prior_selection = d.app().capture_visual_selection()?;
    at(d, 110)?;
    let before = d.revision();
    d.command("hold 2f")?;
    d.changed(&before)?;
    idle(d)?;
    let with_hold = document(d)?.clone();
    let hold = d
        .app()
        .selected_beat
        .clone()
        .ok_or("AI regression has no selected Hold")?;
    let eligible = d.app().ai_capture()?;
    d.check(
        "The node-set AI regression starts from an actually eligible ordinary Hold",
        matches!(
            with_hold.nodes().get(&hold).map(|node| &node.kind),
            Some(NodeKind::Hold { .. })
        ) && d.app().ai_hold() == Some(hold.clone()),
        json!({"hold":hold,"eligible_before_set":true}),
        state(d),
    )?;
    select(d, std::slice::from_ref(&hold))?;
    let set_selection = d.app().capture_visual_selection()?;
    let hold_bank = bank(d)?;
    let before_jobs = generation_state(d)?;
    d.check(
        "A one-member node set containing that same Hold revokes AI targeting",
        d.app().selected_beat.as_ref() == Some(&hold)
            && d.app().ai_hold().is_none()
            && set_selection
                == Some(SemanticVisualSelection::Nodes {
                    nodes: vec![hold.clone()],
                }),
        json!({"hold":hold,"eligible_after_set":false}),
        state(d),
    )?;
    d.command("generate-ai")?;
    idle(d)?;
    d.check(
        "AI command refuses an eligible-Hold node set without history, bank, preparation, request or job changes",
        document(d)? == &with_hold && bank(d)? == hold_bank
            && generation_state(d)? == before_jobs
            && d.app().capture_visual_selection()? == set_selection
            && d.app().error.as_deref().is_some_and(|error| error.contains("node set")),
        json!({"hold":hold,"no_write":true,"no_generation_work":true}),
        state(d),
    )?;
    d.app_mut().ai_action(
        crate::navigation::AiAction::Generate { variants: 1 },
        Some(Ok(eligible)),
    );
    idle(d)?;
    d.check(
        "Direct AI dispatch rejects a formerly eligible captured Hold while its node set is active",
        document(d)? == &with_hold
            && bank(d)? == hold_bank
            && generation_state(d)? == before_jobs
            && d.app().capture_visual_selection()? == set_selection
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("node set")),
        json!({"hold":hold,"captured_before_set":true,"no_write":true,"no_generation_work":true}),
        state(d),
    )?;
    undo(d, &fixture)?;
    idle(d)?;
    // Undo verified the authored fixture but intentionally created a fresh head.
    let fixture = document(d)?.clone();
    select(d, &requested)?;
    d.app_mut().sequence_cursor = prior_cursor;
    d.app_mut().request_picture(false);
    idle(d)?;
    d.check(
        "Temporary AI regression restores the existing document, scope, cursor and exact set",
        document(d)? == &fixture
            && d.app().sequence_scope == prior_scope
            && d.app().sequence_cursor == prior_cursor
            && d.app().selected_beat == prior_selected
            && d.app().capture_visual_selection()? == prior_selection,
        json!({"temporary_hold_removed":true,"selection_restored":true}),
        state(d),
    )?;
    // Every fixture edit is one native transaction; restore the protected full Original.
    while d
        .app()
        .workspace
        .as_ref()
        .ok_or("Missing fixture")?
        .can_undo
    {
        d.key(Key::Escape)?;
        let before = d.revision();
        d.key(Key::U)?;
        d.changed(&before)?;
        idle(d)?;
    }
    d.check(
        "Node-set replay restores the complete Original baseline",
        same_document(document(d)?, &baseline)? && d.app().sequence_length() == 120,
        json!({"frames":120,"undo":false}),
        state(d),
    )
}
