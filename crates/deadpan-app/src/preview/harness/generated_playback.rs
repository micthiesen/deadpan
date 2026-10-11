//! Real device playback through the app's worker and complete offscreen Metal UI.

use super::*;
use deadpan_core::ProjectFrame;
use deadpan_playback::Phase;

pub(super) fn run(d: &mut Driver<'_>, looping: bool) -> Result<(), String> {
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
    let (start, excluded, laps) = if looping {
        let start = i64::try_from(first).map_err(|e| e.to_string())?;
        let excluded = i64::try_from(last + 1).map_err(|e| e.to_string())?;
        let length = rate
            .audio_boundary(ProjectFrame(excluded))
            .map_err(|e| e.to_string())?
            .0
            - rate
                .audio_boundary(ProjectFrame(start))
                .map_err(|e| e.to_string())?
                .0;
        if last == first || length <= 0 || length > 8 * 48_000 {
            return Err("Generated loop requires at least two pictures and a window of at most eight seconds".into());
        }
        d.command("audition-context lead=0ms follow=0ms")?;
        motion(d, first)?;
        d.key(egui::Key::V)?;
        motion(d, last + 1 - first)?;
        d.key(egui::Key::V)?;
        // Preview the included start before audio begins. This is ordinary
        // stopped preview, whose one-slot policy releases the old Original.
        d.chord(&[egui::Key::G, egui::Key::G])?;
        motion(d, first)?;
        let selected = d
            .app()
            .selected_edit_range()
            .ok_or("Loop selection is absent")?;
        if selected.start().0 != start || selected.end().0 != excluded {
            return Err("Loop selection differs from the exact Generated interval".into());
        }
        (
            first,
            last + 1,
            usize::try_from((30 * 48_000 + length - 1) / length).map_err(|e| e.to_string())?,
        )
    } else {
        (0, generated.len(), 1)
    };
    d.settled()?;
    let revision = d.revision();
    d.check("Real playback engine, explicit low monitor gain and exact pictures",
        !d.app().feedback.simulate_playback && d.app().monitor_gain == 0.05,
        json!({"simulated_delivery":false,"monitor_gain":0.05}),
        json!({"simulated_delivery":d.app().feedback.simulate_playback,"monitor_gain":d.app().monitor_gain}))?;
    let before = d.app().playback.diagnostics();
    let pictures_before = deadpan_diagnostics::snapshot().playback_pictures;
    let started = Instant::now();
    if looping {
        d.key_modified(egui::Key::Space, egui::Modifiers::SHIFT)?;
    } else {
        d.key(egui::Key::Space)?;
    }
    let mut first_audio_ms = None;
    let mut generation = None;
    let mut coverage = Coverage::new(start, excluded, laps)?;
    let mut pictures = Vec::new();
    let mut previous = None;
    let mut last_sample = None;
    let mut last_content = None;
    let mut heard_lap = 0;
    let mut loop_window_matches = true;
    let mut lost_device_lap = false;
    let cadence = Duration::from_secs_f64(1.0 / f64::from(d.options.hz));
    while started.elapsed() < Duration::from_secs(48) {
        let step_started = Instant::now();
        d.step("Real accepted-project playback", false)?;
        if let Some(run) = d.app().transport.as_ref()
            && matches!(run.phase, Phase::Playing | Phase::Ended)
        {
            if run.phase == Phase::Playing {
                first_audio_ms.get_or_insert(started.elapsed().as_secs_f64() * 1000.0);
            }
            generation = run.generation;
            last_sample = Some(run.sample);
            last_content = Some(run.content_sample()?);
            heard_lap = run.lap()?;
            if looping {
                loop_window_matches &= run.window().looping()
                    && run.window().start()
                        == rate
                            .audio_boundary(ProjectFrame(start as i64))
                            .map_err(|e| e.to_string())?
                    && run.window().end()
                        == rate
                            .audio_boundary(ProjectFrame(excluded as i64))
                            .map_err(|e| e.to_string())?;
            }
        }
        let terminal_handoff = d
            .app()
            .feedback
            .last_playback_update
            .as_ref()
            .is_some_and(|update| update.phase == Phase::Ended);
        if terminal_handoff {
            last_sample = d
                .app()
                .feedback
                .last_playback_update
                .as_ref()
                .and_then(|update| update.sample);
            last_content = last_sample;
        }
        if let Some((frame, ticket)) = d.app().presentation.displayed_sequence()
            && generation.is_some()
            && ticket.transport == generation
            && previous != Some((frame, ticket))
        {
            let index = usize::try_from(frame.0).map_err(|e| e.to_string())?;
            let lap = coverage.record(index)?;
            if looping && heard_lap > lap as u64 + 1 {
                lost_device_lap = true;
            }
            let heard = last_content
                .map(|sample| crate::transport::frame_at_sample(rate, frames, sample))
                .transpose()?;
            let lag = heard.map(|heard| {
                (i128::from(heard_lap) - lap as i128) * (excluded - start) as i128
                    + i128::from(heard)
                    - i128::from(frame.0)
            });
            pictures.push(json!({
                "frame":frame.0,"generated":generated[index],
                "lap":lap,"heard_lap":heard_lap,
                "wall_ms":started.elapsed().as_secs_f64()*1000.0,
                "heard_sample":last_sample.map(|sample|sample.0),
                "after_audio_end":terminal_handoff,
                "sampled_clock_lag_frames":lag,
                "ticket":format!("{ticket:?}"),
            }));
            previous = Some((frame, ticket));
        }
        if d.app().transport.is_none() {
            break;
        }
        if looping && coverage.lap >= laps {
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
    let ended = if looping {
        coverage.completed(laps, heard_lap)
            && loop_window_matches
            && !lost_device_lap
            && d.app().transport.is_some()
    } else {
        d.app().transport.is_none()
            && d.app().sequence_cursor == u64::try_from(frames).map_err(|e| e.to_string())?
    };
    let errors = json!({"app":d.app().error,"project":d.app().project_error,"picture":d.app().presentation.error()});
    let failed = d.app().error.is_some()
        || d.app().project_error.is_some()
        || d.app().presentation.error().is_some();
    // Stop on every result before reporting an assertion failure.
    let stopped_picture = d.app().presentation.displayed_sequence();
    d.app_mut().stop_playback();
    if looping {
        // Stop intentionally retains the last submitted picture even when
        // the heard cursor has advanced beyond it. Do not demand a new seek.
        d.step("Stopped real accepted loop", false)?;
    } else {
        d.settled()?;
    }
    let stop_preserved_picture =
        !looping || d.app().presentation.displayed_sequence() == stopped_picture;
    let after = d.app().playback.diagnostics();
    let pictures_after = deadpan_diagnostics::snapshot().playback_pictures;
    let missed = coverage
        .shown
        .iter()
        .enumerate()
        .filter_map(|(frame, yes)| (!yes).then_some(frame))
        .collect::<Vec<_>>();
    let summary = json!({
        "presentation_basis":workspace.document.presentation_basis(),
        "viewport_points":[d.harness.ctx.content_rect().width(),d.harness.ctx.content_rect().height()],
        "frames":frames,"rate":rate,"duration_samples":end.0,"elapsed_ms":elapsed_ms,
        "looping":looping,"window_frames":[start,excluded],"expected_laps":laps,
        "lost_device_lap":lost_device_lap,
        "stop_preserved_picture":stop_preserved_picture,
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
        d.options.output.join(if looping {
            "generated-loop.json"
        } else {
            "generated-playback.json"
        }),
        serde_json::to_vec_pretty(&summary).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    d.check(
        "Real accepted playback reaches the end without audio or picture faults",
        ended
            && stop_preserved_picture
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
        "Every Original and Generated frame reaches Metal through the terminal handoff",
        missed.is_empty(),
        json!([]),
        json!(missed),
    )
}

fn motion(d: &mut Driver<'_>, frames: usize) -> Result<(), String> {
    if frames == 0 {
        return Ok(());
    }
    for digit in frames.to_string().chars() {
        d.key(egui::Key::from_name(&digit.to_string()).ok_or("Invalid frame-count digit")?)?;
    }
    d.key(egui::Key::L)
}

/// Count displayed wraps independently of the heard clock, so reusing a
/// picture from an earlier lap cannot fill a later lap's coverage.
struct Coverage {
    start: usize,
    excluded: usize,
    previous: Option<usize>,
    lap: usize,
    shown: Vec<bool>,
}

impl Coverage {
    fn new(start: usize, excluded: usize, laps: usize) -> Result<Self, String> {
        let count = excluded
            .checked_sub(start)
            .and_then(|span| span.checked_mul(laps))
            .filter(|count| *count > 0 && *count <= 200_000)
            .ok_or("Invalid or excessive playback coverage window")?;
        Ok(Self {
            start,
            excluded,
            previous: None,
            lap: 0,
            shown: vec![false; count],
        })
    }

    fn record(&mut self, frame: usize) -> Result<usize, String> {
        if !(self.start..self.excluded).contains(&frame) {
            return Err("Displayed frame escaped the captured playback window".into());
        }
        if self.previous.is_some_and(|previous| frame < previous) {
            self.lap += 1;
        }
        self.previous = Some(frame);
        let ordinal = self.lap * (self.excluded - self.start) + frame - self.start;
        if let Some(shown) = self.shown.get_mut(ordinal) {
            *shown = true;
        }
        Ok(self.lap)
    }

    fn completed(&self, laps: usize, heard_lap: u64) -> bool {
        self.lap >= laps && heard_lap >= laps as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_keeps_each_lap_and_missing_boundary_independent() {
        let mut coverage = Coverage::new(60, 64, 2).unwrap();
        for frame in [60, 61, 62, 60, 61, 63, 60] {
            coverage.record(frame).unwrap();
        }
        assert_eq!(
            coverage.shown,
            [true, true, true, false, true, true, false, true]
        );
        assert_eq!(coverage.lap, 2);
        assert!(
            !coverage.completed(2, 1),
            "pictures cannot substitute for device delivery"
        );
        assert!(coverage.completed(2, 2));
        assert!(coverage.record(64).is_err());
        assert!(coverage.record(59).is_err());
    }
}
