//! Opt-in paired real-device playback with real app-owned Extension inference.
//! Scheduling times are UI observations, not exact GPU preemption timestamps.

use super::*;
use crate::jobs::{JobKind, RowState};
use crate::project::generation::{Outcome, Phase as AiPhase};
use deadpan_core::{HoldVideo, NodeKind, ProjectFrame};
use deadpan_jobs::{ConditioningMode, WorkerStage};
use deadpan_playback::Phase;
use egui::{Key, Modifiers};

const STARTUP_LIMIT: Duration = Duration::from_secs(10 * 60);
const CONTROL_LIMIT: Duration = Duration::from_secs(120);

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let mut evidence = json!({
        "scope":"paired real audio and offscreen Metal; scheduling timestamps are sampled UI observations",
        "limitations":["No physical display, compositor latency, acoustic sync or listening measurement", "No exact GPU preemption or protocol acknowledgment timestamp measurement"],
        "intervals":[], "interaction_samples":[], "scheduling_observations":[],
        "startup_control_observation_hz":10,"playback_observation_hz":d.options.hz,
        "interaction_scope":"three stopped seeks, one split and one undo per interval: smoke samples, not percentile qualification",
    });
    let origin = Instant::now();
    let result = exercise(d, origin, &mut evidence);
    // Always stop owned audio and ask the production generation service to
    // cancel, even after a measurement assertion or a native presentation error.
    d.app_mut().stop_playback();
    let cancel_error = if model_live(d) {
        cancel_model(d).err()
    } else {
        None
    };
    let drained = drain_model(d, origin, &mut evidence);
    let cleanup = drained.and_then(|()| cancel_error.map_or(Ok(()), Err));
    evidence["result"] = json!(result.as_ref().err());
    evidence["cleanup_error"] = json!(cleanup.as_ref().err());
    evidence["elapsed_seconds"] = json!(origin.elapsed().as_secs_f64());
    std::fs::write(
        d.options.output.join("ai-playback-interference.json"),
        serde_json::to_vec_pretty(&evidence).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    result?;
    cleanup
}

