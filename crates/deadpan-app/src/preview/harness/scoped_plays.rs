//! Real scoped authoring through native keyboard routing, SQLite and Metal.

use super::*;
use deadpan_core::{GainDb, NodeId, NodeKind, ProjectDocument};
use egui::Key;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    idle(d)?;
    let before = d.revision();
    d.command("wrap-repeat 3")?;
    d.changed(&before)?;
    idle(d)?;
    let repeat = d.app().selected_beat.clone().ok_or("No Repeat selected")?;
    let shared = match &document(d)?.nodes()[&repeat].kind {
        NodeKind::Repeat { child, .. } => child.clone(),
        _ => return Err("Expected a Repeat".into()),
    };
    let before = document(d)?.clone();
    d.key(Key::Enter)?;
    d.command("scope play 2")?;
    idle(d)?;
    d.check(
        "Entering play two is read-only and displays its concrete first picture",
        document(d)? == &before
            && d.app().scoped.is_some()
            && d.app().sequence_cursor == 120
            && d.app().presentation.displayed_source_frame() == Some(SourceFrameId(0)),
        json!({"Edit":120,"Original_picture":0,"history":"unchanged"}),
        d.snapshot(),
    )?;
    resize(d, 960.0, 640.0)?;
    painted(d, "This play 2/3")?;
    painted(d, "All plays  :scope all")?;
    painted(d, "Previous")?;
    painted(d, "Next")?;
    painted(d, "Repeat [play 2/3]")?;
    painted(d, "1. cfr-bframes.mp4 · Source")?;
    scenarios::footer_anchored(d, "Scoped controls keep the minimum-size footer anchored")?;
    d.capture("All plays and This play remain visible at minimum size")?;
    d.command("gain")?;
    d.wait_for("Unchanged scoped gain proposal is ready", |app| {
        app.gain
            .as_ref()
            .is_some_and(|draft| draft.prepared_snapshot().is_some())
    })?;
    painted(d, "Repeat · play 2/3")?;
    d.check(
        "Opening Gain at an untouched play creates no isolation or authored history",
        document(d)? == &before && document(d)?.overrides().is_empty(),
        json!("unchanged document and no overrides"),
        d.snapshot(),
    )?;
    scenarios::footer_anchored(
        d,
        "Scoped Gain retains its scope above the minimum-size footer",
    )?;
    d.capture("Gain draft names its exact Repeat play at minimum size")?;
    d.key(Key::Escape)?;
    resize(d, 1280.0, 820.0)?;
    scoped_marks(d)?;
    let old_target = d.app().scoped_target()?.ok_or("No scoped target")?;
    d.command("sounds")?;
    d.key(Key::J)?;
    d.key(Key::H)?;
    d.check(
        "Placed sounds focus keeps nested picture navigation untouched",
        d.app().pane == Pane::Sounds && d.app().scoped_target()?.as_ref() == Some(&old_target),
        json!({"pane":"Sounds","scoped_target":"unchanged"}),
        d.snapshot(),
    )?;
    d.command("sequence")?;
    let before = d.revision();
    d.command("gain -6")?;
    d.changed(&before)?;
    idle(d)?;
    let isolated = d
        .app()
        .scoped_target()?
        .ok_or("Commit lost scoped inspector")?;
    d.check(
        "This play gain creates one override and follows its remapped identity",
        isolated.target.node != shared
            && isolated.target.node != old_target.target.node
            && d.app().selected_beat.as_ref() == Some(&repeat)
            && d.app().sequence_cursor == 120
            && gain(d, &isolated.target.node)? == crate::gain::parse_db("-6")?
            && gain(d, &shared)? == crate::gain::parse_db("0")?
            && document(d)?
                .overrides()
                .get(&repeat)
                .is_some_and(|overrides| overrides.len() == 1),
        json!({"overrides":1,"this_play_db":-6,"shared_db":0,"Edit":120}),
        d.snapshot(),
    )?;
    d.capture("This play has independent gain and a visible scope label")?;

    let before = document(d)?.clone();
    d.key(Key::D)?;
    d.key(Key::D)?;
    idle(d)?;
    d.check(
        "Unsupported temporal commands cannot delete the outer Repeat from inside it",
        document(d)? == &before && d.app().scoped.is_some(),
        json!("unchanged"),
        d.snapshot(),
    )?;
    d.command("scope all")?;
    let before = d.revision();
    d.command("gain -3")?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "All plays changes shared content and preserves the existing override",
        gain(d, &shared)? == crate::gain::parse_db("-3")?
            && gain(d, &isolated.target.node)? == crate::gain::parse_db("-6")?
            && document(d)?
                .overrides()
                .get(&repeat)
                .is_some_and(|overrides| overrides.len() == 1),
        json!({"shared_db":-3,"this_play_db":-6,"overrides":1}),
        d.snapshot(),
    )?;

    d.command("scope play 3")?;
    d.key(Key::L)?;
    idle(d)?;
    d.check(
        "Seeking inside play three retains its scope and uses the root Edit clock",
        d.app().sequence_cursor == 241
            && d.app().presentation.displayed_source_frame() == Some(SourceFrameId(1)),
        json!({"Edit":241,"Original_picture":1}),
        d.snapshot(),
    )?;
    let before = d.revision();
    d.chord(&[Key::Comma, Key::F])?;
    d.wait_for("Scoped Camera opens on the selected play picture", |app| {
        app.camera.is_some()
    })?;
    painted(d, "Scope: Repeat · play 3/3")?;
    d.key(Key::Plus)?;
    d.click("Apply  Enter")?;
    d.changed(&before)?;
    idle(d)?;
    let camera_target = d
        .app()
        .scoped_target()?
        .ok_or("Camera lost the scoped continuation")?;
    d.check(
        "Camera isolates the selected third play and leaves the other frames unframed",
        d.app().sequence_cursor == 241
            && document(d)?.nodes()[&camera_target.target.node]
                .framing
                .is_some()
            && document(d)?.nodes()[&shared].framing.is_none()
            && document(d)?.nodes()[&isolated.target.node]
                .framing
                .is_none()
            && document(d)?
                .overrides()
                .get(&repeat)
                .is_some_and(|overrides| overrides.len() == 2),
        json!({"framed_play":3,"overrides":2,"Edit":241}),
        d.snapshot(),
    )?;
    scenarios::footer_anchored(
        d,
        "Scoped Camera commit keeps the footer above status notices",
    )?;
    d.capture("Play three framing commits to its own occurrence")?;

    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Undo closes stale scoped navigation and removes the complete Camera isolation",
        d.app().scoped.is_none()
            && document(d)?
                .overrides()
                .get(&repeat)
                .is_some_and(|overrides| overrides.len() == 1)
            && document(d)?.nodes()[&shared].framing.is_none(),
        json!({"scope":null,"overrides":1}),
        d.snapshot(),
    )?;
    d.key(Key::Enter)?;
    d.command("scope play 2")?;
    idle(d)?;
    d.app_mut().feedback.hold_project_updates = true;
    d.command("gain -9")?;
    d.wait_for(
        "Scoped edit is durable while its UI receipt is withheld",
        |app| !app.service.is_busy(),
    )?;
    d.command("scope play 1")?;
    let cursor = d.app().sequence_cursor;
    d.app_mut().feedback.hold_project_updates = false;
    idle(d)?;
    d.check(
        "A saved edit cannot reclaim navigation after another play is selected",
        d.app().sequence_cursor == cursor
            && cursor == 0
            && d.app().scoped.is_none()
            && gain(d, &isolated.target.node)? == crate::gain::parse_db("-9")?,
        json!({"Edit":0,"saved_play_two_db":-9,"scope":"closed on new revision"}),
        d.snapshot(),
    )?;
    d.key(Key::Enter)?;
    d.app_mut().feedback.simulate_playback = true;
    d.key(Key::Space)?;
    d.check(
        "Scoped playback preparation starts before cancellation",
        d.app().transport.is_some(),
        json!("preparing"),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    d.check(
        "Escape exits scoped contents and cancels playback",
        d.app().scoped.is_none() && d.app().transport.is_none(),
        json!({"scope":null,"playback":null}),
        d.snapshot(),
    )?;
    d.app_mut().feedback.simulate_playback = false;
    d.report.skipped.push("This replay uses real native widgets, project service and Metal with scripted keyboard delivery. Its final Escape check simulates playback preparation; physical keyboard/IME and acoustic playback are separate checks.".into());
    Ok(())
}

