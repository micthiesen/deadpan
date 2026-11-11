//! The shared default gap after one play is its own inspector row: `j`
//! reaches it, Enter changes that play's gap length in one undoable edit and
//! the other plays keep the shared gap.
use super::*;

const CHANGE: &str = "Change length…  Enter";

pub(super) fn run(d: &mut Driver<'_>, repeat: &NodeId) -> Result<(), String> {
    while d.app().scoped.is_some() {
        d.key(Key::Escape)?;
        idle(d)?;
    }
    d.chord(&[Key::G, Key::G])?;
    idle(d)?;
    let before = d.revision();
    d.command("repeat 3 gap=6f")?;
    d.changed(&before)?;
    idle(d)?;
    let shared = document(d)?.clone();
    let total = shared.duration().map_err(|error| error.to_string())?;
    d.key(Key::Enter)?;
    idle(d)?;
    d.chord(&[Key::CloseBracket, Key::R, Key::CloseBracket, Key::R])?;
    idle(d)?;
    d.key(Key::J)?;
    idle(d)?;
    let row = d
        .app()
        .scoped_shared_gap()
        .map(|(recipe, play, _)| (recipe.duration.frames(), play));
    d.check(
        "j in play 2 reaches the shared gap after it as its own row",
        row == Some((6, 2)) && d.app().error.is_none(),
        json!({"gap_frames":6,"after_play":2}),
        json!({"row":row,"error":d.app().error}),
    )?;
    painted(d, "Gap after play 2")?;
    painted(d, CHANGE)?;
    d.capture("The shared gap after play 2 selected inside its play")?;
    footer_teaches_only_the_gap_row(d)?;
    let revision = d.revision();
    d.command("gain +3dB")?;
    idle(d)?;
    d.check(
        "Other edits on the shared gap row refuse readably without saving",
        d.revision() == revision
            && d.app().error.as_deref() == Some(crate::preview::scoped::SHARED_GAP_ONLY),
        json!({"history":"unchanged","error":crate::preview::scoped::SHARED_GAP_ONLY}),
        json!({"error":d.app().error}),
    )?;
    d.key(Key::Enter)?;
    d.check(
        "Enter on the shared gap teaches its current length",
        d.app().command_open && d.app().command == "hold-duration 6f",
        json!("hold-duration 6f"),
        json!({"open":d.app().command_open,"command":d.app().command}),
    )?;
    d.key_modified(Key::A, egui::Modifiers::COMMAND)?;
    d.events(
        "Replace the shared gap's length",
        vec![
            egui::Event::Text("hold-duration 12f".into()),
            super::super::key_event(Key::Enter, egui::Modifiers::NONE, true),
            super::super::key_event(Key::Enter, egui::Modifiers::NONE, false),
        ],
    )?;
    d.changed(&revision)?;
    d.wait_for("Shared gap receipt is admitted", |app| {
        !app.service.is_busy() && !app.macros.is_pending()
    })?;
    d.settled()?;
    let document = document(d)?.clone();
    let NodeKind::Repeat {
        iterations, gap, ..
    } = &document.nodes()[repeat].kind
    else {
        return Err("Expected the Repeat".into());
    };
    let second = iterations.at(1).ok_or("The Repeat has no second play")?;
    let owned = document.gap_overrides().get(repeat);
    let own = owned.and_then(|plays| plays.get(&second)).cloned();
    let own_frames = own
        .as_ref()
        .and_then(|id| match &document.nodes()[id].kind {
            NodeKind::Hold { recipe } => Some(recipe.duration.frames()),
            _ => None,
        });
    let selected = d
        .app()
        .scoped
        .as_ref()
        .map(|state| state.selected_target().node.clone());
    let grown = document
        .duration()
        .map_err(|error| error.to_string())?
        .frames()
        - total.frames();
    d.check(
        "Enter gives play 2 its own 12-frame gap in one edit; plays 1 and 3 keep the shared 6",
        owned.map_or(0, |plays| plays.len()) == 1
            && own_frames == Some(12)
            && gap.as_ref().map(|gap| gap.duration.frames()) == Some(6)
            && grown == 6
            && selected.is_some()
            && selected == own
            && d.app().scoped_shared_gap().is_none()
            && d.app().error.is_none(),
        json!({"own_gap_frames":12,"shared_gap_frames":6,"grown":6,"selected":"play 2's gap"}),
        json!({"own_gap_frames":own_frames,"owned":owned.map(|plays| plays.len()),"grown":grown,"selected":selected,"error":d.app().error}),
    )?;
    painted(d, "Change duration…  Enter")?;
    d.capture("Play 2's gap is now its own 12-frame pause")?;
    dot_on_another_gap_row(d, repeat)?;
    gap_row_sound(d, repeat)?;
    let edited = d.revision();
    d.key(Key::U)?;
    d.changed(&edited)?;
    idle(d)?;
    let restored = document_value(d)?;
    d.check(
        "One Undo restores the shared gap exactly",
        restored == normalized(&shared, d)?,
        json!("document before the gap edit"),
        json!({"gap_overrides":d.app().workspace.as_ref().map(|w| w.document.gap_overrides().len())}),
    )?;
    let edited = d.revision();
    d.key(Key::U)?;
    d.changed(&edited)?;
    idle(d)
}

