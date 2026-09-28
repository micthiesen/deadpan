//! One captured beat's measured audio, independent of proposed gain and transport.

use std::sync::Arc;

use deadpan_audio::DefinitionWaveform;
use deadpan_core::{ExactRatio, NodeId};
use deadpan_playback::{
    Engine, Snapshot, WaveformRequest, WaveformStatus, WaveformTicket, WaveformUpdate,
};
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

use super::controls::FocusReveal;
use crate::project::gain::Target;

pub(super) const GRAPH: &str = "Measured stereo waveform · owner-output frames · before effects";
const RETRY: &str = "Retry waveform";

pub(super) struct Display {
    pub(super) ticket: Option<WaveformTicket>,
    pub(super) status: WaveformStatus,
    pub(super) data: Option<Arc<DefinitionWaveform>>,
    error: Option<String>,
    previous: bool,
}

impl Default for Display {
    fn default() -> Self {
        Self {
            ticket: None,
            status: WaveformStatus::Queued,
            data: None,
            error: None,
            previous: false,
        }
    }
}

impl Display {
    pub(super) fn request(&mut self, engine: &Engine, base: &Arc<Snapshot>, owner: &NodeId) {
        self.cancel(engine);
        self.previous = self.data.is_some();
        self.error = None;
        match engine.request_waveform(WaveformRequest {
            snapshot: base.clone(),
            owner: owner.clone(),
        }) {
            Ok(ticket) => {
                self.ticket = Some(ticket);
                self.status = WaveformStatus::Queued;
            }
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

    pub(super) fn receive(&mut self, target: &Target, update: WaveformUpdate) {
        if self.ticket != Some(update.ticket)
            || update.session != target.session
            || update.project_id != target.project
            || update.revision_id != target.revision
            || update.owner != target.node
        {
            return;
        }
        if let Some(data) = update.waveform {
            let descriptor = data.descriptor();
            if descriptor.project_id != target.project
                || descriptor.revision_id != target.revision
                || descriptor.root != target.node
                || descriptor.definition
                    != (deadpan_plan::AudioDefinitionSelector::Node {
                        node: target.node.clone(),
                    })
            {
                return;
            }
            self.data = Some(data);
            self.previous = false;
        }
        self.status = update.status;
        self.error = update.error;
    }

    pub(super) fn show(&self, ui: &mut egui::Ui, owner_frames: i64) -> bool {
        let mut retry = false;
        ui.horizontal_wrapped(|ui| {
            ui.strong("Beat audio · before effects").on_hover_text(
                "Measured left/right audio with this beat's timing and pitch. Creative fades, gain, mastering, enclosing occurrence processing and independent placed sounds are excluded. Before/Draft auditions the full edit mix.",
            );
            if self.can_retry() {
                retry = ui.button(RETRY).reveal_on_focus().clicked();
            }
        });
        ui.weak(self.caption());
        if let Some(error) = &self.error {
            ui.small(error);
        }
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width().max(1.0), 96.0),
            egui::Sense::hover(),
        );
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, GRAPH));
        let plot = super::controls::plot_rect(rect);
        if owner_frames > 0 && plot.width() > 1.0 && ui.is_rect_visible(rect) {
            self.paint(ui, rect, plot, owner_frames);
        } else if owner_frames <= 0 {
            ui.small("This beat has no allocated audio frames.");
        }
        ui.add_space(4.0);
        retry
    }

    fn can_retry(&self) -> bool {
        matches!(
            self.status,
            WaveformStatus::Partial | WaveformStatus::Interrupted | WaveformStatus::Unavailable
        )
    }

    fn caption(&self) -> String {
        let state = match self.status {
            WaveformStatus::Queued => "Waiting for idle audio",
            WaveformStatus::Measuring => "Measuring",
            WaveformStatus::Complete => "Measured",
            WaveformStatus::Partial => "Partial measurement",
            WaveformStatus::Interrupted => "Measurement interrupted",
            WaveformStatus::Unavailable => "Measurement unavailable",
        };
        let prior = if self.previous {
            "Previous measurement · "
        } else {
            ""
        };
        self.data.as_ref().map_or_else(
            || format!("{state} · unknown audio is not silence"),
            |data| {
                format!(
                    "{prior}{state} · {:.2} / {:.2} s · amplitude ±1",
                    data.measured_end().0 as f64 / 48_000.0,
                    data.descriptor().total_samples.0 as f64 / 48_000.0,
                )
            },
        )
    }

    fn paint(&self, ui: &egui::Ui, rect: Rect, plot: Rect, owner_frames: i64) {
        let painter = ui.painter_at(rect);
        let font = egui::FontId::proportional(10.0);
        let grey = Color32::from_gray(130);
        let lanes = [
            Rect::from_min_max(plot.min, Pos2::new(plot.right(), plot.center().y - 3.0)),
            Rect::from_min_max(Pos2::new(plot.left(), plot.center().y + 3.0), plot.max),
        ];
        for (channel, lane) in lanes.iter().enumerate() {
            for amplitude in [-1.0, 0.0, 1.0] {
                let y = amplitude_y(*lane, amplitude);
                painter.line_segment(
                    [Pos2::new(lane.left(), y), Pos2::new(lane.right(), y)],
                    Stroke::new(
                        1.0,
                        Color32::from_gray(if amplitude == 0.0 { 65 } else { 44 }),
                    ),
                );
                painter.text(
                    Pos2::new(plot.left() - 6.0, y),
                    egui::Align2::RIGHT_CENTER,
                    format!("{amplitude:+.0}"),
                    font.clone(),
                    grey,
                );
            }
            painter.text(
                Pos2::new(rect.left(), lane.center().y),
                egui::Align2::LEFT_CENTER,
                if channel == 0 { "L" } else { "R" },
                font.clone(),
                grey,
            );
        }
        for step in 0..=4 {
            let x = plot.left() + plot.width() * step as f32 / 4.0;
            painter.line_segment(
                [Pos2::new(x, plot.top()), Pos2::new(x, plot.bottom())],
                Stroke::new(1.0, Color32::from_gray(48)),
            );
        }
        painter.text(
            Pos2::new(plot.left(), rect.bottom()),
            egui::Align2::LEFT_BOTTOM,
            "0 f",
            font.clone(),
            grey,
        );
        painter.text(
            Pos2::new(plot.right(), rect.bottom()),
            egui::Align2::RIGHT_BOTTOM,
            format!("{owner_frames} owner frames"),
            font.clone(),
            grey,
        );

        let mut measured_x = plot.left();
        let mut over_range = false;
        if let Some(data) = &self.data {
            let preferred = preferred_level(data, plot.width());
            let mut covered = 0;
            // Full parents cover the prefix; descend for a final partial-parent
            // tail. A coarser display must not discard known complete leaves.
            for level in (0..=preferred).rev() {
                if covered >= data.measured_end().0 {
                    break;
                }
                let Some(stride) = u32::try_from(level)
                    .ok()
                    .and_then(|shift| data.descriptor().leaf_stride.checked_shl(shift))
                else {
                    break;
                };
                let Some(first) = u64::try_from(covered)
                    .ok()
                    .and_then(|value| usize::try_from(value / stride).ok())
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
                    if samples.start.0 != covered {
                        continue;
                    }
                    let Ok(frames) = data.bin_owner_frames(level, index) else {
                        continue;
                    };
                    let x0 = owner_x(plot, owner_frames, frames.start);
                    let x1 = owner_x(plot, owner_frames, frames.end);
                    let minimum = extrema.minimum();
                    let maximum = extrema.maximum();
                    for channel in 0..2 {
                        let lane = lanes[channel];
                        let bottom = amplitude_y(lane, minimum[channel]);
                        let top = amplitude_y(lane, maximum[channel]);
                        painter.rect_filled(
                            Rect::from_min_max(
                                Pos2::new(x0, top),
                                Pos2::new(x1, bottom.max(top + 0.7)),
                            ),
                            0.0,
                            Color32::from_rgb(137, 145, 165),
                        );
                        if maximum[channel] > 1.0 || minimum[channel] < -1.0 {
                            over_range = true;
                            let y = if maximum[channel] > 1.0 {
                                lane.top()
                            } else {
                                lane.bottom()
                            };
                            painter.line_segment(
                                [Pos2::new(x0, y), Pos2::new(x1, y)],
                                Stroke::new(2.0, Color32::from_rgb(220, 187, 110)),
                            );
                        }
                    }
                    covered = samples.end.0;
                    measured_x = x1;
                }
            }
        }
        if measured_x < plot.right() {
            let unknown = Rect::from_min_max(Pos2::new(measured_x, plot.top()), plot.max);
            let painter = painter.with_clip_rect(unknown.intersect(ui.clip_rect()));
            painter.rect_filled(unknown, 0.0, Color32::from_rgb(35, 39, 47));
            let mut x = unknown.left() - unknown.height();
            while x < unknown.right() {
                painter.line_segment(
                    [
                        Pos2::new(x, unknown.bottom()),
                        Pos2::new(x + unknown.height(), unknown.top()),
                    ],
                    Stroke::new(1.0, Color32::from_gray(53)),
                );
                x += 9.0;
            }
            if unknown.width() > 95.0 {
                painter.text(
                    unknown.center(),
                    egui::Align2::CENTER_CENTER,
                    "Not measured",
                    font.clone(),
                    Color32::from_gray(177),
                );
            }
        }
        if over_range {
            painter.text(
                Pos2::new(plot.center().x, rect.top()),
                egui::Align2::CENTER_TOP,
                "Peaks beyond ±1 marked at edge",
                font,
                Color32::from_rgb(220, 187, 110),
            );
        }
    }
}

