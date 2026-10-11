//! Real device playback through the app's worker and complete offscreen Metal UI.

use super::*;
use deadpan_core::ProjectFrame;
use deadpan_playback::Phase;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("Offscreen submission does not measure compositor/physical display latency or listening. Clock lag uses the last device estimate admitted by the UI before that frame's GPU completion; it is not an acoustic sync measurement.".into());
    let workspace = Arc::clone(d.app().workspace.as_ref().ok_or("Missing workspace")?);
    let frames = workspace.plan.duration().frames();
    let rate = workspace.document.presentation_basis().frame_rate;
    let end = rate
        .audio_boundary(ProjectFrame(frames))
        .map_err(|e| e.to_string())?;
    if frames <= 0 || frames > 3_600 || end.0 > 32 * 48_000 {
        return Err(
            "Generated playback requires a nonempty fixture of at most 32 seconds and 3600 frames"
                .into(),
        );
    }
    let generated = (0..frames)
        .map(|frame| {
            workspace
                .plan
                .picture(ProjectFrame(frame))
                .map(|sample| {
                    matches!(
                        sample.picture,
                        deadpan_plan::Picture::Accepted {
                            generated: Some(_),
                            ..
                        }
                    )
                })
                .map_err(|e| e.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let first = generated
        .iter()
        .position(|yes| *yes)
        .ok_or("Fixture has no accepted Generated pictures")?;
    let last = generated
        .iter()
        .rposition(|yes| *yes)
        .ok_or("Fixture has no accepted Generated pictures")?;
    if first == 0 || last + 1 == generated.len() {
        return Err("Fixture must include Original pictures before and after its accepted Generated pictures".into());
    }
    d.command("proxies off")?;
    d.command("monitor 5%")?;
    d.command("sequence")?;
    d.chord(&[egui::Key::G, egui::Key::G])?;
    d.settled()?;
    let revision = d.revision();
    d.check("Real playback engine, explicit low monitor gain and exact pictures",
        !d.app().feedback.simulate_playback && d.app().monitor_gain == 0.05,
        json!({"simulated_delivery":false,"monitor_gain":0.05}),
        json!({"simulated_delivery":d.app().feedback.simulate_playback,"monitor_gain":d.app().monitor_gain}))?;
    let before = d.app().playback.diagnostics();
    let pictures_before = deadpan_diagnostics::snapshot().playback_pictures;
    let started = Instant::now();
    d.key(egui::Key::Space)?;
    let mut first_audio_ms = None;
    let mut generation = None;
    let mut shown = vec![false; generated.len()];
    let mut pictures = Vec::new();
    let mut previous = None;
    let mut last_sample = None;
    let cadence = Duration::from_secs_f64(1.0 / f64::from(d.options.hz));
    while started.elapsed() < Duration::from_secs(48) {
        let step_started = Instant::now();
        d.step("Real accepted-project playback", false)?;
        if let Some(run) = d.app().transport.as_ref()
            && run.phase == Phase::Playing
        {
            first_audio_ms.get_or_insert(started.elapsed().as_secs_f64() * 1000.0);
            generation = run.generation;
            last_sample = Some(run.sample);
        }
        if let Some((frame, ticket)) = d.app().presentation.displayed_sequence()
            && generation.is_some()
            && ticket.transport == generation
            && previous != Some((frame, ticket))
        {
            let index = usize::try_from(frame.0).map_err(|e| e.to_string())?;
            *shown
                .get_mut(index)
                .ok_or("Displayed frame escaped the captured edit")? = true;
            let heard = last_sample
                .map(|sample| crate::transport::frame_at_sample(rate, frames, sample))
                .transpose()?;
            pictures.push(json!({
                "frame":frame.0,"generated":generated[index],
                "wall_ms":started.elapsed().as_secs_f64()*1000.0,
                "heard_sample":last_sample.map(|sample|sample.0),
                "sampled_clock_lag_frames":heard.map(|heard|i128::from(heard)-i128::from(frame.0)),
                "ticket":format!("{ticket:?}"),
            }));
            previous = Some((frame, ticket));
        }
        if d.app().transport.is_none() {
            break;
        }
        if first_audio_ms.is_none() && started.elapsed() > Duration::from_secs(15) {
            break;
        }
        if let Some(rest) = cadence.checked_sub(step_started.elapsed()) {
            std::thread::sleep(rest);
        }
    }
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    let ended = d.app().transport.is_none()
        && d.app().sequence_cursor == u64::try_from(frames).map_err(|e| e.to_string())?;
    let errors = json!({"app":d.app().error,"project":d.app().project_error,"picture":d.app().presentation.error()});
    let failed = d.app().error.is_some()
        || d.app().project_error.is_some()
        || d.app().presentation.error().is_some();
    // Stop on every result before reporting an assertion failure.
    d.app_mut().stop_playback();
    d.settled()?;
    let after = d.app().playback.diagnostics();
    let pictures_after = deadpan_diagnostics::snapshot().playback_pictures;
    let missed = shown
        .iter()
        .enumerate()
        .filter_map(|(frame, yes)| (!yes).then_some(frame))
        .collect::<Vec<_>>();
    let summary = json!({
        "presentation_basis":workspace.document.presentation_basis(),
        "viewport_points":[d.harness.ctx.content_rect().width(),d.harness.ctx.content_rect().height()],
        "frames":frames,"rate":rate,"duration_samples":end.0,"elapsed_ms":elapsed_ms,
        "first_audio_ms":first_audio_ms,"last_sample":last_sample.map(|sample|sample.0),
        "ended":ended,"errors":errors,"missing_frames":missed,"pictures":pictures,
        "generated_frames":generated.iter().filter(|yes|**yes).count(),
        "native_picture_counts":{
            "requested":pictures_after.requested-pictures_before.requested,
            "skipped":pictures_after.skipped-pictures_before.skipped,
            "exact":pictures_after.exact-pictures_before.exact,
            "reduced":pictures_after.reduced-pictures_before.reduced,
            "lookahead_started":pictures_after.lookahead_started-pictures_before.lookahead_started,
            "lookahead_swaps":pictures_after.lookahead_swaps-pictures_before.lookahead_swaps,
            "lookahead_failed":pictures_after.lookahead_failed-pictures_before.lookahead_failed,
        },
        "audio":{"generations":after.generations-before.generations,
            "device_reports":after.reports-before.reports,"underruns":after.starved-before.starved,
            "faults":after.faults-before.faults,"silent_padding_frames":after.silent_frames-before.silent_frames,
            "max_callback_render_cost_ns":after.max_render_cost_ns},
    });
    std::fs::write(
        d.options.output.join("generated-playback.json"),
        serde_json::to_vec_pretty(&summary).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    d.check(
        "Real accepted playback reaches the end without audio or picture faults",
        ended
            && !failed
            && first_audio_ms.is_some()
            && after.reports > before.reports
            && after.starved == before.starved
            && after.faults == before.faults
            && pictures_after.reduced == pictures_before.reduced
            && d.revision() == revision,
        json!("unchanged revision; complete device delivery; no underruns or faults"),
        summary,
    )?;
    d.check(
        "Every Original and Generated frame reaches Metal during playback",
        missed.is_empty(),
        json!([]),
        json!(missed),
    )
}
