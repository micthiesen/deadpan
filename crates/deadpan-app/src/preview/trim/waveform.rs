//! Measured Before/Proposed edit audio on its retained absolute sample clock.

use std::{ops::Range, sync::Arc};

use deadpan_audio::{EditWaveform, EditWaveformStage, WaveformLimits};
use deadpan_core::{AudioSample, FrameDuration, FrameRate, NodeId, ProjectFrame, RevisionId};
use deadpan_playback::{
    ContentIdentity, EditWaveformRequest, EditWaveformUpdate, Engine, Snapshot, WaveformStatus,
    WaveformTicket,
};
use eframe::egui::{self, Color32, Rect, Stroke};

use crate::worker::{EditJunctionIdentity, EditJunctionInput, JunctionSide};

pub(super) const GRAPH: &str =
    "Trim stereo waveform · absolute Edit samples · authored mix before limiter";

struct Expected {
    junction: EditJunctionIdentity,
    samples: Range<AudioSample>,
    revision: RevisionId,
    content: ContentIdentity,
    root: NodeId,
    rate: FrameRate,
    duration: FrameDuration,
}

/// Authenticate the whole comparison before selecting its measured side. This
/// checks retained value/capability identity only; preparation stays off the UI.
fn admit_request(
    junction: EditJunctionIdentity,
    base: Arc<Snapshot>,
    candidate: Option<Arc<Snapshot>>,
    samples: Range<AudioSample>,
) -> Result<(Expected, EditWaveformRequest), String> {
    if junction.session == 0
        || junction.draft == 0
        || junction.change == 0
        || junction.inspection == 0
    {
        return Err("Edit junctions require an active captured inspection.".into());
    }
    if base.content != ContentIdentity::Committed
        || junction.session != base.session
        || junction.project != *base.document.project_id()
        || junction.base_revision != *base.document.revision_id()
    {
        return Err("Edit junction belongs to another committed base.".into());
    }
    let proposed = match (&junction.content, candidate) {
        (ContentIdentity::Committed, None) if junction.proposal_revision.is_none() => base.clone(),
        (
            ContentIdentity::Proposed {
                base_revision,
                draft,
                change,
            },
            Some(snapshot),
        ) => {
            if base_revision != &junction.base_revision
                || *draft != junction.draft
                || *change != junction.change
                || snapshot.content != junction.content
                || snapshot.session != junction.session
                || junction.proposal_revision.as_ref() != Some(snapshot.document.revision_id())
                || !snapshot.originals.same_session(&base.originals)
            {
                return Err(
                    "Edit junction proposal identity differs from its admitted Snapshot.".into(),
                );
            }
            snapshot
                .validate_original_proposal()
                .map_err(|error| error.to_string())?;
            snapshot
                .validate_proposed_base(base.session, &base.document)
                .map_err(|error| error.to_string())?;
            snapshot
        }
        _ => return Err("Edit junction requires the exact active proposal Snapshot.".into()),
    };
    let snapshot = match junction.side {
        JunctionSide::Before => base.clone(),
        JunctionSide::Proposed => proposed,
    };
    let duration = snapshot
        .document
        .duration()
        .map_err(|error| error.to_string())?;
    if junction.boundary.0 < 0 || junction.boundary.0 > duration.frames() {
        return Err("Edit junction boundary is outside the selected Edit.".into());
    }
    let outgoing = (junction.boundary.0 > 0).then(|| ProjectFrame(junction.boundary.0 - 1));
    let incoming = (junction.boundary.0 < duration.frames()).then_some(junction.boundary);
    if junction.outgoing != outgoing || junction.incoming != incoming {
        return Err("Edit junction frame addresses do not match its boundary.".into());
    }
    let rate = snapshot.document.presentation_basis().frame_rate;
    let end = rate
        .audio_boundary(ProjectFrame(duration.frames()))
        .map_err(|error| error.to_string())?;
    if samples.start.0 < 0 || samples.end < samples.start || samples.end > end {
        return Err("Waveform window is outside the selected Edit.".into());
    }
    let expected = Expected {
        junction,
        samples: samples.clone(),
        revision: snapshot.document.revision_id().clone(),
        content: snapshot.content.clone(),
        root: snapshot.document.root().clone(),
        rate,
        duration,
    };
    Ok((
        expected,
        EditWaveformRequest {
            base,
            snapshot,
            samples,
            limits: WaveformLimits::default(),
        },
    ))
}