/// The footer and Keys context on the gap row teach its own edits and
/// navigation only; contents edits that refuse there are not offered.
fn footer_teaches_only_the_gap_row(d: &mut Driver<'_>) -> Result<(), String> {
    // The one-row footer keeps the gap's own edits; navigation and Undo
    // are in Help and the header.
    let taught = [
        "change length",
        ":room-tone",
        ":hold-silence",
        ":isolate-gap",
    ];
    let refused = [
        "gain 3 dB",
        "mute / unmute",
        "punch in / creep",
        "split in this play",
        "camera",
    ];
    let visible = |d: &Driver<'_>, label: &str| {
        scenarios::text_paint_visibility(d, label)
            .iter()
            .any(|part| part["fully_visible"] == true)
    };
    let missing: Vec<_> = taught.iter().filter(|label| !visible(d, label)).collect();
    let shown: Vec<_> = refused
        .iter()
        .filter(|label| !scenarios::text_paint_visibility(d, label).is_empty())
        .collect();
    let context = d.app().help_context();
    d.check(
        "The footer on the shared gap row teaches only the gap's edits and navigation",
        missing.is_empty()
            && shown.is_empty()
            && context == crate::navigation::registry::Contexts::SHARED_GAP,
        json!({"taught":taught,"absent":refused,"keys_context":"Shared gap row"}),
        json!({"missing":missing,"shown":shown,"keys_context":context.label()}),
    )?;
    scenarios::footer_anchored(d, "The shared gap row footer fits on its first paint")
}

/// Select the shared gap row after `play` (one-based) from the top level.
fn gap_row(d: &mut Driver<'_>, play: u32) -> Result<(), String> {
    while d.app().scoped.is_some() {
        d.key(Key::Escape)?;
        idle(d)?;
    }
    d.chord(&[Key::G, Key::G])?;
    d.key(Key::Enter)?;
    for _ in 0..play {
        d.chord(&[Key::CloseBracket, Key::R])?;
    }
    d.key(Key::J)?;
    idle(d)?;
    let row = d.app().scoped_shared_gap().map(|(_, after, _)| after);
    if row != Some(play) {
        return Err(format!(
            "Expected the shared gap row after play {play}, found {row:?}"
        ));
    }
    Ok(())
}

/// Each play's own gap pause as (one-based play, frames, sound), and the
/// shared gap's length and sound.
fn gaps(d: &Driver<'_>, repeat: &NodeId) -> Result<serde_json::Value, String> {
    let document = document(d)?;
    let NodeKind::Repeat {
        iterations, gap, ..
    } = &document.nodes()[repeat].kind
    else {
        return Err("Expected the Repeat".into());
    };
    let sound = |audio: &deadpan_core::HoldAudio| match audio {
        deadpan_core::HoldAudio::Silence => "silence",
        deadpan_core::HoldAudio::RoomTone { .. } => "room tone",
        _ => "other",
    };
    let mut own = Vec::new();
    for index in 0..iterations.len() {
        let play = iterations.at(index).ok_or("Missing Repeat play")?;
        let Some(node) = document
            .gap_overrides()
            .get(repeat)
            .and_then(|plays| plays.get(&play))
        else {
            continue;
        };
        let NodeKind::Hold { recipe } = &document.nodes()[node].kind else {
            return Err("An owned gap is not a pause".into());
        };
        own.push(json!([
            index + 1,
            recipe.duration.frames(),
            sound(&recipe.audio)
        ]));
    }
    let shared = gap
        .as_ref()
        .map(|gap| json!([gap.duration.frames(), sound(&gap.audio)]));
    Ok(json!({"own":own,"shared":shared}))
}

