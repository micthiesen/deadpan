use deadpan_core::{AudioSample, ProjectFrame};
use deadpan_playback::{Phase, Snapshot, SourceEntry};

use super::*;

impl DeadpanApp {
    /// Stop before changing the cursor. This method deliberately never restores
    /// an old audio position over a new navigation or edit target.
    pub(super) fn stop_playback(&mut self) -> bool {
        self.resume = None;
        self.playback.stop();
        if self.transport.take().is_none() {
            return false;
        }
        self.worker.cancel();
        self.presentation.invalidate_pending();
        true
    }

    pub(super) fn pause_playback(&mut self) {
        let Some(run) = self.transport.as_ref() else {
            return;
        };
        let resume = run.resume(self.sequence_cursor);
        if self.stop_playback() {
            self.resume = Some(resume);
            self.request_picture_for_transport(false, None);
        }
    }

    pub(super) fn toggle_playback(&mut self) {
        if self.transport.is_some() {
            // Admit the latest device estimate before immediate revocation.
            // Already submitted native buffers are not retractable.
            self.receive_playback();
            self.pause_playback();
            self.select_at_cursor();
            self.reveal_beat = true;
            return;
        }
        if self.view != View::Sequence {
            self.message = Some("Switch to Your edit (:sequence) to audition the sequence. Original playback is not available yet.".into());
            return;
        }
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        if self.service.is_busy() || self.dialogs.is_open() || self.sequence_length() == 0 {
            return;
        }
        if self.sequence_cursor >= self.sequence_length() {
            self.sequence_cursor = 0;
        }
        let rate = workspace.document.presentation_basis().frame_rate;
        let resumed = self.resume.as_ref().and_then(|resume| {
            resume.sample_for(
                workspace.session,
                workspace.document.project_id(),
                workspace.document.revision_id(),
                self.sequence_cursor,
            )
        });
        let start = match resumed.map_or_else(
            || rate.audio_boundary(ProjectFrame(self.sequence_cursor as i64)),
            Ok,
        ) {
            Ok(start) => start,
            Err(error) => {
                self.error = Some(format!("Cannot start audition: {error}"));
                return;
            }
        };
        let Some(ticket) = self.next_serial() else {
            return;
        };
        let snapshot = Arc::new(Snapshot {
            session: workspace.session,
            document: Arc::clone(&workspace.document),
            originals: workspace.originals.clone(),
            sources: workspace
                .sources
                .iter()
                .map(|(asset, source)| {
                    (
                        asset.clone(),
                        SourceEntry {
                            receipt: Arc::clone(&source.receipt),
                            original: source.original.clone(),
                        },
                    )
                })
                .collect(),
        });
        match self
            .playback
            .play(ticket, snapshot, start, self.monitor_gain)
        {
            Ok(()) => {
                self.resume = None;
                self.worker.cancel();
                self.presentation.invalidate_pending();
                self.transport = Some(crate::transport::Run::new(
                    ticket,
                    workspace.session,
                    workspace.document.project_id().clone(),
                    workspace.document.revision_id().clone(),
                    rate,
                    workspace.plan.duration().frames(),
                    start,
                ));
                self.error = None;
                self.message = None;
                self.bindings.clear();
            }
            Err(error) => self.error = Some(format!("Cannot start audition: {error}")),
        }
    }

    pub(super) fn receive_playback(&mut self) {
        let Some(update) = self.playback.poll() else {
            return;
        };
        let Some(run) = self.transport.as_mut() else {
            if update.phase == Phase::Failed
                && self
                    .resume
                    .as_ref()
                    .is_some_and(|resume| resume.matches(&update))
            {
                self.resume = None;
                self.error = Some(format!(
                    "Audition stopped: {}",
                    update.error.as_deref().unwrap_or("device pause failed")
                ));
            }
            return;
        };
        match run.receive(&update) {
            Ok(Some(frame)) => self.sequence_cursor = frame,
            Ok(None) => return,
            Err(error) => {
                self.pause_playback();
                self.error = Some(error);
                return;
            }
        }
        match update.phase {
            Phase::Preparing | Phase::Playing => {}
            Phase::Stopped | Phase::Ended | Phase::Failed => {
                self.pause_playback();
                self.resume = None;
                self.select_at_cursor();
                self.reveal_beat = true;
                if let Some(error) = update.error {
                    self.error = Some(format!("Audition stopped: {error}"));
                }
            }
        }
    }

    pub(super) fn schedule_playback_picture(&mut self) {
        if self.transport.is_some()
            && let Some(error) = self.presentation.error().map(str::to_owned)
        {
            self.stop_playback();
            self.error = Some(format!(
                "Audition stopped because the picture failed: {error}"
            ));
            return;
        }
        let busy = self.presentation.loading() || self.presentation.needs_render();
        let frame = self
            .sequence_cursor
            .min(self.sequence_length().saturating_sub(1));
        if let Some(generation) = self
            .transport
            .as_mut()
            .and_then(|run| run.picture(frame, busy))
        {
            self.request_picture_for_transport(false, Some(generation));
        }
    }

    pub(super) fn playback_controls(&mut self, ui: &mut egui::Ui) {
        if self.view != View::Sequence || self.workspace.is_none() {
            self.monitor_control = None;
            return;
        }
        ui.horizontal_wrapped(|ui| {
            let active = self.transport.is_some();
            let preparing = self.transport.as_ref().is_some_and(|run| run.phase == Phase::Preparing);
            let enabled = active || (self.sequence_length() > 0 && !self.service.is_busy());
            let label = if preparing { "Cancel preparation  ·  Space" } else if active { "Pause  ·  Space" } else { "Play edit  ·  Space" };
            if ui.add_enabled(enabled, egui::Button::new(label).fill(style::SELECTED)).clicked() {
                self.toggle_playback();
            }
            ui.label(egui::RichText::new("Limited audition").size(10.5).color(style::MUTED))
                .on_hover_text("Sequence sound with edge fades and a −1 dBTP safety limiter. Voice effects and the full mix remain unavailable. Monitor volume does not change the project or export gain.");
            let monitor = ui.add_enabled(!active, egui::Slider::new(&mut self.monitor_gain, 0.0..=1.0)
                .text("Monitor · :monitor").show_value(false))
                .on_hover_text(format!("Monitor {:.1}%. Pause to change volume. The initial level is 12.5%.", self.monitor_gain * 100.0));
            self.monitor_control = Some(monitor.id);
            if let Some(run) = &self.transport {
                let AudioSample(sample) = run.sample;
                let status = if preparing { "Preparing" } else { "Playing" };
                let millis = sample / 48;
                ui.label(egui::RichText::new(format!("{status} · {:02}:{:02}.{:03}", millis / 60_000, (millis / 1000) % 60, millis % 1000)).monospace().size(10.0).color(style::MUTED));
            }
        });
    }
}
