//! Real sound registration and production input routing with fake delivery only.

use super::*;
use deadpan_core::AudioSample;
use deadpan_playback::{Phase, Update};
use egui::{Key, Modifiers};

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("Sound registration and widgets use production services. Delivery reports are deterministic fakes, not real PCM playback, device timing, or listening evidence.".into());
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/audio-fixtures")
        .canonicalize()
        .map_err(|error| error.to_string())?;
    d.app_mut().dialogs = Dialogs::scripted(vec![
        (
            DialogKind::ImportSound,
            Some(fixtures.join("pcm-stereo-48000.wav")),
        ),
        (
            DialogKind::ImportSound,
            Some(fixtures.join("pcm-mono-44100.wav")),
        ),
    ]);
    for count in 1..=2 {
        d.click("Add sound…  ⌘I")?;
        d.wait_for(
            "Sound registration commits a qualified catalog descriptor",
            |app| app.sound_rows.len() == count && !app.service.is_busy() && !app.importing(),
        )?;
        d.settled()?;
    }
    let sounds = d.app().sound_rows.clone();
    d.command("sequence")?;
    d.chord(&[Key::Num2, Key::L])?;
    d.command("source")?;
    d.chord(&[Key::Num5, Key::L])?;
    d.settled()?;
    d.check(
        "Sound audition starts with distinct nonzero Original and Edit positions",
        d.app().source_cursor == 5 && d.app().sequence_cursor == 2,
        json!({"original":5,"edit":2}),
        d.snapshot(),
    )?;
    let baseline = editor_context(d);
    let first_sound_step = d.report.steps.len();
    d.click(&sounds[1].1)?;
    d.key(Key::K)?;
    d.check(
        "Sources j/k selects sound without changing Original or edit selection",
        d.app().selected_sound.as_ref() == Some(&sounds[0].0)
            && d.app().sound_focused()
            && editor_context(d) == baseline,
        baseline.clone(),
        d.snapshot(),
    )?;

    d.chord(&[Key::D, Key::D, Key::S])?;
    d.check(
        "Destructive timeline shortcuts do not act on the retained beat from sound focus",
        editor_context(d) == baseline && !d.app().service.is_busy(),
        baseline.clone(),
        d.snapshot(),
    )?;
    d.key(Key::Space)?;
    let end = d
        .app()
        .selected_sound_descriptor()
        .ok_or("Missing sound descriptor")?
        .duration_samples();
    d.check(
        "Space prepares the complete sound with its exact measured endpoint",
        d.app().transport.as_ref().is_some_and(|run| {
            run.domain().is_sound()
                && run.phase == Phase::Preparing
                && run.window().start() == AudioSample(0)
                && run.window().end() == end
                && !run.window().looping()
        }) && d.rect("Cancel preparation  ·  Space").is_ok(),
        json!({"domain":"sound","end":end.0}),
        d.snapshot(),
    )?;
    let (mut feed, _callback) = deadpan_output::channel().map_err(|e| e.to_string())?;
    let generation = feed.restart(0).map_err(|e| e.to_string())?;
    let first = update(d, Phase::Playing, AudioSample(125), generation, None)?;
    inject(
        d,
        first.clone(),
        "Fake sound delivery advances only the source-local sound clock",
    )?;
    preserved(
        d,
        &baseline,
        "Playing sound retains the exact picture and editor context",
    )?;
    d.key(Key::Space)?;
    inject(
        d,
        first.clone(),
        "A paused sound ignores its old delivery report",
    )?;
    d.check(
        "Sound pause retains the exact sample without requesting a picture",
        d.app().sound_cursor == 125
            && d.app().transport.is_none()
            && d.app().resume.is_some()
            && editor_context(d) == baseline,
        json!({"sample":125,"paused":true}),
        d.snapshot(),
    )?;
    d.key(Key::Space)?;
    let resumed_generation = feed.restart(125).map_err(|e| e.to_string())?;
    let resumed = update(
        d,
        Phase::Playing,
        AudioSample(250),
        resumed_generation,
        None,
    )?;
    inject(
        d,
        resumed.clone(),
        "Resume admits the fresh sound generation",
    )?;
    inject(
        d,
        first,
        "A previous sound ticket cannot move the resumed clock",
    )?;
    d.check(
        "Sound resumes its exact sample and rejects stale deliveries",
        d.app().sound_cursor == 250 && editor_context(d) == baseline,
        json!({"sample":250}),
        d.snapshot(),
    )?;
    let ended = update(d, Phase::Ended, end, resumed_generation, None)?;
    inject(
        d,
        ended,
        "The sound ends at the measured sample, without rounding to a picture frame",
    )?;
    d.check(
        "Terminal sound delivery revokes resume and leaves both editing clocks untouched",
        d.app().sound_cursor == end.0 as u64
            && d.app().transport.is_none()
            && d.app().resume.is_none()
            && editor_context(d) == baseline,
        json!({"sound_sample":end.0,"stopped":true}),
        d.snapshot(),
    )?;

    d.click("Play sound  ·  Space")?;
    d.check(
        "Pointer Play after the terminal boundary restarts the sound at zero",
        d.app()
            .transport
            .as_ref()
            .is_some_and(|run| run.sample == AudioSample(0)),
        json!(0),
        d.snapshot(),
    )?;
    let pointer_generation = feed.restart(0).map_err(|e| e.to_string())?;
    let pointer = update(
        d,
        Phase::Playing,
        AudioSample(300),
        pointer_generation,
        None,
    )?;
    inject(d, pointer, "Pointer sound playback receives fake delivery")?;
    d.click("Pause sound  ·  Space")?;
    d.check(
        "Pointer release pauses sound immediately at the retained sample",
        d.app().transport.is_none()
            && d.app().sound_cursor == 300
            && d.app()
                .resume
                .as_ref()
                .is_some_and(|resume| resume.domain().is_sound())
            && editor_context(d) == baseline,
        json!({"sample":300,"paused":true,"resume":true}),
        d.snapshot(),
    )?;
    // The release is handled after the button is painted. Check its new label
    // on the very next frame without waiting for an eventual settled state.
    d.capture("Pointer sound pause paints Resume on the next frame")?;
    let resume_paint = scenarios::text_paint_visibility(d, "Resume sound  ·  Space");
    d.check(
        "Pointer pause exposes the retained sound clock and Resume control",
        d.app().transport.is_none()
            && d.app().sound_cursor == 300
            && d.app()
                .resume
                .as_ref()
                .is_some_and(|resume| resume.domain().is_sound())
            && d.rect("Resume sound  ·  Space").is_ok()
            && !resume_paint.is_empty()
            && resume_paint
                .iter()
                .all(|paint| paint["fully_visible"] == true),
        json!({"sample":300,"resume":true}),
        json!({"state":d.snapshot(),"resume_paint":resume_paint}),
    )?;
    for (width, height) in [(960.0, 640.0), (1280.0, 820.0)] {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
        let input = d.harness.input_mut();
        input.screen_rect = Some(rect);
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .ok_or("Native resize has no root viewport")?
            .inner_rect = Some(rect);
        d.capture(&format!(
            "Pinned sound controls on first resize frame at {width}x{height}"
        ))?;
        for label in [
            "Resume sound  ·  Space",
            "Loop sound  ·  Shift+Space",
            "Paused",
        ] {
            let paint = scenarios::text_paint_visibility(d, label);
            d.check(
                "Sound transport remains fully painted without catalog scrolling",
                !paint.is_empty() && paint.iter().all(|p| p["fully_visible"] == true),
                json!(label),
                json!(paint),
            )?;
        }
        d.check(
            "Resize preserves the paused sound and editor context",
            d.app().sound_cursor == 300
                && d.app().resume.is_some()
                && editor_context(d) == baseline,
            json!("sample 300, retained resume and unchanged editor"),
            d.snapshot(),
        )?;
    }
    d.key_modified(Key::Space, Modifiers::SHIFT)?;
    let loop_generation = feed.restart(0).map_err(|e| e.to_string())?;
    let loop_update = update(
        d,
        Phase::Playing,
        AudioSample(end.0 * 2 + 37),
        loop_generation,
        None,
    )?;
    inject(
        d,
        loop_update.clone(),
        "Fake monotonic delivery crosses two complete sound loops",
    )?;
    d.check(
        "Shift Space loops exactly the full sound and wraps only its content coordinate",
        d.app().sound_cursor == 37
            && d.app().transport.as_ref().is_some_and(|run| {
                run.window().looping()
                    && run.window().start() == AudioSample(0)
                    && run.window().end() == end
                    && run.lap() == Ok(2)
            })
            && editor_context(d) == baseline,
        json!({"window":[0,end.0],"sound_sample":37,"lap":2}),
        d.snapshot(),
    )?;
    d.key(Key::Space)?;
    d.key(Key::J)?;
    d.check(
        "Selecting another sound clears the old resume and resets only the sound cursor",
        d.app().selected_sound.as_ref() == Some(&sounds[1].0)
            && d.app().sound_cursor == 0
            && d.app().transport.is_none()
            && d.app().resume.is_none()
            && editor_context(d) == baseline,
        json!({"selected_sound":sounds[1].0,"sample":0}),
        d.snapshot(),
    )?;
    d.key(Key::Space)?;
    inject(
        d,
        loop_update,
        "The new sound ignores the previous sound's loop delivery",
    )?;
    d.key(Key::Tab)?;
    d.check(
        "Leaving catalog focus stops sound playback and revokes resume",
        !d.app().sound_focused()
            && d.app().transport.is_none()
            && d.app().resume.is_none()
            && editor_context(d) == baseline,
        json!({"stopped":true,"resume":false}),
        d.snapshot(),
    )?;

    d.click(&sounds[1].1)?;
    d.key(Key::D)?;
    d.click(&sounds[0].1)?;
    d.check(
        "Changing the selected sound clears a pending operator scope",
        d.app().bindings.pending().is_empty() && editor_context(d) == baseline,
        json!("No pending operator"),
        d.snapshot(),
    )?;
    d.click("Loop sound  ·  Shift+Space")?;
    let fault_generation = feed.restart(0).map_err(|e| e.to_string())?;
    let fault = update(
        d,
        Phase::Failed,
        AudioSample(47),
        fault_generation,
        Some("Sound output device failure".into()),
    )?;
    inject(
        d,
        fault,
        "Sound output failure is visible and revokes resume",
    )?;
    inject(
        d,
        resumed,
        "A delayed report cannot restart sound after a fault",
    )?;
    d.check(
        "Sound device faults stop without changing the displayed picture or editor targets",
        d.app().transport.is_none()
            && d.app().resume.is_none()
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Sound output device failure"))
            && editor_context(d) == baseline,
        json!({"stopped":true,"resume":false}),
        d.snapshot(),
    )?;

    d.key(Key::Colon)?;
    d.events(
        "Native command text retains space and destructive letters",
        vec![
            egui::Event::Text("dd ".into()),
            key_event(Key::Space, Modifiers::NONE, true),
            key_event(Key::Space, Modifiers::NONE, false),
            egui::Event::Text(" ".into()),
        ],
    )?;
    d.check(
        "Sound focus does not steal Space from native command text",
        d.app().command_open
            && d.app().command.contains("dd ")
            && d.app().transport.is_none()
            && editor_context(d) == baseline,
        json!("Text entry retains Space without playback or editing"),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    let requested_picture = d.report.steps[first_sound_step..].iter().any(|step| {
        step.semantic["stages"].as_array().is_some_and(|stages| {
            stages
                .iter()
                .any(|event| event["stage"] == "picture_requested")
        })
    });
    d.check(
        "Selection, playback, pause, loop, switching and failure never request a picture",
        !requested_picture && editor_context(d) == baseline,
        baseline.clone(),
        d.snapshot(),
    )?;

    d.click("Play sound  ·  Space")?;
    d.app_mut().feedback.hold_preview = true;
    d.click("Browse  :source")?;
    d.check(
        "Returning to Original stops sound and preserves both retained editing positions",
        d.app().selected_sound.is_none()
            && d.app().transport.is_none()
            && d.app().resume.is_none()
            && d.app().source_cursor == 5
            && d.app().sequence_cursor == 2
            && d.app().selected_source.as_ref()
                == d.app().workspace.as_ref().and_then(|w| original_asset(w)),
        json!({"original":5,"edit":2,"sound_stopped":true}),
        d.snapshot(),
    )?;
    d.check(
        "Returning to the same Original keeps its displayed frame while refresh prepares",
        d.snapshot()["picture"]["displayed"] == baseline["picture"]["displayed"]
            && d.snapshot()["picture"]["geometry_revision"]
                == baseline["picture"]["geometry_revision"]
            && d.app().presentation.loading(),
        baseline["picture"].clone(),
        d.snapshot()["picture"].clone(),
    )?;
    let held = d.app_mut().feedback.held_reply.take();
    d.app_mut().feedback.hold_preview = false;
    d.app_mut().feedback.release_reply = held;
    d.settled()?;
    d.check(
        "The refreshed Original picture matches its retained cursor",
        d.snapshot()["picture"]["source_frame"] == 5,
        json!(5),
        d.snapshot()["picture"].clone(),
    )?;
    d.command("sequence")?;
    d.settled()?;
    d.click("Browse  :source")?;
    d.settled()?;
    d.check(
        "Browsing Original from Your edit preserves both independent positions",
        d.app().view == View::Source
            && d.app().source_cursor == 5
            && d.app().sequence_cursor == 2
            && d.snapshot()["picture"]["source_frame"] == 5
            && d.snapshot()["selected_beat"] == baseline["selected_beat"],
        json!({"original":5,"edit":2,"selected_beat":baseline["selected_beat"]}),
        d.snapshot(),
    )
}

