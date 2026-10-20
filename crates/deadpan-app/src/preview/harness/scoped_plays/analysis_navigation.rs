//! Successful analysis navigation revokes a delayed temporal continuation.
use super::*;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    if d.app().scoped.is_some() {
        d.key(Key::Escape)?;
    }
    let (key, words, _) = super::super::transcript::synthetic(d)?;
    let session = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No qualified workspace")?
        .session;
    d.wait_for("Annotation lane ready for navigation witness", |app| {
        !app.service.annotation_busy()
    })?;
    if !d.app_mut().submit(ProjectRequest::SaveTranscript {
        expected_session: session,
        lifecycle_epoch: None,
        attempt: 0,
        key,
        transcript: Arc::new(words),
    }) {
        return Err("Synthetic navigation transcript was not admitted".into());
    }
    d.wait_for("Stored navigation words are available", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.transcript.is_some())
            && app.transcription.status_name() == "ready"
            && !app.service.annotation_busy()
    })?;
    d.command("source")?;
    idle(d)?;
    // Use the actual Find words field before entering Repeat contents. The
    // prefix 'a' selects alpha in each source play, so n/N leave and return.
    d.key(Key::Slash)?;
    d.key_modified(Key::A, egui::Modifiers::COMMAND)?;
    d.events(
        "Search the stored synthetic words for alpha",
        vec![egui::Event::Text("a".into())],
    )?;
    d.key(Key::Escape)?;
    d.check(
        "Find words contains the user-entered alpha prefix",
        d.app().transcription.search_text() == "a",
        json!({"search":"a","source":"explicit synthetic annotation"}),
        d.snapshot(),
    )?;
    for search in [false, true] {
        let result = roundtrip(d, search);
        d.app_mut().feedback.hold_project_updates = false;
        result?;
    }
    d.report.skipped.push("Analysis-navigation witnesses use explicit synthetic words saved over the real qualified Original through SaveTranscript. They exercise native word/search routing and delayed edit receipts, not model inference.".into());
    Ok(())
}

fn roundtrip(d: &mut Driver<'_>, search: bool) -> Result<(), String> {
    d.command("sequence")?;
    super::super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G])?;
    d.key(Key::Enter)?;
    d.command("scope play 2")?;
    d.key(Key::W)?;
    idle(d)?;
    let captured = d
        .app()
        .scoped_target()?
        .ok_or("No word-navigation scoped capture")?;
    let cursor = d.app().sequence_cursor;
    let before = document(d)?.clone();
    let duration = d.app().sequence_length();
    let revision = d.revision();
    let name = if search { "n/N" } else { "w/b" };
    d.app_mut().feedback.hold_project_updates = true;
    d.command("hold 3f")?;
    d.wait_for(
        &format!("Pause is durable before the {name} roundtrip"),
        |app| !app.service.is_busy(),
    )?;
    if search {
        d.key(Key::N)?;
    } else {
        d.key(Key::W)?;
    }
    let away = d.app().sequence_cursor;
    if search {
        d.key_modified(Key::N, egui::Modifiers::SHIFT)?;
    } else {
        d.key(Key::B)?;
    }
    let returned = d
        .app()
        .scoped_target()?
        .ok_or("Analysis navigation unexpectedly closed the old inspector")?;
    let mut equal_address = captured.clone();
    equal_address.navigation = returned.navigation;
    d.check(
        &format!("{name} returns to the same captured address with a revoked navigation token"),
        away != cursor
            && returned == equal_address
            && returned.navigation != captured.navigation
            && d.app().sequence_cursor == cursor
            && d.revision() == revision,
        json!({"cursor":cursor,"same_address":true,"navigation":"changed","receipt":"withheld"}),
        d.snapshot(),
    )?;
    d.app_mut().feedback.hold_project_updates = false;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        &format!("The saved pause cannot reclaim scoped navigation after {name}"),
        d.app().scoped.is_none()
            && d.app().sequence_cursor == cursor
            && d.app().sequence_length() == duration + 3,
        json!({"saved_frames":3,"cursor":cursor,"inspector":"closed"}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        &format!("One Undo restores the complete pause after the {name} witness"),
        document(d)?.nodes() == before.nodes()
            && document(d)?.audio_bindings() == before.audio_bindings()
            && d.app().sequence_length() == duration,
        json!({"restored_frames":duration,"history":"one Undo"}),
        d.snapshot(),
    )
}