pub(super) struct Display {
    expected: Option<Expected>,
    ticket: Option<WaveformTicket>,
    data: Option<Arc<EditWaveform>>,
    status: WaveformStatus,
    error: Option<String>,
}

impl Default for Display {
    fn default() -> Self {
        Self {
            expected: None,
            ticket: None,
            data: None,
            status: WaveformStatus::Queued,
            error: None,
        }
    }
}

impl Display {
    #[cfg(feature = "ui-harness")]
    pub(super) fn state_for_check(&self) -> serde_json::Value {
        use serde_json::json;
        json!({
            "status":format!("{:?}", self.status), "error":self.error,
            "expected":self.expected.as_ref().map(|expected| json!({
                "identity":format!("{:?}", expected.junction),
                "samples":[expected.samples.start.0, expected.samples.end.0],
                "revision":expected.revision,
            })),
            "data":self.data.as_ref().map(|data| json!({
                "document":{"project":data.descriptor().project_id,
                    "revision":data.descriptor().revision_id,"root":data.descriptor().root,
                    "rate":data.descriptor().frame_rate,
                    "duration":data.descriptor().project_duration.frames()},
                "stage":data.descriptor().stage.label(),
                "samples":[data.descriptor().samples.start.0, data.descriptor().samples.end.0],
                "measured_end":data.measured_end().0,
                "leaf_stride":data.descriptor().leaf_stride,
                "level_count":data.level_count(),
                "bins":(0..data.level_count().min(64)).map(|level| data.level(level).map_or(0, |bins| bins.len())).collect::<Vec<_>>(),
            })),
        })
    }

    pub(super) fn request(
        &mut self,
        engine: &Engine,
        junction: EditJunctionIdentity,
        input: &EditJunctionInput,
        samples: Range<AudioSample>,
    ) {
        self.clear(engine);
        let (expected, request) = match admit_request(
            junction,
            Arc::new(input.base.playback_snapshot()),
            input.snapshot.clone(),
            samples,
        ) {
            Ok(admitted) => admitted,
            Err(error) => {
                self.status = WaveformStatus::Unavailable;
                self.error = Some(error);
                return;
            }
        };
        self.expected = Some(expected);
        match engine.request_edit_waveform(request) {
            Ok(ticket) => self.ticket = Some(ticket),
            Err(error) => {
                self.status = WaveformStatus::Unavailable;
                self.error = Some(error.to_string());
            }
        }
    }

    pub(super) fn cancel(&mut self, engine: &Engine) {
        if let Some(ticket) = self.ticket.take() {
            engine.cancel_waveform(ticket);
            if matches!(
                self.status,
                WaveformStatus::Queued | WaveformStatus::Measuring
            ) {
                self.status = WaveformStatus::Interrupted;
            }
        }
    }

    pub(super) fn clear(&mut self, engine: &Engine) {
        self.cancel(engine);
        *self = Self::default();
    }

    pub(super) fn receive(&mut self, update: EditWaveformUpdate) {
        let Some(expected) = &self.expected else {
            return;
        };
        if self.ticket != Some(update.ticket)
            || update.session != expected.junction.session
            || update.project_id != expected.junction.project
            || update.base_revision != expected.junction.base_revision
            || update.revision_id != expected.revision
            || update.content != expected.content
            || update.samples != expected.samples
        {
            return;
        }
        self.receive_data(update.status, update.waveform, update.error);
    }