fn exercise(d: &mut Driver<'_>, origin: Instant, evidence: &mut Value) -> Result<(), String> {
    if d.app().feedback.simulate_playback {
        return Err("Interference qualification requires real device audio".into());
    }
    let workspace = d
        .app()
        .workspace
        .clone()
        .ok_or("Missing Original project")?;
    let rate = workspace.document.presentation_basis().frame_rate;
    let original_frames = workspace.plan.duration().frames();
    if rate
        .audio_boundary(ProjectFrame(original_frames))
        .map_err(|e| e.to_string())?
        .0
        < 40 * 48_000
        || workspace
            .document
            .nodes()
            .values()
            .any(|node| matches!(node.kind, NodeKind::Hold { .. }))
    {
        return Err("Interference fixture must be a fresh Original of at least forty seconds, without Holds".into());
    }
    let loop_frames = i64::from(rate.numerator()) * 5 / i64::from(rate.denominator());
    if !(2..=600).contains(&loop_frames) {
        return Err(
            "Interference loop requires 2 to 600 pictures in its first five seconds".into(),
        );
    }
    evidence["fixture"] = json!({"project":workspace.path,"presentation_basis":workspace.document.presentation_basis(),"original_frames":original_frames,"loop_frames":loop_frames});
    d.command("proxies off")?;
    d.command("monitor 5%")?;
    d.command("sequence")?;
    super::transcript::focus_your_edit(d)?;
    at(d, original_frames)?;
    let before = d.revision();
    d.command("hold 3s")?;
    d.changed(&before)?;
    let hold = d.app().ai_hold().ok_or("Trailing Hold was not selected")?;
    let fallback = d
        .app()
        .workspace
        .clone()
        .ok_or("Missing fallback workspace")?;
    if !matches!(&fallback.document.nodes()[&hold].kind, NodeKind::Hold { recipe } if !matches!(recipe.video, HoldVideo::Generated { .. }))
    {
        return Err("Trailing Hold did not commit an unaccepted fallback".into());
    }
    evidence["hold"] =
        json!({"id":hold,"revision":d.revision(),"duration_seconds":3,"accepted":false});
    interaction_samples(d, "baseline", evidence)?;
    playback_interval(d, origin, evidence, "baseline", loop_frames, false)?;

    at(d, original_frames)?;
    d.command("generate mode=extend-left")?;
    pump_until(
        d,
        origin,
        evidence,
        "Real Extension inference startup",
        STARTUP_LIMIT,
        |d| completed_steps(d).is_some_and(|steps| steps >= 1),
    )?;
    let job = d
        .app()
        .ai
        .job()
        .cloned()
        .ok_or("Missing real Extension job")?;
    if job.operation != Some(ConditioningMode::ExtendFromLeft) || job.request.is_none() {
        return Err("Generation did not start a recorded left Extension attempt".into());
    }
    evidence["generation"] = json!({"ticket":job.ticket,"request":job.request,"operation":format!("{:?}",job.operation),"plan":format!("{:?}",job.plan),"startup_seconds":origin.elapsed().as_secs_f64()});
    playback_interval(d, origin, evidence, "inference", loop_frames, true)?;
    let paused_steps =
        completed_steps(d).ok_or("Model left inference before resume was observed")?;
    pump_until(
        d,
        origin,
        evidence,
        "Real inference resumes after playback",
        CONTROL_LIMIT,
        |d| {
            model_state(d).is_some_and(|state| state == RowState::Running)
                && completed_steps(d).is_some_and(|steps| steps > paused_steps)
        },
    )?;
    if !model_live(d) {
        return Err("Model completed before active-inference interactions".into());
    }
    interaction_samples(d, "inference", evidence)?;
    cancel_model(d)?;
    pump_until(
        d,
        origin,
        evidence,
        "Real inference cancellation and teardown",
        CONTROL_LIMIT,
        |d| {
            !model_live(d)
                && d.app()
                    .ai
                    .job()
                    .is_some_and(|job| job.outcome == Some(Outcome::Cancelled))
        },
    )?;
    let current = d
        .app()
        .workspace
        .as_ref()
        .ok_or("Missing final workspace")?;
    if !matches!(&current.document.nodes()[&hold].kind, NodeKind::Hold { recipe } if !matches!(recipe.video, HoldVideo::Generated { .. }))
    {
        return Err("Qualification unexpectedly accepted generated pictures".into());
    }
    evidence["cancelled"] = json!({"outcome":format!("{:?}",d.app().ai.job().and_then(|job| job.outcome.as_ref())),"model_rows":0,"revision":d.revision(),"fallback_preserved":true});
    let passed = evidence["intervals"].as_array().is_some_and(|intervals| {
        intervals.len() == 2 && intervals.iter().all(|interval| interval["passed"] == true)
    });
    d.check("Real inference yields to device playback, resumes, permits edits and cancels without acceptance", passed, json!("paired complete playback intervals without underruns or faults; model paused, resumed and cancelled"), json!({"intervals_passed":passed,"evidence":"ai-playback-interference.json"}))
}

fn cancel_model(d: &mut Driver<'_>) -> Result<(), String> {
    let Some(row) = d
        .app()
        .service
        .jobs()
        .snapshot()
        .into_iter()
        .find(|row| row.kind == JobKind::AiPause)
    else {
        return Ok(());
    };
    let result = (|| {
        d.key(Key::Escape)?;
        d.command("jobs")?;
        for _ in 0..64 {
            if d.app().jobs.selected() == Some(&crate::preview::jobs::RowKey::Job(row.id)) {
                d.key(Key::X)?;
                d.key(Key::Escape)?;
                return Ok(());
            }
            d.key(Key::J)?;
        }
        Err("Could not select the captured AI job for cancellation through Jobs".to_owned())
    })();
    if result.is_err() {
        // An already failed UI frame must not leave inference running. The
        // coordinator's captured-id cancellation flag works without painting;
        // also send the production service cancellation for this exact ticket.
        d.app().service.jobs().cancel(row.id);
        if let Some(ticket) = row.tag {
            d.app_mut().ai_cancel_ticket(ticket)?;
        }
    }
    result
}

fn drain_model(d: &mut Driver<'_>, origin: Instant, evidence: &mut Value) -> Result<(), String> {
    let started = Instant::now();
    let mut frame_error = None;
    while model_live(d) {
        if started.elapsed() >= CONTROL_LIMIT {
            return Err("Timed out draining the real model during cleanup".into());
        }
        let tick = Instant::now();
        if let Err(error) = d.step("Final worker cleanup", false) {
            frame_error.get_or_insert(error);
        }
        observe(d, origin, evidence);
        if let Some(rest) = Duration::from_millis(100).checked_sub(tick.elapsed()) {
            std::thread::sleep(rest);
        }
    }
    evidence["cleanup_model_rows"] = json!(0);
    frame_error.map_or(Ok(()), Err)
}

