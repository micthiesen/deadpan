//! Waveform range (`:wave`, specification §8.3 and §4.5): select exact 48 kHz
//! mix samples on the measured Edit waveform, audition them, and mute or step
//! the gain of exactly those samples of the beat containing them. Changes go
//! through the semantic path as `SampleRangeMute` / `SampleRangeStep`, so they
//! are recorded, repeated by `.` and undone once. The selection itself is
//! runtime state bound to one session; it never enters authored history.

use std::ops::Range;
use std::sync::Arc;

use deadpan_audio::EditWaveform;
use deadpan_core::{AudioSample, ProjectFrame};
use deadpan_playback::{EditWaveformRequest, WaveformStatus, WaveformTicket};
use eframe::egui;

use super::*;
use crate::navigation::wave_range::{WaveKey, WaveStep};
use crate::transport::Domain;

const FOCUS_ID: &str = "wave-range-focus";
/// The narrowest view: 50 ms, one measured bin per sample.
const MIN_VIEW: i64 = 2_400;
/// The widest view: 4 s.
const MAX_VIEW: i64 = 192_000;
/// Samples measured on each side of the head for a zero-crossing snap; small
/// enough that every leaf bin is one sample.
const SNAP_REACH: i64 = 480;
/// A range step's size, as the editor's `+`/`-`.
const STEP_MILLIDECIBELS: i32 = 3_000;

#[derive(Default)]
struct Measurement {
    ticket: Option<WaveformTicket>,
    /// Revision and samples of the request in flight or shown.
    key: Option<(RevisionId, Range<i64>)>,
    data: Option<(Range<i64>, Arc<EditWaveform>)>,
    status: Option<WaveformStatus>,
    error: Option<String>,
}

pub(super) struct WaveRange {
    session: u64,
    /// Fixed end of the selection, in absolute Edit mix samples.
    anchor: i64,
    /// Moving end.
    head: i64,
    /// The samples drawn.
    view: Range<i64>,
    measurement: Measurement,
    /// A zero-crossing request for the head, measured at one bin per sample.
    snap: Option<(WaveformTicket, i64)>,
    keys: Vec<WaveKey>,
    focus_pending: bool,
    /// A pointer drag is selecting; the view holds still under it.
    dragging: bool,
    note: Option<String>,
}

impl WaveRange {
    /// The selected half-open sample range.
    pub(super) fn samples(&self) -> Range<i64> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    #[cfg(feature = "ui-harness")]
    pub(super) fn state_for_check(&self) -> serde_json::Value {
        let samples = self.samples();
        serde_json::json!({
            "anchor": self.anchor,
            "head": self.head,
            "samples": [samples.start, samples.end],
            "view": [self.view.start, self.view.end],
            "status": self.measurement.status.map(|status| format!("{status:?}")),
            "snapping": self.snap.is_some(),
            "note": self.note,
        })
    }
}

/// A view around `samples` that fits it with margin, inside the project.
fn view_for(samples: &Range<i64>, head: i64, total: i64) -> Range<i64> {
    let width = ((samples.end - samples.start) * 3)
        .clamp(MIN_VIEW, MAX_VIEW)
        .min(total.max(1));
    let centre = if samples.end - samples.start > width / 2 {
        (samples.start + samples.end) / 2
    } else {
        head
    };
    let start = (centre - width / 2).clamp(0, (total - width).max(0));
    start..start + width
}

/// The boundary nearest `head` where the mid signal changes sign, from the
/// one-sample leaf bins of `data`; None when its bins are coarser.
fn zero_crossing(data: &EditWaveform, head: i64) -> Option<i64> {
    let bins = data.level(0)?;
    let mut values = Vec::with_capacity(bins.len());
    for (index, extrema) in bins.iter().enumerate() {
        let samples = data.bin_samples(0, index)?;
        if samples.end.0 - samples.start.0 != 1 {
            return None;
        }
        let mid = (extrema.maximum()[0] + extrema.maximum()[1]) / 2.0;
        values.push((samples.start.0, mid));
    }
    values
        .windows(2)
        .filter(|pair| pair[1].1 == 0.0 || (pair[0].1 < 0.0) != (pair[1].1 < 0.0))
        .map(|pair| pair[1].0)
        .min_by_key(|boundary| (boundary - head).abs())
}

