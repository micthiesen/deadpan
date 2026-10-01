//! Native event order, repeat suppression, button focus and composition in Move.

use super::*;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let saved = document(d)?.clone();
    d.key(Key::F)?;
    let id = draft(d)?.proposal_for_check().id.clone();
    batch(d, &[Key::Num1, Key::Num1, Key::L])?;
    d.check(
        "One native Move picture batch consumes both count digits once",
        draft(d)?.cursor == 61 && draft(d)?.proposal_for_check().id == id,
        json!({"cursor":61,"same_proposal":true}),
        state(d),
    )?;
    // egui derives repeat from keys_down and overwrites the supplied flag.
    // Hold modified initial presses across frames, as the deletion replay does,
    // then release the modifier while the keys remain physically held.
    let held = [Key::M, Key::S, Key::F, Key::Enter, Key::Num2];
    d.events(
        "Hold modified Move, site, Apply and count keys without releasing them",
        held.iter()
            .map(|key| key_event(*key, Modifiers::ALT, true))
            .collect(),
    )?;
    d.check(
        "egui retains every held key without routing its modified initial press",
        d.harness
            .ctx
            .input(|input| held.iter().all(|key| input.key_down(*key)))
            && draft(d)?.proposal_for_check().id == id
            && draft(d)?.cursor == 61
            && *document(d)? == saved,
        json!({"held":["M","S","F","Enter","2"],"cursor":61}),
        state(d),
    )?;
    d.events(
        "Deliver unmodified repeats of the genuinely held Move keys",
        held.iter()
            .map(|key| key_event(*key, Modifiers::NONE, true))
            .collect(),
    )?;
    d.check(
        "Repeated operation, site, Apply and count keys do not mutate Move or create history",
        draft(d)?.proposal_for_check().id == id
            && draft(d)?.proposal_for_check().operation == Operation::Move
            && draft(d)?.site_for_check() == "insertion"
            && draft(d)?.cursor == 61
            && *document(d)? == saved,
        json!({"operation":"Move","site":"insertion","cursor":61}),
        state(d),
    )?;
    d.events(
        "Release the held Move operation and site keys",
        held.iter()
            .map(|key| key_event(*key, Modifiers::NONE, false))
            .collect(),
    )?;
    d.events(
        "Hold h after the ignored repeated count",
        vec![key_event(Key::H, Modifiers::NONE, true)],
    )?;
    d.check(
        "The initial h press inspects one frame without an inherited repeat count",
        draft(d)?.cursor == 60 && draft(d)?.proposal_for_check().id == id,
        json!(60),
        state(d),
    )?;
    d.events(
        "Repeat h while its initial key remains held",
        vec![key_event(Key::H, Modifiers::NONE, true)],
    )?;
    d.check(
        "Held h repeats one-frame inspection through the production Move router",
        draft(d)?.cursor == 59 && draft(d)?.proposal_for_check().id == id,
        json!(59),
        state(d),
    )?;
    d.events(
        "Release held h after its repeated inspection",
        vec![key_event(Key::H, Modifiers::NONE, false)],
    )?;

    d.key(Key::I)?;
    d.key(Key::L)?;
    wait_ready(d)?;
    let refined = draft(d)?.proposal_for_check().id.clone();
    d.check(
        "Move source refinement advances the proposal identity",
        refined.draft == id.draft
            && refined.change > id.change
            && draft(d)?.proposal_for_check().source.boundaries()? == (21..30),
        json!([21, 30]),
        state(d),
    )?;
    d.key(Key::H)?;
    wait_ready(d)?;
    d.key(Key::D)?;
    batch(d, &[Key::Num1, Key::Num1, Key::L])?;
    wait_ready(d)?;
    d.check(
        "Counted Move destination motion uses the saved pre-edit clock",
        prepared(d)?
            .movement
            .as_ref()
            .is_some_and(|movement| movement.destination_before == ProjectFrame(71))
            && prepared(d)?.range == range(61, 71)?,
        json!({"destination":71,"inserted":[61,71]}),
        state(d),
    )?;
    batch(d, &[Key::Num1, Key::Num1, Key::H])?;
    wait_ready(d)?;

    focus_with_tab(d, "Copy instead · m")?;
    let focused_id = draft(d)?.proposal_for_check().id.clone();
    batch(d, &[Key::S, Key::M])?;
    d.check(
        "Focused native operation buttons keep letter shortcuts out of the heading router",
        draft(d)?.proposal_for_check().id == focused_id
            && draft(d)?.proposal_for_check().operation == Operation::Move,
        json!("Move retained while button focused"),
        state(d),
    )?;
    d.key(Key::Enter)?;
    wait_ready(d)?;
    d.check(
        "Native Enter activates Copy instead once without applying the draft",
        draft(d)?.proposal_for_check().operation == Operation::Copy
            && prepared(d)?.range == range(60, 70)?
            && *document(d)? == saved,
        json!("Copy at 60, saved unchanged"),
        state(d),
    )?;
    focus_with_tab(d, HEADING)?;
    d.key(Key::M)?;
    wait_ready(d)?;
    d.key(Key::F)?;
    let id = draft(d)?.proposal_for_check().id.clone();
    d.events(
        "Start synthetic IME composition with Move and removal keys in the same batch",
        vec![
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "move".into(),
                active_range_chars: Some(0..4),
            }),
            key_event(Key::M, Modifiers::NONE, true),
            key_event(Key::M, Modifiers::NONE, false),
            key_event(Key::S, Modifiers::NONE, true),
            key_event(Key::S, Modifiers::NONE, false),
        ],
    )?;
    d.check(
        "Composition owns Move and site letters",
        d.app().ime_composing
            && draft(d)?.proposal_for_check().id == id
            && draft(d)?.site_for_check() == "insertion",
        json!({"composing":true,"proposal_unchanged":true}),
        state(d),
    )?;
    d.events(
        "Complete composition with Apply and Move presses in the same native batch",
        vec![
            egui::Event::Ime(egui::ImeEvent::Commit("move".into())),
            key_event(Key::Enter, Modifiers::NONE, true),
            key_event(Key::Enter, Modifiers::NONE, false),
            key_event(Key::M, Modifiers::NONE, true),
            key_event(Key::M, Modifiers::NONE, false),
        ],
    )?;
    d.check(
        "Composition completion cannot leak Move or Apply into the draft",
        !d.app().ime_composing && draft(d)?.proposal_for_check().id == id && *document(d)? == saved,
        json!({"composing":false,"saved_unchanged":true}),
        state(d),
    )?;
    d.key_modified(Key::M, Modifiers::COMMAND)?;
    d.check(
        "Modified m remains outside the Move router",
        draft(d)?.proposal_for_check().id == id,
        json!("proposal unchanged"),
        state(d),
    )?;
    focus_with_tab(d, "Removal join · s")?;
    d.key(Key::Enter)?;
    d.check(
        "Native Enter selects removal without rebuilding the Move proposal",
        draft(d)?.site_for_check() == "removal"
            && draft(d)?.cursor == 20
            && draft(d)?.proposal_for_check().id == id,
        json!({"site":"removal","cursor":20}),
        state(d),
    )?;
    focus_with_tab(d, HEADING)?;
    d.key(Key::F)
}

fn batch(d: &mut Driver<'_>, keys: &[Key]) -> Result<(), String> {
    d.events(
        &format!("Deliver one Move input batch {keys:?}"),
        keys.iter()
            .flat_map(|key| {
                [
                    key_event(*key, Modifiers::NONE, true),
                    key_event(*key, Modifiers::NONE, false),
                ]
            })
            .collect(),
    )
}
