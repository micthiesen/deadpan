//! Timing view (specification §9.2): the current group's beats drawn at their
//! exact length over the measured Edit mix waveform. It is the same sequence
//! as the story view's cards, at a frame scale; the detail view adds a lane of
//! each beat's attachments (cutaways, captions, gain envelopes, mutes and
//! placed sounds). `:timeline story|timing|detail` switches between them. The waveform comes from the playback engine's idle
//! Edit-window analysis, at most one minute around the cursor at a time.

use std::ops::Range;
use std::sync::Arc;

use deadpan_audio::EditWaveform;
use deadpan_core::{ProjectFrame, RevisionId};
use deadpan_playback::{EditWaveformRequest, WaveformStatus, WaveformTicket};
use eframe::egui;

use super::{BeatRow, DeadpanApp, style};

/// The widest window measured and drawn at once.
const WINDOW_SECONDS: i64 = 60;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum TimelineMode {
    #[default]
    Story,
    Timing,
    Detail,
}

/// One attachment drawn in the detail lane, in absolute Edit frames.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Attachment {
    pub(super) frames: (f64, f64),
    pub(super) kind: &'static str,
    pub(super) label: String,
}

/// The most play boundaries drawn on one Repeat.
const MAX_PLAY_TICKS: u32 = 256;
/// Bounds the attachment walk through nested plays and groups.
const MAX_VISITED: usize = 20_000;

/// Local starts of a Repeat's later plays (`§9.2` iteration ticks), from its
/// exact compiled layout: play durations, overrides and gaps included. None
/// for other beats.
pub(super) fn repeat_play_starts(
    document: &deadpan_core::ProjectDocument,
    id: &deadpan_core::NodeId,
) -> Option<Vec<i64>> {
    let deadpan_core::NodeKind::Repeat {
        child,
        iterations,
        gap,
        ..
    } = &document.nodes().get(id)?.kind
    else {
        return None;
    };
    let durations = document.durations_shared().ok()?;
    let layout = deadpan_core::RepeatLayout::compile_with_gap_overrides(
        iterations,
        child,
        document.overrides().get(id),
        gap.as_ref()
            .map_or(deadpan_core::FrameDuration::ZERO, |recipe| recipe.duration),
        document.gap_overrides().get(id),
        &durations,
    )
    .ok()?;
    Some(
        (1..iterations.len().min(MAX_PLAY_TICKS))
            .filter_map(|index| layout.play(&iterations.at(index)?))
            .map(|play| play.start)
            .collect(),
    )
}

fn ratio(value: deadpan_core::ExactRatio) -> f64 {
    value.numerator() as f64 / value.denominator() as f64
}