impl DeadpanApp {
    fn mix_boundary(&self, frame: i64) -> Option<i64> {
        let workspace = self.workspace.as_ref()?;
        workspace
            .document
            .presentation_basis()
            .frame_rate
            .audio_boundary(ProjectFrame(frame))
            .ok()
            .map(|sample| sample.0)
    }

    fn mix_end(&self) -> Option<i64> {
        let workspace = self.workspace.as_ref()?;
        self.mix_boundary(workspace.plan.duration().frames())
    }

    /// `:wave`: one frame selected at the Edit cursor, in Your edit.
    pub(super) fn open_wave_range(&mut self, context: &egui::Context) {
        let result = (|| {
            self.capture_macro_target()?;
            let workspace = self.workspace.as_ref().ok_or("Open a project first.")?;
            let total = self.mix_end().ok_or("The edit has no audio clock.")?;
            if total <= 0 {
                return Err("The edit is empty; there is no waveform to select.".to_string());
            }
            let cursor = i64::try_from(self.sequence_cursor).map_err(|_| "cursor overflow")?;
            let frames = workspace.plan.duration().frames();
            let frame = cursor.min(frames - 1).max(0);
            let anchor = self.mix_boundary(frame).ok_or("cursor overflow")?;
            let head = self.mix_boundary(frame + 1).ok_or("cursor overflow")?;
            Ok::<_, String>((workspace.session, anchor, head, total))
        })();
        let (session, anchor, head, total) = match result {
            Ok(value) => value,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        self.stop_playback();
        self.cancel_camera();
        self.help_open = false;
        self.bindings.clear();
        self.error = None;
        self.cancel_timing_for_wave();
        self.wave = Some(WaveRange {
            session,
            anchor,
            head,
            view: view_for(&(anchor..head), head, total),
            measurement: Measurement::default(),
            snap: None,
            keys: Vec::new(),
            focus_pending: true,
            dragging: false,
            note: None,
        });
        context.request_repaint();
    }

    fn cancel_timing_for_wave(&mut self) {
        if let Some(ticket) = self.timing_ticket_take() {
            self.playback.cancel_waveform(ticket);
        }
    }

    pub(super) fn close_wave_range(&mut self, context: &egui::Context) {
        if let Some(wave) = self.wave.take() {
            for ticket in [wave.measurement.ticket, wave.snap.map(|(ticket, _)| ticket)]
                .into_iter()
                .flatten()
            {
                self.playback.cancel_waveform(ticket);
            }
        }
        self.stop_playback();
        self.bindings.clear();
        context.memory_mut(|memory| memory.request_focus(pane_id(self.pane)));
        context.request_discard("waveform range closed");
        context.request_repaint();
    }

    /// A session change ends the selection; its samples belong to that edit.
    pub(super) fn reconcile_wave_range(&mut self, context: &egui::Context) {
        let current = self.workspace.as_ref().map(|workspace| workspace.session);
        if self
            .wave
            .as_ref()
            .is_some_and(|wave| Some(wave.session) != current)
        {
            self.close_wave_range(context);
        }
    }

    /// Modal routing before the editor's keys. Movement keys may repeat.
    pub(super) fn wave_range_keyboard(&mut self, context: &egui::Context) {
        let events = context.input(|input| input.events.clone());
        help_scroll::observe_composition(&events, &mut self.ime_composing);
        self.bindings.clear();
        if self.ime_composing
            || events
                .iter()
                .any(|event| matches!(event, egui::Event::Ime(_)))
        {
            return;
        }
        let background = context.memory(|memory| {
            memory
                .focused()
                .is_none_or(|id| id == egui::Id::new(FOCUS_ID) || id == pane_id(self.pane))
        });
        for event in events {
            let egui::Event::Key {
                key,
                modifiers,
                pressed: true,
                repeat,
                ..
            } = event
            else {
                continue;
            };
            let Some(action) = crate::navigation::wave_range::route_key(
                key, modifiers, background, false, false, repeat,
            ) else {
                continue;
            };
            if let Some(wave) = &mut self.wave {
                wave.keys.push(action);
            }
            context.input_mut(|input| {
                input.consume_key(modifiers, key);
            });
        }
    }

    fn wave_move(&mut self, step: WaveStep) {
        let Some(total) = self.mix_end() else {
            return;
        };
        let Some(head) = self.wave.as_ref().map(|wave| wave.head) else {
            return;
        };
        let moved = match step {
            WaveStep::Samples(samples) => Some(head + samples),
            WaveStep::Frames(frames) => (|| {
                let workspace = self.workspace.as_ref()?;
                let rate = workspace.document.presentation_basis().frame_rate;
                let position = deadpan_core::ExactRatio::new(
                    i128::from(head) * i128::from(rate.numerator()),
                    i128::from(rate.denominator()) * 48_000,
                )
                .ok()?;
                let frame = i64::try_from(position.floor()).ok()?;
                // The nearest rounded frame boundary strictly beyond the head
                // in the moving direction, then further whole frames.
                let target = if frames > 0 {
                    let next = (frame - 1..=frame + 2)
                        .find(|k| self.mix_boundary(*k).is_some_and(|b| b > head))?;
                    next + frames - 1
                } else {
                    let previous = (frame - 1..=frame + 1)
                        .rev()
                        .find(|k| self.mix_boundary(*k).is_some_and(|b| b < head))?;
                    previous + frames + 1
                };
                self.mix_boundary(target)
            })(),
        };
        if let (Some(wave), Some(moved)) = (&mut self.wave, moved) {
            wave.head = moved.clamp(0, total);
            wave.note = None;
        }
    }

    /// Which direct child of the shown group holds every selected sample, and
    /// its exact start frame.
    fn wave_beat(&self, samples: &Range<i64>) -> Result<(NodeId, i64, String), String> {
        if samples.start >= samples.end {
            return Err("Select at least one sample: h/l, Shift+h/l or [ / ].".into());
        }
        for row in self.beat_rows.iter() {
            let start = i64::try_from(row.start).map_err(|_| "beat overflow")?;
            let end = start + i64::try_from(row.frames).map_err(|_| "beat overflow")?;
            let (Some(first), Some(last)) = (self.mix_boundary(start), self.mix_boundary(end))
            else {
                continue;
            };
            if first <= samples.start && samples.end <= last {
                return Ok((row.id.clone(), start, row.label.clone()));
            }
        }
        Err("The selection crosses a beat boundary; select samples inside one beat.".into())
    }

    fn wave_apply(&mut self, millidecibels: Option<i32>) {
        let Some(samples) = self.wave.as_ref().map(WaveRange::samples) else {
            return;
        };
        let result = (|| {
            let (beat, start, _) = self.wave_beat(&samples)?;
            let rate = self
                .workspace
                .as_ref()
                .ok_or("Open a project first.")?
                .document
                .presentation_basis()
                .frame_rate;
            let range = deadpan_core::GainRange::from_mix_samples(
                rate,
                ProjectFrame(start),
                samples.start,
                samples.end,
            )
            .map_err(|error| error.to_string())?;
            let change = match millidecibels {
                None => deadpan_core::AudioChange::SampleRangeMute {
                    start: range.start(),
                    end: range.end(),
                },
                Some(millidecibels) => deadpan_core::AudioChange::SampleRangeStep {
                    start: range.start(),
                    end: range.end(),
                    millidecibels,
                },
            };
            let capture = self.capture_macro_target()?.for_child(beat);
            Ok::<_, String>((capture, change))
        })();
        match result {
            Ok((capture, change)) => {
                self.stop_playback();
                self.apply_recorded_instruction(
                    Ok(capture),
                    Ok(deadpan_core::SemanticInstruction::SetAudio { change }),
                );
            }
            Err(error) => {
                if let Some(wave) = &mut self.wave {
                    wave.note = Some(error);
                }
            }
        }
    }

    fn wave_audition(&mut self) {
        if self.transport.is_some() {
            self.stop_playback();
            return;
        }
        let Some(samples) = self.wave.as_ref().map(WaveRange::samples) else {
            return;
        };
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let domain = Domain::Sequence {
            rate: workspace.document.presentation_basis().frame_rate,
            frames: workspace.plan.duration().frames(),
        };
        match deadpan_playback::Window::new(
            AudioSample(samples.start),
            AudioSample(samples.end),
            false,
        ) {
            Ok(window) if samples.start < samples.end => {
                self.start_domain_playback(domain, window, AudioSample(samples.start));
            }
            _ => {
                if let Some(wave) = &mut self.wave {
                    wave.note = Some("Select at least one sample to audition.".into());
                }
            }
        }
    }

    fn wave_snap(&mut self) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let Some(total) = self.mix_end() else {
            return;
        };
        let Some(head) = self.wave.as_ref().map(|wave| wave.head) else {
            return;
        };
        let samples = (head - SNAP_REACH).max(0)..(head + SNAP_REACH).min(total);
        let snapshot = Arc::new(workspace.playback_snapshot());
        if let Some(ticket) = self
            .wave
            .as_mut()
            .and_then(|wave| wave.measurement.ticket.take())
        {
            self.playback.cancel_waveform(ticket);
        }
        let request = self.playback.request_edit_waveform(EditWaveformRequest {
            base: snapshot.clone(),
            snapshot,
            samples: AudioSample(samples.start)..AudioSample(samples.end),
            limits: deadpan_audio::WaveformLimits::default(),
        });
        if let Some(wave) = &mut self.wave {
            // The view measures again after the snap.
            wave.measurement.key = None;
            match request {
                Ok(ticket) => {
                    wave.snap = Some((ticket, head));
                    wave.note = Some("Finding the nearest zero crossing…".into());
                }
                Err(error) => wave.note = Some(error.to_string()),
            }
        }
    }

