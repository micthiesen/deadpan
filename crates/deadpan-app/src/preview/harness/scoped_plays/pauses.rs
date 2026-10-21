use super::*;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.key(Key::Enter)?;
    d.command("scope play 2")?;
    idle(d)?;
    let before = document(d)?.clone();
    let cursor = d.app().sequence_cursor;
    let selected = d.app().scoped_target()?.ok_or("No This play capture")?;
    let preceding = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No workspace")?
        .plan
        .picture(ProjectFrame(cursor as i64 - 1))
        .map_err(|error| error.to_string())?;
    let revision = d.revision();
    d.chord(&[Key::Comma, Key::H])?;
    d.changed(&revision)?;
    idle(d)?;
    let saved = d
        .app()
        .scoped_target()?
        .ok_or("Scoped pause lost the inspector")?;
    let after = document(d)?;
    let NodeKind::Hold { recipe } = &after.nodes()[&saved.target.node].kind else {
        return Err("Scoped pause did not select its Hold".into());
    };
    let index = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No workspace")?
        .sources
        .values()
        .find_map(|source| source.video_index.as_ref())
        .ok_or("No measured picture index")?;
    let shown = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No workspace")?
        .plan
        .picture(ProjectFrame(cursor as i64))
        .map_err(|error| error.to_string())?;
    d.check("A pause at This play's start grows only that play and selects the saved Hold",
        after.duration().map_err(|error| error.to_string())?.frames()
            == before.duration().map_err(|error| error.to_string())?.frames() + 15
            && recipe.duration.frames() == 15
            && saved.target.repeats == selected.target.repeats
            && d.app().sequence_cursor == cursor
            && shown.picture.select_source_frame(index).map_err(|error| error.to_string())?
                == preceding.picture.select_source_frame(index).map_err(|error| error.to_string())?,
        json!({"added_frames":15,"scope":"This play 2","cursor":cursor,"freeze":"preceding measured picture"}), d.snapshot())?;
    resize(d, 960.0, 640.0)?;
    painted(d, "This play 2/3")?;
    painted(d, "Insert pause")?;
    scenarios::footer_anchored(d, "Scoped pause controls retain the compact footer")?;
    d.capture("The inserted Hold remains selected within This play")?;
    resize(d, 1280.0, 820.0)?;

    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "One Undo restores the selected play's complete pre-pause structure",
        document(d)?.nodes() == before.nodes()
            && document(d)?.audio_bindings() == before.audio_bindings()
            && d.app().scoped.is_none(),
        json!("restored; stale inspector closed"),
        d.snapshot(),
    )?;
    d.key(Key::Enter)?;
    d.command("scope all")?;
    idle(d)?;
    let before = document(d)?.clone();
    let root = d
        .app()
        .scoped_target()?
        .ok_or("No All plays capture")?
        .root
        .clone();
    let NodeKind::Repeat { iterations, .. } = &before.nodes()[&root].kind else {
        return Err("The inspected root is not a Repeat".into());
    };
    let default_plays = i64::from(iterations.len())
        - i64::try_from(before.overrides().get(&root).map_or(0, |plays| plays.len()))
            .map_err(|error| error.to_string())?;
    let revision = d.revision();
    d.command("hold 3f")?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "All plays inserts one pause into the shared definition, growing every default play",
        document(d)?
            .duration()
            .map_err(|error| error.to_string())?
            .frames()
            == before
                .duration()
                .map_err(|error| error.to_string())?
                .frames()
                + 3 * default_plays
            && d.app().message.as_deref()
                == Some("Pause inserted in the selected shared scope and saved")
            && d.app().scoped_target()?.is_some_and(|target| {
                target
                    .target
                    .repeats
                    .iter()
                    .all(|step| matches!(step.branch, deadpan_core::RepeatEditBranch::Default))
            }),
        json!({"added_frames":3 * default_plays,"scope":"All plays"}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "One Undo restores the shared definition",
        document(d)?.nodes() == before.nodes()
            && document(d)?.audio_bindings() == before.audio_bindings(),
        json!("restored"),
        d.snapshot(),
    )?;
    if d.app().scoped.is_none() {
        d.key(Key::Enter)?;
    }
    d.command("scope plays 1,2")?;
    idle(d)?;
    let before = document(d)?.clone();
    let revision = d.revision();
    d.command("hold 3f")?;
    d.changed(&revision)?;
    idle(d)?;
    let selected = d
        .app()
        .scoped_target()?
        .ok_or("Selected pauses lost their inspector")?;
    let inserted = document(d)?;
    d.check(
        "Several selected plays each receive a pause and retain the selected set",
        document(d)?
            .duration()
            .map_err(|error| error.to_string())?
            .frames()
            == before
                .duration()
                .map_err(|error| error.to_string())?
                .frames()
                + 6
            && selected.also.len() == 1
            && matches!(
                inserted.nodes()[&selected.target.node].kind,
                NodeKind::Hold { .. }
            )
            && selected
                .also
                .iter()
                .all(|other| matches!(inserted.nodes()[&other.node].kind, NodeKind::Hold { .. })),
        json!({"pauses":2,"frames_each":3,"Undo":"one"}),
        d.snapshot(),
    )?;
    resize(d, 960.0, 640.0)?;
    painted(d, "Plays 1, 2 edited together  :scope plays")?;
    painted(d, "Insert pause")?;
    scenarios::footer_anchored(d, "Selected-play pause keeps the compact footer")?;
    d.capture("The new pauses remain selected across both chosen plays")?;
    resize(d, 1280.0, 820.0)?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "One Undo restores both selected plays together",
        document(d)?.nodes() == before.nodes()
            && document(d)?.audio_bindings() == before.audio_bindings(),
        json!("both restored"),
        d.snapshot(),
    )?;
    delayed_pane(d)?;
    delayed_view(d)?;
    delayed_source(d)?;
    d.key(Key::Enter)?;
    d.command("scope play 2")?;
    let revision = d.revision();
    d.command("hold 2f")?;
    d.changed(&revision)?;
    idle(d)?;
    let revision = d.revision();
    let expected_revision = document(d)?.revision_id().clone();
    d.key(Key::Colon)?;
    d.events(
        "Capture a scoped pause before history changes",
        vec![egui::Event::Text("hold 4f".into())],
    )?;
    d.app_mut().feedback.hold_project_updates = true;
    d.app()
        .service
        .submit(ProjectRequest::Undo { expected_revision })?;
    d.wait_for("Undo completes with the scoped command open", |app| {
        !app.service.is_busy()
    })?;
    d.app_mut().feedback.hold_project_updates = false;
    d.changed(&revision)?;
    let restored = document(d)?.clone();
    d.key(Key::Enter)?;
    idle(d)?;
    d.check(
        "A late history reply cannot retarget the command-entry scoped pause",
        document(d)? == &restored
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("changed")),
        json!("no insertion"),
        d.snapshot(),
    )?;
    mixed(d)?;
    Ok(())
}

