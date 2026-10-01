//! Source endpoint pairs use the same composition and SDR renderer as the viewer.

use super::*;
use crate::worker::{EndpointPictures, EndpointReply, EndpointSourceId};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Accepted {
    identity: EndpointIdentity,
    raster: (u32, u32),
    canvas: Option<(u32, u32)>,
    background: bool,
}

#[derive(Default)]
struct PairState {
    accepted: [Option<Accepted>; 2],
}

impl PairState {
    fn ready(&self, expected: &EndpointIdentity) -> bool {
        self.accepted
            .iter()
            .all(|slot| slot.as_ref().is_some_and(|slot| &slot.identity == expected))
    }

    fn needs_submission(
        &self,
        slot: usize,
        expected: &EndpointIdentity,
        raster: (u32, u32),
    ) -> bool {
        self.accepted[slot].as_ref().is_none_or(|accepted| {
            &accepted.identity != expected || (!accepted.background && accepted.raster != raster)
        })
    }
}

pub(super) struct Display {
    state: egui_wgpu::RenderState,
    pictures: Option<EndpointPictures>,
    identity: Option<EndpointIdentity>,
    targets: [Option<RegisteredTarget>; 2],
    pair: PairState,
    error: Option<String>,
}

impl Display {
    #[cfg(feature = "ui-harness")]
    pub(super) fn empty_for_check(&self) -> bool {
        self.identity.is_none()
            && self.pictures.is_none()
            && self.targets.iter().all(Option::is_none)
    }

    pub fn new(state: egui_wgpu::RenderState) -> Self {
        Self {
            state,
            pictures: None,
            identity: None,
            targets: [None, None],
            pair: PairState::default(),
            error: None,
        }
    }

    pub fn receive(&mut self, reply: EndpointReply) {
        self.identity = Some(reply.identity);
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
        let size = egui::vec2(
            ui.available_width().min(216.0),
            ((height - 120.0) / 2.0).clamp(48.0, 92.0),
        );
        // Footer retries must never submit a discarded viewport's dimensions.
        if self.identity.as_ref() == Some(expected) && !ui.ctx().will_discard() {
            self.prepare(ui.ctx(), renderer, expected, size);
        }
        let current_error = current_error(self.identity.as_ref(), expected, self.error.as_deref());
        let ready = self.pair.ready(expected);
        for (slot, title) in [(0, "First included"), (1, "Last included")] {
            let label = if ready {
                endpoint_label(expected, slot, title)
            } else {
                title.into()
            };
            ui.label(&label);
            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
            ui.painter().rect_filled(rect, 3.0, egui::Color32::BLACK);
            let accessible = if ready {
                let accepted = self.pair.accepted[slot].as_ref().expect("ready pair");
                if let Some(target) = &self.targets[slot] {
                    ui.painter().image(
                        target.texture,
                        picture_rect(rect, accepted.canvas),
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }
                endpoint_accessibility_label(expected, slot, title)
            } else {
                let text = if current_error.is_some() {
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
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Image, true, &accessible)
            });
        }
        if let Some(error) = current_error {
            ui.colored_label(style::LAVENDER, error);
        }
    }

    fn prepare(
        &mut self,
        context: &egui::Context,
        renderer: &mut PictureRenderer,
        expected: &EndpointIdentity,
        size: egui::Vec2,
    ) {
        for slot in 0..2 {
            let Some(pair) = &self.pictures else { return };
            let picture = if slot == 0 { &pair.first } else { &pair.last };
            let rect = picture_rect(
                egui::Rect::from_min_size(egui::Pos2::ZERO, size),
                picture.canvas,
            );
            let raster = target_size(rect.size(), context.pixels_per_point());
            if !self.pair.needs_submission(slot, expected, raster) {
                continue;
            }
            let accepted = Accepted {
                identity: expected.clone(),
                raster,
                canvas: picture.canvas,
                background: picture.frame.is_none(),
            };
            // Authored Background is an explicit successful black transition.
            // Decode failures cannot reach this branch.
            let Some(frame) = picture.frame.as_ref() else {
                self.forget_target(slot);
                self.pair.accepted[slot] = Some(accepted);
                self.error = None;
                continue;
            };
            match renderer.is_idle() {
                Ok(true) => {}
                Ok(false) => {
                    context.request_repaint_after(Duration::from_millis(16));
                    return;
                }
                Err(error) => {
                    self.error = Some(error.to_string());
                    return;
                }
            }
            let target = match renderer.create_target(raster.0, raster.1) {
                Ok(target) => target,
                Err(error) => {
                    self.error = Some(error.to_string());
                    return;
                }
            };
            let result = if let Some((width, height)) = picture.canvas {
                match camera::render_layers(picture) {
                    Ok(layers) => renderer.render_composed(
                        frame,
                        &target,
                        picture.picture_context.as_deref(),
                        [width, height],
                        FitMode::Fit,
                        &layers,
                    ),
                    Err(error) => {
                        self.error = Some(error);
                        return;
                    }
                }
            } else {
                renderer.render(frame, &target, FitMode::Fit)
            };
            match result {
                Ok(_) => {
                    let texture = self.state.renderer.write().register_native_texture(
                        &self.state.device,
                        target.display_view(),
                        eframe::wgpu::FilterMode::Linear,
                    );
                    // The old registration and accepted clock survive every
                    // allocation/composition/submission failure, including resize.
                    self.forget_target(slot);
                    self.targets[slot] = Some(RegisteredTarget { target, texture });
                    self.pair.accepted[slot] = Some(accepted);
                    self.error = None;
                    context.request_repaint_after(Duration::from_millis(16));
                }
                Err(error) => self.error = Some(error.to_string()),
            }
            // Main viewer and endpoints share one renderer. Submit at most one
            // picture here, even if a very fast GPU already reports completion.
            return;
        }
    }