    fn wave_measure(&mut self) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let Some(wave) = self.wave.as_ref() else {
            return;
        };
        if wave.snap.is_some() {
            return;
        }
        let key = (workspace.document.revision_id().clone(), wave.view.clone());
        if wave.measurement.key.as_ref() == Some(&key) {
            return;
        }
        let snapshot = Arc::new(workspace.playback_snapshot());
        if let Some(ticket) = self
            .wave
            .as_mut()
            .and_then(|wave| wave.measurement.ticket.take())
        {
            self.playback.cancel_waveform(ticket);
        }
        let request = self.playback.request_edit_waveform(EditWaveformRequest {
            base: snapshot.clone(),
            snapshot,
            samples: AudioSample(key.1.start)..AudioSample(key.1.end),
            limits: deadpan_audio::WaveformLimits::default(),
        });
        let Some(wave) = self.wave.as_mut() else {
            return;
        };
        match request {
            Ok(ticket) => {
                wave.measurement.ticket = Some(ticket);
                wave.measurement.status = Some(WaveformStatus::Queued);
                wave.measurement.error = None;
            }
            Err(error) => {
                wave.measurement.status = Some(WaveformStatus::Unavailable);
                wave.measurement.error = Some(error.to_string());
            }
        }
        wave.measurement.key = Some(key);
    }

    fn wave_receive(&mut self) {
        let Some(update) = self.playback.poll_edit_waveform() else {
            return;
        };
        let Some(wave) = self.wave.as_mut() else {
            return;
        };
        if let Some((ticket, head)) = wave.snap
            && update.ticket == ticket
        {
            if update.status == WaveformStatus::Complete
                || update.status == WaveformStatus::Unavailable
            {
                wave.snap = None;
                match update
                    .waveform
                    .as_deref()
                    .and_then(|data| zero_crossing(data, head))
                {
                    Some(boundary) => {
                        wave.head = boundary;
                        wave.note = Some(format!(
                            "Moved {} samples to a zero crossing.",
                            boundary - head
                        ));
                    }
                    None => wave.note = Some("No zero crossing within 10 ms.".into()),
                }
            }
            return;
        }
        if wave.measurement.ticket != Some(update.ticket) {
            return;
        }
        if let (Some(waveform), Some((_, samples))) = (update.waveform, &wave.measurement.key) {
            wave.measurement.data = Some((samples.clone(), waveform));
        }
        wave.measurement.status = Some(update.status);
        wave.measurement.error = update.error;
    }

    pub(super) fn wave_range_sheet(&mut self, context: &egui::Context) {
        if self.wave.is_none() {
            return;
        }
        self.wave_receive();
        let keys = self
            .wave
            .as_mut()
            .map(|wave| std::mem::take(&mut wave.keys))
            .unwrap_or_default();
        for key in keys {
            match key {
                WaveKey::Close => {
                    self.close_wave_range(context);
                    return;
                }
                other => self.wave_action(other),
            }
        }
        let Some(total) = self.mix_end() else {
            return;
        };
        // Follow the head and the selection's size.
        if let Some(wave) = &mut self.wave
            && !wave.dragging
        {
            let samples = wave.samples();
            let inside = |sample: i64| {
                let margin = (wave.view.end - wave.view.start) / 10;
                sample >= wave.view.start + margin && sample <= wave.view.end - margin
            };
            let width = wave.view.end - wave.view.start;
            let span = samples.end - samples.start;
            if !inside(wave.head) || span * 2 > width || (span * 8 < width && width > MIN_VIEW) {
                let view = view_for(&samples, wave.head, total);
                if view != wave.view {
                    wave.view = view;
                }
            }
        }
        self.wave_measure();
        let Some(mut wave) = self.wave.take() else {
            return;
        };
        let samples = wave.samples();
        let beat = self.wave_beat(&samples);
        let rate = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.document.presentation_basis().frame_rate);
        let frame_of = |sample: i64| {
            rate.map_or(0.0, |rate| {
                sample as f64 * f64::from(rate.numerator())
                    / (f64::from(rate.denominator()) * 48_000.0)
            })
        };
        let boundaries: Vec<(i64, i64)> = {
            let first = frame_of(wave.view.start).floor() as i64;
            let last = frame_of(wave.view.end).ceil() as i64;
            if last - first <= 600 {
                (first..=last)
                    .filter_map(|frame| Some((frame, self.mix_boundary(frame)?)))
                    .collect()
            } else {
                Vec::new()
            }
        };
        let beat_starts: Vec<i64> = self
            .beat_rows
            .iter()
            .filter_map(|row| self.mix_boundary(i64::try_from(row.start).ok()?))
            .collect();
        let playing = self.transport.as_ref().map(|run| run.sample.0);
        let busy = self.service.is_busy();
        let mut action = None;
        let width = (context.content_rect().width() - 64.0).clamp(360.0, 960.0);
        let description = format!(
            "Samples {}–{} · {} samples · {:.3} ms · Edit frames {:.2}–{:.2}{}",
            samples.start,
            samples.end,
            samples.end - samples.start,
            (samples.end - samples.start) as f64 / 48.0,
            frame_of(samples.start),
            frame_of(samples.end),
            match &beat {
                Ok((_, start, label)) => {
                    let origin = self.mix_boundary(*start).unwrap_or(0);
                    format!(
                        " · {label}: samples {}–{} of the beat",
                        samples.start - origin,
                        samples.end - origin
                    )
                }
                Err(_) => String::new(),
            }
        );
        egui::Modal::new(egui::Id::new("wave-range-sheet"))
            .frame(
                egui::Frame::popup(&context.style_of(egui::Theme::Dark))
                    .inner_margin(20)
                    .corner_radius(10),
            )
            .show(context, |ui| {
                super::accessibility::dialog(ui, "Waveform range");
                ui.set_width(width);
                let heading = ui
                    .horizontal(|ui| {
                        ui.heading("Waveform range");
                        ui.colored_label(style::LAVENDER, "EXACT SAMPLES");
                    })
                    .response;
                let focus = ui.interact(
                    heading.rect,
                    egui::Id::new(FOCUS_ID),
                    egui::Sense::focusable_noninteractive(),
                );
                if std::mem::take(&mut wave.focus_pending) {
                    focus.request_focus();
                }
                ui.weak("The measured Edit mix before the limiter. Mute or change the gain of exactly the selected samples of one beat.");
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 180.0),
                    egui::Sense::click_and_drag(),
                );
                let view = wave.view.clone();
                let span = (view.end - view.start).max(1) as f32;
                let x = |sample: i64| rect.left() + rect.width() * (sample - view.start) as f32 / span;
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 4.0, style::PANEL);
                let pixels_per_frame = boundaries
                    .windows(2)
                    .next()
                    .map_or(0.0, |pair| x(pair[1].1) - x(pair[0].1));
                for (frame, sample) in &boundaries {
                    painter.line_segment(
                        [egui::pos2(x(*sample), rect.top()), egui::pos2(x(*sample), rect.bottom())],
                        egui::Stroke::new(1.0, egui::Color32::from_gray(48)),
                    );
                    if pixels_per_frame > 36.0 {
                        painter.text(
                            egui::pos2(x(*sample) + 2.0, rect.top() + 2.0),
                            egui::Align2::LEFT_TOP,
                            format!("{frame}"),
                            egui::FontId::proportional(9.5),
                            egui::Color32::from_gray(120),
                        );
                    }
                }
                for sample in &beat_starts {
                    if view.contains(sample) {
                        painter.line_segment(
                            [egui::pos2(x(*sample), rect.top()), egui::pos2(x(*sample), rect.bottom())],
                            egui::Stroke::new(1.5, style::LAVENDER),
                        );
                    }
                }
                let selected = egui::Rect::from_min_max(
                    egui::pos2(x(samples.start.max(view.start)), rect.top()),
                    egui::pos2(
                        x(samples.end.min(view.end)).max(x(samples.start.max(view.start)) + 1.0),
                        rect.bottom(),
                    ),
                );
                painter.rect_filled(selected, 0.0, style::LAVENDER.gamma_multiply(0.22));
                let wave_rect = rect.shrink2(egui::vec2(0.0, 14.0));
                if let Some((_, data)) = &wave.measurement.data {
                    super::timing::paint_peaks(&painter, wave_rect, data, x);
                }
                for (sample, color) in [(wave.anchor, style::LAVENDER), (wave.head, style::CURSOR)] {
                    painter.line_segment(
                        [egui::pos2(x(sample), rect.top()), egui::pos2(x(sample), rect.bottom())],
                        egui::Stroke::new(1.5, color),
                    );
                }
                if let Some(sample) = playing
                    && view.contains(&sample)
                {
                    painter.line_segment(
                        [egui::pos2(x(sample), rect.top()), egui::pos2(x(sample), rect.bottom())],
                        egui::Stroke::new(1.0, egui::Color32::WHITE),
                    );
                }
                let state = match (wave.measurement.status, &wave.measurement.error) {
                    (_, Some(error)) => Some(error.clone()),
                    (Some(WaveformStatus::Complete), None) => None,
                    (Some(WaveformStatus::Measuring), None) if wave.measurement.data.is_none() => {
                        Some("Measuring the Edit mix…".into())
                    }
                    (_, None) if wave.measurement.data.is_none() => {
                        Some("Waiting for idle audio".into())
                    }
                    _ => None,
                };
                if let Some(state) = state {
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        state,
                        egui::FontId::proportional(11.0),
                        egui::Color32::from_gray(177),
                    );
                }
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Image, true, &description)
                });
                let at = |pointer: egui::Pos2| {
                    view.start
                        + (((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0) * span).round()
                            as i64
                };
                wave.dragging = response.drag_started() || response.dragged();
                // A drag starts once the pointer has moved past egui's
                // threshold; the selection starts where it was pressed.
                if response.drag_started()
                    && let Some(pointer) = ui
                        .input(|input| input.pointer.press_origin())
                        .or(response.interact_pointer_pos())
                {
                    wave.anchor = at(pointer);
                    wave.head = wave.anchor;
                    wave.note = None;
                } else if response.dragged()
                    && let Some(pointer) = response.interact_pointer_pos()
                {
                    wave.head = at(pointer);
                } else if response.clicked()
                    && let Some(pointer) = response.interact_pointer_pos()
                {
                    wave.head = at(pointer);
                    wave.note = None;
                }
                ui.label(egui::RichText::new(&description).monospace());
                match (&beat, &wave.note) {
                    (_, Some(note)) => {
                        ui.colored_label(style::LAVENDER, note);
                    }
                    (Err(error), None) => {
                        ui.colored_label(ui.visuals().warn_fg_color, error);
                    }
                    _ => {
                        ui.weak("Lavender line: fixed end. Cursor line: moving end. Thin lines: frame boundaries.");
                    }
                }
                ui.add_space(6.0);
                let ready = beat.is_ok() && !busy;
                ui.horizontal_wrapped(|ui| {
                    let play = if playing.is_some() { "Stop" } else { "Play selection" };
                    if ui.add_enabled(beat.is_ok(), style::action(play, "Space")).clicked() {
                        action = Some(WaveKey::Play);
                    }
                    if ui
                        .add_enabled(ready, style::action("Mute selection", "m").fill(style::SELECTED))
                        .clicked()
                    {
                        action = Some(WaveKey::Mute);
                    }
                    if ui.add_enabled(ready, style::action("−3 dB", "-")).clicked() {
                        action = Some(WaveKey::Gain { louder: false });
                    }
                    if ui.add_enabled(ready, style::action("+3 dB", "+")).clicked() {
                        action = Some(WaveKey::Gain { louder: true });
                    }
                    if ui.add(style::action("Snap to zero crossing", "z")).clicked() {
                        action = Some(WaveKey::ZeroCrossing);
                    }
                    if ui.add(style::action("Close", "Esc")).clicked() {
                        action = Some(WaveKey::Close);
                    }
                });
                ui.weak("h/l one frame · Shift+h/l 1 ms (48 samples) · [ ] one sample · o swap ends · v new selection · drag to select · . repeats the last change in another beat");
            });
        self.wave = Some(wave);
        match action {
            Some(WaveKey::Close) => self.close_wave_range(context),
            Some(other) => self.wave_action(other),
            None => {}
        }
    }

    fn wave_action(&mut self, action: WaveKey) {
        match action {
            WaveKey::Move(step) => self.wave_move(step),
            WaveKey::ZeroCrossing => self.wave_snap(),
            WaveKey::Swap => {
                if let Some(wave) = &mut self.wave {
                    std::mem::swap(&mut wave.anchor, &mut wave.head);
                }
            }
            WaveKey::Restart => {
                if let Some(wave) = &mut self.wave {
                    wave.anchor = wave.head;
                    wave.note = None;
                }
            }
            WaveKey::Play => self.wave_audition(),
            WaveKey::Mute => self.wave_apply(None),
            WaveKey::Gain { louder } => self.wave_apply(Some(if louder {
                STEP_MILLIDECIBELS
            } else {
                -STEP_MILLIDECIBELS
            })),
            WaveKey::Close => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn views_fit_the_selection_inside_the_project() {
        assert_eq!(view_for(&(100..200), 200, 1_000_000), 0..2_400);
        assert_eq!(
            view_for(&(10_000..10_100), 10_100, 1_000_000),
            8_900..11_300
        );
        let wide = view_for(&(0..100_000), 100_000, 1_000_000);
        assert_eq!(wide.end - wide.start, MAX_VIEW);
        assert!(wide.start <= 0 && wide.end >= 100_000);
        assert_eq!(view_for(&(0..10), 10, 1_000), 0..1_000, "a short edit");
    }
}