fn delayed_pane(d: &mut Driver<'_>) -> Result<(), String> {
    d.key(Key::Enter)?;
    d.command("scope play 2")?;
    idle(d)?;
    let revision = d.revision();
    let duration = d.app().sequence_length();
    let captured = d.app().scoped_target()?.ok_or("No pause pane capture")?;
    d.app_mut().feedback.hold_project_updates = true;
    d.command("hold 3f")?;
    d.wait_for("Pause is saved before changing to Placed sounds", |app| {
        !app.service.is_busy()
    })?;
    d.command("sounds")?;
    let current = d
        .app()
        .scoped_target()?
        .ok_or("Pane focus lost the retained scope")?;
    let cursor = d.app().sequence_cursor;
    let pane = d.app().pane;
    d.check(
        "Placed sounds command revokes the pending pause continuation",
        current.navigation != captured.navigation,
        json!("new navigation generation"),
        d.snapshot(),
    )?;
    d.app_mut().feedback.hold_project_updates = false;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "A saved pause cannot reclaim the inspector after Placed sounds takes focus",
        d.app().pane == pane
            && d.app().scoped.is_none()
            && d.app().sequence_cursor == cursor
            && d.app().sequence_length() == duration + 3,
        json!({"pane":format!("{pane:?}"),"scope":null,"saved_frames":3,"cursor":"unchanged"}),
        d.snapshot(),
    )?;
    d.command("sequence")?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    idle(d)
}