    fn receive_data(
        &mut self,
        status: WaveformStatus,
        waveform: Option<Arc<EditWaveform>>,
        error: Option<String>,
    ) {
        let Some(expected) = &self.expected else {
            return;
        };
        if let Some(data) = &waveform {
            let descriptor = data.descriptor();
            if descriptor.stage != EditWaveformStage::AuthoredBusBeforeLimiter
                || descriptor.project_id != expected.junction.project
                || descriptor.revision_id != expected.revision
                || descriptor.root != expected.root
                || descriptor.frame_rate != expected.rate
                || descriptor.project_duration != expected.duration
                || descriptor.sample_rate != deadpan_core::MIX_SAMPLE_RATE
                || descriptor.samples != expected.samples
            {
                self.data = None;
                self.status = WaveformStatus::Unavailable;
                self.error = Some("Waveform returned a different Edit window.".into());
                return;
            }
        }
        // A state-only progress/interruption can follow a consumed prefix reply.
        // Unavailable (including revoked media) always removes retained peaks.
        if status == WaveformStatus::Unavailable {
            self.data = None;
        } else if waveform.is_some()
            || !matches!(
                status,
                WaveformStatus::Measuring | WaveformStatus::Interrupted
            )
        {
            self.data = waveform;
        }
        self.status = status;
        self.error = error;
    }

