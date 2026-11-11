//! The whole edit as one strip above the beat cards: every beat to scale on
//! the absolute Edit clock, with the cursor, the selection, the viewed group,
//! pauses, repeats, speed changes and placed sounds. Beat cards stay the
//! detail view. Beats narrower than a pixel share their column, which shows
//! the most specific kind it covers, so 10 and 10,000 beats both read.

use std::sync::Arc;

use deadpan_core::{NodeId, NodeKind, ProjectDocument, RevisionId};
use eframe::egui;

use super::{DeadpanApp, Pane, View, pane_id, style, timing};

/// Bounds the walk through nested groups.
const MAX_ROWS: usize = 50_000;
const MAX_PLAY_TICKS: usize = 64;
/// From top: a time ruler, the beat lane, a speech lane and a placed-sound
/// lane, with one-point gaps.
const RULER_HEIGHT: f32 = 11.0;
const BEAT_HEIGHT: f32 = 14.0;
const SPEECH_HEIGHT: f32 = 3.0;
const SOUND_HEIGHT: f32 = 3.0;
pub(super) const HEIGHT: f32 = RULER_HEIGHT + BEAT_HEIGHT + SPEECH_HEIGHT + SOUND_HEIGHT + 3.0;
/// Words closer than this many seconds join one speech span.
const SPEECH_JOIN_SECONDS: f64 = 0.4;
/// The context id where the beat strip records its visible cards.
pub(super) const VISIBLE_CARDS: &str = "beat-cards-visible";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Original,
    Speed,
    Repeat,
    Pause,
}

impl Kind {
    fn color(self, alternate: bool) -> egui::Color32 {
        match (self, alternate) {
            (Kind::Original, false) => egui::Color32::from_rgb(0x3f, 0x47, 0x57),
            (Kind::Original, true) => egui::Color32::from_rgb(0x4b, 0x54, 0x66),
            (Kind::Pause, false) => egui::Color32::from_rgb(0xb0, 0x7d, 0x3a),
            (Kind::Pause, true) => egui::Color32::from_rgb(0xc4, 0x91, 0x4c),
            (Kind::Repeat, false) => egui::Color32::from_rgb(0x6d, 0x5f, 0xb0),
            (Kind::Repeat, true) => egui::Color32::from_rgb(0x80, 0x72, 0xc2),
            (Kind::Speed, false) => egui::Color32::from_rgb(0x2f, 0x7f, 0x86),
            (Kind::Speed, true) => egui::Color32::from_rgb(0x3a, 0x92, 0x99),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Kind::Original => "Original",
            Kind::Pause => "pause",
            Kind::Repeat => "repeat",
            Kind::Speed => "speed change",
        }
    }
}

struct Row {
    start: u64,
    frames: u64,
    kind: Kind,
    /// Absolute starts of a Repeat's later plays.
    plays: Vec<u64>,
}

/// Spoken spans and the speech timeline they were derived from.
type SpeechSpans = (Arc<deadpan_core::SpeechTimeline>, Arc<Vec<(u64, u64)>>);

#[derive(Default)]
pub(super) struct Overview {
    key: Option<(u64, RevisionId)>,
    rows: Arc<Vec<Row>>,
    sounds: Arc<Vec<(f64, f64)>>,
    total: u64,
    /// Spoken spans on the Edit clock, from the speech timeline they came
    /// from; annotations such as "[music playing]" are not speech.
    speech: Option<SpeechSpans>,
    /// Painting geometry for the current revision and width.
    layout: Option<Layout>,
}

impl Overview {
    fn refresh(&mut self, session: u64, document: &ProjectDocument) {
        let key = (session, document.revision_id().clone());
        if self.key.as_ref() == Some(&key) {
            return;
        }
        let (rows, total) = rows(document);
        let beats: Vec<super::BeatRow> = rows
            .iter()
            .map(|row| super::BeatRow {
                id: row.0.clone(),
                label: String::new(),
                kind: String::new(),
                start: row.1.start,
                frames: row.1.frames,
            })
            .collect();
        let mut sounds: Vec<(f64, f64)> = timing::placed_sounds(document, &beats);
        sounds.sort_by(|a, b| a.0.total_cmp(&b.0));
        self.rows = Arc::new(rows.into_iter().map(|(_, row)| row).collect());
        self.sounds = Arc::new(sounds);
        self.total = total;
        self.key = Some(key);
    }
}