    fn forget_target(&mut self, slot: usize) {
        if let Some(old) = self.targets[slot].take() {
            self.state.renderer.write().free_texture(&old.texture);
        }
    }
}

fn current_error<'a>(
    decoded: Option<&EndpointIdentity>,
    expected: &EndpointIdentity,
    error: Option<&'a str>,
) -> Option<&'a str> {
    (decoded == Some(expected)).then_some(error).flatten()
}

fn picture_rect(rect: egui::Rect, canvas: Option<(u32, u32)>) -> egui::Rect {
    canvas.map_or(rect, |(width, height)| {
        fit_rect(rect, width as f32 / height as f32)
    })
}

fn endpoint_coordinate(identity: &EndpointIdentity, slot: usize) -> (&'static str, i128) {
    match &identity.source {
        EndpointSourceId::Original {
            in_frame,
            out_frame,
            ..
        } => {
            let frame = if slot == 0 {
                in_frame.0
            } else {
                out_frame.0.saturating_sub(1)
            };
            ("Original", i128::from(frame) + 1)
        }
        EndpointSourceId::Copied(id) => {
            let frame = if slot == 0 {
                id.range.start().0
            } else {
                id.range.end().0 - 1
            };
            ("copied Edit", i128::from(frame) + 1)
        }
    }
}

fn endpoint_label(identity: &EndpointIdentity, slot: usize, title: &str) -> String {
    let (domain, frame) = endpoint_coordinate(identity, slot);
    format!("{title} · {domain} frame {frame}")
}

fn endpoint_accessibility_label(identity: &EndpointIdentity, slot: usize, title: &str) -> String {
    let (domain, frame) = endpoint_coordinate(identity, slot);
    format!("{title} {domain} picture, frame {frame}")
}

impl Drop for Display {
    fn drop(&mut self) {
        for slot in 0..2 {
            self.forget_target(slot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::slice::{CopiedViewId, CopyId};
    use deadpan_core::{FrameRange, NodeId, ProjectId};

    fn identity(change: u64) -> EndpointIdentity {
        let revision = RevisionId::new("historical").unwrap();
        let project = ProjectId::new("endpoint-display").unwrap();
        EndpointIdentity {
            session: 1,
            project: project.clone(),
            revision: RevisionId::new("destination").unwrap(),
            draft: 2,
            change,
            source: EndpointSourceId::Copied(CopiedViewId {
                copy: CopyId {
                    session: 1,
                    project,
                    source_revision: revision,
                    request: 3,
                },
                parent: NodeId::new("owner").unwrap(),
                range: FrameRange::new(ProjectFrame(13), ProjectFrame(20)).unwrap(),
            }),
        }
    }

    fn accepted(identity: &EndpointIdentity, background: bool) -> Accepted {
        Accepted {
            identity: identity.clone(),
            raster: (100, 60),
            canvas: Some((101, 61)),
            background,
        }
    }

    #[test]
    fn labels_wait_for_both_clocks_and_background_completes_a_pair() {
        let first = identity(1);
        let next = identity(2);
        let mut state = PairState::default();
        state.accepted[0] = Some(accepted(&first, false));
        assert!(!state.ready(&first));
        state.accepted[1] = Some(accepted(&first, true));
        assert!(state.ready(&first));
        assert!(!state.needs_submission(1, &first, (200, 120)));
        state.accepted[0] = Some(accepted(&next, false));
        assert!(
            !state.ready(&next),
            "an old Out cannot accompany the new In"
        );
        assert!(!state.ready(&first));
        state.accepted[1] = Some(accepted(&next, true));
        assert!(state.ready(&next));
        assert_eq!(
            endpoint_label(&next, 0, "First included"),
            "First included · copied Edit frame 14"
        );
        assert_eq!(
            endpoint_label(&next, 1, "Last included"),
            "Last included · copied Edit frame 20"
        );
    }

    #[test]
    fn prior_source_error_cannot_label_the_refined_range_unavailable() {
        let before = identity(1);
        let refined = identity(2);
        assert_eq!(
            current_error(Some(&before), &refined, Some("old source failed")),
            None
        );
        assert_eq!(
            current_error(Some(&refined), &refined, Some("current source failed")),
            Some("current source failed")
        );
        assert_eq!(current_error(Some(&refined), &refined, None), None);
    }

    #[test]
    fn failed_resize_preserves_the_accepted_pair_and_odd_canvas_aspect() {
        let id = identity(1);
        let state = PairState {
            accepted: [Some(accepted(&id, false)), Some(accepted(&id, true))],
        };
        assert!(state.needs_submission(0, &id, (200, 120)));
        // Failed replacement does not call the success transition.
        assert!(state.ready(&id));
        assert_eq!(state.accepted[0].as_ref().unwrap().raster, (100, 60));
        let rect = picture_rect(
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(216.0, 92.0)),
            Some((101, 61)),
        );
        assert!((rect.width() / rect.height() - 101.0 / 61.0).abs() < 0.00001);
        let raster = target_size(rect.size(), 2.0);
        assert_eq!(raster.1, 184);
        assert!((raster.0 as f32 / raster.1 as f32 - 101.0 / 61.0).abs() < 0.01);
    }
}
