use deadpan_core::AudioSample;
use deadpan_playback::{ContentIdentity, Phase, Snapshot, Target, Window};

use super::*;
use crate::transport::{ContentRef, Domain, Identity, Run};

#[derive(Clone, Copy)]
pub(super) struct AuditionContext {
    pub lead: AudioSample,
    pub follow: AudioSample,
}

impl Default for AuditionContext {
    fn default() -> Self {
        Self {
            lead: AudioSample(24_000),
            follow: AudioSample(36_000),
        }
    }
}

impl AuditionContext {
    pub fn lead_label(self) -> String {
        sample_label(self.lead)
    }
    pub fn follow_label(self) -> String {
        sample_label(self.follow)
    }

    fn window(self, domain: &Domain, range: std::ops::Range<u64>) -> Result<Window, String> {
        domain.selection_window(range, self.lead, self.follow)
    }
}

fn sample_label(sample: AudioSample) -> String {
    if sample.0 % 48 == 0 {
        format!("{}ms", sample.0 / 48)
    } else {
        format!("{} samples", sample.0)
    }
}

pub(super) fn sound_time(sample: u64) -> String {
    let millis = sample / 48;
    format!(
        "{:02}:{:02}.{:03}",
        millis / 60_000,
        (millis / 1000) % 60,
        millis % 1000
    )
}

impl DeadpanApp {
    pub(super) fn sound_focused(&self) -> bool {
        self.pane == Pane::Sources && self.selected_sound.is_some()
    }

    pub(super) fn selected_sound_descriptor(&self) -> Option<Arc<deadpan_playback::Sound>> {
        self.workspace
            .as_ref()?
            .sources
            .get(self.selected_sound.as_ref()?)?
            .sound_audition
            .clone()
    }

    pub(super) fn reconcile_sound_playback(&mut self) {
        let descriptor = self.selected_sound_descriptor();
        if self.selected_sound.is_some() && descriptor.is_none() {
            self.stop_playback();
            self.selected_sound = None;
            self.sound_cursor = 0;
            self.bindings.clear();
            return;
        }
        let captured = self
            .transport
            .as_ref()
            .map(|run| run.domain())
            .or_else(|| self.resume.as_ref().map(|resume| resume.domain()));
        if let Some(Domain::Sound(sound)) = captured
            && (!self.sound_focused() || descriptor.as_ref() != Some(sound))
        {
            self.stop_playback();
            self.bindings.clear();
        }
    }

    /// Revoke before navigation/editing; never restore an old audio position
    /// over a new command target.
    pub(super) fn stop_playback(&mut self) -> bool {
        self.resume = None;
        self.playback.stop();
        let Some(run) = self.transport.take() else {
            return false;
        };
        if let Some(draft) = &mut self.trim {
            draft.position = run.content_sample().ok();
        }
        if !run.domain().is_audio_only() {
            self.worker.cancel();
            self.presentation.invalidate_pending();
        }
        true
    }

    pub(super) fn pause_playback(&mut self) {
        let Some(run) = self.transport.as_ref() else {
            return;
        };
        let Ok(position) = run.position() else {
            self.stop_playback();
            return;
        };
        let resume = run.resume(position.cursor);
        let sound = run.domain().is_audio_only();
        if self.stop_playback() {
            self.resume = Some(resume);
            // Cursor can name excluded Out, but the picture must not.
            if !sound && self.trim.is_none() {
                self.request_picture_for_transport_at(false, None, Some(position.picture));
            }
        }
    }

    fn follow_stopped_playback(&mut self, looping: bool) {
        if self.room_tone.is_none()
            && self.gain.is_none()
            && self.splice.is_none()
            && self.slip.is_none()
            && self.trim.is_none()
            && !self.sound_focused()
            && self.view == View::Sequence
            && !looping
        {
            self.follow_playhead_scope();
            self.select_at_cursor();
            self.reveal_beat = true;
        }
    }

    pub(super) fn toggle_playback(&mut self) {
        if self.trim.is_some() {
            self.error = Some("Use the Trim audition controls while Trim is open.".into());
            return;
        }
        if self.slip.is_some() {
            self.error = Some("Finish or cancel Slip preview before playback.".into());
            return;
        }
        if !self.sound_focused() {
            self.cancel_camera();
        }
        if let Some(run) = &self.transport {
            let looping = run.window().looping();
            self.receive_playback();
            self.pause_playback();
            self.follow_stopped_playback(looping);
        } else {
            self.start_playback(false);
        }
    }

