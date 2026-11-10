//! Real scoped authoring through native keyboard routing, SQLite and Metal.

use super::*;
use deadpan_core::{GainDb, NodeId, NodeKind, ProjectDocument};
use egui::Key;

mod analysis_navigation;
mod audio_setters;
mod copy_paste;
mod fixed_hold_duration;
pub(super) mod fixed_retime_duration;
mod framing_macros;
mod gain_macros;
mod heterogeneous_pauses;
mod hold_audio;
mod hold_duration;
pub(super) mod mixed_fixed_duration;
mod mixed_hold_duration;
mod mute_macros;
pub(super) mod nested_fixed_duration;
pub(super) mod owned_gap_duration;
mod pause_macros;
mod pauses;
pub(super) mod selected_fixed_duration;
mod shared_gap;
mod shared_hold_duration;
pub(super) mod this_play_fixed_duration;

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
    let mut sound_target = d
        .app()
        .scoped_target()?
        .ok_or("No retained sound-pane scope")?;
    let changed_navigation = sound_target.navigation != old_target.navigation;
    sound_target.navigation = old_target.navigation;
    d.check(
        "Placed sounds focus keeps nested picture navigation untouched",
        d.app().pane == Pane::Sounds && sound_target == old_target && changed_navigation,
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
    framing_macros::run(d)?;
    gain_macros::run(d)?;
    mute_macros::run(d)?;
    audio_setters::run(d)?;
    pause_macros::run(d)?;
    hold_duration::run(d)?;
    fixed_hold_duration::run(d)?;
    shared_hold_duration::run(d)?;
    mixed_hold_duration::run(d)?;
    hold_audio::run(d)?;
    pauses::run(d)?;
    analysis_navigation::run(d)?;
    heterogeneous_pauses::run(d)?;
    scoped_speed(d)?;
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

/// `:retime` inside one play's contents isolates and changes only that play.
fn scoped_speed(d: &mut Driver<'_>) -> Result<(), String> {
    while d.app().scoped.is_some() {
        d.key(Key::Escape)?;
        idle(d)?;
    }
    d.chord(&[Key::G, Key::G])?;
    idle(d)?;
    let repeat = d.app().selected_beat.clone().ok_or("No beat selected")?;
    let plays = |d: &Driver<'_>| -> Result<Vec<i64>, String> {
        let document = document(d)?;
        let NodeKind::Repeat {
            child, iterations, ..
        } = &document.nodes()[&repeat].kind
        else {
            return Err("Expected the first beat to be the Repeat".into());
        };
        let durations = document
            .durations_shared()
            .map_err(|error| error.to_string())?;
        Ok((0..iterations.len())
            .map(|index| {
                let iteration = iterations.at(index).expect("a play");
                let root = document
                    .overrides()
                    .get(&repeat)
                    .and_then(|plays| plays.get(&iteration))
                    .unwrap_or(child);
                durations[root].frames()
            })
            .collect())
    };
    let before = plays(d)?;
    let revision = d.revision();
    d.key(Key::Enter)?;
    d.command("scope play 2")?;
    idle(d)?;
    d.command("retime 2 pitch=tape")?;
    d.changed(&revision)?;
    idle(d)?;
    let after = plays(d)?;
    d.check(
        ":retime 2 inside play 2 halves only that play and keeps its contents open",
        after[0] == before[0]
            && after[2] == before[2]
            && after[1] == before[1] / 2
            && d.app().scoped.is_some(),
        json!({"before":before,"play_two":before[1] / 2,"scope":"play 2"}),
        json!({"after":after,"scoped":d.app().scoped.is_some(),"error":d.app().error}),
    )?;
    d.capture("One play sped up inside its contents")?;
    d.command("scope play 3")?;
    idle(d)?;
    let hint = d.app().repeat_hint();
    let edited = d.revision();
    d.events(
        "Dot repeats the speed change on play 3",
        vec![
            egui::Event::Key {
                key: Key::Period,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Text(".".into()),
            egui::Event::Key {
                key: Key::Period,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    )?;
    d.changed(&edited)?;
    idle(d)?;
    let dotted = plays(d)?;
    d.check(
        "Dot inside play 3 halves that play from its own input",
        dotted[0] == before[0]
            && dotted[1] == before[1] / 2
            && dotted[2] == before[2] / 2
            && hint
                .as_deref()
                .is_some_and(|hint| hint.starts_with("repeat: speed")),
        json!({"plays":[before[0], before[1] / 2, before[2] / 2],"hint":"repeat: speed"}),
        json!({"plays":dotted,"hint":hint,"error":d.app().error}),
    )?;
    let edited = d.revision();
    d.key(Key::U)?;
    d.changed(&edited)?;
    idle(d)?;
    let edited = d.revision();
    d.key(Key::U)?;
    d.changed(&edited)?;
    idle(d)?;
    d.check(
        "Two Undos restore every play's length",
        plays(d)? == before,
        json!(before),
        json!(plays(d)?),
    )?;
    isolate_gap(d, &repeat)
}

/// `:isolate-gap` in play 2 gives that play alone its own gap pause.
fn isolate_gap(d: &mut Driver<'_>, repeat: &NodeId) -> Result<(), String> {
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
    d.key(Key::Enter)?;
    d.command("scope play 2")?;
    idle(d)?;
    let before = d.revision();
    d.command("isolate-gap")?;
    d.changed(&before)?;
    idle(d)?;
    let document = document(d)?;
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[repeat].kind else {
        return Err("Expected the Repeat".into());
    };
    let second = iterations.at(1).ok_or("The Repeat has no second play")?;
    let gaps = document.gap_overrides().get(repeat);
    let owned = gaps.map_or(0, |plays| plays.len());
    let second_owned = gaps.and_then(|plays| plays.get(&second)).cloned();
    let second = format!("{second:?}");
    d.check(
        ":isolate-gap in play 2 gives only that play its own gap pause",
        owned == 1 && second_owned.is_some() && d.app().error.is_none(),
        json!({"owned_gaps":[second]}),
        json!({"owned_gaps":owned,"second":second_owned,"error":d.app().error}),
    )?;
    for _ in 0..2 {
        let edited = d.revision();
        d.key(Key::U)?;
        d.changed(&edited)?;
        idle(d)?;
    }
    shared_gap::run(d, repeat)?;
    play_structure(d, repeat)
}

/// Each play's length, from its own branch or the shared definition.
fn play_lengths(d: &Driver<'_>, repeat: &NodeId) -> Result<Vec<i64>, String> {
    let document = document(d)?;
    let NodeKind::Repeat {
        child, iterations, ..
    } = &document.nodes()[repeat].kind
    else {
        return Err("Expected the Repeat".into());
    };
    let durations = document
        .durations_shared()
        .map_err(|error| error.to_string())?;
    Ok((0..iterations.len())
        .filter_map(|index| iterations.at(index))
        .map(|iteration| {
            let root = document
                .overrides()
                .get(repeat)
                .and_then(|plays| plays.get(&iteration))
                .unwrap_or(child);
            durations[root].frames()
        })
        .collect())
}

/// `s`, `rr`, `dd`, `:repeat N` and `:repeat N gap=` inside play 2 change only
/// that play, keep the inspector on the result, and undo in five steps.
fn play_structure(d: &mut Driver<'_>, repeat: &NodeId) -> Result<(), String> {
    while d.app().scoped.is_some() {
        d.key(Key::Escape)?;
        idle(d)?;
    }
    d.chord(&[Key::G, Key::G])?;
    idle(d)?;
    let NodeKind::Repeat { child: shared, .. } = &document(d)?.nodes()[repeat].kind else {
        return Err("Expected the Repeat".into());
    };
    let shared = shared.clone();
    let shared_node = document(d)?.nodes()[&shared].clone();
    let before = play_lengths(d, repeat)?;
    let original = document(d)?.clone();
    d.key(Key::Enter)?;
    d.command("scope play 2")?;
    idle(d)?;
    for _ in 0..10 {
        d.key(Key::L)?;
    }
    idle(d)?;
    let start = d.app().sequence_cursor;
    let edited = d.revision();
    d.key(Key::S)?;
    d.changed(&edited)?;
    idle(d)?;
    let owned = {
        let document = document(d)?;
        let NodeKind::Repeat { iterations, .. } = &document.nodes()[repeat].kind else {
            return Err("Expected the Repeat".into());
        };
        let second = iterations.at(1).ok_or("The Repeat has no second play")?;
        document
            .overrides()
            .get(repeat)
            .and_then(|plays| plays.get(&second))
            .cloned()
    };
    let fragments: Vec<NodeId> = owned
        .as_ref()
        .map(|root| document(d).map(|doc| doc.children(root).cloned().collect()))
        .transpose()?
        .unwrap_or_default();
    let selected = d.app().scoped_target()?.map(|target| target.target.node);
    d.check(
        "s inside play 2 splits only that play and selects the right fragment",
        fragments.len() == 2
            && selected.as_ref() == fragments.get(1)
            && document(d)?.nodes()[&shared] == shared_node
            && play_lengths(d, repeat)? == before
            && d.app().error.is_none(),
        json!({"fragments":2,"selected":"right","plays":before}),
        json!({"fragments":fragments,"selected":selected,"plays":play_lengths(d, repeat)?,"error":d.app().error}),
    )?;
    let edited = d.revision();
    d.key(Key::R)?;
    d.key(Key::R)?;
    d.changed(&edited)?;
    idle(d)?;
    let lengths = play_lengths(d, repeat)?;
    let right = before[1] - 10;
    let selected = d.app().scoped_target()?.map(|target| target.target.node);
    let wrapped = selected.as_ref().is_some_and(|node| {
        document(d).is_ok_and(|doc| {
            matches!(
                doc.nodes().get(node).map(|beat| &beat.kind),
                Some(NodeKind::Repeat { .. })
            )
        })
    });
    d.check(
        "rr inside play 2 repeats the right fragment there only and selects the new Repeat",
        lengths[1] == before[1] + right
            && lengths[0] == before[0]
            && lengths[2] == before[2]
            && wrapped
            && d.app().error.is_none(),
        json!({"plays":[before[0], before[1] + right, before[2]],"selected":"new Repeat"}),
        json!({"plays":lengths,"selected":selected,"error":d.app().error}),
    )?;
    d.key(Key::K)?;
    idle(d)?;
    let edited = d.revision();
    d.key(Key::D)?;
    d.key(Key::D)?;
    d.changed(&edited)?;
    idle(d)?;
    let lengths = play_lengths(d, repeat)?;
    let remaining: Vec<NodeId> = owned
        .as_ref()
        .map(|root| document(d).map(|doc| doc.children(root).cloned().collect()))
        .transpose()?
        .unwrap_or_default();
    d.check(
        "dd inside play 2 deletes the left fragment there only and the play closes up",
        lengths[1] == 2 * right
            && lengths[0] == before[0]
            && lengths[2] == before[2]
            && remaining.len() == 1
            && d.app().scoped.is_some()
            && d.app().sequence_cursor == start - 10
            && document(d)?.nodes()[&shared] == shared_node
            && d.app().error.is_none(),
        json!({"plays":[before[0], 2 * right, before[2]],"children":1,"Edit":start - 10}),
        json!({"plays":lengths,"children":remaining.len(),"Edit":d.app().sequence_cursor,"error":d.app().error}),
    )?;
    // The deleted beat's neighbour is the new Repeat; :repeat recounts it
    // in this play only.
    let recounted = d.app().scoped_target()?.map(|target| target.target.node);
    let edited = d.revision();
    d.command("repeat 3")?;
    d.changed(&edited)?;
    idle(d)?;
    let lengths = play_lengths(d, repeat)?;
    let selected = d.app().scoped_target()?.map(|target| target.target.node);
    let counted = selected.as_ref().and_then(|node| {
        document(d)
            .ok()
            .and_then(|doc| match &doc.nodes().get(node)?.kind {
                NodeKind::Repeat { iterations, .. } => Some(iterations.len()),
                _ => None,
            })
    });
    d.check(
        ":repeat 3 on the Repeat inside play 2 sets its plays there only and keeps it selected",
        lengths[1] == 3 * right
            && lengths[0] == before[0]
            && lengths[2] == before[2]
            && selected == recounted
            && counted == Some(3)
            && document(d)?.nodes()[&shared] == shared_node
            && d.app().error.is_none(),
        json!({"plays":[before[0], 3 * right, before[2]],"selected":"same Repeat","total_plays":3}),
        json!({"plays":lengths,"selected":selected,"total_plays":counted,"error":d.app().error}),
    )?;
    painted(d, "Repeat set to 3 total plays in this play")?;
    d.capture("Play 2 split, its right part repeated three times and its left part deleted")?;
    // :repeat 4 gap=6f grows the same Repeat and adds its gaps in play 2 only.
    let edited = d.revision();
    d.command("repeat 4 gap=6f")?;
    d.changed(&edited)?;
    idle(d)?;
    let lengths = play_lengths(d, repeat)?;
    let selected = d.app().scoped_target()?.map(|target| target.target.node);
    let (counted, gap) = selected
        .as_ref()
        .and_then(|node| {
            document(d)
                .ok()
                .and_then(|doc| match &doc.nodes().get(node)?.kind {
                    NodeKind::Repeat {
                        iterations, gap, ..
                    } => Some((
                        Some(iterations.len()),
                        gap.as_ref().map(|gap| gap.duration.frames()),
                    )),
                    _ => None,
                })
        })
        .unwrap_or((None, None));
    d.check(
        ":repeat 4 gap=6f on the Repeat inside play 2 sets its plays and gaps there only",
        lengths[1] == 4 * right + 18
            && lengths[0] == before[0]
            && lengths[2] == before[2]
            && selected == recounted
            && counted == Some(4)
            && gap == Some(6)
            && document(d)?.nodes()[&shared] == shared_node
            && d.app().error.is_none(),
        json!({"plays":[before[0], 4 * right + 18, before[2]],"selected":"same Repeat","total_plays":4,"gap":6}),
        json!({"plays":lengths,"selected":selected,"total_plays":counted,"gap":gap,"error":d.app().error}),
    )?;
    painted(d, "Repeat set: 4 plays; gaps 6f in this play")?;
    d.capture("The Repeat inside play 2 plays four times with 6f gaps")?;
    for _ in 0..5 {
        let edited = d.revision();
        d.key(Key::U)?;
        d.changed(&edited)?;
        idle(d)?;
    }
    d.check(
        "Five Undos restore every play and play 2's earlier branch exactly",
        play_lengths(d, repeat)? == before
            && document(d)?.nodes() == original.nodes()
            && document(d)?.overrides() == original.overrides(),
        json!({"plays":before,"nodes_and_overrides":"as before s"}),
        json!({"plays":play_lengths(d, repeat)?}),
    )?;
    play_structure_macro(d, repeat, &before, &original, &shared_node)
}

/// `s` and `dd` recorded inside play 2 save only their intent; `@a` in play 3
/// splits and deletes there the same way, leaving play 2 as recorded.
fn play_structure_macro(
    d: &mut Driver<'_>,
    repeat: &NodeId,
    before: &[i64],
    original: &ProjectDocument,
    shared_node: &deadpan_core::BeatNode,
) -> Result<(), String> {
    let shared = match &original.nodes()[repeat].kind {
        NodeKind::Repeat { child, .. } => child.clone(),
        _ => return Err("Expected the Repeat".into()),
    };
    let enter_play = |d: &mut Driver<'_>, play: u32| -> Result<(), String> {
        while d.app().scoped.is_some() {
            d.key(Key::Escape)?;
            idle(d)?;
        }
        d.chord(&[Key::G, Key::G])?;
        idle(d)?;
        d.key(Key::Enter)?;
        d.command(&format!("scope play {play}"))?;
        idle(d)?;
        for _ in 0..10 {
            d.key(Key::L)?;
        }
        idle(d)
    };
    enter_play(d, 2)?;
    d.chord(&[Key::Q, Key::A])?;
    let edited = d.revision();
    d.key(Key::S)?;
    d.changed(&edited)?;
    idle(d)?;
    let edited = d.revision();
    d.key(Key::D)?;
    d.key(Key::D)?;
    d.changed(&edited)?;
    idle(d)?;
    let recording =
        d.app().macros.recording_name() == Some('a') && d.app().macros.instruction_count() == 2;
    d.key(Key::Q)?;
    d.wait_for("The recorded play-structure macro is saved", |app| {
        !app.service.is_busy()
            && !app.macros.is_pending()
            && app.copied.entries().any(|(register, _)| register == 'a')
    })?;
    idle(d)?;
    let program = d
        .app()
        .copied
        .entries()
        .find_map(|(register, value)| match value {
            crate::preview::copied::Content::Macro(program) if register == 'a' => {
                Some(program.instructions().to_vec())
            }
            _ => None,
        });
    let recorded = play_lengths(d, repeat)?;
    d.check(
        "Recording s and dd inside play 2 edits that play only and saves only Split and the beat cut",
        recording
            && program
                == Some(vec![
                    deadpan_core::SemanticInstruction::Split,
                    deadpan_core::SemanticInstruction::Cut {
                        selector: deadpan_core::SemanticSelector::SelectedBeat,
                        register: deadpan_core::RegisterName::unnamed(),
                    },
                ])
            && recorded == [before[0], 10, before[2]]
            && document(d)?.nodes()[&shared] == *shared_node
            && d.app().error.is_none(),
        json!({"program":["split","cut selected beat"],"plays":[before[0], 10, before[2]]}),
        json!({"recording":recording,"program":program,"plays":recorded,"error":d.app().error}),
    )?;
    let played = document(d)?.clone();
    enter_play(d, 3)?;
    let edited = d.revision();
    super::macros::execute(d, 'a', None)?;
    d.changed(&edited)?;
    d.wait_for("The play-structure macro receipt is admitted", |app| {
        !app.service.is_busy() && !app.macros.is_pending()
    })?;
    idle(d)?;
    let lengths = play_lengths(d, repeat)?;
    let second = |document: &ProjectDocument| {
        let NodeKind::Repeat { iterations, .. } = &document.nodes()[repeat].kind else {
            return None;
        };
        let play = iterations.at(1)?;
        let root = document.overrides().get(repeat)?.get(&play)?;
        Some(document.children(root).cloned().collect::<Vec<_>>())
    };
    let selected = d.app().scoped_target()?;
    d.check(
        "@a in play 3 splits and deletes at the new play's cursor and leaves play 2 as recorded",
        lengths == [before[0], 10, 10]
            && second(document(d)?) == second(&played)
            && document(d)?.nodes()[&shared] == *shared_node
            && selected.is_some_and(|target| target.has_single_play_authority())
            && d.app().sequence_cursor == u64::try_from(before[0] + 10 + 10).unwrap_or(0)
            && d.app().error.is_none(),
        json!({"plays":[before[0], 10, 10],"play_2":"unchanged","Edit":before[0] + 20}),
        json!({"plays":lengths,"Edit":d.app().sequence_cursor,"error":d.app().error}),
    )?;
    painted(d, "This play 3/3")?;
    d.capture("The recorded split and delete replay in play 3 at its own cursor")?;
    for _ in 0..3 {
        let edited = d.revision();
        d.key(Key::U)?;
        d.changed(&edited)?;
        idle(d)?;
    }
    d.check(
        "Three Undos remove the replay and the recorded edits",
        play_lengths(d, repeat)? == before && document(d)?.overrides() == original.overrides(),
        json!({"plays":before}),
        json!({"plays":play_lengths(d, repeat)?}),
    )?;
    copy_paste::run(d, repeat, before, original, shared_node)
}