/// The attachments of `beats` (direct children of the shown group), of their
/// contents in every Repeat play and speed change, and every placed root
/// sound, in absolute Edit frames.
pub(super) fn attachments(
    document: &deadpan_core::ProjectDocument,
    beats: &[BeatRow],
) -> Vec<Attachment> {
    let mut found = Vec::new();
    // Beats and every nested child at its own start. Each entry carries the
    // Edit frames one local frame occupies: a speed change scales its
    // contents, and each play of a Repeat draws its own definition (the
    // shared child or that play's override) at the play's exact start.
    let durations = document.durations_shared().ok();
    // The fourth value is the zero-based play of the innermost enclosing
    // Repeat, which decides a caption's `reveal`.
    let mut visit: Vec<(deadpan_core::NodeId, f64, f64, Option<u32>)> = beats
        .iter()
        .map(|beat| (beat.id.clone(), beat.start as f64, 1.0, None))
        .collect();
    let frames = |id: &deadpan_core::NodeId| {
        durations
            .as_ref()
            .and_then(|durations| durations.get(id))
            .map_or(0.0, |duration| duration.frames() as f64)
    };
    let mut index = 0;
    while index < visit.len() && visit.len() <= MAX_VISITED {
        let (id, start, scale, within) = visit[index].clone();
        index += 1;
        match document.nodes().get(&id).map(|node| &node.kind) {
            Some(deadpan_core::NodeKind::Sequence { children }) => {
                let mut at = start;
                for child in children {
                    visit.push((child.clone(), at, scale, within));
                    at += frames(child) * scale;
                }
            }
            Some(deadpan_core::NodeKind::Retime { child, .. }) => {
                let input = frames(child);
                if input > 0.0 {
                    visit.push((child.clone(), start, scale * frames(&id) / input, within));
                }
            }
            Some(deadpan_core::NodeKind::Repeat {
                child,
                iterations,
                gap,
                ..
            }) => {
                let Some(durations) = durations.as_ref() else {
                    continue;
                };
                let Ok(layout) = deadpan_core::RepeatLayout::compile_with_gap_overrides(
                    iterations,
                    child,
                    document.overrides().get(&id),
                    gap.as_ref()
                        .map_or(deadpan_core::FrameDuration::ZERO, |recipe| recipe.duration),
                    document.gap_overrides().get(&id),
                    durations,
                ) else {
                    continue;
                };
                for (number, play) in (0..iterations.len().min(MAX_PLAY_TICKS))
                    .filter_map(|n| Some((n, iterations.at(n)?)))
                {
                    let Some(placed) = layout.play(&play) else {
                        continue;
                    };
                    let root = document
                        .overrides()
                        .get(&id)
                        .and_then(|plays| plays.get(&play))
                        .unwrap_or(child)
                        .clone();
                    visit.push((
                        root,
                        start + placed.start as f64 * scale,
                        scale,
                        Some(number),
                    ));
                }
            }
            _ => {}
        }
    }
    for (id, start, scale, within) in &visit {
        let Some(node) = document.nodes().get(id) else {
            continue;
        };
        let (at, scale) = (*start, *scale);
        if let Some(clip) = node.audio_treatments.clip_gain() {
            for mute in clip.mute_ranges() {
                found.push(Attachment {
                    frames: (
                        at + ratio(mute.start()) * scale,
                        at + ratio(mute.end()) * scale,
                    ),
                    kind: "mute",
                    label: "Muted".into(),
                });
            }
            for envelope in clip.envelopes() {
                found.push(Attachment {
                    frames: (
                        at + ratio(envelope.range().start()) * scale,
                        at + ratio(envelope.range().end()) * scale,
                    ),
                    kind: "envelope",
                    label: format!("Gain envelope · {} points", envelope.segments().len() + 1),
                });
            }
        }
        if let Some((host, offset)) = deadpan_core::cutaway_host(document, id)
            && let Some(host) = document.nodes().get(&host)
        {
            let local = |frame: i64| at + (frame - offset) as f64 * scale;
            for cutaway in &host.cutaways {
                found.push(Attachment {
                    frames: (local(cutaway.range.start().0), local(cutaway.range.end().0)),
                    kind: if cutaway.removed {
                        "removed"
                    } else {
                        "cutaway"
                    },
                    label: if cutaway.removed {
                        "Picture removed".into()
                    } else {
                        "Cutaway".into()
                    },
                });
            }
            for caption in host.captions.iter().filter(|caption| {
                caption
                    .reveal
                    .is_none_or(|reveal| within.unwrap_or(0) + 1 >= reveal.get())
            }) {
                found.push(Attachment {
                    frames: (local(caption.range.start().0), local(caption.range.end().0)),
                    kind: "caption",
                    label: format!("“{}”", caption.text),
                });
            }
        }
    }
    let rate = document.presentation_basis().frame_rate;
    for event in document.sounds().values() {
        if let Ok(selected) = event.mapping.selection_frames_with_offset(
            deadpan_core::FrameDuration::ZERO,
            event.offset,
            rate,
        ) {
            found.push(Attachment {
                frames: (ratio(selected.start), ratio(selected.end)),
                kind: "sound",
                label: event.label.clone(),
            });
        }
    }
    found.retain(|attachment| attachment.frames.0 < attachment.frames.1);
    found.sort_by(|a, b| a.frames.0.total_cmp(&b.frames.0));
    found
}

#[derive(Default)]
pub(super) struct Timing {
    pub(super) mode: TimelineMode,
    ticket: Option<WaveformTicket>,
    /// Session, revision and absolute Edit samples of the last request.
    key: Option<(u64, RevisionId, Range<i64>)>,
    data: Option<Arc<EditWaveform>>,
    status: Option<WaveformStatus>,
    error: Option<String>,
}

impl Timing {
    #[cfg(feature = "ui-harness")]
    pub(super) fn state_for_check(&self) -> serde_json::Value {
        serde_json::json!({
            "mode": format!("{:?}", self.mode),
            "status": self.status.map(|status| format!("{status:?}")),
            "samples": self.key.as_ref().map(|(_, _, samples)| [samples.start, samples.end]),
            "measured_end": self.data.as_ref().map(|data| data.measured_end().0),
            "error": self.error,
        })
    }
}