    pub(super) fn audition_selection(&mut self) {
        if self.trim.is_some() {
            self.error = Some("Use the Trim audition controls while Trim is open.".into());
            return;
        }
        if self.slip.is_some() {
            self.error = Some("Finish or cancel Slip preview before audition.".into());
            return;
        }
        if !self.sound_focused() {
            self.cancel_camera();
        }
        if self
            .transport
            .as_ref()
            .is_some_and(|run| run.window().looping())
        {
            self.receive_playback();
            self.pause_playback();
        } else {
            self.stop_playback();
            self.start_playback(true);
        }
    }

    fn playback_domain(&self) -> Result<Domain, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("Open a project to audition.")?;
        if self.sound_focused() {
            return self
                .selected_sound_descriptor()
                .map(Domain::Sound)
                .ok_or_else(|| "Choose a qualified sound to audition.".into());
        }
        match self.view {
            View::Sequence => Ok(Domain::Sequence {
                rate: workspace.document.presentation_basis().frame_rate,
                frames: workspace.plan.duration().frames(),
            }),
            View::Source => self
                .selected_source
                .as_ref()
                .and_then(|asset| workspace.sources.get(asset))
                .and_then(|source| source.original_audition.clone())
                .map(Domain::Original)
                .ok_or_else(|| "Choose a qualified Original to audition.".into()),
        }
    }

    fn selected_playback_range(&self) -> Option<std::ops::Range<u64>> {
        if self.sound_focused() {
            return self.selected_sound_descriptor().and_then(|sound| {
                u64::try_from(sound.duration_samples().0)
                    .ok()
                    .map(|end| 0..end)
            });
        }
        match self.view {
            View::Source => self.moment.range(),
            View::Sequence => self
                .selected_edit_range()
                .map(|range| range.start().0 as u64..range.end().0 as u64)
                .or_else(|| {
                    self.beat_rows
                        .iter()
                        .find(|row| Some(&row.id) == self.selected_beat.as_ref())
                        .and_then(|row| row.start.checked_add(row.frames).map(|end| row.start..end))
                }),
        }
    }

    fn start_playback(&mut self, selected: bool) {
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        if self.service.is_busy() || self.dialogs.is_open() {
            return;
        }
        if !self.sound_focused() {
            self.reconcile_moment();
        }
        let prepared = (|| {
            let domain = self.playback_domain()?;
            let end = domain.end()?;
            if end.0 == 0 {
                return Err("There is no time to audition.".into());
            }
            let cursor = if domain.is_sound() {
                self.sound_cursor
            } else {
                match self.view {
                    View::Source => self.source_cursor,
                    View::Sequence => self.sequence_cursor,
                }
            };
            let (window, start) = if selected {
                let range = self
                    .selected_playback_range()
                    .ok_or_else(|| match self.view {
                        View::Source => format!(
                            "Select an Original moment with {} and {} before looping.",
                            self.editor_key(EditorKey::Visual),
                            self.editor_pair(EditorKey::FramePrevious, EditorKey::FrameNext, "/")
                        ),
                        View::Sequence => "Select a beat before looping.".into(),
                    })?;
                let window = if domain.is_sound() {
                    Window::new(AudioSample(0), end, true).map_err(|e| e.to_string())?
                } else {
                    self.audition_context.window(&domain, range)?
                };
                (window, window.start())
            } else {
                let resumed = self.resume.as_ref().and_then(|resume| {
                    resume
                        .sample_for_domain(
                            ContentRef {
                                session: workspace.session,
                                project: workspace.document.project_id(),
                                revision: workspace.document.revision_id(),
                                content: &ContentIdentity::Committed,
                            },
                            &domain,
                            resume.window(),
                            cursor,
                        )
                        .map(|sample| (*resume.window(), sample))
                });
                if let Some(pair) =
                    resumed.filter(|(window, sample)| window.looping() || *sample < end)
                {
                    pair
                } else {
                    let sample = domain.sample_at_boundary(cursor)?;
                    (
                        Window::new(AudioSample(0), end, false).map_err(|e| e.to_string())?,
                        if sample >= end {
                            AudioSample(0)
                        } else {
                            sample
                        },
                    )
                }
            };
            Ok::<_, String>((domain, window, start))
        })();
        let (domain, window, start) = match prepared {
            Ok(value) => value,
            Err(error) => {
                self.error = Some(format!("Cannot start audition: {error}"));
                return;
            }
        };
        self.start_domain_playback(domain, window, start);
    }

    pub(super) fn start_domain_playback(
        &mut self,
        domain: Domain,
        window: Window,
        start: AudioSample,
    ) {
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        self.start_snapshot_playback(
            Arc::new(workspace.playback_snapshot()),
            domain,
            window,
            start,
        );
    }

    pub(super) fn start_snapshot_playback(
        &mut self,
        snapshot: Arc<Snapshot>,
        domain: Domain,
        window: Window,
        start: AudioSample,
    ) {
        let target = match &domain {
            Domain::Sequence { .. } => Target::Sequence,
            Domain::Original(original) => Target::Original(original.clone()),
            Domain::Sound(sound) => Target::Sound(sound.clone()),
            Domain::AudioRange(range) => Target::AudioRange(range.clone()),
        };
        let Some(ticket) = self.next_serial() else {
            return;
        };
        let run = match Run::with_domain(
            Identity {
                ticket,
                session: snapshot.session,
                project: snapshot.document.project_id().clone(),
                revision: snapshot.document.revision_id().clone(),
                content: snapshot.content.clone(),
            },
            domain,
            window,
            start,
        ) {
            Ok(run) => run,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        #[cfg(feature = "ui-harness")]
        let result = if self.feedback.simulate_playback {
            Ok(())
        } else {
            self.playback
                .play_window(ticket, snapshot, target, window, start, self.monitor_gain)
        };
        #[cfg(not(feature = "ui-harness"))]
        let result =
            self.playback
                .play_window(ticket, snapshot, target, window, start, self.monitor_gain);
        match result {
            Ok(()) => {
                self.resume = None;
                if !run.domain().is_audio_only() {
                    self.worker.cancel();
                    self.presentation.invalidate_pending();
                }
                if let Some(draft) = &mut self.trim {
                    draft.position = run.content_sample().ok();
                } else if let Ok(position) = run.position() {
                    match run.domain() {
                        Domain::Original(_) => self.source_cursor = position.cursor,
                        Domain::Sequence { .. } => {
                            if let Some(draft) = &mut self.splice {
                                draft.cursor = position.cursor;
                                draft.position = run.content_sample().ok();
                            } else if self.gain.is_none() {
                                self.sequence_cursor = position.cursor;
                            }
                        }
                        Domain::Sound(_) => self.sound_cursor = position.cursor,
                        Domain::AudioRange(_) => {
                            if let Some(draft) = &mut self.room_tone {
                                draft.cursor = position.cursor;
                            }
                        }
                    }
                }
                self.transport = Some(run);
                self.error = None;
                self.message = None;
                self.bindings.clear();
            }
            Err(error) => self.error = Some(format!("Cannot start audition: {error}")),
        }
    }

    pub(super) fn receive_playback(&mut self) {
        #[cfg(feature = "ui-harness")]
        let update = if self.feedback.simulate_playback {
            self.feedback.playback_updates.pop_front()
        } else {
            self.playback.poll()
        };
        #[cfg(not(feature = "ui-harness"))]
        let update = self.playback.poll();
        let Some(update) = update else {
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
            Ok(Some(_)) if self.trim.is_some() => {
                // The delivery clock updates only Trim's local audio position.
                // Its captured editor cursors and boundary pictures stay fixed.
                if let Some(draft) = &mut self.trim {
                    draft.position = run.content_sample().ok();
                }
            }
            Ok(Some(frame)) => match run.domain() {
                Domain::Original(_) => self.source_cursor = frame,
                Domain::Sequence { .. } => {
                    if let Some(draft) = &mut self.splice {
                        draft.cursor = frame;
                    } else if self.gain.is_none() {
                        self.sequence_cursor = frame;
                    }
                }
                Domain::Sound(_) => self.sound_cursor = frame,
                Domain::AudioRange(_) => {
                    if let Some(draft) = &mut self.room_tone {
                        draft.cursor = frame;
                    }
                }
            },
            Ok(None) => return,
            Err(error) => {
                self.pause_playback();
                self.resume = None;
                self.error = Some(error);
                return;
            }
        }
        let looping = run.window().looping();
        if let Some(draft) = &mut self.splice {
            draft.position = run.content_sample().ok();
        }
        if let Some(draft) = &mut self.gain
            && let Ok(position) = run.content_sample()
        {
            draft.position = position;
        }
        match update.phase {
            Phase::Preparing | Phase::Playing => {}
            Phase::Stopped | Phase::Ended | Phase::Failed => {
                self.pause_playback();
                self.resume = None;
                self.follow_stopped_playback(looping);
                if let Some(error) = update.error {
                    self.error = Some(format!("Audition stopped: {error}"));
                }
            }
        }
    }

    pub(super) fn schedule_playback_picture(&mut self) {
        if self.trim.is_some() {
            return;
        }
        if self
            .transport
            .as_ref()
            .is_some_and(|run| run.domain().is_audio_only())
        {
            return;
        }
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
        if let Some(run) = self.transport.as_mut()
            && let Ok(frame) = run.picture_frame()
            && let Some(generation) = run.picture(frame, busy)
        {
            self.request_picture_for_transport_at(false, Some(generation), Some(frame));
        }
    }

    pub(super) fn playback_controls(&mut self, ui: &mut egui::Ui) {
        if self.workspace.is_none() {
            self.monitor_control = None;
            return;
        }
        if self.gain.is_some() || self.slip.is_some() || self.trim.is_some() {
            self.monitor_control = None;
            return;
        }
        let compact = self.compact_sound_layout(ui.ctx())
            && self.transport.is_none()
            && !self.sound_focused();
        if !self.sound_focused() {
            ui.horizontal_wrapped(|ui| {
            let active = self.transport.is_some();
            let preparing = self.transport.as_ref().is_some_and(|run| run.phase == Phase::Preparing);
            let enabled = active || (!self.service.is_busy() && self.playback_domain().and_then(|domain| domain.end()).is_ok_and(|end| end.0 > 0));
            let action = if preparing { "Cancel preparation" } else if active { "Pause" } else if self.resume.as_ref().is_some_and(|resume| resume.window().looping()) { "Resume loop" } else if self.sound_focused() { "Play sound" } else if self.view == View::Source { "Play Original" } else { "Play edit" };
            let label = format!("{action}  ·  {}", self.editor_key(EditorKey::Playback));
            if ui.add_enabled(enabled, egui::Button::new(label).wrap().fill(style::SELECTED)).clicked() { self.toggle_playback(); }
            let looping = self.transport.as_ref().is_some_and(|run| run.window().looping());
            let action = if looping { "Pause loop" } else if self.sound_focused() { "Loop sound" } else { "Loop selection" };
            let label = format!("{action}  ·  {}", self.editor_key(EditorKey::Audition));
            if ui.add_enabled(enabled && (looping || self.selected_playback_range().is_some()), egui::Button::new(label).wrap()).on_hover_text(format!("Loop the selected Original moment, Edit range or edited beat with {} lead-in and {} follow-through. {} pauses and resumes the exact heard position. Change context with :audition-context.", self.audition_context.lead_label(), self.audition_context.follow_label(), self.editor_key(EditorKey::Playback))).clicked() { self.audition_selection(); }
            if let Some(run) = &self.transport {
                let sample = run.content_sample().unwrap_or(run.sample).0;
                let millis = sample / 48;
                let status = if preparing { "Preparing" } else { "Playing" };
                let lap = if looping { format!(" · loop {}", run.lap().unwrap_or(0) + 1) } else { String::new() };
                ui.label(egui::RichText::new(format!("{status} · {:02}:{:02}.{:03}{lap}", millis / 60_000, (millis / 1000) % 60, millis % 1000)).monospace().size(10.0).color(style::MUTED));
            }
            if compact { self.monitor_slider(ui); }
        });
        }
        if !compact {
            ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(if self.sound_focused() { "Sound loop · complete measured audio".into() } else { format!("Loop context  {} / {}", self.audition_context.lead_label(), self.audition_context.follow_label()) }).size(10.5).color(style::MUTED))
                .on_hover_text("Lead-in / follow-through, clamped to this source or edit. Change with :audition-context lead=500ms follow=750ms. Playback uses edge fades and a safety limiter; full voice effects and mastering remain unavailable.");
            self.monitor_slider(ui);
        });
        }
        // Pointer activation runs while painting these controls, after the
        // viewer reserved their previous height. Reflow the same outer frame
        // before presenting an inline row that just gained a live clock.
        let compact_now = self.compact_sound_layout(ui.ctx())
            && self.transport.is_none()
            && !self.sound_focused();
        if compact != compact_now {
            ui.ctx().request_discard("playback controls changed height");
        }
    }

    fn monitor_slider(&mut self, ui: &mut egui::Ui) {
        let monitor = ui
            .add_enabled(
                self.transport.is_none(),
                egui::Slider::new(&mut self.monitor_gain, 0.0..=1.0)
                    .text("Monitor · :monitor")
                    .show_value(false),
            )
            .on_hover_text(format!(
                "Monitor {:.1}%. Pause to change volume. Does not change export gain.",
                self.monitor_gain * 100.0
            ));
        self.monitor_control = Some(monitor.id);
    }

    pub(super) fn sound_preview_controls(&mut self, ui: &mut egui::Ui) {
        // Keep the panel in the tree even without a selection, so the catalog
        // retains its widget identities when audition controls appear.
        let panel = egui::Panel::bottom("sound-preview")
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::NONE);
        let Some(sound) = self.selected_sound_descriptor() else {
            panel.exact_size(0.0).show(ui, |_| {});
            return;
        };
        let sound_label = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.sources.get(sound.asset()))
            .map_or_else(|| sound.asset().to_string(), |source| source.label.clone());
        let end = sound.duration_samples().0 as u64;
        let active = self
            .transport
            .as_ref()
            .is_some_and(|run| run.domain().is_sound());
        let preparing = active
            && self
                .transport
                .as_ref()
                .is_some_and(|run| run.phase == Phase::Preparing);
        let paused = self
            .resume
            .as_ref()
            .is_some_and(|resume| resume.domain().is_sound());
        let looping = active
            && self
                .transport
                .as_ref()
                .is_some_and(|run| run.window().looping());
        let enabled = active || (!self.service.is_busy() && end > 0);
        let action = if preparing {
            "Cancel preparation"
        } else if active {
            "Pause sound"
        } else if paused {
            "Resume sound"
        } else {
            "Play sound"
        };
        let label = format!("{action}  ·  {}", self.editor_key(EditorKey::Playback));
        let loop_label = format!(
            "{}  ·  {}",
            if looping { "Pause loop" } else { "Loop sound" },
            self.editor_key(EditorKey::Audition)
        );
        // Use this exact label for measurement, paint and accessibility.
        let placement_label = format!(
            "Place at edit cursor  ·  {}",
            self.editor_key(EditorKey::PlaceSound)
        );
        let frame = egui::Frame::new()
            .fill(style::PANEL)
            .stroke(egui::Stroke::new(1.0, style::BORDER))
            .corner_radius(6)
            .inner_margin(8);
        let width = (ui.available_width() - frame.total_margin().sum().x).max(1.0);
        let text = |text: egui::RichText, wrap, width| {
            egui::WidgetText::from(text).into_galley(ui, Some(wrap), width, egui::TextStyle::Body)
        };
        let title = text(
            egui::RichText::new("Sound").strong(),
            egui::TextWrapMode::Extend,
            width,
        );
        let status = text(
            egui::RichText::new(if preparing {
                "Preparing"
            } else if active {
                "Playing"
            } else if paused {
                "Paused"
            } else {
                "Ready"
            })
            .color(if active && !preparing {
                style::SAVED
            } else {
                style::MUTED
            }),
            egui::TextWrapMode::Wrap,
            (width - title.size().x - ui.spacing().item_spacing.x).max(1.0),
        );
        let name = text(
            egui::RichText::new(&sound_label),
            egui::TextWrapMode::Truncate,
            width,
        );
        let clock = text(
            egui::RichText::new(format!(
                "{} / {}",
                sound_time(self.sound_cursor),
                sound_time(end)
            ))
            .monospace()
            .size(12.0),
            egui::TextWrapMode::Wrap,
            width,
        );
        let note = text(
            egui::RichText::new("Source-local audio · picture stays in place")
                .size(10.5)
                .color(style::MUTED),
            egui::TextWrapMode::Wrap,
            width,
        );
        let padding = ui.spacing().button_padding * 2.0;
        let play = text(
            egui::RichText::new(label),
            egui::TextWrapMode::Wrap,
            (width - padding.x).max(1.0),
        );
        let looping_text = text(
            egui::RichText::new(loop_label),
            egui::TextWrapMode::Wrap,
            (width - padding.x).max(1.0),
        );
        let placement_text = text(
            egui::RichText::new(&placement_label),
            egui::TextWrapMode::Wrap,
            (width - padding.x).max(1.0),
        );
        let destination = text(
            egui::RichText::new(format!("Destination: Edit {} f", self.sequence_cursor))
                .size(11.0)
                .color(style::CURSOR),
            egui::TextWrapMode::Wrap,
            width,
        );
        // Reuse these exact galleys for layout and paint. This bottom panel
        // must fit on its first frame, including narrow windows and wrapped
        // shortcut labels; it cannot inherit the previous selection's height.
        let button_height =
            |galley: &egui::Galley| (galley.size().y + padding.y).max(ui.spacing().interact_size.y);
        let height = frame.total_margin().sum().y
            + ui.spacing()
                .interact_size
                .y
                .max(title.size().y)
                .max(status.size().y)
            + name.size().y
            + clock.size().y
            + 4.0
            + button_height(&play)
            + button_height(&looping_text)
            + note.size().y
            + button_height(&placement_text)
            + destination.size().y
            + 8.0 * 4.0;
        panel.exact_size(height).show(ui, |ui| { frame.show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(title));
                ui.add(egui::Label::new(status));
            });
            ui.add(egui::Label::new(name)).on_hover_text(&sound_label);
            ui.add(egui::Label::new(clock));
            ui.add(egui::ProgressBar::new(if end == 0 { 0.0 } else { self.sound_cursor.min(end) as f32 / end as f32 }).desired_width(ui.available_width()).desired_height(4.0));
            if ui.add_enabled(enabled, egui::Button::new(egui::WidgetText::from(play)).fill(style::SELECTED)).clicked() {
                if !self.sound_focused() { self.stop_playback(); }
                self.pane = Pane::Sources;
                ui.memory_mut(|m| m.request_focus(pane_id(Pane::Sources)));
                self.toggle_playback();
            }
            if ui.add_enabled(enabled, egui::Button::new(egui::WidgetText::from(looping_text))).on_hover_text(format!("Loop the complete measured sound. {} pauses and resumes its exact heard position.", self.editor_key(EditorKey::Playback))).clicked() {
                if !self.sound_focused() { self.stop_playback(); }
                self.pane = Pane::Sources;
                ui.memory_mut(|m| m.request_focus(pane_id(Pane::Sources)));
                self.audition_selection();
            }
            ui.add(egui::Label::new(note));
            let placement = ui.add_enabled(!self.service.is_busy(), egui::Button::new(egui::WidgetText::from(placement_text)).fill(style::SELECTED))
                .on_hover_text("Place the complete catalog sound at the retained edit cursor. Picture duration stays unchanged; overflow is rejected.");
            placement.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, placement.enabled(), &placement_label));
            if placement.clicked() {
                self.sound_action(navigation::SoundAction::Place, ui.ctx());
            }
            ui.add(egui::Label::new(destination));
        }); });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::FrameRate;

    #[test]
    fn loop_context_is_exact_in_audio_samples_and_clamps_to_the_captured_domain() {
        let domain = Domain::Sequence {
            rate: FrameRate::new(30_000, 1001).unwrap(),
            frames: 120,
        };
        let window = AuditionContext::default().window(&domain, 30..60).unwrap();
        assert_eq!(window.start(), AudioSample(24_048));
        assert_eq!(window.end(), AudioSample(132_096));
        assert!(window.looping());
        let context = AuditionContext {
            lead: AudioSample(i64::MAX),
            follow: AudioSample(i64::MAX),
        };
        let window = context.window(&domain, 30..60).unwrap();
        assert_eq!(window.start(), AudioSample(0));
        assert_eq!(window.end(), domain.end().unwrap());
        assert!(context.window(&domain, 40..40).is_err());
        assert!(context.window(&domain, 110..121).is_err());
    }
}