fn model_live(d: &Driver<'_>) -> bool {
    d.app()
        .service
        .jobs()
        .snapshot()
        .iter()
        .any(|row| row.kind == JobKind::AiPause)
}

fn model_state(d: &Driver<'_>) -> Option<RowState> {
    d.app()
        .service
        .jobs()
        .snapshot()
        .into_iter()
        .find(|row| row.kind == JobKind::AiPause)
        .map(|row| row.state)
}

fn completed_steps(d: &Driver<'_>) -> Option<u64> {
    match &d.app().ai.job()?.phase {
        AiPhase::Step {
            stage: WorkerStage::Inference,
            completed,
            ..
        } if d.app().ai.job()?.running() => Some(*completed),
        _ => None,
    }
}

fn observe(d: &Driver<'_>, origin: Instant, evidence: &mut Value) {
    let state = json!({"model":model_state(d).map(|state|format!("{state:?}")),"phase":d.app().ai.job().map(|job|format!("{:?}",job.phase)),"outcome":d.app().ai.job().map(|job|format!("{:?}",job.outcome)),"playback":d.app().transport.as_ref().map(|run|format!("{:?}",run.phase))});
    let observations = evidence["scheduling_observations"]
        .as_array_mut()
        .expect("observation array");
    if observations
        .last()
        .is_none_or(|previous| previous["state"] != state)
    {
        observations
            .push(json!({"observed_ms":origin.elapsed().as_secs_f64()*1000.0,"state":state}));
    }
}