/// The strip at one width: beats wide enough for their own block, and
/// what each shared one-point column shows.
struct Layout {
    key: Option<(u64, RevisionId)>,
    width: u32,
    wide: Vec<usize>,
    dense: Vec<Option<(Kind, bool)>>,
    counts: [usize; 4],
}

impl Layout {
    fn compute(overview: &Overview, width: f32) -> Self {
        let total = overview.total.max(1) as f64;
        let x = |frame: f64| width * (frame / total).clamp(0.0, 1.0) as f32;
        let columns = width.ceil() as usize + 1;
        let mut dense: Vec<Option<(Kind, bool)>> = vec![None; columns];
        let mut wide = Vec::new();
        let mut counts = [0usize; 4];
        for (index, row) in overview.rows.iter().enumerate() {
            counts[row.kind as usize] += 1;
            let left = x(row.start as f64);
            let right = x((row.start + row.frames) as f64);
            if right - left >= 3.0 {
                wide.push(index);
            } else {
                let widen = usize::from(row.kind != Kind::Original);
                let first = (left.floor().max(0.0) as usize)
                    .saturating_sub(widen)
                    .min(columns - 1);
                let last = ((right.floor().max(0.0) as usize) + widen).min(columns - 1);
                for column in &mut dense[first..=last] {
                    if column.is_none_or(|(kind, _)| kind <= row.kind) {
                        *column = Some((row.kind, index % 2 == 1));
                    }
                }
            }
        }
        Self {
            key: overview.key.clone(),
            width: width.round() as u32,
            wide,
            dense,
            counts,
        }
    }
}

/// Leaves of the ordinary Sequence tree below the root, in Edit order.
fn rows(document: &ProjectDocument) -> (Vec<(NodeId, Row)>, u64) {
    let Ok(durations) = document.durations_shared() else {
        return (Vec::new(), 0);
    };
    let frames = |id: &NodeId| {
        durations
            .get(id)
            .map_or(0, |duration| u64::try_from(duration.frames()).unwrap_or(0))
    };
    let mut rows = Vec::new();
    // Depth-first with an explicit stack, children pushed in reverse.
    let mut stack = vec![(document.root().clone(), 0u64)];
    while let Some((id, start)) = stack.pop() {
        if rows.len() >= MAX_ROWS {
            break;
        }
        let Some(node) = document.nodes().get(&id) else {
            continue;
        };
        // A Split fragment shows its full beat's kind; its window starts
        // `offset` frames into that beat.
        let mut leaf = id.clone();
        let mut offset = 0i64;
        let mut kind_node = node;
        while let NodeKind::Retime {
            child,
            purpose: deadpan_core::RetimePurpose::Partition,
            mapping,
            ..
        } = &kind_node.kind
        {
            let Some(next) = document.nodes().get(child) else {
                break;
            };
            offset = offset.saturating_add(mapping.start().0);
            leaf = child.clone();
            kind_node = next;
        }
        let kind = match &kind_node.kind {
            NodeKind::Sequence { children } if leaf == id => {
                let mut at = start;
                let mut placed = Vec::with_capacity(children.len());
                for child in children {
                    placed.push((child.clone(), at));
                    at = at.saturating_add(frames(child));
                }
                stack.extend(placed.into_iter().rev());
                continue;
            }
            NodeKind::Sequence { .. } => Kind::Original,
            NodeKind::Hold { .. } => Kind::Pause,
            NodeKind::Repeat { .. } => Kind::Repeat,
            NodeKind::Retime { .. } => Kind::Speed,
            _ => Kind::Original,
        };
        let length = frames(&id);
        let plays = if kind == Kind::Repeat {
            timing::repeat_play_starts(document, &leaf)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|local| u64::try_from(local - offset).ok())
                .filter(|local| *local > 0 && *local < length)
                .take(MAX_PLAY_TICKS)
                .map(|local| start.saturating_add(local))
                .collect()
        } else {
            Vec::new()
        };
        rows.push((
            id.clone(),
            Row {
                start,
                frames: length,
                kind,
                plays,
            },
        ));
    }
    (rows, frames(document.root()))
}

