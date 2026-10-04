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
        let action = if preparing {
            "Cancel preparation"
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
        let status = app.transport.as_ref().map(|run| {
            let millis = run.content_sample().unwrap_or(run.sample).0 / 48;
            let status = if preparing { "Preparing" } else { "Playing" };
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
                .color(style::MUTED),
                width,
            )
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
            .color(style::MUTED),
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
        let inline_monitor = !active
            && !app.sound_focused()
            && (app.compact_sound_layout(ui.ctx()) || app.compact_original_controls(ui.ctx()))
            && play_size.x + audition_size.x + monitor_size.x + 2.0 * ui.spacing().item_spacing.x
                <= width;
        // In a compact Original, the live clock shares the context/monitor
        // row when it fits. Long action paths then keep both buttons together.
        let status_in_context = app.compact_original_controls(ui.ctx())
            && !app.sound_focused()
            && status.as_ref().is_some_and(|status| {
                status.size().x
                    + context.size().x
                    + monitor_size.x
                    + 2.0 * ui.spacing().item_spacing.x
                    <= width
            });
        let mut height = 0.0;
        if !app.sound_focused() {
            let mut widgets = vec![play_size, audition_size];
            if let Some(status) = &status
                && !status_in_context
            {
                widgets.push(status.size());
            }
            if inline_monitor {
                widgets.push(monitor_size);
            }
            height += row_height(ui, width, &widgets) + ui.spacing().item_spacing.y;
        }
        if !inline_monitor {
            let mut widgets = Vec::new();
            if let Some(status) = &status
                && status_in_context
            {
                widgets.push(status.size());
            }
            widgets.extend([context.size(), monitor_size]);
            height += row_height(ui, width, &widgets) + ui.spacing().item_spacing.y;
        }
        Self {
            play,
            audition,
            status,
            context,
            inline_monitor,
            status_in_context,
            height,
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