impl DeadpanApp {
    pub(super) fn timeline_command(&mut self, mode: TimelineMode) {
        if mode == TimelineMode::Story {
            self.cancel_timing_waveform();
        }
        self.timing.mode = mode;
        self.message = Some(match mode {
            TimelineMode::Story => "Story view: one card per beat.".into(),
            TimelineMode::Timing => {
                "Timing view: beats at their length over the Edit waveform.".into()
            }
            TimelineMode::Detail => {
                "Detail view: beats, their attachments and the Edit waveform.".into()
            }
        });
    }

    /// `,w`: story → timing → detail → story.
    pub(super) fn cycle_timeline(&mut self) {
        self.timeline_command(match self.timing.mode {
            TimelineMode::Story => TimelineMode::Timing,
            TimelineMode::Timing => TimelineMode::Detail,
            TimelineMode::Detail => TimelineMode::Story,
        });
    }

    fn cancel_timing_waveform(&mut self) {
        if let Some(ticket) = self.timing.ticket.take() {
            self.playback.cancel_waveform(ticket);
        }
        self.timing.key = None;
        self.timing.status = None;
        self.timing.error = None;
    }

    /// Frames shown: the whole group up to a minute, else a minute around the
    /// Edit cursor, inside the group.
    fn timing_window(&self, beats: &[BeatRow]) -> Option<Range<i64>> {
        let workspace = self.workspace.as_ref()?;
        let first = beats.first()?;
        let last = beats.last()?;
        let start = i64::try_from(first.start).ok()?;
        let end = i64::try_from(last.start + last.frames).ok()?;
        let rate = workspace.document.presentation_basis().frame_rate;
        let window = (WINDOW_SECONDS * i64::from(rate.numerator()))
            .div_euclid(i64::from(rate.denominator()).max(1))
            .max(1);
        if end - start <= window {
            return (start < end).then_some(start..end);
        }
        let cursor = i64::try_from(self.sequence_cursor).ok()?;
        let from = (cursor - window / 2).clamp(start, end - window);
        Some(from..from + window)
    }

    /// Ask for the window's waveform when it changed. Trim and the room-tone
    /// sheet share the engine's one Edit waveform slot and take precedence.
    fn request_timing_waveform(&mut self, frames: &Range<i64>) {
        if self.trim.is_some() || self.room_tone.is_some() {
            self.timing.ticket = None;
            self.timing.key = None;
            return;
        }
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let rate = workspace.document.presentation_basis().frame_rate;
        let (Ok(start), Ok(end)) = (
            rate.audio_boundary(ProjectFrame(frames.start)),
            rate.audio_boundary(ProjectFrame(frames.end)),
        ) else {
            return;
        };
        let key = (
            workspace.session,
            workspace.document.revision_id().clone(),
            start.0..end.0,
        );
        if self.timing.key.as_ref() == Some(&key) {
            return;
        }
        if let Some(ticket) = self.timing.ticket.take() {
            self.playback.cancel_waveform(ticket);
        }
        let snapshot = Arc::new(workspace.playback_snapshot());
        self.timing.data = None;
        self.timing.error = None;
        self.timing.status = Some(WaveformStatus::Queued);
        match self.playback.request_edit_waveform(EditWaveformRequest {
            base: snapshot.clone(),
            snapshot,
            samples: start..end,
            limits: deadpan_audio::WaveformLimits::default(),
        }) {
            Ok(ticket) => self.timing.ticket = Some(ticket),
            Err(error) => {
                self.timing.status = Some(WaveformStatus::Unavailable);
                self.timing.error = Some(error.to_string());
            }
        }
        self.timing.key = Some(key);
    }

    pub(super) fn receive_timing_waveform(&mut self) {
        if self.timing.mode == TimelineMode::Story
            || self.trim.is_some()
            || self.room_tone.is_some()
        {
            return;
        }
        let Some(ticket) = self.timing.ticket else {
            return;
        };
        let Some(update) = self.playback.poll_edit_waveform() else {
            return;
        };
        if update.ticket != ticket {
            return;
        }
        if update.waveform.is_some() || update.status == WaveformStatus::Unavailable {
            self.timing.data = update.waveform;
        }
        self.timing.status = Some(update.status);
        self.timing.error = update.error;
    }

