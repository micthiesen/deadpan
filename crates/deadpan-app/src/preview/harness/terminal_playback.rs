//! Deterministic endpoint delivery with actual decoding and Metal submission.

use super::*;
use deadpan_core::AudioSample;
use deadpan_playback::{Phase, Update};
use egui::Key;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("Device updates are explicitly injected; decoding and Metal submission are real. This scenario does not measure device timing or physical display latency.".into());
    let revision = d.revision();
    let (mut feed, _callback) = deadpan_output::channel().map_err(|e| e.to_string())?;
    for view in ["sequence", "source"] {
        for mode in [
            "pending",
            "decoded",
            "unscheduled",
            "stop",
            "navigate",
            "fault",
        ] {
            d.command(view)?;
            d.chord(&[Key::G, Key::G])?;
            d.settled()?;
            d.key(Key::Space)?;
            let generation = feed.restart(0).map_err(|e| e.to_string())?;
            let run = d.app().transport.as_ref().ok_or("No replay transport")?;
            let end = run.window().end();
            let update = |phase, sample, error| Update {
                preparation: None,
                ticket: run.ticket,
                session: run.session,
                project_id: run.project.clone(),
                revision_id: run.revision.clone(),
                content: run.content.clone(),
                phase,
                sample: Some(sample),
                generation: Some(generation),
                error,
            };
            let playing = update(Phase::Playing, AudioSample(end.0 - 1), None);
            let ended = update(Phase::Ended, end, None);
            let failed = update(
                Phase::Failed,
                end,
                Some("Terminal replay device failure".into()),
            );
            d.app_mut().feedback.hold_preview = true;
            if mode != "unscheduled" {
                d.app_mut().feedback.playback_updates.push_back(playing);
                d.capture(&format!("{view} {mode}: request final included picture"))?;
                d.wait_for("Real final decode is held before delivery", |app| {
                    app.feedback.held_reply.is_some()
                })?;
            }
            let mut ticket = d
                .app()
                .feedback
                .held_reply
                .as_ref()
                .map(|reply| reply.ticket);
            if mode == "decoded" {
                let reply = d
                    .app_mut()
                    .feedback
                    .held_reply
                    .take()
                    .ok_or("No held decode")?;
                d.app_mut()
                    .presentation
                    .receive(reply)
                    .ok_or("Final decode rejected")??;
                d.check(
                    "Final decode awaits GPU before End",
                    d.app().presentation.needs_render(),
                    json!(true),
                    d.snapshot(),
                )?;
            }
            d.app_mut().feedback.playback_updates.push_back(ended);
            // Deliver End before the next GPU pass, exactly the ordering that
            // previously discarded a decoded final picture.
            d.app_mut().receive_playback();
            d.check(
                "End retains picture work at the exact frozen audio endpoint",
                d.app()
                    .transport
                    .as_ref()
                    .is_some_and(|run| run.phase == Phase::Ended && run.sample == end)
                    && d.app().resume.is_none()
                    && (mode != "decoded" || d.app().presentation.needs_render()),
                json!({"phase":"Ended","sample":end.0,"resume":false}),
                d.snapshot(),
            )?;
            d.capture(&format!("{view} {mode}: finish picture after audio End"))?;
            if mode != "decoded" {
                d.check(
                    "Finishing picture offers Stop while audio remains ended",
                    d.rect("Stop  Space").is_ok()
                        && d.app()
                            .transport
                            .as_ref()
                            .is_some_and(|run| run.phase == Phase::Ended),
                    json!("Stop with Ended transport"),
                    d.snapshot(),
                )?;
            }
            if mode == "unscheduled" {
                d.wait_for("End schedules the missing final picture once", |app| {
                    app.feedback.held_reply.is_some()
                })?;
                ticket = d
                    .app()
                    .feedback
                    .held_reply
                    .as_ref()
                    .map(|reply| reply.ticket);
            }
            let expected = ticket.ok_or("Final playback picture was never submitted")?;
            if matches!(mode, "stop" | "navigate" | "fault") {
                match mode {
                    "stop" => d.key(Key::Space)?,
                    "navigate" => d.key(Key::H)?,
                    _ => {
                        d.app_mut().feedback.playback_updates.push_back(failed);
                        d.capture("Device fault revokes the terminal handoff")?;
                    }
                }
                d.check(
                    "Stop, navigation and fault revoke the handoff without resume",
                    d.app().transport.is_none()
                        && d.app().resume.is_none()
                        && (mode != "fault"
                            || d.app()
                                .error
                                .as_deref()
                                .is_some_and(|e| e.contains("Terminal replay device failure"))),
                    json!("stopped; no automatic resume"),
                    d.snapshot(),
                )?;
            }
            d.app_mut().feedback.release_reply = d.app_mut().feedback.held_reply.take();
            d.app_mut().feedback.hold_preview = false;
            d.wait_for(
                "Terminal picture handoff completes or remains revoked",
                |app| app.transport.is_none(),
            )?;
            if matches!(mode, "pending" | "decoded" | "unscheduled") {
                let shown = d.app().presentation.diagnostic_snapshot();
                d.check(
                    "The original final request reaches Metal before stopped refinement",
                    shown["displayed"]["ticket"] == format!("{expected:?}")
                        && d.app().resume.is_none(),
                    json!(format!("{expected:?}")),
                    shown,
                )?;
            }
            d.settled()?;
            d.check(
                "Late work cannot revive transport or alter authored history",
                d.app().transport.is_none()
                    && d.app().resume.is_none()
                    && d.revision() == revision
                    && d.app().presentation.diagnostic_snapshot()["displayed"]["ticket"]
                        != format!("{expected:?}"),
                json!("stopped picture; unchanged revision"),
                d.snapshot(),
            )?;
        }
    }
    Ok(())
}