fn preferred_level(data: &DefinitionWaveform, width: f32) -> usize {
    let mut level = 0;
    while level + 1 < data.level_count()
        && data
            .level(level)
            .is_some_and(|bins| bins.len() > width.max(1.0) as usize)
    {
        level += 1;
    }
    level
}

fn amplitude_y(lane: Rect, value: f32) -> f32 {
    lane.center().y - value.clamp(-1.0, 1.0) * lane.height() * 0.5
}

fn owner_x(plot: Rect, frames: i64, position: ExactRatio) -> f32 {
    let value = position.numerator() as f64 / position.denominator() as f64;
    plot.left() + (value / frames as f64).clamp(0.0, 1.0) as f32 * plot.width()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_and_interrupted_measurements_never_describe_unknown_audio_as_silence() {
        let mut display = Display::default();
        assert!(display.caption().contains("unknown audio is not silence"));
        assert!(!display.can_retry());
        display.status = WaveformStatus::Interrupted;
        assert!(display.can_retry());
        assert!(display.caption().starts_with("Measurement interrupted"));
        display.status = WaveformStatus::Unavailable;
        assert!(display.can_retry());
    }

    #[test]
    fn owner_coordinates_keep_fractional_phase_and_fixed_signed_amplitude() {
        let plot = Rect::from_min_max(Pos2::new(10.0, 0.0), Pos2::new(110.0, 40.0));
        assert_eq!(owner_x(plot, 1, ExactRatio::new(1, 2).unwrap()), 60.0);
        assert_eq!(owner_x(plot, 1, ExactRatio::integer(1)), 110.0);
        assert_eq!(amplitude_y(plot, -1.0), 40.0);
        assert_eq!(amplitude_y(plot, 0.0), 20.0);
        assert_eq!(amplitude_y(plot, 1.0), 0.0);
        assert_eq!(amplitude_y(plot, 4.0), 0.0);
    }
}