fn scoped_marks(d: &mut Driver<'_>) -> Result<(), String> {
    let before = retained_scope(d)?;
    let revision = d.revision();
    d.chord(&[Key::M, Key::A])?;
    d.changed(&revision)?;
    idle(d)?;
    retained_after_mark(
        d,
        "Saving a mark preserves the selected Repeat play and child",
        before,
    )?;

    let before = retained_scope(d)?;
    let revision = d.revision();
    d.command("unmark a")?;
    d.changed(&revision)?;
    idle(d)?;
    retained_after_mark(
        d,
        "Deleting a mark preserves the selected Repeat play and child",
        before,
    )?;

    d.app_mut().feedback.hold_project_updates = true;
    d.chord(&[Key::M, Key::A])?;
    d.wait_for("Mark is saved while its UI receipt is withheld", |app| {
        !app.service.is_busy()
    })?;
    d.command("scope play 3")?;
    d.key(Key::L)?;
    let current = retained_scope(d)?;
    let revision = d.revision();
    d.app_mut().feedback.hold_project_updates = false;
    d.changed(&revision)?;
    idle(d)?;
    retained_after_mark(
        d,
        "A delayed mark save preserves the current play and cursor",
        current,
    )?;

    let before = retained_scope(d)?;
    let revision = d.revision();
    d.command("unmark a")?;
    d.changed(&revision)?;
    idle(d)?;
    retained_after_mark(
        d,
        "Deleting a mark from another play retains current navigation",
        before,
    )?;
    d.command("scope play 2")?;
    idle(d)
}

