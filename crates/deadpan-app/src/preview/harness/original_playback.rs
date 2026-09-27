//! Original audition through production inputs and explicit delivery updates.

use super::*;
use deadpan_core::AudioSample;
use deadpan_playback::{Phase, Update};
use egui::{Key, Modifiers};

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("Playback service updates are explicitly injected. This scenario exercises Original controls, clock admission, looping and picture routing, not PCM preparation, device timing or listening.".into());
    let revision = d.revision();
    let sequence_cursor = d.app().sequence_cursor;
    let selected_beat = d.app().selected_beat.clone();
    let scope = d.app().sequence_scope.clone();
    d.command("source")?;
    let original = d
        .app()
        .selected_source
        .as_ref()
        .and_then(|asset| d.app().workspace.as_ref()?.sources.get(asset))
        .and_then(|source| source.original_audition.clone())
        .ok_or("The replay Original has no audition descriptor")?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    d.key(Key::Space)?;
    let first_sample = original.sample_at_boundary(10)?;
    d.check(
        "Original Space prepares its own full-source clock at the cursor",
        d.app().transport.as_ref().is_some_and(|run| {
            matches!(run.domain(), crate::transport::Domain::Original(_))
                && run.phase == Phase::Preparing
                && run.sample == first_sample
                && !run.window().looping()
                && run.window().start() == AudioSample(0)
                && run.window().end() == original.end()
        }) && d.rect("Cancel preparation  ·  Space").is_ok(),
        json!("Original preparation at boundary 10 with full-source window"),
        d.snapshot(),
    )?;
    let (mut feed, _callback) = deadpan_output::channel().map_err(|error| error.to_string())?;
    let first_generation = feed
        .restart(first_sample.0)
        .map_err(|error| error.to_string())?;
    let heard = AudioSample(original.sample_at_boundary(12)?.0 + 5);
    let playing = update(d, Phase::Playing, heard, first_generation, None)?;
    inject(
        d,
        playing.clone(),
        "Original delivery advances only its source cursor",
    )?;
    d.settled()?;
    d.check(
        "Original delivery maps exact samples to the displayed source frame",
        d.app().source_cursor == 12
            && d.app().sequence_cursor == sequence_cursor
            && d.app().selected_beat == selected_beat
            && d.app().sequence_scope == scope
            && d.revision() == revision,
        json!({"source_cursor":12,"sequence_cursor":sequence_cursor,"revision":revision}),
        d.snapshot(),
    )?;
    d.key(Key::Space)?;
    inject(d, playing, "Paused Original ignores an old delivery report")?;
    d.check(
        "Space pause retains the source position and its exact resume estimate",
        d.app().transport.is_none() && d.app().resume.is_some() && d.app().source_cursor == 12,
        json!("paused at source frame 12 with an exact resume estimate"),
        d.snapshot(),
    )?;
    d.key(Key::Space)?;
    d.check(
        "Space resume preserves the Original subframe sample",
        d.app()
            .transport
            .as_ref()
            .is_some_and(|run| run.sample == heard),
        json!(heard.0),
        d.snapshot(),
    )?;
    d.click("Cancel preparation  ·  Space")?;
    d.capture("Pointer cancellation exposes Original playback again")?;
    d.click("Play Original  ·  Space")?;
    d.capture("Pointer Original playback exposes preparation cancellation")?;
    d.check(
        "Pointer Original playback uses the same exact resume path",
        d.app().transport.as_ref().is_some_and(|run| {
            run.sample == heard && matches!(run.domain(), crate::transport::Domain::Original(_))
        }),
        json!(heard.0),
        d.snapshot(),
    )?;
    d.click("Cancel preparation  ·  Space")?;

    d.chord(&[
        Key::G,
        Key::G,
        Key::Num1,
        Key::Num0,
        Key::L,
        Key::V,
        Key::Num1,
        Key::Num4,
        Key::L,
    ])?;
    d.key_modified(Key::Space, Modifiers::SHIFT)?;
    let default_start = AudioSample((original.sample_at_boundary(10)?.0 - 24_000).max(0));
    let default_end =
        AudioSample((original.sample_at_boundary(24)?.0 + 36_000).min(original.end().0));
    d.check(
        "Shift Space applies the default 500ms lead and 750ms follow within the Original",
        d.app().moment.range() == Some(10..24)
            && d.app().moment.active
            && d.app().audition_context.lead == AudioSample(24_000)
            && d.app().audition_context.follow == AudioSample(36_000)
            && d.app().transport.as_ref().is_some_and(|run| {
                run.window().looping()
                    && run.window().start() == default_start
                    && run.window().end() == default_end
            }),
        json!({"selection":[10,24],"window":[default_start.0,default_end.0]}),
        d.snapshot(),
    )?;
    d.key(Key::Space)?;
    d.command("audition-context lead=0ms follow=0ms")?;
    d.key_modified(Key::Space, Modifiers::SHIFT)?;
    let window = *d
        .app()
        .transport
        .as_ref()
        .ok_or("Selection loop did not start")?
        .window();
    let start = original.sample_at_boundary(10)?;
    let end = original.sample_at_boundary(24)?;
    d.check(
        "Zero context loops the selected half-open Original moment exactly",
        window.start() == start && window.end() == end && window.looping(),
        json!({"window":[start.0,end.0],"looping":true}),
        d.snapshot(),
    )?;
    let generation = feed.restart(start.0).map_err(|error| error.to_string())?;
    let initial_loop = update(d, Phase::Playing, start, generation, None)?;
    inject(d, initial_loop, "Loop starts at its included In boundary")?;
    let seam = update(d, Phase::Playing, end, generation, None)?;
    inject(d, seam, "The excluded Out boundary wraps directly to In")?;
    d.check(
        "A loop seam shows In and never the excluded Out frame",
        d.app().source_cursor == 10
            && d.app().moment.range() == Some(10..24)
            && d.app().transport.as_ref().is_some_and(|run| {
                run.content_sample() == Ok(start) && run.picture_frame() == Ok(10)
            }),
        json!({"source_cursor":10,"content":start.0,"range":[10,24]}),
        d.snapshot(),
    )?;
    let length = end.0 - start.0;
    let content = AudioSample(original.sample_at_boundary(17)?.0 + 5);
    let delivery = AudioSample(content.0 + 2 * length);
    let looping = update(d, Phase::Playing, delivery, generation, None)?;
    inject(
        d,
        looping.clone(),
        "Monotonic delivery crosses two Original loop seams",
    )?;
    d.settled()?;
    d.check(
        "Repeated delivery wraps content without extending Visual selection or editing",
        d.app().source_cursor == 17
            && d.app().moment.range() == Some(10..24)
            && d.app().moment.active
            && d.revision() == revision
            && d.app().transport.as_ref().is_some_and(|run| {
                run.sample == delivery
                    && run.content_sample() == Ok(content)
                    && run.lap() == Ok(2)
            }),
        json!({"source_cursor":17,"range":[10,24],"delivery":delivery.0,"content":content.0,"lap":2}),
        d.snapshot(),
    )?;
    d.key(Key::Space)?;
    d.key(Key::Space)?;
    d.check(
        "Space resumes the exact delivery sample beyond two loop laps",
        d.app()
            .transport
            .as_ref()
            .is_some_and(|run| run.sample == delivery && *run.window() == window),
        json!({"sample":delivery.0,"loop_window":[start.0,end.0]}),
        d.snapshot(),
    )?;
    let resumed_generation = feed
        .restart(delivery.0)
        .map_err(|error| error.to_string())?;
    let resumed = update(d, Phase::Playing, delivery, resumed_generation, None)?;
    inject(
        d,
        resumed.clone(),
        "Resumed loop admits its new output generation",
    )?;
    let sample_after_resume = d
        .app()
        .transport
        .as_ref()
        .ok_or("Resumed loop disappeared")?
        .sample;
    let mut stale_ticket = looping;
    stale_ticket.sample = Some(AudioSample(delivery.0 + 123));
    inject(
        d,
        stale_ticket,
        "Resumed loop rejects the previous request ticket",
    )?;
    let mut stale_generation = resumed.clone();
    stale_generation.generation = Some(generation);
    stale_generation.sample = Some(AudioSample(delivery.0 + 123));
    inject(
        d,
        stale_generation,
        "Resumed loop rejects the previous output generation",
    )?;
    d.check(
        "Stale request and generation cannot move the resumed Original loop",
        d.app().source_cursor == 17
            && d.app()
                .transport
                .as_ref()
                .is_some_and(|run| run.sample == sample_after_resume)
            && d.app().moment.range() == Some(10..24),
        json!({"source_cursor":17,"sample":sample_after_resume.0,"range":[10,24]}),
        d.snapshot(),
    )?;
    d.click("Pause loop  ·  ⇧Space")?;
    d.capture("Pointer pause exposes the selection loop control")?;
    d.click("Loop selection  ·  ⇧Space")?;
    d.check(
        "Pointer loop restarts the same half-open window selected by Shift Space",
        d.app()
            .transport
            .as_ref()
            .is_some_and(|run| *run.window() == window && run.sample == start)
            && d.app().moment.range() == Some(10..24),
        json!({"sample":start.0,"window":[start.0,end.0]}),
        d.snapshot(),
    )?;
    d.key(Key::L)?;
    d.check(
        "Source navigation stops the loop and discards its exact resume estimate",
        d.app().transport.is_none() && d.app().resume.is_none() && d.app().source_cursor == 11,
        json!("stopped without resume at source frame 11"),
        d.snapshot(),
    )?;
    d.key(Key::Space)?;
    let fault_sample = d
        .app()
        .transport
        .as_ref()
        .ok_or("Fault test playback did not start")?
        .sample;
    let fault_generation = feed
        .restart(fault_sample.0)
        .map_err(|error| error.to_string())?;
    let fault = update(
        d,
        Phase::Failed,
        fault_sample,
        fault_generation,
        Some("UI replay simulated an Original output device failure".into()),
    )?;
    inject(d, fault, "Original output fault stops audition explicitly")?;
    inject(
        d,
        resumed,
        "A report arriving after the fault cannot restart Original playback",
    )?;
    d.capture("Original remains stopped after a device failure")?;
    d.check(
        "Original faults never silently resume and leave all Sequence editing context untouched",
        d.app().transport.is_none()
            && d.app().resume.is_none()
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Original output device failure"))
            && d.app().sequence_cursor == sequence_cursor
            && d.app().selected_beat == selected_beat
            && d.app().sequence_scope == scope
            && d.revision() == revision,
        json!({"stopped":true,"sequence_cursor":sequence_cursor,"revision":revision}),
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
        .ok_or("No active playback request")?;
    Ok(Update {
        ticket: run.ticket,
        session: run.session,
        project_id: run.project.clone(),
        revision_id: run.revision.clone(),
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
