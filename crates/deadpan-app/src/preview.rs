use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use deadpan_core::SourceFrameId;
use deadpan_render::{FitMode, PictureRenderer, RenderTarget};
use eframe::{egui, egui_wgpu};

use crate::navigation::{self, Motion, TextAction};
use crate::worker::{Picture, PreviewWorker, SourceSummary, Ticket, Work};

const PATH_ID: &str = "source-preview-path";
const MAX_TARGET_PIXELS: f64 = 1920.0 * 1080.0;

struct RegisteredTarget {
    target: RenderTarget,
    texture: egui::TextureId,
}

pub struct DeadpanApp {
    smoke_frames: Option<u8>,
    exited: Rc<Cell<bool>>,
    worker: PreviewWorker,
    render_state: egui_wgpu::RenderState,
    renderer: PictureRenderer,
    target: Option<RegisteredTarget>,
    path: String,
    source_path: Option<PathBuf>,
    serial: u64,
    latest: Option<Ticket>,
    summary: Option<SourceSummary>,
    picture: Option<Picture>,
    requested: u64,
    shown: Option<(u64, i64)>,
    dirty: bool,
    loading: bool,
    error: Option<String>,
    ime_composing: bool,
}

impl DeadpanApp {
    pub fn new(
        context: &eframe::CreationContext<'_>,
        render_state: egui_wgpu::RenderState,
        smoke_test: bool,
        exited: Rc<Cell<bool>>,
        initial_path: Option<String>,
    ) -> Result<Self, std::io::Error> {
        context.egui_ctx.all_styles_mut(|style| {
            style.visuals.weak_text_color = Some(if style.visuals.dark_mode {
                egui::Color32::from_gray(165)
            } else {
                egui::Color32::from_gray(85)
            });
            for text in [egui::TextStyle::Body, egui::TextStyle::Button] {
                style
                    .text_styles
                    .insert(text, egui::FontId::proportional(14.0));
            }
        });
        let worker = PreviewWorker::new(context.egui_ctx.clone())?;
        let renderer = PictureRenderer::new(&render_state.device, &render_state.queue);
        let mut app = Self {
            smoke_frames: smoke_test.then_some(0),
            exited,
            worker,
            render_state,
            renderer,
            target: None,
            path: initial_path.clone().unwrap_or_default(),
            source_path: None,
            serial: 0,
            latest: None,
            summary: None,
            picture: None,
            requested: 0,
            shown: None,
            dirty: false,
            loading: false,
            error: None,
            ime_composing: false,
        };
        if initial_path.is_some() {
            app.open();
        }
        Ok(app)
    }

    fn next_serial(&mut self) -> Option<u64> {
        match self.serial.checked_add(1) {
            Some(value) => {
                self.serial = value;
                Some(value)
            }
            None => {
                self.error =
                    Some("Preview request identities are exhausted. Reopen the app.".into());
                None
            }
        }
    }

    fn open(&mut self) {
        if self.path.is_empty() {
            self.error = Some("Enter the path to a local video file.".into());
            return;
        }
        let Some(serial) = self.next_serial() else {
            return;
        };
        let ticket = Ticket {
            source: serial,
            request: serial,
        };
        let path = PathBuf::from(&self.path);
        self.latest = Some(ticket);
        self.source_path = Some(path.clone());
        self.summary = None;
        self.picture = None;
        self.shown = None;
        self.requested = 0;
        self.dirty = false;
        self.loading = true;
        self.error = None;
        self.worker.submit(ticket, Work::Open(path));
    }

    fn navigate(&mut self, motion: Motion) {
        let Some(summary) = &self.summary else {
            return;
        };
        let Some(next) = navigation::destination(self.requested, summary.frame_count, motion)
        else {
            return;
        };
        let Some(current) = self.latest else {
            return;
        };
        let Some(serial) = self.next_serial() else {
            return;
        };
        let ticket = Ticket {
            source: current.source,
            request: serial,
        };
        self.latest = Some(ticket);
        self.requested = next;
        self.loading = true;
        self.error = None;
        // A decoded but not yet rendered older request must not become visible.
        self.dirty = false;
        if self
            .picture
            .as_ref()
            .is_some_and(|picture| self.shown.is_none_or(|(shown, _)| shown != picture.id.0))
        {
            self.picture = None;
        }
        self.worker.submit(ticket, Work::Frame(SourceFrameId(next)));
    }