fn retained_scope(
    d: &Driver<'_>,
) -> Result<(crate::project::scoped::Target, View, Pane, u64), String> {
    Ok((
        d.app().scoped_target()?.ok_or("No scoped target")?,
        d.app().view,
        d.app().pane,
        d.app().source_cursor,
    ))
}

fn retained_after_mark(
    d: &mut Driver<'_>,
    name: &str,
    (mut target, view, pane, source): (crate::project::scoped::Target, View, Pane, u64),
) -> Result<(), String> {
    target.revision = document(d)?.revision_id().clone();
    d.check(
        name,
        d.app().scoped_target()?.as_ref() == Some(&target)
            && d.app().view == view
            && d.app().pane == pane
            && d.app().source_cursor == source
            && d.app().selected_beat.as_ref() == Some(&target.root),
        json!("Only the scoped target revision changes"),
        d.snapshot(),
    )
}

fn document<'a>(d: &'a Driver<'_>) -> Result<&'a ProjectDocument, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.document.as_ref())
        .ok_or_else(|| "No workspace".into())
}
fn gain(d: &Driver<'_>, node: &NodeId) -> Result<GainDb, String> {
    Ok(crate::gain::GainEdit::new(document(d)?.nodes()[node].audio_treatments.clone()).trim())
}
fn idle(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Scoped edit receipt is admitted", |app| {
        !app.service.is_busy() && !app.repeat_queue.active()
    })?;
    d.settled()
}

fn resize(d: &mut Driver<'_>, width: f32, height: f32) -> Result<(), String> {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing scoped replay viewport")?
        .inner_rect = Some(rect);
    d.step(&format!("Scoped inspector at {width}x{height}"), true)
}

fn painted(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    let parts = scenarios::text_paint_visibility(d, label);
    d.check(
        &format!("Scoped label is fully painted: {label}"),
        !parts.is_empty() && parts.iter().all(|part| part["fully_visible"] == true),
        json!("text fits its paint clip without later opaque overlap"),
        json!(parts),
    )
}