    pub(super) fn show(&self, ui: &mut egui::Ui, heard: Option<AudioSample>, height: f32) -> bool {
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().max(0.0), height.max(0.0)),
            egui::Sense::hover(),
        );
        let layout = waveform_layout(rect, row_heights(ui), row_gap(ui));
        let mut retry = false;
        let mut heading = row_ui(ui, "waveform-heading", layout.rows[0]);
        heading.horizontal(|ui| {
            ui.strong("Edit mix · before limiter").on_hover_text(
                "Measured stereo audio includes timing, pitch, fades, gain, placed sounds and silence permissions. Limiting, mastering and Monitor volume are excluded.",
            );
            if matches!(self.status, WaveformStatus::Partial | WaveformStatus::Interrupted | WaveformStatus::Unavailable) {
                let response = ui.button("Retry waveform");
                if response.has_focus() { response.scroll_to_me(None); }
                retry = response.clicked();
            }
        });
        let state = match self.status {
            WaveformStatus::Queued => "Waiting for idle audio",
            WaveformStatus::Measuring => "Measuring",
            WaveformStatus::Complete => "Measured",
            WaveformStatus::Partial => "Partial measurement",
            WaveformStatus::Interrupted => "Measurement interrupted",
            WaveformStatus::Unavailable => "Measurement unavailable",
        };
        let summary = match (&self.expected, &self.data) {
            (Some(expected), Some(data)) => format!(
                "{state} · Edit samples [{}..{}) · {} / {} samples measured · amplitude ±1",
                expected.samples.start.0,
                expected.samples.end.0,
                data.measured_end().0 - expected.samples.start.0,
                expected.samples.end.0 - expected.samples.start.0,
            ),
            _ => format!("{state} · unmeasured audio is unknown"),
        };
        fixed_label(ui, "waveform-summary", layout.rows[1], &summary, false);
        fixed_label(
            ui,
            "waveform-error",
            layout.rows[2],
            self.error.as_deref().unwrap_or(""),
            true,
        );
        let rect = layout.graph;
        let response = ui.interact(rect, ui.id().with("waveform-graph"), egui::Sense::hover());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, GRAPH));
        let plot = Rect::from_min_max(
            rect.min + egui::vec2(28.0, 13.0),
            rect.max - egui::vec2(4.0, 17.0),
        );
        if plot.width() > 1.0 && plot.height() > 6.0 && ui.is_rect_visible(rect) {
            self.paint(ui, rect, plot, heard);
        }
        retry
    }

    fn paint(&self, ui: &egui::Ui, rect: Rect, plot: Rect, heard: Option<AudioSample>) {
        let painter = ui.painter_at(rect);
        let font = egui::FontId::proportional(10.0);
        let lanes = [
            Rect::from_min_max(plot.min, egui::pos2(plot.right(), plot.center().y - 3.0)),
            Rect::from_min_max(egui::pos2(plot.left(), plot.center().y + 3.0), plot.max),
        ];
        for (channel, lane) in lanes.iter().enumerate() {
            for amplitude in [-1.0, 0.0, 1.0] {
                let y = amplitude_y(*lane, amplitude);
                painter.line_segment(
                    [egui::pos2(lane.left(), y), egui::pos2(lane.right(), y)],
                    Stroke::new(1.0, Color32::from_gray(55)),
                );
            }
            painter.text(
                egui::pos2(rect.left(), lane.center().y),
                egui::Align2::LEFT_CENTER,
                if channel == 0 { "L" } else { "R" },
                font.clone(),
                Color32::from_gray(170),
            );
        }
        let mut measured_x = plot.left();
        if let Some(expected) = &self.expected {
            if expected.samples.is_empty() {
                painter.text(
                    plot.center(),
                    egui::Align2::CENTER_CENTER,
                    "No allocated audio samples",
                    font,
                    Color32::from_gray(170),
                );
                return;
            }
            for (sample, align, x) in [
                (
                    expected.samples.start,
                    egui::Align2::LEFT_BOTTOM,
                    plot.left(),
                ),
                (
                    expected.samples.end,
                    egui::Align2::RIGHT_BOTTOM,
                    plot.right(),
                ),
            ] {
                painter.text(
                    // Font placement snaps fractionally; keep the full glyph
                    // inside the graph clip at either display scale.
                    egui::pos2(x, rect.bottom() - 1.0),
                    align,
                    format!("{} samples", sample.0),
                    font.clone(),
                    Color32::from_gray(170),
                );
            }
            if let Some(data) = &self.data {
                let mut preferred = 0;
                while preferred + 1 < data.level_count()
                    && data
                        .level(preferred)
                        .is_some_and(|bins| bins.len() > plot.width().max(1.0) as usize)
                {
                    preferred += 1;
                }
                let mut covered = expected.samples.start;
                for level in (0..=preferred).rev() {
                    if covered >= data.measured_end() {
                        break;
                    }
                    let Some(stride) = u32::try_from(level)
                        .ok()
                        .and_then(|shift| data.descriptor().leaf_stride.checked_shl(shift))
                        .filter(|stride| *stride != 0)
                    else {
                        break;
                    };
                    let Some(first) = u64::try_from(covered.0 - expected.samples.start.0)
                        .ok()
                        .and_then(|offset| usize::try_from(offset / stride).ok())
                    else {
                        break;
                    };
                    for (index, extrema) in data
                        .level(level)
                        .unwrap_or_default()
                        .iter()
                        .enumerate()
                        .skip(first)
                    {
                        let Some(samples) = data.bin_samples(level, index) else {
                            continue;
                        };
                        if samples.start != covered {
                            continue;
                        }
                        let x0 = sample_x(plot, &expected.samples, samples.start);
                        let x1 = sample_x(plot, &expected.samples, samples.end);
                        for (channel, lane) in lanes.iter().enumerate() {
                            let min = extrema.minimum()[channel];
                            let max = extrema.maximum()[channel];
                            let top = amplitude_y(*lane, max);
                            let bottom = amplitude_y(*lane, min).max(top + 0.7);
                            painter.rect_filled(
                                Rect::from_min_max(egui::pos2(x0, top), egui::pos2(x1, bottom)),
                                0.0,
                                Color32::from_rgb(151, 146, 184),
                            );
                            for (exceeds, y) in
                                [(max > 1.0, lane.top()), (min < -1.0, lane.bottom())]
                            {
                                if exceeds {
                                    painter.line_segment(
                                        [egui::pos2(x0, y), egui::pos2(x1, y)],
                                        Stroke::new(2.0, crate::preview::style::CURSOR),
                                    );
                                }
                            }
                        }
                        covered = samples.end;
                        measured_x = x1;
                    }
                }
            }
        }
        if measured_x < plot.right() {
            let unknown = Rect::from_min_max(egui::pos2(measured_x, plot.top()), plot.max);
            let unknown_painter = painter.with_clip_rect(unknown.intersect(ui.clip_rect()));
            unknown_painter.rect_filled(unknown, 0.0, Color32::from_rgb(35, 39, 47));
            let mut x = unknown.left() - unknown.height();
            while x < unknown.right() {
                unknown_painter.line_segment(
                    [
                        egui::pos2(x, unknown.bottom()),
                        egui::pos2(x + unknown.height(), unknown.top()),
                    ],
                    Stroke::new(1.0, Color32::from_gray(55)),
                );
                x += 9.0;
            }
            if unknown.width() > 95.0 {
                unknown_painter.text(
                    unknown.center(),
                    egui::Align2::CENTER_CENTER,
                    "Not measured",
                    font,
                    Color32::from_gray(177),
                );
            }
        }
        if let Some(expected) = &self.expected {
            if let Ok(boundary) = expected.rate.audio_boundary(expected.junction.boundary) {
                marker(
                    &painter,
                    plot,
                    &expected.samples,
                    boundary,
                    crate::preview::style::CURSOR,
                );
            }
            if let Some(heard) = heard {
                marker(
                    &painter,
                    plot,
                    &expected.samples,
                    heard,
                    crate::preview::style::SAVED,
                );
            }
        }
    }
}