impl DeadpanApp {
    /// Spoken spans of Your edit, when its words are already analysed.
    fn overview_speech(&mut self) -> Option<Arc<Vec<(u64, u64)>>> {
        if !self.transcription_ready() {
            return None;
        }
        let timeline = self.edit_speech().ok()?;
        if let Some((cached, spans)) = &self.overview.speech
            && Arc::ptr_eq(cached, &timeline)
        {
            return Some(Arc::clone(spans));
        }
        let workspace = self.workspace.as_ref()?;
        let transcript = workspace.transcript.as_ref()?;
        let annotation = transcript.transcript.non_speech();
        let rate = workspace.document.presentation_basis().frame_rate;
        let join = (SPEECH_JOIN_SECONDS * f64::from(rate.numerator())
            / f64::from(rate.denominator().max(1))) as u64;
        let mut spans: Vec<(u64, u64)> = Vec::new();
        for run in timeline.runs() {
            if annotation.get(run.word as usize).copied().unwrap_or(false) {
                continue;
            }
            let (start, end) = (
                u64::try_from(run.range.start().0).unwrap_or(0),
                u64::try_from(run.range.end().0).unwrap_or(0),
            );
            match spans.last_mut() {
                Some(last) if start <= last.1 + join => last.1 = last.1.max(end),
                _ => spans.push((start, end)),
            }
        }
        let spans = Arc::new(spans);
        self.overview.speech = Some((timeline, Arc::clone(&spans)));
        Some(spans)
    }