/// `.` on play 1's shared gap row repeats play 2's length change there only.
fn dot_on_another_gap_row(d: &mut Driver<'_>, repeat: &NodeId) -> Result<(), String> {
    let before = gaps(d, repeat)?;
    gap_row(d, 1)?;
    painted(d, "Gap after play 1")?;
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    idle(d)?;
    let expected = json!({"own":[[1,12,"silence"],[2,12,"silence"]],"shared":[6,"silence"]});
    d.check(
        ". on play 1's shared gap row gives only play 1 its own 12-frame gap",
        gaps(d, repeat)? == expected && d.app().error.is_none(),
        expected.clone(),
        json!({"gaps":gaps(d, repeat)?,"error":d.app().error}),
    )?;
    d.capture("Dot repeats the gap length after play 1")?;
    let dotted = d.revision();
    d.key(Key::U)?;
    d.changed(&dotted)?;
    idle(d)?;
    d.check(
        "One Undo removes the dotted gap and keeps play 2's",
        gaps(d, repeat)? == before,
        before.clone(),
        gaps(d, repeat)?,
    )
}

/// `:hold-silence` refuses on an already silent shared gap; `:room-tone` on
/// play 1's gap row gives only that gap its own room tone in one Undo.
fn gap_row_sound(d: &mut Driver<'_>, repeat: &NodeId) -> Result<(), String> {
    let before = gaps(d, repeat)?;
    while d.app().scoped.is_some() {
        d.key(Key::Escape)?;
        idle(d)?;
    }
    let original = d.app().source_cursor;
    d.command("source")?;
    d.chord(&[
        Key::G,
        Key::G,
        Key::Num1,
        Key::Num0,
        Key::L,
        Key::V,
        Key::Num1,
        Key::Num0,
        Key::L,
        Key::Y,
    ])?;
    d.wait_for("Copy a quiet Original range for room tone", |app| {
        !app.service.is_busy() && !app.copied.is_pending()
    })?;
    d.command("sequence")?;
    idle(d)?;
    gap_row(d, 1)?;
    let revision = d.revision();
    d.command("hold-silence")?;
    d.wait_for("The gap row's silence refusal is admitted", |app| {
        !app.service.is_busy() && !app.macros.is_pending()
    })?;
    idle(d)?;
    d.check(
        ":hold-silence on the already silent shared gap refuses without saving",
        d.revision() == revision
            && [&d.app().error, &d.app().message].iter().any(|text| {
                text.as_deref()
                    .is_some_and(|text| text.contains("already has that sound"))
            }),
        json!({"history":"unchanged","refusal":"already has that sound"}),
        json!({"error":d.app().error,"message":d.app().message,"revision_unchanged":d.revision() == revision}),
    )?;
    d.command("room-tone")?;
    d.wait_for("The gap row's room tone is prepared", |app| {
        !app.service.is_busy()
            && app
                .room_tone
                .as_ref()
                .is_some_and(|draft| draft.prepared.is_some())
    })?;
    scenarios::footer_anchored(d, "The gap row's room tone sheet keeps the footer anchored")?;
    d.capture("Room tone prepared for the shared gap after play 1")?;
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    idle(d)?;
    let expected = json!({"own":[[1,6,"room tone"],[2,12,"silence"]],"shared":[6,"silence"]});
    d.check(
        ":room-tone on play 1's gap row gives only that gap its own room tone",
        gaps(d, repeat)? == expected && d.app().room_tone.is_none(),
        expected.clone(),
        json!({"gaps":gaps(d, repeat)?,"error":d.app().error}),
    )?;
    let toned = d.revision();
    d.key(Key::U)?;
    d.changed(&toned)?;
    idle(d)?;
    d.check(
        "One Undo restores play 1's shared silent gap",
        gaps(d, repeat)? == before,
        before.clone(),
        gaps(d, repeat)?,
    )?;
    // Leave the Original cursor where the later scenarios expect it.
    d.command("source")?;
    d.chord(&[Key::G, Key::G])?;
    if original > 0 {
        let mut keys = original
            .to_string()
            .chars()
            .map(|c| Key::from_name(&c.to_string()).ok_or("Invalid frame digit".to_owned()))
            .collect::<Result<Vec<_>, _>>()?;
        keys.push(Key::L);
        d.chord(&keys)?;
    }
    d.command("sequence")?;
    idle(d)
}

fn document_value(d: &Driver<'_>) -> Result<serde_json::Value, String> {
    serde_json::to_value(document(d)?).map_err(|error| error.to_string())
}

/// The expected document at the current revision identity.
fn normalized(expected: &ProjectDocument, d: &Driver<'_>) -> Result<serde_json::Value, String> {
    let mut value = serde_json::to_value(expected).map_err(|error| error.to_string())?;
    value["revision_id"] = json!(document(d)?.revision_id());
    Ok(value)
}