fn marker(
    painter: &egui::Painter,
    plot: Rect,
    samples: &Range<AudioSample>,
    sample: AudioSample,
    color: Color32,
) {
    if sample < samples.start || sample > samples.end {
        return;
    }
    let x = sample_x(plot, samples, sample);
    painter.line_segment(
        [egui::pos2(x, plot.top()), egui::pos2(x, plot.bottom())],
        Stroke::new(1.0, color),
    );
}

fn row_heights(ui: &egui::Ui) -> [f32; 3] {
    let body = ui.text_style_height(&egui::TextStyle::Body);
    [
        ui.spacing().interact_size.y.max(body),
        body,
        ui.text_style_height(&egui::TextStyle::Small),
    ]
}

fn row_gap(ui: &egui::Ui) -> f32 {
    ui.spacing().item_spacing.y.clamp(0.0, 4.0)
}

pub(super) fn preferred_height(ui: &egui::Ui) -> f32 {
    row_heights(ui).iter().sum::<f32>() + 3.0 * row_gap(ui) + 112.0
}

struct WaveformLayout {
    rows: [Rect; 3],
    graph: Rect,
}

fn waveform_layout(rect: Rect, heights: [f32; 3], gap: f32) -> WaveformLayout {
    let reserved = heights.iter().sum::<f32>() + 3.0 * gap;
    let scale = if reserved > 0.0 {
        (rect.height() / reserved).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let mut top = rect.top();
    let rows = heights.map(|height| {
        let row = Rect::from_min_size(
            egui::pos2(rect.left(), top),
            egui::vec2(rect.width(), height * scale),
        );
        top = (row.bottom() + gap * scale).min(rect.bottom());
        row
    });
    WaveformLayout {
        rows,
        graph: Rect::from_min_max(egui::pos2(rect.left(), top), rect.max),
    }
}

fn row_ui(ui: &mut egui::Ui, id: &str, rect: Rect) -> egui::Ui {
    let mut child = ui.new_child(egui::UiBuilder::new().id_salt(id).max_rect(rect));
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    child
}

fn fixed_label(ui: &mut egui::Ui, id: &str, rect: Rect, text: &str, small: bool) {
    let mut label_ui = row_ui(ui, id, rect);
    let text_style = if small {
        egui::RichText::new(text).small()
    } else {
        egui::RichText::new(text).weak()
    };
    let response = label_ui.add_sized(
        rect.size(),
        egui::Label::new(text_style)
            .truncate()
            .halign(egui::Align::LEFT),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, text));
    if !text.is_empty() {
        response.on_hover_text(text);
    }
}

fn amplitude_y(lane: Rect, value: f32) -> f32 {
    lane.center().y - value.clamp(-1.0, 1.0) * lane.height() * 0.5
}

