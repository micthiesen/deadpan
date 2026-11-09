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

fn document_value(d: &Driver<'_>) -> Result<serde_json::Value, String> {
    serde_json::to_value(document(d)?).map_err(|error| error.to_string())
}

/// The expected document at the current revision identity.
fn normalized(expected: &ProjectDocument, d: &Driver<'_>) -> Result<serde_json::Value, String> {
    let mut value = serde_json::to_value(expected).map_err(|error| error.to_string())?;
    value["revision_id"] = json!(document(d)?.revision_id());
    Ok(value)
}
