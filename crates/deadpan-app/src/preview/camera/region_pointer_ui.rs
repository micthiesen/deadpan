//! Pointer admission and capture for the temporary target editor. The viewer
//! registers input during layout; only the final pass changes the draft.

use super::region_pointer::{Gesture, GestureKind, Projection};
use super::*;

#[derive(Clone, Debug, PartialEq)]
struct Basis {
    ticket: Ticket,
    painted: egui::Rect,
    raster: [u32; 2],
    layers: Vec<FramingLayer>,
}

struct Capture {
    basis: Basis,
    gesture: Gesture,
}

struct Pending {
    basis: Basis,
    projection: Projection,
    response: egui::Response,
    origin: Option<egui::Pos2>,
    position: Option<egui::Pos2>,
    down: bool,
}

#[derive(Default)]
pub(super) struct State {
    capture: Option<Capture>,
    pending: Option<Pending>,
    revoked: bool,
}

impl State {
    pub(super) fn revoke(&mut self) {
        self.capture = None;
        self.pending = None;
        self.revoked = true;
    }

    #[cfg(feature = "ui-harness")]
    pub(super) fn active(&self) -> bool {
        self.capture.is_some()
    }
}

impl DeadpanApp {
    pub(in super::super) fn camera_pointer_surface(
        &mut self,
        ui: &mut egui::Ui,
        painted: Option<egui::Rect>,
    ) {
        let Some(camera) = self.camera.as_mut() else {
            return;
        };
        camera.pointer.pending = None;
        let Some(painted) = painted else { return };
        if camera.blocked()
            || camera.draft.phase() != CameraPhase::Region
            || camera.picture.is_none()
            || self.help_open
            || ui.ctx().will_discard()
            || !ui.input(|input| {
                input.focused
                    && !input.events.iter().any(|event| {
                        matches!(
                            event,
                            egui::Event::PointerGone | egui::Event::WindowFocused(false)
                        )
                    })
            })
            || ui.ctx().memory(|memory| {
                memory
                    .top_modal_layer()
                    .is_some_and(|layer| layer != ui.layer_id())
            })
        {
            return;
        }
        let Ok(cursor) = i64::try_from(camera.cursor) else {
            return;
        };
        if self.presentation.stable_sequence_ticket(
            camera.session,
            &camera.revision,
            ProjectFrame(cursor),
        ) != Some(camera.ticket)
            || self.presentation.refining()
        {
            return;
        }
        let Some(picture) = self.presentation.picture() else {
            return;
        };
        let Some(frame) = &picture.frame else { return };
        let Some((width, height)) = picture.canvas else {
            return;
        };
        let Some(target) = &self.target else { return };
        let raster = [target.target.width(), target.target.height()];
        let Ok(layers) = render_layers(picture) else {
            return;
        };
        let Ok(geometry) = deadpan_render::PictureGeometry::composed(
            frame.metadata(),
            picture.picture_context.as_deref(),
            [width, height],
            raster,
            FitMode::Fit,
            &layers,
        ) else {
            return;
        };
        let Some(projection) = Projection::new(painted, geometry) else {
            return;
        };
        let response = ui.interact(
            painted,
            ui.id().with("camera-target-pointer"),
            egui::Sense::drag(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Image, true, "Target drawing surface")
        });
        if response.hovered() {
            let position = response.hover_pos();
            let region = camera.draft.region();
            let kind = position
                .and_then(|position| Gesture::start(&projection, region?.geometry(), position))
                .map(|gesture| gesture.kind());
            ui.ctx().set_cursor_icon(match kind {
                Some(GestureKind::Move) => egui::CursorIcon::Grab,
                Some(GestureKind::Resize) => {
                    let opposite_diagonal = position
                        .zip(region.and_then(|region| projection.screen_region(region.region())))
                        .is_some_and(|(position, rectangle)| {
                            let delta = position - rectangle.center();
                            delta.x * delta.y < 0.0
                        });
                    if opposite_diagonal {
                        egui::CursorIcon::ResizeNeSw
                    } else {
                        egui::CursorIcon::ResizeNwSe
                    }
                }
                Some(GestureKind::Draw) => egui::CursorIcon::Crosshair,
                None => egui::CursorIcon::Default,
            });
        }
        let (origin, position, down) = ui.input(|input| {
            (
                input.pointer.press_origin(),
                input.pointer.interact_pos(),
                input.pointer.button_down(egui::PointerButton::Primary),
            )
        });
        camera.pointer.pending = Some(Pending {
            basis: Basis {
                ticket: camera.ticket,
                painted,
                raster,
                layers,
            },
            projection,
            response,
            origin,
            position,
            down,
        });
    }

    pub(in super::super) fn finish_camera_pointer(&mut self, context: &egui::Context) {
        let Some(camera) = self.camera.as_mut() else {
            return;
        };
        let pending = camera.pointer.pending.take();
        let revoked = std::mem::take(&mut camera.pointer.revoked);
        let Some(pending) = pending.filter(|pending| {
            !revoked
                && !camera.blocked()
                && camera.draft.phase() == CameraPhase::Region
                && pending.response.enabled()
                && context.input(|input| input.focused)
                && context.memory(|memory| {
                    memory
                        .top_modal_layer()
                        .is_none_or(|layer| layer == pending.response.layer_id)
                })
        }) else {
            camera.pointer.capture = None;
            return;
        };
        if camera
            .pointer
            .capture
            .as_ref()
            .is_some_and(|capture| capture.basis != pending.basis)
        {
            // A resize or picture/pose replacement cannot rebind a held drag.
            camera.pointer.capture = None;
        }
        if pending
            .response
            .drag_started_by(egui::PointerButton::Primary)
            && let Some(origin) = pending.origin
            && let Some(region) = camera.draft.region()
            && let Some(gesture) = Gesture::start(&pending.projection, region.geometry(), origin)
        {
            camera.draft.clear_count();
            camera.pointer.capture = Some(Capture {
                basis: pending.basis,
                gesture,
            });
            self.pane = Pane::Viewer;
            context.memory_mut(|memory| memory.request_focus(pane_id(Pane::Viewer)));
        }
        if let Some(capture) = &camera.pointer.capture
            && let Some(position) = pending.position
            && (pending.down
                || pending
                    .response
                    .drag_stopped_by(egui::PointerButton::Primary))
            && let Some(geometry) = capture.gesture.update(position)
        {
            match camera.draft.set_region_geometry(geometry) {
                CameraEffect::RegionChanged(_) => self.error = None,
                CameraEffect::Rejected(error) => self.error = Some(error.to_string()),
                _ => {}
            }
            context.request_repaint();
        }
        if !pending.down {
            camera.pointer.capture = None;
        }
    }
}