fn pump_until(
    d: &mut Driver<'_>,
    origin: Instant,
    evidence: &mut Value,
    label: &str,
    limit: Duration,
    ready: impl Fn(&Driver<'_>) -> bool,
) -> Result<(), String> {
    let start = Instant::now();
    while !ready(d) {
        if start.elapsed() >= limit {
            return Err(format!("Timed out waiting for {label}: {}", d.snapshot()));
        }
        let tick = Instant::now();
        d.step(label, false)?;
        if let Some(rest) = Duration::from_millis(100).checked_sub(tick.elapsed()) {
            std::thread::sleep(rest);
        }
        observe(d, origin, evidence);
        if let Some(job) = d.app().ai.job()
            && matches!(
                job.outcome,
                Some(Outcome::Failed(_) | Outcome::Unavailable(_))
            )
        {
            return Err(format!("{label}: model concluded {:?}", job.outcome));
        }
    }
    observe(d, origin, evidence);
    Ok(())
}

fn paced_step(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    let start = Instant::now();
    d.step(label, false)?;
    if let Some(rest) =
        Duration::from_secs_f64(1.0 / f64::from(d.options.hz)).checked_sub(start.elapsed())
    {
        std::thread::sleep(rest);
    }
    Ok(())
}

fn at(d: &mut Driver<'_>, frame: i64) -> Result<(), String> {
    d.key(Key::Escape)?;
    d.chord(&[Key::G, Key::G])?;
    motion(d, frame)?;
    d.settled()
}

fn motion(d: &mut Driver<'_>, frames: i64) -> Result<(), String> {
    if frames == 0 {
        return Ok(());
    }
    if frames < 0 {
        return Err("Negative replay frame".into());
    }
    for digit in frames.to_string().chars() {
        d.key(Key::from_name(&digit.to_string()).ok_or("Invalid frame digit")?)?;
    }
    d.key(Key::L)
}

fn interaction_samples(
    d: &mut Driver<'_>,
    label: &str,
    evidence: &mut Value,
) -> Result<(), String> {
    let rate = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No workspace")?
        .document
        .presentation_basis()
        .frame_rate;
    for second in [2, 12, 4] {
        if label == "inference" && completed_steps(d).is_none() {
            return Err("Model left active inference before interaction sampling".into());
        }
        let started = Instant::now();
        at(
            d,
            i64::from(rate.numerator()) * second / i64::from(rate.denominator()),
        )?;
        evidence["interaction_samples"].as_array_mut().expect("samples").push(json!({"interval":label,"operation":"stopped_seek","second":second,"settled_ms":started.elapsed().as_secs_f64()*1000.0,"model_state":model_state(d).map(|state|format!("{state:?}"))}));
    }
    if label == "inference" && completed_steps(d).is_none() {
        return Err("Model left active inference before split".into());
    }
    let before = d.revision();
    let started = Instant::now();
    d.command("split")?;
    d.changed(&before)?;
    let split = d.revision();
    evidence["interaction_samples"].as_array_mut().expect("samples").push(json!({"interval":label,"operation":"split","settled_ms":started.elapsed().as_secs_f64()*1000.0,"before":before,"after":split,"model_state":model_state(d).map(|state|format!("{state:?}"))}));
    let started = Instant::now();
    d.key(Key::U)?;
    d.changed(&split)?;
    evidence["interaction_samples"].as_array_mut().expect("samples").push(json!({"interval":label,"operation":"undo","settled_ms":started.elapsed().as_secs_f64()*1000.0,"revision":d.revision()}));
    Ok(())
}

fn playback_interval(
    d: &mut Driver<'_>,
    origin: Instant,
    evidence: &mut Value,
    label: &str,
    frames: i64,
    inference: bool,
) -> Result<(), String> {
    let rate = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No workspace")?
        .document
        .presentation_basis()
        .frame_rate;
    let length = rate
        .audio_boundary(ProjectFrame(frames))
        .map_err(|e| e.to_string())?
        .0;
    let laps = usize::try_from((30 * 48_000 + length - 1) / length).map_err(|e| e.to_string())?;
    let count = usize::try_from(frames).map_err(|e| e.to_string())?;
    let mut shown = vec![false; count * laps];
    d.command("audition-context lead=0ms follow=0ms")?;
    at(d, 0)?;
    d.key(Key::V)?;
    motion(d, frames)?;
    d.key(Key::V)?;
    // Retain the captured Edit range while seeking its included beginning.
    d.chord(&[Key::G, Key::G])?;
    d.settled()?;
    let selected = d.app().selected_edit_range().ok_or("Loop range absent")?;
    if selected.start().0 != 0 || selected.end().0 != frames {
        return Err("Loop selection differs from the first five seconds".into());
    }
    let revision = d.revision();
    let audio_before = d.app().playback.diagnostics();
    let pictures_before = deadpan_diagnostics::snapshot().playback_pictures;
    let started = Instant::now();
    let mut first_audio = None;
    let mut generation = None;
    let mut heard_lap = 0_u64;
    let mut previous = None;
    let mut pictures = Vec::new();
    let mut acknowledged_pause = false;
    let mut window_matches = true;
    let mut missing_attribution = 0_u64;
    d.key_modified(Key::Space, Modifiers::SHIFT)?;
    while started.elapsed() < Duration::from_secs(55) {
        paced_step(d, "Real playback interference interval")?;
        observe(d, origin, evidence);
        let mut heard_frame = None;
        if let Some(run) = d.app().transport.as_ref() {
            if run.phase == Phase::Playing {
                first_audio.get_or_insert(started.elapsed().as_secs_f64() * 1000.0);
            }
            generation = run.generation;
            heard_lap = run.lap()?;
            heard_frame = Some(crate::transport::frame_at_sample(
                rate,
                frames,
                run.content_sample()?,
            )?);
            window_matches &= run.window().looping()
                && run.window().start().0 == 0
                && run.window().end().0 == length;
            if inference
                && run.phase == Phase::Playing
                && matches!(model_state(d), Some(RowState::Paused(_)))
            {
                acknowledged_pause = true;
            }
        }
        if let Some((frame, ticket)) = d.app().presentation.displayed_sequence()
            && generation.is_some()
            && ticket.transport == generation
            && previous != Some((frame, ticket))
        {
            // Bind to the immutable device lap retained when this exact
            // presentation ticket was requested. Stalled full laps cannot be
            // filled by a later observation of the same stale picture.
            let attribution = d
                .app()
                .feedback
                .transport_picture_requests
                .iter()
                .rev()
                .find(|(requested, _, _)| *requested == ticket)
                .map(|(_, lap, sample)| (*lap, *sample));
            if !record_picture(&mut shown, count, frame.0, attribution.map(|(lap, _)| lap)) {
                missing_attribution += 1;
            }
            let lag = attribution.and_then(|(picture_lap, _)| {
                heard_frame.map(|heard| {
                    (i128::from(heard_lap) - i128::from(picture_lap)) * i128::from(frames)
                        + i128::from(heard)
                        - i128::from(frame.0)
                })
            });
            pictures.push(json!({"observed_ms":started.elapsed().as_secs_f64()*1000.0,"frame":frame.0,"request_lap":attribution.map(|(lap,_)|lap),"request_sample":attribution.map(|(_,sample)|sample),"heard_lap":heard_lap,"heard_frame":heard_frame,"clock_lag_frames":lag,"ticket":format!("{ticket:?}")}));
            previous = Some((frame, ticket));
        }
        if heard_lap >= laps as u64 || d.app().transport.is_none() {
            break;
        }
    }
    let errors = json!({"app":d.app().error,"project":d.app().project_error,"picture":d.app().presentation.error()});
    let audio_after = d.app().playback.diagnostics();
    let pictures_after = deadpan_diagnostics::snapshot().playback_pictures;
    let missing = shown
        .iter()
        .enumerate()
        .filter_map(|(index, shown)| {
            (!shown).then_some(json!({"lap":index/count,"frame":index%count}))
        })
        .collect::<Vec<_>>();
    let passed = heard_lap >= laps as u64
        && first_audio.is_some()
        && window_matches
        && audio_after.reports > audio_before.reports
        && audio_after.starved == audio_before.starved
        && audio_after.faults == audio_before.faults
        && missing.is_empty()
        && missing_attribution == 0
        && pictures_after.reduced == pictures_before.reduced
        && (!inference || acknowledged_pause)
        && d.revision() == revision
        && errors
            .as_object()
            .is_some_and(|errors| errors.values().all(Value::is_null));
    evidence["intervals"].as_array_mut().expect("intervals").push(json!({
        "label":label,"passed":passed,"duration_ms":started.elapsed().as_secs_f64()*1000.0,"first_audio_ms":first_audio,
        "expected_laps":laps,"heard_laps":heard_lap,"window_frames":[0,frames],"window_matches":window_matches,
        "revision":revision,"revision_unchanged":d.revision()==revision,"acknowledged_pause_while_playing":acknowledged_pause,
        "missing_pictures":missing,"missing_ticket_attributions":missing_attribution,"pictures":pictures,"errors":errors,
        "audio":{"reports":audio_after.reports-audio_before.reports,"underruns":audio_after.starved-audio_before.starved,"faults":audio_after.faults-audio_before.faults,"silent_padding_frames":audio_after.silent_frames-audio_before.silent_frames,"max_callback_render_cost_ns":audio_after.max_render_cost_ns},
        "pictures_count":{"requested":pictures_after.requested-pictures_before.requested,"skipped":pictures_after.skipped-pictures_before.skipped,"exact":pictures_after.exact-pictures_before.exact,"reduced":pictures_after.reduced-pictures_before.reduced},
    }));
    // Keep measurement failures in evidence and continue the paired workload.
    // Space is the production transport toggle; navigation revokes its resume.
    if d.app().transport.is_some() {
        d.key(Key::Space)?;
    }
    d.chord(&[Key::G, Key::G])?;
    d.app_mut().stop_playback();
    paced_step(d, "Stopped interference loop")?;
    observe(d, origin, evidence);
    Ok(())
}

/// Coverage belongs to request-time lap identity, never the current heard lap.
/// Pictures of the next partial lap are valid but outside the measured laps.
fn record_picture(shown: &mut [bool], frames: usize, frame: i64, request_lap: Option<u64>) -> bool {
    let Some(lap) = request_lap.and_then(|lap| usize::try_from(lap).ok()) else {
        return false;
    };
    let Ok(frame) = usize::try_from(frame) else {
        return false;
    };
    if frame >= frames {
        return false;
    }
    let Some(index) = lap
        .checked_mul(frames)
        .and_then(|start| start.checked_add(frame))
    else {
        return false;
    };
    if let Some(shown) = shown.get_mut(index) {
        *shown = true;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::record_picture;

    #[test]
    fn stale_picture_after_whole_device_lap_cannot_fill_that_lap() {
        let mut shown = [false; 12];
        for frame in 0..4 {
            assert!(record_picture(&mut shown, 4, frame, Some(0)));
        }
        // Device delivery is now in lap two. A delayed lap-zero ticket remains
        // lap zero, even when its picture coordinate equals the current one.
        assert!(record_picture(&mut shown, 4, 2, Some(0)));
        assert!(record_picture(&mut shown, 4, 2, Some(2)));
        assert_eq!(
            shown,
            [
                true, true, true, true, false, false, false, false, false, false, true, false
            ]
        );
    }

    #[test]
    fn missing_request_identity_never_counts_as_delivered_coverage() {
        let mut shown = [false; 8];
        assert!(!record_picture(&mut shown, 4, 1, None));
        assert!(!record_picture(&mut shown, 4, 4, Some(0)));
        assert!(!record_picture(&mut shown, 4, -1, Some(0)));
        assert_eq!(shown, [false; 8]);
        assert!(
            record_picture(&mut shown, 4, 0, Some(2)),
            "next partial lap is outside requested coverage"
        );
        assert_eq!(shown, [false; 8]);
    }
}
