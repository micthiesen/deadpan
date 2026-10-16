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
            && d.app().message.as_deref() == Some("Pause inserted in all plays and saved")
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
    d.command("hold 3f")?;
    idle(d)?;
    d.check(
        "Several selected plays cannot silently insert in only the displayed play",
        document(d)? == &before
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Choose This play")),
        json!("no authored change; choose one play"),
        d.snapshot(),
    )?;
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
    Ok(())
}