fn sample_x(plot: Rect, samples: &Range<AudioSample>, position: AudioSample) -> f32 {
    let width = i128::from(samples.end.0) - i128::from(samples.start.0);
    if width <= 0 {
        return plot.left();
    }
    let offset = i128::from(position.0) - i128::from(samples.start.0);
    plot.left() + (offset as f64 / width as f64).clamp(0.0, 1.0) as f32 * plot.width()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::JunctionRole;
    use deadpan_audio::{
        AudioSourceProvider, PreparationError, PreparedSource, StageAudio, WaveformControl,
        WaveformMemory,
    };
    use deadpan_core::{
        AssetId, BeatNode, Command, CommandRequest, HoldAudio, HoldRecipe, HoldVideo,
        ProjectDocument, ProjectId, Subtree,
    };
    use deadpan_plan::RenderPlan;
    use deadpan_store::ProjectStore;
    use std::{collections::BTreeMap, sync::atomic::AtomicBool, time::Duration};

    #[test]
    fn waveform_rows_and_graph_stay_inside_their_allocated_region() {
        for height in [0.0, 8.0, 32.0, 64.0, 130.0, 176.0] {
            let rect = Rect::from_min_size(egui::pos2(12.0, 420.0), egui::vec2(936.0, height));
            let layout = waveform_layout(rect, [26.0, 14.0, 12.0], 4.0);
            for part in layout.rows.into_iter().chain([layout.graph]) {
                assert!(rect.contains_rect(part));
                assert!(part.height() >= 0.0);
            }
            assert!(layout.rows[2].bottom() <= layout.graph.top());
            assert_eq!(layout.graph.bottom(), rect.bottom());
            if height >= 130.0 {
                assert!(layout.graph.height() >= 66.0);
            }
        }
    }

    fn insert_hold(base: &ProjectDocument, revision: &str, name: &str) -> Arc<ProjectDocument> {
        let id = NodeId::new(name).unwrap();
        let request = CommandRequest {
            project_id: base.project_id().clone(),
            expected_revision: base.revision_id().clone(),
            new_revision: RevisionId::new(revision).unwrap(),
            command: Command::Insert {
                parent: base.root().clone(),
                index: 0,
                subtree: Subtree {
                    root: id.clone(),
                    nodes: BTreeMap::from([(
                        id,
                        BeatNode::hold(
                            name,
                            HoldRecipe {
                                picture_context: None,
                                duration: FrameDuration::new(3).unwrap(),
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                            },
                        ),
                    )]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        };
        Arc::new(
            deadpan_core::apply(base, &request)
                .unwrap()
                .forward
                .apply(base)
                .unwrap(),
        )
    }

    struct Fixture {
        base: Arc<Snapshot>,
        first: Arc<Snapshot>,
        second: Arc<Snapshot>,
        _store: ProjectStore,
        _directory: tempfile::TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            let empty = ProjectDocument::new_automatic(
                ProjectId::new("trim-waveform").unwrap(),
                RevisionId::new("empty").unwrap(),
                NodeId::new("root").unwrap(),
            )
            .unwrap();
            let document = insert_hold(&empty, "base", "base-hold");
            let directory = tempfile::tempdir().unwrap();
            let store =
                ProjectStore::create(&directory.path().join("test.deadpan"), &document).unwrap();
            let base = Arc::new(Snapshot::committed(
                17,
                document,
                BTreeMap::new(),
                store.original_import_handle().unwrap(),
            ));
            let first = Arc::new(
                Snapshot::proposed(
                    &base,
                    insert_hold(&base.document, "first", "first-hold"),
                    9,
                    1,
                )
                .unwrap(),
            );
            let second = Arc::new(
                Snapshot::proposed(
                    &base,
                    insert_hold(&base.document, "second", "second-hold"),
                    9,
                    2,
                )
                .unwrap(),
            );
            Self {
                base,
                first,
                second,
                _store: store,
                _directory: directory,
            }
        }

        fn junction(&self, snapshot: &Snapshot, side: JunctionSide) -> EditJunctionIdentity {
            let (draft, change) = match snapshot.content {
                ContentIdentity::Proposed { draft, change, .. } => (draft, change),
                ContentIdentity::Committed => (9, 3),
            };
            EditJunctionIdentity {
                session: self.base.session,
                project: self.base.document.project_id().clone(),
                base_revision: self.base.document.revision_id().clone(),
                draft,
                change,
                content: snapshot.content.clone(),
                proposal_revision: (snapshot.content != ContentIdentity::Committed)
                    .then(|| snapshot.document.revision_id().clone()),
                inspection: 4,
                side,
                role: JunctionRole::In,
                boundary: ProjectFrame(1),
                outgoing: Some(ProjectFrame(0)),
                incoming: Some(ProjectFrame(1)),
            }
        }

        fn display(&self) -> Display {
            let (expected, _) = admit_request(
                self.junction(&self.first, JunctionSide::Proposed),
                self.base.clone(),
                Some(self.first.clone()),
                window(),
            )
            .unwrap();
            Display {
                expected: Some(expected),
                ..Display::default()
            }
        }
    }

    fn window() -> Range<AudioSample> {
        AudioSample(128)..AudioSample(1152)
    }

    #[test]
    fn authenticates_same_base_candidates_before_selecting_either_side() {
        let fixture = Fixture::new();
        for side in [JunctionSide::Before, JunctionSide::Proposed] {
            let junction = fixture.junction(&fixture.second, side);
            assert!(
                admit_request(
                    junction.clone(),
                    fixture.base.clone(),
                    Some(fixture.first.clone()),
                    window()
                )
                .is_err()
            );
            let (expected, request) = admit_request(
                junction.clone(),
                fixture.base.clone(),
                Some(fixture.second.clone()),
                window(),
            )
            .unwrap();
            let selected = if side == JunctionSide::Before {
                &fixture.base
            } else {
                &fixture.second
            };
            assert!(Arc::ptr_eq(&request.snapshot, selected));
            assert_eq!(expected.content, selected.content);
            assert_eq!(&expected.revision, selected.document.revision_id());
            assert_eq!(expected.junction, junction);
        }
    }

    #[test]
    fn rejects_missing_unexpected_and_wrong_revision_candidates_even_before() {
        let fixture = Fixture::new();
        for side in [JunctionSide::Before, JunctionSide::Proposed] {
            let junction = fixture.junction(&fixture.first, side);
            assert!(admit_request(junction.clone(), fixture.base.clone(), None, window()).is_err());
            let mut wrong_revision = junction;
            wrong_revision.proposal_revision = Some(fixture.second.document.revision_id().clone());
            assert!(
                admit_request(
                    wrong_revision,
                    fixture.base.clone(),
                    Some(fixture.first.clone()),
                    window()
                )
                .is_err()
            );
            let zero = fixture.junction(&fixture.base, side);
            assert!(
                admit_request(
                    zero.clone(),
                    fixture.base.clone(),
                    Some(fixture.first.clone()),
                    window()
                )
                .is_err()
            );
            let (_, request) =
                admit_request(zero.clone(), fixture.base.clone(), None, window()).unwrap();
            assert!(Arc::ptr_eq(&request.snapshot, &fixture.base));
            let mut wrong_zero = zero;
            wrong_zero.proposal_revision = Some(fixture.first.document.revision_id().clone());
            assert!(admit_request(wrong_zero, fixture.base.clone(), None, window()).is_err());
        }
    }

    #[test]
    fn rejects_foreign_base_tampered_capability_and_bad_junction_geometry() {
        let fixture = Fixture::new();
        let junction = fixture.junction(&fixture.first, JunctionSide::Before);
        let reject = |identity| {
            assert!(
                admit_request(
                    identity,
                    fixture.base.clone(),
                    Some(fixture.first.clone()),
                    window()
                )
                .is_err()
            )
        };
        let mut wrong = junction.clone();
        wrong.session += 1;
        reject(wrong);
        let mut wrong = junction.clone();
        wrong.project = ProjectId::new("other").unwrap();
        reject(wrong);
        let mut wrong = junction.clone();
        wrong.base_revision = RevisionId::new("other").unwrap();
        reject(wrong);
        let mut wrong = junction.clone();
        wrong.draft += 1;
        reject(wrong);
        let mut wrong = junction.clone();
        wrong.change += 1;
        reject(wrong);
        let mut wrong = junction.clone();
        wrong.inspection = 0;
        reject(wrong);
        let mut wrong = junction.clone();
        wrong.boundary = ProjectFrame(-1);
        reject(wrong);
        let mut wrong = junction.clone();
        wrong.boundary = ProjectFrame(4);
        reject(wrong);
        let mut wrong = junction.clone();
        wrong.outgoing = None;
        reject(wrong);
        let mut wrong = junction.clone();
        wrong.incoming = None;
        reject(wrong);
        let mut forged =
            Snapshot::proposed(&fixture.base, fixture.first.document.clone(), 9, 1).unwrap();
        forged.document = Arc::new((*forged.document).clone());
        assert!(
            admit_request(
                junction.clone(),
                fixture.base.clone(),
                Some(Arc::new(forged)),
                window()
            )
            .is_err()
        );
        let equal_base = Arc::new(Snapshot::committed(
            fixture.base.session,
            Arc::new((*fixture.base.document).clone()),
            BTreeMap::new(),
            fixture.base.originals.clone(),
        ));
        assert!(
            admit_request(
                junction.clone(),
                equal_base,
                Some(fixture.first.clone()),
                window()
            )
            .is_err()
        );
        assert!(
            admit_request(
                junction.clone(),
                fixture.base.clone(),
                Some(fixture.first.clone()),
                AudioSample(-1)..AudioSample(10)
            )
            .is_err()
        );
        assert!(
            admit_request(
                junction,
                fixture.base.clone(),
                Some(fixture.first.clone()),
                AudioSample(0)..AudioSample(i64::MAX)
            )
            .is_err()
        );
    }

    struct NoSources;
    impl AudioSourceProvider for NoSources {
        fn source(
            &mut self,
            _: &ProjectId,
            _: &RevisionId,
            _: &AssetId,
            _: &AtomicBool,
        ) -> Result<&PreparedSource, PreparationError> {
            panic!("silent Hold fixture must not read media")
        }
    }

    fn prefix(
        document: &ProjectDocument,
        samples: Range<AudioSample>,
        count: u64,
    ) -> Arc<EditWaveform> {
        let mut audio = StageAudio::new(Arc::new(RenderPlan::compile(document).unwrap()));
        let memory = WaveformMemory::default();
        let cancelled = AtomicBool::new(false);
        audio
            .measure_edit_window(
                &mut NoSources,
                samples,
                WaveformControl {
                    limits: WaveformLimits::new(count, 16, Duration::from_secs(5)).unwrap(),
                    cancelled: &cancelled,
                    memory: &memory,
                },
                |_| {},
            )
            .unwrap()
            .waveform
    }

    #[test]
    fn state_only_progress_and_interruption_retain_prefix_but_unavailable_clears_it() {
        let fixture = Fixture::new();
        let mut display = fixture.display();
        let first = prefix(&fixture.first.document, window(), 256);
        assert_eq!(first.measured_end(), AudioSample(384));
        display.receive_data(WaveformStatus::Measuring, Some(first.clone()), None);
        for status in [WaveformStatus::Measuring, WaveformStatus::Interrupted] {
            display.receive_data(status, None, Some("state only".into()));
            assert!(Arc::ptr_eq(display.data.as_ref().unwrap(), &first));
            assert_eq!(display.status, status);
            assert_eq!(display.error.as_deref(), Some("state only"));
        }
        let next = prefix(&fixture.first.document, window(), 512);
        display.receive_data(WaveformStatus::Measuring, Some(next.clone()), None);
        assert!(Arc::ptr_eq(display.data.as_ref().unwrap(), &next));
        assert_eq!(
            display.data.as_ref().unwrap().measured_end(),
            AudioSample(640)
        );
        display.receive_data(
            WaveformStatus::Unavailable,
            None,
            Some("media admission expired".into()),
        );
        assert!(display.data.is_none());
        assert_eq!(display.status, WaveformStatus::Unavailable);
        display.receive_data(WaveformStatus::Unavailable, Some(next), None);
        assert!(
            display.data.is_none(),
            "unavailable cannot republish even valid peaks"
        );
    }

    #[test]
    fn mismatched_descriptor_clears_known_prefix() {
        let fixture = Fixture::new();
        for bad in [
            prefix(&fixture.second.document, window(), 256),
            prefix(
                &fixture.first.document,
                AudioSample(0)..AudioSample(1024),
                256,
            ),
        ] {
            let mut display = fixture.display();
            display.receive_data(
                WaveformStatus::Measuring,
                Some(prefix(&fixture.first.document, window(), 256)),
                None,
            );
            assert!(display.data.is_some());
            display.receive_data(WaveformStatus::Measuring, Some(bad), None);
            assert!(display.data.is_none());
            assert_eq!(display.status, WaveformStatus::Unavailable);
            assert_eq!(
                display.error.as_deref(),
                Some("Waveform returned a different Edit window.")
            );
        }
    }
}
