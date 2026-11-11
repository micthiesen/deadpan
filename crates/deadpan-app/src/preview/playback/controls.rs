//! Measure the transport rows with the same text geometry used for painting.

use super::*;

pub(super) struct Controls {
    pub play: Arc<egui::Galley>,
    pub audition: Arc<egui::Galley>,
    pub status: Option<Arc<egui::Galley>>,
    pub context: Arc<egui::Galley>,
    pub inline_monitor: bool,
    pub status_in_context: bool,
    pub height: f32,
    /// The monitor slider and its label: one unit that cannot wrap.
    pub monitor: egui::Vec2,
}

impl Controls {
    pub fn new(app: &DeadpanApp, ui: &egui::Ui) -> Self {
        let width = ui.available_width().max(1.0);
        let padding = ui.spacing().button_padding * 2.0;
        let text = |value: egui::RichText, width| {
            egui::WidgetText::from(value).into_galley(
                ui,
                Some(egui::TextWrapMode::Wrap),
                width,
                egui::TextStyle::Button,
            )
        };
        let active = app.transport.is_some();
        let preparing = app
            .transport
            .as_ref()
            .is_some_and(|run| run.phase == Phase::Preparing);
        let looping = app
            .transport
            .as_ref()
            .is_some_and(|run| run.window().looping());
        let finishing = app
            .transport
            .as_ref()
            .is_some_and(|run| run.phase == Phase::Ended);
        let action = if preparing {
            "Cancel preparation"
        } else if finishing {
            "Stop"
        } else if active {
            "Pause"
        } else if app
            .resume
            .as_ref()
            .is_some_and(|resume| resume.window().looping())
        {
            "Resume loop"
        } else if app.view == View::Source {
            "Play Original"
        } else {
            "Play edit"
        };
        let play = style::action_galley(
            ui,
            action,
            &app.editor_key(EditorKey::Playback),
            (width - padding.x).max(1.0),
        );
        let action = if looping {
            "Pause loop"
        } else {
            "Loop selection"
        };
        let audition = style::action_galley(
            ui,
            action,
            &app.editor_key(EditorKey::Audition),
            (width - padding.x).max(1.0),
        );
        let status = app
            .transport
            .as_ref()
            .map(|run| {
                let millis = run.content_sample().unwrap_or(run.sample).0 / 48;
                let status = if preparing {
                    run.preparation_label()
                } else if finishing {
                    "Finishing picture".into()
                } else {
                    "Playing".into()
                };
                let lap = if looping {
                    format!(" · loop {}", run.lap().unwrap_or(0) + 1)
                } else {
                    String::new()
                };
                text(
                    egui::RichText::new(format!(
                        "{status} · {:02}:{:02}.{:03}{lap}",
                        millis / 60_000,
                        (millis / 1000) % 60,
                        millis % 1000
                    ))
                    .monospace()
                    .size(10.0)
                    .weak(),
                    width,
                )
            })
            .or_else(|| {
                app.idle_audio_label()
                    .map(|label| text(egui::RichText::new(label).size(10.0).weak(), width))
            });
        let context = text(
            egui::RichText::new(if app.sound_focused() {
                "Sound loop · complete measured audio".into()
            } else {
                format!(
                    "Loop context  {} / {}",
                    app.audition_context.lead_label(),
                    app.audition_context.follow_label()
                )
            })
            .size(10.5)
            .weak(),
            width,
        );
        let button_size =
            |galley: &egui::Galley| (galley.size() + padding).max(ui.spacing().interact_size);
        let play_size = button_size(&play);
        let audition_size = button_size(&audition);
        let monitor_text = text(egui::RichText::new("Monitor · :monitor"), f32::INFINITY);
        let monitor_size = egui::vec2(
            ui.spacing().slider_width + ui.spacing().item_spacing.x + monitor_text.size().x,
            ui.spacing().interact_size.y.max(monitor_text.size().y),
        );
        // One row when actions, live status, loop context and the monitor all
        // fit; otherwise the loop context and monitor take a second row. The
        // monitor slider and its label lay out as one unit that cannot wrap.
        let spacing = ui.spacing().item_spacing.x;
        let mut first = Vec::new();
        if !app.sound_focused() {
            first.extend([play_size, audition_size]);
        }
        let status_in_context = app.sound_focused() && status.is_some();
        if let Some(status) = &status
            && !status_in_context
        {
            first.push(status.size());
        }
        let inline_monitor = !app.sound_focused()
            && first.iter().map(|size| size.x + spacing).sum::<f32>()
                + context.size().x
                + spacing
                + monitor_size.x
                <= width;
        let height = if inline_monitor {
            first.extend([context.size(), monitor_size]);
            row_height(ui, width, &first) + ui.spacing().item_spacing.y
        } else {
            let mut second = Vec::new();
            if let Some(status) = &status
                && status_in_context
            {
                second.push(status.size());
            }
            second.extend([context.size(), monitor_size]);
            let first_height = if first.is_empty() {
                0.0
            } else {
                row_height(ui, width, &first) + ui.spacing().item_spacing.y
            };
            first_height + row_height(ui, width, &second) + ui.spacing().item_spacing.y
        };
        Self {
            play,
            audition,
            status,
            context,
            inline_monitor,
            status_in_context,
            height,
            monitor: monitor_size,
        }
    }
}

fn row_height(ui: &egui::Ui, width: f32, widgets: &[egui::Vec2]) -> f32 {
    let mut used = 0.0;
    let mut completed = 0.0;
    let mut row = ui.spacing().interact_size.y;
    for size in widgets {
        if used > 0.0 && used + size.x > width {
            completed += row + ui.spacing().item_spacing.y;
            used = 0.0;
            row = ui.spacing().interact_size.y;
        }
        used += size.x + ui.spacing().item_spacing.x;
        row = row.max(size.y);
    }
    completed + row
}