fn delayed_view(d: &mut Driver<'_>) -> Result<(), String> {
    d.key(Key::Enter)?;
    d.command("scope play 2")?;
    idle(d)?;
    let (source, sequence) = if d.app().focused_workflow() {
        ("Original", "Your edit")
    } else {
        ("Source", "Sequence")
    };
    d.click(sequence)?;
    let revision = d.revision();
    let duration = d.app().sequence_length();
    let captured = d.app().scoped_target()?.ok_or("No viewer pause capture")?;
    d.app_mut().feedback.hold_project_updates = true;
    d.command("hold 3f")?;
    d.wait_for("Pause is saved before pointer view navigation", |app| {
        !app.service.is_busy()
    })?;
    d.click(source)?;
    d.click(sequence)?;
    let cursor = d.app().sequence_cursor;
    d.check(
        "Pointer view tabs revoke a pending pause even when Viewer keeps focus",
        d.app().pane == Pane::Viewer
            && d.app().view == View::Sequence
            && d.app().scoped.is_none()
            && d.app().scoped_navigation != captured.navigation,
        json!({"pane":"Viewer","view":"Sequence","scope":null}),
        d.snapshot(),
    )?;
    d.app_mut().feedback.hold_project_updates = false;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Returning by pointer cannot revive the saved pause's inspector",
        d.app().pane == Pane::Viewer
            && d.app().scoped.is_none()
            && d.app().sequence_cursor == cursor
            && d.app().sequence_length() == duration + 3,
        json!({"saved_frames":3,"scope":null,"cursor":"unchanged"}),
        d.snapshot(),
    )?;
    d.command("sequence")?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    idle(d)
}