    /// Draw the timing strip. Returns the Edit frame a click chose.
    pub(super) fn timing_strip(
        &mut self,
        ui: &mut egui::Ui,
        height: f32,
        beats: &[BeatRow],
        selected: Option<&deadpan_core::NodeId>,
        range: Option<Range<u64>>,
    ) -> Option<u64> {
        let frames = self.timing_window(beats)?;
        self.request_timing_waveform(&frames);
        let width = ui.available_width().max(1.0);
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
        let span = (frames.end - frames.start).max(1) as f32;
        let x = |frame: i64| rect.left() + rect.width() * (frame - frames.start) as f32 / span;
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, style::PANEL);
        let detail = self.timing.mode == TimelineMode::Detail;
        let beats_lane = egui::Rect::from_min_max(
            rect.min,
            egui::pos2(
                rect.right(),
                rect.top() + rect.height() * if detail { 0.32 } else { 0.42 },
            ),
        );
        let attachment_lane = egui::Rect::from_min_max(
            egui::pos2(rect.left(), beats_lane.bottom() + 2.0),
            egui::pos2(
                rect.right(),
                beats_lane.bottom() + if detail { rect.height() * 0.3 } else { 0.0 },
            ),
        );
        let wave_lane = egui::Rect::from_min_max(
            egui::pos2(rect.left(), attachment_lane.bottom() + 4.0),
            rect.max,
        );
        let attached = match (detail, self.workspace.as_ref()) {
            (true, Some(workspace)) => attachments(&workspace.document, beats),
            _ => Vec::new(),
        };
        // Overlapping attachments stack in up to three rows.
        let rows = 3.0;
        let row_height = (attachment_lane.height() / rows).max(1.0);
        let mut row_ends = [f64::MIN; 3];
        for attachment in &attached {
            let (start, end) = attachment.frames;
            if end <= frames.start as f64 || start >= frames.end as f64 {
                continue;
            }
            let row = row_ends
                .iter()
                .position(|until| *until <= start)
                .unwrap_or(2);
            row_ends[row] = end;
            let fx = |frame: f64| {
                rect.left()
                    + rect.width()
                        * ((frame - frames.start as f64) / span as f64).clamp(0.0, 1.0) as f32
            };
            let bar = egui::Rect::from_min_max(
                egui::pos2(
                    fx(start),
                    attachment_lane.top() + row as f32 * row_height + 1.0,
                ),
                egui::pos2(
                    fx(end).max(fx(start) + 2.0),
                    attachment_lane.top() + (row as f32 + 1.0) * row_height - 1.0,
                ),
            );
            let color = match attachment.kind {
                "mute" => egui::Color32::from_rgb(0x8f, 0x3b, 0x45),
                "envelope" => egui::Color32::from_rgb(0x5b, 0x4d, 0x8c),
                "cutaway" => egui::Color32::from_rgb(0x2f, 0x5d, 0x7c),
                "removed" => egui::Color32::from_rgb(0x44, 0x44, 0x44),
                "caption" => egui::Color32::from_rgb(0x6b, 0x5b, 0x2e),
                _ => egui::Color32::from_rgb(0x2f, 0x6b, 0x4f),
            };
            painter.rect_filled(bar, 2.0, color);
            painter.with_clip_rect(bar.shrink(1.0)).text(
                bar.left_center() + egui::vec2(3.0, 0.0),
                egui::Align2::LEFT_CENTER,
                &attachment.label,
                egui::FontId::proportional(10.0),
                style::TEXT,
            );
        }
        for beat in beats {
            let (Ok(start), Ok(end)) = (
                i64::try_from(beat.start),
                i64::try_from(beat.start + beat.frames),
            ) else {
                continue;
            };
            if end <= frames.start || start >= frames.end {
                continue;
            }
            let block = egui::Rect::from_min_max(
                egui::pos2(x(start.max(frames.start)) + 1.0, beats_lane.top() + 4.0),
                egui::pos2(
                    (x(end.min(frames.end)) - 1.0).max(x(start.max(frames.start)) + 2.0),
                    beats_lane.bottom() - 2.0,
                ),
            );
            let chosen = selected == Some(&beat.id);
            painter.rect_filled(
                block,
                3.0,
                if chosen {
                    style::SELECTED
                } else {
                    style::PANEL_RAISED
                },
            );
            if chosen {
                painter.rect_stroke(
                    block,
                    3.0,
                    egui::Stroke::new(1.5, style::LAVENDER),
                    egui::StrokeKind::Inside,
                );
            }
            if let Some(workspace) = self.workspace.as_ref()
                && let Some(starts) = repeat_play_starts(&workspace.document, &beat.id)
            {
                for local in starts {
                    let frame = start + local;
                    if frame > frames.start && frame < frames.end {
                        painter.line_segment(
                            [
                                egui::pos2(x(frame), block.top() + 2.0),
                                egui::pos2(x(frame), block.bottom() - 2.0),
                            ],
                            egui::Stroke::new(1.0, style::LAVENDER),
                        );
                    }
                }
            }
            painter.with_clip_rect(block.shrink(3.0)).text(
                block.left_center() + egui::vec2(5.0, 0.0),
                egui::Align2::LEFT_CENTER,
                format!("{} · {} f", beat.label, beat.frames),
                egui::FontId::proportional(11.5),
                style::TEXT,
            );
        }
        if let Some(range) = range {
            let (start, end) = (
                i64::try_from(range.start).unwrap_or(i64::MAX),
                i64::try_from(range.end).unwrap_or(i64::MAX),
            );
            if start < frames.end && end > frames.start {
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(x(start.max(frames.start)), beats_lane.bottom() - 4.0),
                        egui::pos2(x(end.min(frames.end)), beats_lane.bottom()),
                    ),
                    1.0,
                    style::LAVENDER,
                );
            }
        }
        if let (Some(data), Some((_, _, samples))) = (&self.timing.data, &self.timing.key) {
            let samples = samples.clone();
            paint_peaks(&painter, wave_lane, data, |sample| {
                wave_lane.left()
                    + wave_lane.width() * (sample - samples.start) as f32
                        / (samples.end - samples.start).max(1) as f32
            });
        }
        let state = match (self.timing.status, &self.timing.error) {
            (_, Some(error)) => Some(error.as_str()),
            (Some(WaveformStatus::Complete), None) => None,
            (Some(WaveformStatus::Measuring), None) => Some("Measuring the Edit mix…"),
            _ => Some("Waiting for idle audio"),
        };
        if let Some(state) = state {
            painter.text(
                wave_lane.center(),
                egui::Align2::CENTER_CENTER,
                state,
                egui::FontId::proportional(11.0),
                egui::Color32::from_gray(177),
            );
        }
        let cursor = i64::try_from(self.sequence_cursor).unwrap_or(i64::MAX);
        if (frames.start..=frames.end).contains(&cursor) {
            painter.line_segment(
                [
                    egui::pos2(x(cursor), rect.top()),
                    egui::pos2(x(cursor), rect.bottom()),
                ],
                egui::Stroke::new(1.5, style::CURSOR),
            );
        }
        let label = format!(
            "{} view · Edit frames {}–{} · {} beats{} · Edit mix waveform {}",
            if detail { "Detail" } else { "Timing" },
            frames.start,
            frames.end,
            beats.len(),
            if detail {
                format!(" · {} attachments", attached.len())
            } else {
                String::new()
            },
            match self.timing.status {
                Some(WaveformStatus::Complete) => "measured",
                Some(WaveformStatus::Unavailable) => "unavailable",
                _ => "pending",
            }
        );
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, &label));
        let pointer = response
            .clicked()
            .then(|| response.interact_pointer_pos())
            .flatten()?;
        let frame = frames.start
            + (((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0) * span) as i64;
        u64::try_from(frame.min(frames.end - 1)).ok()
    }
}