fn editor_context(d: &Driver<'_>) -> Value {
    let snapshot = d.snapshot();
    json!({
        "context":snapshot["context"],"selected_source":snapshot["selected_source"],
        "source_cursor":snapshot["source_cursor"],"sequence_cursor":snapshot["sequence_cursor"],
        "selected_beat":snapshot["selected_beat"],"sequence_scope":snapshot["sequence_scope"],
        "original_selection":snapshot["original_selection"],"visual_selection":snapshot["visual_selection"],
        "revision":snapshot["revision"],"picture":snapshot["picture"],
    })
}

fn preserved(d: &mut Driver<'_>, expected: &Value, label: &str) -> Result<(), String> {
    d.check(
        label,
        editor_context(d) == *expected,
        expected.clone(),
        d.snapshot(),
    )
}

fn update(
    d: &Driver<'_>,
    phase: Phase,
    sample: AudioSample,
    generation: deadpan_output::Generation,
    error: Option<String>,
) -> Result<Update, String> {
    let run = d
        .app()
        .transport
        .as_ref()
        .ok_or("No active sound playback request")?;
    Ok(Update {
        ticket: run.ticket,
        session: run.session,
        project_id: run.project.clone(),
        revision_id: run.revision.clone(),
        content: run.content.clone(),
        phase,
        sample: Some(sample),
        generation: Some(generation),
        error,
    })
}

fn inject(d: &mut Driver<'_>, update: Update, label: &str) -> Result<(), String> {
    d.app_mut().feedback.playback_updates.push_back(update);
    d.capture(label)
}