fn delayed_source(d: &mut Driver<'_>) -> Result<(), String> {
    // A real copied moment reveals the Original inspector's return control.
    d.command("source")?;
    d.key(Key::V)?;
    d.key(Key::L)?;
    d.key(Key::Y)?;
    d.wait_for(
        "Original moment copied before delayed pause navigation",
        |app| !app.service.is_busy() && !app.copied.is_pending() && app.copied.original().is_some(),
    )?;
    d.command("sequence")?;
    d.key(Key::Enter)?;
    d.command("scope play 2")?;
    idle(d)?;
    let source_label = if d.app().focused_workflow() {
        "Browse  :source".to_owned()
    } else {
        let label = &d.app().source_rows.first().ok_or("No source row")?.1;
        format!("Video  {label}")
    };
    let revision = d.revision();
    let duration = d.app().sequence_length();
    d.app_mut().feedback.hold_project_updates = true;
    d.command("hold 3f")?;
    d.wait_for("Pause is saved before selecting its Original card", |app| {
        !app.service.is_busy()
    })?;
    d.click(&source_label)?;
    d.click("Your edit  :sequence")?;
    let cursor = d.app().sequence_cursor;
    d.check(
        "Original selection and its inspector return discard the old scoped continuation",
        d.app().pane == Pane::Sequence
            && d.app().view == View::Sequence
            && d.app().scoped.is_none(),
        json!({"pane":"Sequence","view":"Sequence","scope":null}),
        d.snapshot(),
    )?;
    d.app_mut().feedback.hold_project_updates = false;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "A pending pause stays saved without reclaiming an Original-card round trip",
        d.app().pane == Pane::Sequence
            && d.app().scoped.is_none()
            && d.app().sequence_cursor == cursor
            && d.app().sequence_length() == duration + 3,
        json!({"saved_frames":3,"scope":null,"cursor":"unchanged"}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    idle(d)
}

fn mixed(d: &mut Driver<'_>) -> Result<(), String> {
    if d.app().scoped.is_some() {
        d.key(Key::Escape)?;
    }
    let before = document(d)?.clone();
    let revision = d.revision();
    d.command("wrap-repeat 2")?;
    d.changed(&revision)?;
    idle(d)?;
    d.key(Key::Enter)?;
    d.command("scope all")?;
    d.key(Key::Enter)?;
    d.command("scope play 2")?;
    idle(d)?;
    let captured = d.app().scoped_target()?.ok_or("No mixed pause scope")?;
    let duration = document(d)?
        .duration()
        .map_err(|error| error.to_string())?
        .frames();
    let revision = d.revision();
    d.command("hold 3f")?;
    d.changed(&revision)?;
    idle(d)?;
    let selected = d
        .app()
        .scoped_target()?
        .ok_or("Mixed pause lost its inspector")?;
    d.check(
        "Outer All plays and inner This play insert once per outer default use",
        captured.target.repeats.len() == 2
            && matches!(
                captured.target.repeats[0].branch,
                deadpan_core::RepeatEditBranch::Default
            )
            && matches!(
                captured.target.repeats[1].branch,
                deadpan_core::RepeatEditBranch::Play { .. }
            )
            && selected
                .target
                .repeats
                .iter()
                .map(|step| &step.branch)
                .eq(captured.target.repeats.iter().map(|step| &step.branch))
            && document(d)?
                .duration()
                .map_err(|error| error.to_string())?
                .frames()
                == duration + 6,
        json!({"outer":"All plays (2)","inner":"This play 2/3","added_frames":6}),
        d.snapshot(),
    )?;
    resize(d, 960.0, 640.0)?;
    painted(d, "This play 2/3")?;
    painted(d, "Insert pause")?;
    scenarios::footer_anchored(d, "Mixed Repeat choices keep the compact footer")?;
    d.capture("Outer All plays and inner This play remain distinct after pause insertion")?;
    resize(d, 1280.0, 820.0)?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    idle(d)?;
    d.key(Key::Enter)?;
    d.command("scope all")?;
    d.key(Key::Enter)?;
    d.command("scope plays 1,2")?;
    idle(d)?;
    let captured = d.app().scoped_target()?;
    let revision = d.revision();
    let cursor = d.app().sequence_cursor;
    d.app_mut().feedback.hold_project_updates = true;
    d.command("hold 3f video=black")?;
    d.wait_for(
        "Selected pauses are durable before their receipt arrives",
        |app| !app.service.is_busy(),
    )?;
    let events = [Key::L, Key::H]
        .into_iter()
        .flat_map(|key| {
            [
                super::super::key_event(key, egui::Modifiers::NONE, true),
                super::super::key_event(key, egui::Modifiers::NONE, false),
            ]
        })
        .collect();
    d.events(
        "Move away and return before admitting the selected-pause receipt",
        events,
    )?;
    let returned = d.app().scoped_target()?;
    let mut same_address = captured.clone();
    if let (Some(expected), Some(actual)) = (&mut same_address, &returned) {
        expected.navigation = actual.navigation;
    }
    d.check(
        "Same-frame cursor navigation restores the exact old selected-play address",
        returned == same_address && returned != captured && d.app().sequence_cursor == cursor,
        json!({"cursor":cursor,"target":"equal before receipt"}),
        d.snapshot(),
    )?;
    d.app_mut().feedback.hold_project_updates = false;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "A selected-pause receipt cannot reclaim revoked navigation",
        d.app().scoped.is_none()
            && d.app().sequence_cursor == cursor
            && document(d)?
                .duration()
                .map_err(|error| error.to_string())?
                .frames()
                == duration + 12,
        json!({"cursor":cursor,"inspector":"closed","committed_pauses":4}),
        d.snapshot(),
    )?;
    resize(d, 960.0, 640.0)?;
    scenarios::footer_anchored(
        d,
        "A delayed selected-pause receipt keeps the compact footer",
    )?;
    d.capture("Returning to equal coordinates cannot revive the selected-pause continuation")?;
    resize(d, 1280.0, 820.0)?;
    for _ in 0..2 {
        let revision = d.revision();
        d.key(Key::U)?;
        d.changed(&revision)?;
        idle(d)?;
    }
    d.check(
        "Undoing the mixed pause and wrapper restores the complete entry document",
        document(d)?.nodes() == before.nodes()
            && document(d)?.audio_bindings() == before.audio_bindings(),
        json!("restored"),
        d.snapshot(),
    )
}