/// Stereo min/max peaks over two lanes, choosing the coarsest level that
/// still has a bin per pixel. `x` maps an absolute Edit sample to a position.
pub(super) fn paint_peaks(
    painter: &egui::Painter,
    rect: egui::Rect,
    data: &EditWaveform,
    x: impl Fn(i64) -> f32,
) {
    let lanes = [
        egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), rect.center().y - 1.0)),
        egui::Rect::from_min_max(egui::pos2(rect.left(), rect.center().y + 1.0), rect.max),
    ];
    let y = |lane: egui::Rect, amplitude: f32| {
        lane.center().y - amplitude.clamp(-1.0, 1.0) * (lane.height() / 2.0 - 1.0)
    };
    for lane in lanes {
        painter.line_segment(
            [
                egui::pos2(lane.left(), lane.center().y),
                egui::pos2(lane.right(), lane.center().y),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_gray(55)),
        );
    }
    let mut level = 0;
    while level + 1 < data.level_count()
        && data
            .level(level)
            .is_some_and(|bins| bins.len() > rect.width().max(1.0) as usize)
    {
        level += 1;
    }
    for (index, extrema) in data.level(level).unwrap_or_default().iter().enumerate() {
        let Some(samples) = data.bin_samples(level, index) else {
            continue;
        };
        let (x0, x1) = (x(samples.start.0), x(samples.end.0));
        let x1 = x1.max(x0 + 0.7);
        for (channel, lane) in lanes.iter().enumerate() {
            let top = y(*lane, extrema.maximum()[channel]);
            let bottom = y(*lane, extrema.minimum()[channel]).max(top + 0.7);
            painter.rect_filled(
                egui::Rect::from_min_max(egui::pos2(x0, top), egui::pos2(x1, bottom)),
                0.0,
                egui::Color32::from_rgb(151, 146, 184),
            );
        }
    }
}
