//! Two source pictures use the same SDR renderer as the main viewer.

use super::*;
use crate::worker::{EndpointPictures, EndpointReply, Picture};

pub(super) struct Display {
    state: egui_wgpu::RenderState,
    pictures: Option<EndpointPictures>,
    identity: Option<EndpointIdentity>,
    targets: [Option<RegisteredTarget>; 2],
    submitted: [bool; 2],
    error: Option<String>,
}

impl Display {
    pub fn new(state: egui_wgpu::RenderState) -> Self {
        Self {
            state,
            pictures: None,
            identity: None,
            targets: [None, None],
            submitted: [false; 2],
            error: None,
        }
    }

    pub fn receive(&mut self, reply: EndpointReply) {
        self.identity = Some(reply.identity);
        self.submitted = [false; 2];
        match reply.pictures {
            Ok(pictures) => {
                self.pictures = Some(pictures);
                self.error = None;
            }
            Err(error) => {
                self.pictures = None;
                self.error = Some(error);
            }
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        renderer: &mut PictureRenderer,
        expected: &EndpointIdentity,
        height: f32,
    ) {
        let current = self.identity.as_ref() == Some(expected);
        let size = egui::vec2(
            ui.available_width().min(216.0),
            ((height - 120.0) / 2.0).clamp(48.0, 92.0),
        );
        let (width, height) = target_size(size, ui.ctx().pixels_per_point());
        for slot in 0..2 {
            if self.targets[slot].as_ref().is_some_and(|target| {
                target.target.width() != width || target.target.height() != height
            }) {
                self.submitted[slot] = false;
            }
        }
        if current && !ui.ctx().will_discard() {
            for slot in 0..2 {
                if self.submitted[slot] {
                    continue;
                }
                match renderer.is_idle() {
                    Ok(true) => {}
                    Ok(false) => {
                        ui.ctx().request_repaint_after(Duration::from_millis(16));
                        break;
                    }
                    Err(error) => {
                        self.error = Some(error.to_string());
                        break;
                    }
                }
                let picture = self
                    .pictures
                    .as_ref()
                    .map(|pair| if slot == 0 { &pair.first } else { &pair.last });
                let Some(Picture {
                    frame: Some(frame), ..
                }) = picture
                else {
                    break;
                };
                let target = match renderer.create_target(width, height) {
                    Ok(target) => target,
                    Err(error) => {
                        self.error = Some(error.to_string());
                        break;
                    }
                };
                match renderer.render(frame, &target, FitMode::Fit) {
                    Ok(_) => {
                        let texture = self.state.renderer.write().register_native_texture(
                            &self.state.device,
                            target.display_view(),
                            eframe::wgpu::FilterMode::Linear,
                        );
                        if let Some(old) =
                            self.targets[slot].replace(RegisteredTarget { target, texture })
                        {
                            self.state.renderer.write().free_texture(&old.texture);
                        }
                        self.submitted[slot] = true;
                        ui.ctx().request_repaint_after(Duration::from_millis(16));
                    }
                    Err(error) => {
                        self.error = Some(error.to_string());
                        break;
                    }
                }
            }
        }
        for (slot, title, frame) in [
            (0, "First included", expected.in_frame.0),
            (1, "Last included", expected.out_frame.0.saturating_sub(1)),
        ] {
            ui.label(format!(
                "{title} · Original frame {}",
                u128::from(frame) + 1
            ));
            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
            ui.painter().rect_filled(rect, 3.0, egui::Color32::BLACK);
            let ready = current
                && self.submitted.iter().all(|submitted| *submitted)
                && self.error.is_none();
            let label = if ready {
                if let Some(target) = &self.targets[slot] {
                    ui.painter().image(
                        target.texture,
                        rect,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }
                format!("{title} Original picture, frame {}", u128::from(frame) + 1)
            } else {
                let text = if self.error.is_some() {
                    "Picture unavailable"
                } else {
                    "Preparing endpoint…"
                };
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    text,
                    egui::FontId::proportional(12.0),
                    style::MUTED,
                );
                text.into()
            };
            response
                .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, &label));
        }
        if let Some(error) = &self.error {
            ui.colored_label(style::LAVENDER, error);
        }
    }
}

impl Drop for Display {
    fn drop(&mut self) {
        for target in self.targets.iter_mut().filter_map(Option::take) {
            self.state.renderer.write().free_texture(&target.texture);
        }
    }
}