    /// Draw the strip across the beat panel. Pointer press or drag moves the
    /// Edit cursor; the beat at that frame becomes selected.
    pub(super) fn overview(&mut self, ui: &mut egui::Ui) {
        let speech = self.overview_speech();
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let rate = workspace.document.presentation_basis().frame_rate;
        self.overview
            .refresh(workspace.session, &workspace.document);
        let width = ui.available_width().max(1.0);
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, HEIGHT), egui::Sense::click_and_drag());
        let overview = &self.overview;
        let total = overview.total.max(1) as f64;
        let x = |frame: f64| rect.left() + rect.width() * (frame / total).clamp(0.0, 1.0) as f32;
        let ruler = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), RULER_HEIGHT));
        let lane = egui::Rect::from_min_size(
            egui::pos2(rect.left(), ruler.bottom() + 1.0),
            egui::vec2(rect.width(), BEAT_HEIGHT),
        );
        let speech_lane = egui::Rect::from_min_size(
            egui::pos2(rect.left(), lane.bottom() + 1.0),
            egui::vec2(rect.width(), SPEECH_HEIGHT),
        );
        let sound_lane = egui::Rect::from_min_size(
            egui::pos2(rect.left(), speech_lane.bottom() + 1.0),
            egui::vec2(rect.width(), SOUND_HEIGHT),
        );
        let painter = ui.painter_at(rect.expand(1.0));
        let muted = style::muted(ui);
        // Time ruler: the coarsest step that keeps labels about 60 points
        // apart, ticked every step and labelled where they fit.
        let fps = f64::from(rate.numerator()) / f64::from(rate.denominator().max(1));
        let seconds = total / fps;
        let step = [
            1.0, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1800.0, 3600.0,
        ]
        .into_iter()
        .find(|step| rect.width() as f64 * step / seconds.max(1e-9) >= 60.0)
        .unwrap_or(3600.0);
        let mut at = 0.0;
        while at <= seconds {
            let tick = x(at * fps);
            painter.line_segment(
                [
                    egui::pos2(tick, ruler.bottom() - 4.0),
                    egui::pos2(tick, ruler.bottom()),
                ],
                egui::Stroke::new(1.0, style::BORDER),
            );
            let label = if step >= 60.0 || at >= 60.0 {
                format!("{}:{:02}", (at / 60.0) as u64, (at % 60.0) as u64)
            } else {
                format!("{}s", at as u64)
            };
            if tick + 30.0 <= rect.right() {
                painter.text(
                    egui::pos2(tick + 2.0, ruler.top()),
                    egui::Align2::LEFT_TOP,
                    label,
                    egui::FontId::monospace(9.0),
                    muted,
                );
            }
            at += step;
        }
        painter.rect_filled(lane, 3.0, style::PANEL);
        // Columns for beats too narrow to draw on their own. Each keeps its
        // most specific kind and alternates shade with its last beat, so
        // dense runs read as many beats; pauses, repeats and speed changes
        // keep at least three points so each stays visible among thousands.
        // Which beats get their own block and what each shared column shows
        // depend only on the revision and the strip's width: computed once
        // for both, so a frame paints from the cache.
        let width_key = rect.width().round() as u32;
        if self
            .overview
            .layout
            .as_ref()
            .is_none_or(|layout| layout.width != width_key || layout.key != self.overview.key)
        {
            let layout = Layout::compute(&self.overview, rect.width());
            self.overview.layout = Some(layout);
        }
        let overview = &self.overview;
        let layout = overview.layout.as_ref().expect("overview layout");
        let columns = layout.dense.len();
        let dense = &layout.dense;
        let counts = layout.counts;
        for &index in &layout.wide {
            let row = &overview.rows[index];
            let left = x(row.start as f64);
            let right = x((row.start + row.frames) as f64);
            let block = egui::Rect::from_min_max(
                egui::pos2(left + 0.5, lane.top() + 1.0),
                egui::pos2(right - 0.5, lane.bottom() - 1.0),
            );
            painter.rect_filled(block, 2.0, row.kind.color(index % 2 == 1));
            if block.width() >= 44.0 {
                painter.with_clip_rect(block.shrink(1.0)).text(
                    block.left_center() + egui::vec2(4.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    format!("{}", index + 1),
                    egui::FontId::monospace(9.5),
                    style::TEXT,
                );
            }
            for play in &row.plays {
                let at = x(*play as f64);
                painter.line_segment(
                    [
                        egui::pos2(at, block.top() + 1.0),
                        egui::pos2(at, block.bottom() - 1.0),
                    ],
                    egui::Stroke::new(1.0, style::TEXT),
                );
            }
        }
        let mut column = 0;
        while column < columns {
            let Some((kind, alternate)) = dense[column] else {
                column += 1;
                continue;
            };
            let run = dense[column..]
                .iter()
                .take_while(|other| **other == Some((kind, alternate)))
                .count();
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(rect.left() + column as f32, lane.top() + 1.0),
                    egui::pos2(rect.left() + (column + run) as f32, lane.bottom() - 1.0),
                ),
                0.0,
                kind.color(alternate),
            );
            column += run;
        }
        // The cards below show this stretch.
        if self.timing.mode == timing::TimelineMode::Story
            && let Some((first, end)) = ui
                .ctx()
                .data(|data| data.get_temp::<(usize, usize)>(egui::Id::new(VISIBLE_CARDS)))
            && first < end
            && let (Some(first), Some(last)) =
                (self.beat_rows.get(first), self.beat_rows.get(end - 1))
        {
            let left = x(first.start as f64);
            let right = x((last.start + last.frames) as f64).max(left + 2.0);
            let stroke = egui::Stroke::new(1.0, style::TEXT);
            let bottom = lane.bottom() + 1.0;
            painter.line_segment(
                [egui::pos2(left, bottom), egui::pos2(right, bottom)],
                stroke,
            );
            for edge in [left, right] {
                painter.line_segment(
                    [egui::pos2(edge, bottom - 4.0), egui::pos2(edge, bottom)],
                    stroke,
                );
            }
        }
        // The viewed group, the selected beat and any selected range.
        if !self.sequence_scope.groups().is_empty() {
            let group = egui::Rect::from_min_max(
                egui::pos2(x(self.scope_start as f64), lane.top()),
                egui::pos2(
                    x(self.scope_end as f64).max(x(self.scope_start as f64) + 2.0),
                    lane.bottom(),
                ),
            );
            painter.rect_stroke(
                group,
                2.0,
                egui::Stroke::new(1.0, style::MUTED),
                egui::StrokeKind::Outside,
            );
        }
        if self.view == View::Sequence
            && let Some(beat) = self
                .beat_rows
                .iter()
                .find(|beat| self.selected_beat.as_ref() == Some(&beat.id))
        {
            let left = x(beat.start as f64);
            let selected = egui::Rect::from_min_max(
                egui::pos2(left, lane.top()),
                egui::pos2(
                    x((beat.start + beat.frames) as f64).max(left + 3.0),
                    lane.bottom(),
                ),
            );
            painter.rect_stroke(
                selected,
                2.0,
                egui::Stroke::new(1.5, style::LAVENDER),
                egui::StrokeKind::Inside,
            );
        }
        if let Some(range) = self.selected_edit_range() {
            let left = x(range.start().0 as f64);
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(left, lane.bottom() - 4.0),
                    egui::pos2(x(range.end().0 as f64).max(left + 2.0), lane.bottom()),
                ),
                0.0,
                style::LAVENDER,
            );
        }
        if let Some(speech) = &speech {
            painter.rect_filled(speech_lane, 0.0, style::PANEL);
            for (start, end) in speech.iter() {
                let left = x(*start as f64);
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(left, speech_lane.top()),
                        egui::pos2(x(*end as f64).max(left + 1.0), speech_lane.bottom()),
                    ),
                    0.0,
                    egui::Color32::from_rgb(0xa9, 0xb4, 0xc8),
                );
            }
        }
        for (start, end) in overview.sounds.iter() {
            let left = x(*start);
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(left, sound_lane.top()),
                    egui::pos2(x(*end).max(left + 2.0), sound_lane.bottom()),
                ),
                1.0,
                egui::Color32::from_rgb(0x4f, 0xa3, 0x7a),
            );
        }
        let cursor = x(self.sequence_cursor as f64);
        painter.line_segment(
            [
                egui::pos2(cursor, ruler.top()),
                egui::pos2(cursor, rect.bottom()),
            ],
            egui::Stroke::new(2.0, style::CURSOR),
        );
        let label = format!(
            "Whole edit overview: {} beats ({} Original, {} pauses, {} repeats, {} speed changes), {} placed sounds, {}; Edit cursor at {} (frame {})",
            overview.rows.len(),
            counts[Kind::Original as usize],
            counts[Kind::Pause as usize],
            counts[Kind::Repeat as usize],
            counts[Kind::Speed as usize],
            overview.sounds.len(),
            super::clock::time(overview.total, rate),
            super::clock::time(self.sequence_cursor, rate),
            self.sequence_cursor,
        );
        let hover = format!(
            "Every beat to scale: {} grey, {} amber, {} violet, {} teal. Below: speech, then placed sounds. The bracket marks the cards shown below. Click or drag to move the Edit cursor.",
            Kind::Original.name(),
            Kind::Pause.name(),
            Kind::Repeat.name(),
            Kind::Speed.name()
        );
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, &label));
        let pointer = (response.clicked() || response.dragged())
            .then(|| response.interact_pointer_pos())
            .flatten();
        response.on_hover_text(hover);
        let Some(pointer) = pointer else {
            return;
        };
        let end = self.overview.total;
        if end == 0 {
            return;
        }
        let frame = (((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64 * end as f64)
            .round() as u64;
        let frame = frame.min(end);
        if frame == self.sequence_cursor && self.view == View::Sequence {
            return;
        }
        self.stop_playback();
        self.revoke_scoped_continuation();
        self.bindings.clear();
        self.selected_event = None;
        self.sequence_cursor = frame;
        self.view.set(View::Sequence, &mut self.message);
        self.follow_playhead_scope();
        self.edit_range.move_to(self.sequence_cursor);
        if let Some(beat) = self
            .beat_rows
            .iter()
            .find(|beat| beat.start <= frame && frame < beat.start + beat.frames)
            && self.selected_beat.as_ref() != Some(&beat.id)
        {
            self.selected_beat = Some(beat.id.clone());
            self.reveal_beat = true;
        }
        self.focus_scoped_pane(Pane::Sequence);
        self.request_picture(false);
        ui.memory_mut(|memory| memory.request_focus(pane_id(Pane::Sequence)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_order_specific_over_original_in_shared_columns() {
        assert!(Kind::Pause > Kind::Repeat);
        assert!(Kind::Repeat > Kind::Speed);
        assert!(Kind::Speed > Kind::Original);
        assert_eq!(
            Some(Kind::Original).max(Some(Kind::Pause)),
            Some(Kind::Pause)
        );
    }
}