    fn receive(&mut self) {
        let Some(reply) = self.worker.take_reply() else {
            return;
        };
        if self.latest != Some(reply.ticket) {
            return;
        }
        self.loading = false;
        match reply.picture {
            Ok(mut picture) => {
                if let Some(summary) = picture.summary.take() {
                    self.summary = Some(summary);
                }
                self.picture = Some(picture);
                self.dirty = true;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn forget_target(&mut self) {
        if let Some(registered) = self.target.take() {
            self.render_state
                .renderer
                .write()
                .free_texture(&registered.texture);
            // The owned textures are released only after their egui registration.
            drop(registered);
        }
    }

    fn render_picture(&mut self, context: &egui::Context, size: egui::Vec2) {
        if self.picture.is_none() {
            return;
        }
        match self.renderer.is_idle() {
            Ok(false) => {
                context.request_repaint_after(Duration::from_millis(16));
                return;
            }
            Err(error) => {
                self.error = Some(format!("Preview renderer: {error}"));
                self.dirty = false;
                return;
            }
            Ok(true) => {}
        }
        let (width, height) = target_size(size, context.pixels_per_point());
        let resize = self.target.as_ref().is_none_or(|registered| {
            registered.target.width() != width || registered.target.height() != height
        });
        if !self.dirty && !resize {
            return;
        }
        if resize {
            self.forget_target();
            let target = match self.renderer.create_target(width, height) {
                Ok(target) => target,
                Err(error) => {
                    self.error = Some(format!("Preview target: {error}"));
                    return;
                }
            };
            let texture = self.render_state.renderer.write().register_native_texture(
                &self.render_state.device,
                target.display_view(),
                eframe::wgpu::FilterMode::Linear,
            );
            self.target = Some(RegisteredTarget { target, texture });
        }
        let picture = self.picture.as_ref().expect("picture checked");
        let registered = self.target.as_ref().expect("target created");
        match self
            .renderer
            .render(&picture.frame, &registered.target, FitMode::Fit)
        {
            Ok(_) => {
                self.shown = Some((picture.id.0, picture.frame.metadata().pts.ticks));
                self.dirty = false;
                context.request_repaint_after(Duration::from_millis(16));
            }
            Err(error) => {
                self.error = Some(format!("Preview renderer: {error}"));
                self.dirty = false;
            }
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("source-header").show(ui, |ui| {
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                ui.heading("Deadpan");
                ui.separator();
                ui.strong("Source preview");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak("Non-destructive · Local file");
                });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let label = ui.label("Video path");
                let reserve = 78.0;
                ui.add_sized(
                    [(ui.available_width() - reserve).max(120.0), 26.0],
                    egui::TextEdit::singleline(&mut self.path)
                        .id(egui::Id::new(PATH_ID))
                        .char_limit(32_768)
                        .hint_text("/path/to/video.mp4"),
                )
                .labelled_by(label.id);
                if ui
                    .add_sized([65.0, 26.0], egui::Button::new("Open"))
                    .clicked()
                {
                    self.open();
                }
            });
            ui.add_space(5.0);
        });
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("source-status").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                let text = ui.memory(|memory| memory.has_focus(egui::Id::new(PATH_ID)));
                ui.strong(if text { "TEXT · Source" } else { "NORMAL · Source" });
                ui.separator();
                if self.loading {
                    ui.spinner();
                    ui.label(if self.summary.is_some() {
                        format!("Decoding frame {}…", self.requested + 1)
                    } else {
                        "Reading and indexing source…".into()
                    });
                } else if let (Some(summary), Some((frame, pts))) = (&self.summary, self.shown) {
                    ui.label(format!("Frame {} / {}", frame + 1, summary.frame_count));
                    ui.separator();
                    ui.label(format!("PTS {pts} × {}/{} s", summary.info.time_base_num, summary.info.time_base_den));
                } else if self.error.is_none() {
                    ui.label("Ready to open a video");
                }
            });
            if let Some(error) = &self.error {
                ui.horizontal_wrapped(|ui| {
                    ui.strong("Preview unavailable:");
                    ui.label(error);
                });
            }
            ui.add_space(3.0);
            ui.horizontal_wrapped(|ui| {
                ui.weak("Left / Right: frame   Home / End: first / last   ⌘O: path   Enter: open   Esc: leave text   Tab: focus");
            });
            ui.add_space(3.0);
        });
    }

    fn viewer(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong(self.source_path.as_ref().and_then(|path| path.file_name()).map_or_else(
                    || "No source open".into(), |name| name.to_string_lossy(),
                ));
                if let Some(summary) = &self.summary {
                    ui.weak(format!("{} × {} · {}", summary.info.width, summary.info.height, summary.info.codec));
                }
            });
            ui.add_space(6.0);
            let available = ui.available_size();
            let picture_size = egui::vec2(available.x.max(1.0), (available.y - 64.0).max(1.0));
            self.render_picture(ui.ctx(), picture_size);
            let (rect, response) = ui.allocate_exact_size(picture_size, egui::Sense::click());
            ui.painter().rect_filled(rect, 5.0, egui::Color32::from_rgb(13, 15, 18));
            if self.shown.is_some() {
                if let Some(registered) = &self.target {
                    ui.painter().image(registered.texture, rect, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                }
            } else {
                ui.painter().text(
                    rect.center(), egui::Align2::CENTER_CENTER,
                    if self.loading { "Preparing source preview…" } else { "Open a local video to inspect its original frames" },
                    egui::FontId::proportional(18.0), egui::Color32::from_gray(175),
                );
            }
            response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, "Source picture preview"));
            if response.clicked() {
                response.request_focus();
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let count = self.summary.as_ref().map_or(0, |summary| summary.frame_count);
                for (label, motion, enabled) in [
                    ("First", Motion::First, self.requested > 0),
                    ("Previous", Motion::Previous, self.requested > 0),
                    ("Next", Motion::Next, self.requested + 1 < count),
                    ("Last", Motion::Last, self.requested + 1 < count),
                ] {
                    if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                        self.navigate(motion);
                    }
                }
                if let Some(summary) = &self.summary {
                    ui.weak(format!("{} original frames", summary.frame_count));
                }
            });
            ui.horizontal_wrapped(|ui| {
                if let Some(summary) = &self.summary {
                    ui.weak(format!("Source clock: {}…{} ticks · SAR {}:{} · Rotation {}°", summary.first_pts, summary.terminal_pts,
                        summary.info.sample_aspect_num, summary.info.sample_aspect_den, u16::from(summary.info.rotation_quarter_turns) * 90));
                } else {
                    ui.weak("Preview only. No project import, editing, audio playback or export in this workspace.");
                }
            });
        });
    }

    fn keyboard(&mut self, context: &egui::Context, prior_text_focus: bool, ime_event: bool) {
        let text_focus =
            prior_text_focus || context.memory(|memory| memory.has_focus(egui::Id::new(PATH_ID)));
        let ime = self.ime_composing || ime_event;
        let keys = context.input(|input| {
            input
                .events
                .iter()
                .filter_map(|event| {
                    if let egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } = event
                    {
                        Some((*key, *modifiers))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        });
        for (key, modifiers) in keys {
            if let Some(motion) = navigation::motion(key, modifiers, text_focus, ime) {
                self.navigate(motion);
                context.input_mut(|input| {
                    input.consume_key(modifiers, key);
                });
            }
        }
    }
}

impl eframe::App for DeadpanApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.receive();
        let context = ui.ctx().clone();
        let path_id = egui::Id::new(PATH_ID);
        let prior_text_focus = context.memory(|memory| memory.has_focus(path_id));
        let events = context.input(|input| input.events.clone());
        let ime_event = events
            .iter()
            .any(|event| matches!(event, egui::Event::Ime(_)));
        for event in &events {
            match event {
                egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) => {
                    self.ime_composing = !text.is_empty()
                }
                egui::Event::Ime(egui::ImeEvent::Commit(_)) => self.ime_composing = false,
                egui::Event::WindowFocused(false) => self.ime_composing = false,
                _ => {}
            }
        }
        let mut path_focused = prior_text_focus;
        let mut open_path = false;
        let mut leave_text = false;
        for event in events {
            if let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                repeat: false,
                ..
            } = event
            {
                if key == egui::Key::O
                    && modifiers.command
                    && !modifiers.alt
                    && !modifiers.shift
                    && !(modifiers.ctrl && modifiers.mac_cmd)
                {
                    // Establish focus before TextEdit processes this batch, so
                    // a fast Cmd+O, Cmd+A, paste sequence selects the old path.
                    context.memory_mut(|memory| memory.request_focus(path_id));
                    path_focused = true;
                    context.input_mut(|input| {
                        input.consume_key(modifiers, key);
                    });
                } else if let Some(action) = navigation::text_action(
                    key,
                    modifiers,
                    path_focused,
                    self.ime_composing || ime_event,
                ) {
                    open_path |= action == TextAction::Open;
                    leave_text = true;
                    context.input_mut(|input| {
                        input.consume_key(modifiers, key);
                    });
                }
            }
        }
        self.header(ui);
        // Apply typing/paste from this event batch before opening the path.
        if open_path {
            self.open();
        }
        if leave_text {
            context.memory_mut(|memory| memory.surrender_focus(path_id));
        }
        self.footer(ui);
        self.viewer(ui);
        self.keyboard(&context, prior_text_focus, ime_event);

        if let Some(frames) = self.smoke_frames.as_mut() {
            *frames += 1;
            if *frames >= 3 {
                context.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                context.request_repaint();
            }
        }
    }

    fn on_exit(&mut self) {
        self.worker.shutdown();
        self.forget_target();
        self.exited.set(true);
    }
}

impl Drop for DeadpanApp {
    fn drop(&mut self) {
        self.worker.shutdown();
        self.forget_target();
    }
}

fn target_size(size: egui::Vec2, pixels_per_point: f32) -> (u32, u32) {
    let width = f64::from(size.x.max(1.0)) * f64::from(pixels_per_point);
    let height = f64::from(size.y.max(1.0)) * f64::from(pixels_per_point);
    let scale = (MAX_TARGET_PIXELS / (width * height))
        .sqrt()
        .min(1.0)
        .min(2048.0 / width)
        .min(2048.0 / height);
    (
        (width * scale).floor().max(1.0) as u32,
        (height * scale).floor().max(1.0) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_target_bounds_large_retina_windows_without_changing_aspect() {
        let (width, height) = target_size(egui::vec2(6000.0, 4000.0), 2.0);
        assert!(u64::from(width) * u64::from(height) <= MAX_TARGET_PIXELS as u64);
        assert!(width <= 2048 && height <= 2048);
        assert!((f64::from(width) / f64::from(height) - 1.5).abs() < 0.002);
        assert_eq!(target_size(egui::Vec2::ZERO, 1.0), (1, 1));
    }
}
