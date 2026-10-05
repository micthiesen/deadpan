//! Source endpoint pairs use the same composition and SDR renderer as the viewer.

use super::*;
use crate::worker::{
    EditJunctionIdentity, EditJunctionPicture, EditJunctionPictures, EditJunctionReply,
    EndpointPictures, EndpointReply, EndpointSourceId, JunctionExterior,
};

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
                    style::muted(ui),
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct PairKey {
    identity: EditJunctionIdentity,
    raster: [(u32, u32); 2],
}

struct AcceptedJunctionPair<S> {
    key: PairKey,
    slots: [S; 2],
}

struct StagedJunctionPair<S> {
    key: PairKey,
    slots: [Option<S>; 2],
    gpu_pending: bool,
}

enum BeginJunctionStage<S> {
    Current,
    // These are only ownership handoffs when replacing a partial pair. Keep
    // the control enum small even when each slot owns a registered GPU target.
    Started(Option<Box<StagedJunctionPair<S>>>),
    Busy,
}

enum JunctionPairCommit<S> {
    NotReady,
    // Return the previous accepted pair for deferred texture unregistration.
    Published(Option<Box<AcceptedJunctionPair<S>>>),
}

enum JunctionPreparation<S> {
    Pending,
    Ready {
        accepted: bool,
        discarded: Option<Box<StagedJunctionPair<S>>>,
    },
}

struct ClearedJunctionPairs<S> {
    accepted: Option<AcceptedJunctionPair<S>>,
    staged: Option<StagedJunctionPair<S>>,
}

/// One atomic displayed pair and one bounded replacement pair. The staging
/// permit stays occupied until the last submitted slot has drained.
struct JunctionPairState<S> {
    accepted: Option<AcceptedJunctionPair<S>>,
    staged: Option<StagedJunctionPair<S>>,
}

impl<S> Default for JunctionPairState<S> {
    fn default() -> Self {
        Self {
            accepted: None,
            staged: None,
        }
    }
}

impl<S> JunctionPairState<S> {
    fn begin(&mut self, key: PairKey) -> BeginJunctionStage<S> {
        if self.staged.as_ref().is_some_and(|staged| staged.key == key) {
            return BeginJunctionStage::Current;
        }
        if self
            .staged
            .as_ref()
            .is_some_and(|staged| staged.gpu_pending)
        {
            return BeginJunctionStage::Busy;
        }
        let old = self.staged.take();
        self.staged = Some(StagedJunctionPair {
            key,
            slots: [None, None],
            gpu_pending: false,
        });
        BeginJunctionStage::Started(old.map(Box::new))
    }

    fn stage_key(&self) -> Option<&PairKey> {
        self.staged.as_ref().map(|staged| &staged.key)
    }

    fn has_gpu_pending(&self) -> bool {
        self.staged
            .as_ref()
            .is_some_and(|staged| staged.gpu_pending)
    }

    fn slot_ready(&self, slot: usize) -> bool {
        self.staged
            .as_ref()
            .is_some_and(|staged| staged.slots[slot].is_some())
    }

    fn stage_slot(&mut self, slot: usize, value: S, gpu_pending: bool) {
        let staged = self.staged.as_mut().expect("junction pair was begun");
        staged.slots[slot] = Some(value);
        staged.gpu_pending |= gpu_pending;
    }

    fn gpu_drained(&mut self) {
        if let Some(staged) = &mut self.staged {
            staged.gpu_pending = false;
        }
    }

    fn discard_stage(&mut self) -> Option<StagedJunctionPair<S>> {
        if self.has_gpu_pending() {
            None
        } else {
            self.staged.take()
        }
    }

    fn accepted(&self) -> Option<&AcceptedJunctionPair<S>> {
        self.accepted.as_ref()
    }

    fn clear(&mut self) -> ClearedJunctionPairs<S> {
        ClearedJunctionPairs {
            accepted: self.accepted.take(),
            staged: self.discard_stage(),
        }
    }

    fn ready_for(&self, key: &PairKey) -> bool {
        self.accepted
            .as_ref()
            .is_some_and(|accepted| &accepted.key == key)
    }

    /// Drain before considering the already-accepted fast path. In particular,
    /// returning to the old raster must retire an in-flight resize replacement.
    fn prepare_key<E>(
        &mut self,
        key: &PairKey,
        poll: impl FnOnce() -> Result<bool, E>,
    ) -> Result<JunctionPreparation<S>, E> {
        if self.has_gpu_pending() {
            if !poll()? {
                return Ok(JunctionPreparation::Pending);
            }
            self.gpu_drained();
        }
        let discarded = if self.stage_key().is_some_and(|staged| staged != key) {
            self.discard_stage().map(Box::new)
        } else {
            None
        };
        Ok(JunctionPreparation::Ready {
            accepted: self.ready_for(key),
            discarded,
        })
    }

    fn commit(&mut self) -> JunctionPairCommit<S> {
        let Some(staged) = self.staged.as_ref() else {
            return JunctionPairCommit::NotReady;
        };
        if staged.gpu_pending || staged.slots.iter().any(Option::is_none) {
            return JunctionPairCommit::NotReady;
        }
        let staged = self.staged.take().expect("complete junction stage");
        let [Some(outgoing), Some(incoming)] = staged.slots else {
            unreachable!("all staged junction slots were checked")
        };
        JunctionPairCommit::Published(
            self.accepted
                .replace(AcceptedJunctionPair {
                    key: staged.key,
                    slots: [outgoing, incoming],
                })
                .map(Box::new),
        )
    }
}

struct RegisteredJunctionTarget {
    // The UI registration and queue callback each retain this allocation. Only
    // the UI owner unregisters the texture; the callback never takes its lock.
    _target: Arc<RenderTarget>,
    texture: egui::TextureId,
    source: JunctionSourceEvidence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct JunctionSourceEvidence {
    ordinal: deadpan_core::SourceFrameId,
    pts: deadpan_core::SourceTimestamp,
}

impl JunctionSourceEvidence {
    fn label(self) -> String {
        format!(
            "source ordinal {} · PTS {} × {}/{} s",
            self.ordinal.0,
            self.pts.ticks,
            self.pts.time_base.numerator(),
            self.pts.time_base.denominator(),
        )
    }
}

enum JunctionVisual {
    Exterior(JunctionExterior),
    Background,
    Texture(RegisteredJunctionTarget, (u32, u32)),
}

/// Paired presentation owned by the app, independently of a logical draft.
/// Drain once per outer frame even while hidden. Every submitted target also
/// has a queue completion owner, so final app drop cannot release it in flight.
pub(in crate::preview) struct JunctionDisplay {
    state: egui_wgpu::RenderState,
    pair: JunctionPairState<JunctionVisual>,
    expected: Option<EditJunctionIdentity>,
    reply: Option<EditJunctionReply>,
    worker_error: Option<(EditJunctionIdentity, String)>,
    render_error: Option<(PairKey, String)>,
    required_key: Option<PairKey>,
}

impl JunctionDisplay {
    #[cfg(feature = "ui-harness")]
    pub(in crate::preview) fn displayed_identity_for_check(&self) -> Option<&EditJunctionIdentity> {
        self.pair.accepted().map(|pair| &pair.key.identity)
    }

    #[cfg(feature = "ui-harness")]
    pub(in crate::preview) fn state_for_check(&self) -> serde_json::Value {
        use serde_json::json;
        let slot = |visual: &JunctionVisual| match visual {
            JunctionVisual::Exterior(edge) => json!({"exterior":format!("{edge:?}")}),
            JunctionVisual::Background => json!({"background":true}),
            JunctionVisual::Texture(target, _) => json!({
                "ordinal":target.source.ordinal.0,
                "pts":target.source.pts.ticks,
                "time_base":[target.source.pts.time_base.numerator(), target.source.pts.time_base.denominator()],
            }),
        };
        json!({
            "expected":self.expected.as_ref().map(|identity| format!("{identity:?}")),
            "required_raster":self.required_key.as_ref().map(|key| key.raster),
            "gpu_pending":self.gpu_work_pending(),
            "displayed":self.pair.accepted().map(|pair| json!({
                "identity":format!("{:?}", pair.key.identity), "raster":pair.key.raster,
                "slots":[slot(&pair.slots[0]),slot(&pair.slots[1])],
            })),
            "worker_error":self.worker_error.as_ref().map(|(_,error)|error),
            "render_error":self.render_error.as_ref().map(|(_,error)|error),
        })
    }

    pub(in crate::preview) fn new(state: egui_wgpu::RenderState) -> Self {
        Self {
            state,
            pair: JunctionPairState::default(),
            expected: None,
            reply: None,
            worker_error: None,
            render_error: None,
            required_key: None,
        }
    }

    /// Capture the new logical target before submitting work. A decoded reply
    /// from a prior request can no longer replace the current pair afterward.
    pub(in crate::preview) fn expect(&mut self, identity: EditJunctionIdentity) {
        if self.expected.as_ref() == Some(&identity) {
            return;
        }
        self.expected = Some(identity);
        self.reply = None;
        self.worker_error = None;
        self.render_error = None;
        self.required_key = None;
    }

    pub(in crate::preview) fn receive(&mut self, reply: EditJunctionReply) -> bool {
        if !junction_reply_is_current(self.expected.as_ref(), &reply) {
            return false;
        }
        self.worker_error = reply
            .pictures
            .as_ref()
            .err()
            .map(|error| (reply.identity.clone(), error.clone()));
        if reply.pictures.is_ok() {
            self.render_error = None;
        }
        self.reply = Some(reply);
        true
    }

    pub(in crate::preview) fn invalidate(&mut self) {
        self.expected = None;
        self.reply = None;
        self.worker_error = None;
        self.render_error = None;
        self.required_key = None;
    }

    /// End a logical draft or project context without showing its pictures in
    /// the next one. A pending stage stays owned until completion is observed.
    pub(in crate::preview) fn clear(&mut self) {
        self.invalidate();
        let cleared = self.pair.clear();
        if let Some(pair) = cleared.accepted {
            free_junction_pair(&self.state, pair);
        }
        if let Some(pair) = cleared.staged {
            free_staged_junction_pair(&self.state, pair);
        }
    }

    #[cfg(feature = "ui-harness")]
    pub(in crate::preview) fn gpu_work_pending(&self) -> bool {
        self.pair.has_gpu_pending()
    }

    pub(in crate::preview) fn ready_for_apply(&self, expected: &EditJunctionIdentity) -> bool {
        let Some(key) = &self.required_key else {
            return false;
        };
        if &key.identity != expected || !self.pair.ready_for(key) {
            return false;
        }
        self.worker_error
            .as_ref()
            .is_none_or(|(identity, _)| identity != expected)
            && self
                .render_error
                .as_ref()
                .is_none_or(|(failed, _)| failed != key)
    }

    /// Drain cancelled/inapplicable submissions even when the junction panel is
    /// temporarily hidden. This never starts a new allocation or render.
    pub(in crate::preview) fn drain(
        &mut self,
        renderer: &mut PictureRenderer,
        context: &egui::Context,
    ) {
        if !self.pair.has_gpu_pending() {
            self.discard_obsolete_stage();
            return;
        }
        match renderer.is_idle() {
            Ok(true) => {
                self.pair.gpu_drained();
                self.discard_obsolete_stage();
                context.request_repaint();
            }
            Ok(false) => context.request_repaint_after(Duration::from_millis(16)),
            Err(error) => {
                if let Some(staged) = self.pair.stage_key().cloned() {
                    self.render_error = Some((staged, error.to_string()));
                }
            }
        }
    }

    pub(in crate::preview) fn show(
        &mut self,
        ui: &mut egui::Ui,
        renderer: &mut PictureRenderer,
        expected: &EditJunctionIdentity,
        height: f32,
    ) {
        self.expect(expected.clone());
        let layout = allocate_junction_layout(ui, height);
        if !ui.ctx().will_discard() && layout.pictures_visible(ui.clip_rect()) {
            self.prepare(ui.ctx(), renderer, expected, layout.pictures[0].size());
        } else {
            self.required_key = None;
            // A zero-size allocation would be rounded up to a GPU texture.
            // That texture cannot authorize Apply while the pair is hidden.
            self.finish_or_discard_pending_stage(renderer, ui.ctx(), expected);
        }
        self.paint_pair(
            ui,
            &layout,
            self.current_error(expected),
            "Preparing junction…",
        );
    }

    /// Paint the complete accepted pair while the next inspection is not ready.
    /// This grants no current-raster Apply authority and starts no preparation.
    pub(in crate::preview) fn show_retained(&mut self, ui: &mut egui::Ui, height: f32) {
        self.required_key = None;
        let layout = allocate_junction_layout(ui, height);
        self.paint_pair(ui, &layout, None, "No accepted junction picture");
    }

    fn paint_pair(
        &self,
        ui: &mut egui::Ui,
        layout: &JunctionLayout,
        error: Option<&str>,
        pending: &str,
    ) {
        for (slot, title) in [(0, "Outgoing"), (1, "Incoming")] {
            let accepted = self.pair.accepted();
            let caption = accepted
                .map(|accepted| {
                    let caption = junction_caption(&accepted.key.identity, slot);
                    match &accepted.slots[slot] {
                        JunctionVisual::Texture(target, _) => {
                            format!(
                                "{caption} · Original {}",
                                u128::from(target.source.ordinal.0) + 1
                            )
                        }
                        _ => caption,
                    }
                })
                .unwrap_or_else(|| format!("{title} junction"));
            let caption_color = ui.visuals().text_color();
            junction_label(ui, layout.captions[slot], &caption, caption_color);
            let rect = layout.pictures[slot];
            let response = ui.interact(
                rect,
                ui.id().with(("junction-picture", slot)),
                egui::Sense::hover(),
            );
            ui.painter().rect_filled(rect, 3.0, egui::Color32::BLACK);
            let accessible = if let Some(accepted) = accepted {
                paint_junction_visual(ui, rect, &accepted.slots[slot]);
                let detail = match &accepted.slots[slot] {
                    JunctionVisual::Exterior(_) => " no picture exists outside the Edit",
                    JunctionVisual::Background => " authored Background picture",
                    JunctionVisual::Texture(_, _) => " decoded picture",
                };
                format!("{caption},{detail}")
            } else {
                let text = if error.is_some() {
                    "Picture unavailable"
                } else {
                    pending
                };
                ui.painter_at(rect).text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    text,
                    egui::FontId::proportional(13.0),
                    style::muted(ui),
                );
                text.to_owned()
            };
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Image, true, &accessible)
            });
            if let Some(accepted) = accepted {
                let source = match &accepted.slots[slot] {
                    JunctionVisual::Texture(target, _) => format!("\n{}", target.source.label()),
                    _ => String::new(),
                };
                response.on_hover_text(format!(
                    "Frame numbers start at 1. Edit cut boundary: {} (zero-based).{source}",
                    accepted.key.identity.boundary.0,
                ));
            }
        }
        // Reserve this row even without an error, so readiness/error text never
        // changes the next frame's requested picture raster.
        junction_label(ui, layout.error, error.unwrap_or(""), style::LAVENDER);
    }

    fn current_error(&self, expected: &EditJunctionIdentity) -> Option<&str> {
        self.worker_error
            .as_ref()
            .filter(|(identity, _)| identity == expected)
            .map(|(_, error)| error.as_str())
            .or_else(|| {
                self.render_error
                    .as_ref()
                    .filter(|(key, _)| {
                        &key.identity == expected && self.required_key.as_ref() == Some(key)
                    })
                    .map(|(_, error)| error.as_str())
            })
    }

    fn prepare(
        &mut self,
        context: &egui::Context,
        renderer: &mut PictureRenderer,
        expected: &EditJunctionIdentity,
        size: egui::Vec2,
    ) {
        let Some(reply) = self
            .reply
            .as_ref()
            .filter(|reply| &reply.identity == expected)
        else {
            self.required_key = None;
            self.finish_or_discard_pending_stage(renderer, context, expected);
            return;
        };
        let Ok(pictures) = reply.pictures.as_ref() else {
            self.required_key = None;
            self.finish_or_discard_pending_stage(renderer, context, expected);
            return;
        };
        let rasters = [
            junction_raster(context, size, pictures, 0),
            junction_raster(context, size, pictures, 1),
        ];
        let key = PairKey {
            identity: expected.clone(),
            raster: rasters,
        };
        self.required_key = Some(key.clone());
        if self
            .render_error
            .as_ref()
            .is_some_and(|(failed, _)| failed.identity == key.identity && failed != &key)
        {
            self.render_error = None;
        }
        if self
            .render_error
            .as_ref()
            .is_some_and(|(failed, _)| failed == &key)
        {
            self.finish_or_discard_pending_stage(renderer, context, expected);
            return;
        }
        // Keep the decoded reply for resize, but service pending staging work
        // before reusing an already accepted identity/raster.
        match self.pair.prepare_key(&key, || renderer.is_idle()) {
            Ok(JunctionPreparation::Pending) => {
                context.request_repaint_after(Duration::from_millis(16));
                return;
            }
            Ok(JunctionPreparation::Ready {
                accepted,
                discarded,
            }) => {
                if let Some(staged) = discarded {
                    free_staged_junction_pair(&self.state, *staged);
                }
                if accepted {
                    return;
                }
            }
            Err(error) => {
                self.render_error = Some((key, error.to_string()));
                return;
            }
        }
        match self.pair.begin(key.clone()) {
            BeginJunctionStage::Current => {}
            BeginJunctionStage::Started(Some(old)) => free_staged_junction_pair(&self.state, *old),
            BeginJunctionStage::Started(None) => {}
            BeginJunctionStage::Busy => {
                context.request_repaint_after(Duration::from_millis(16));
                return;
            }
        }

        for slot in 0..2 {
            if self.pair.slot_ready(slot) {
                continue;
            }
            let picture = junction_picture(pictures, slot);
            let canvas = match picture {
                EditJunctionPicture::Exterior(_) => pictures.canvas,
                EditJunctionPicture::Picture(picture) => picture.canvas.unwrap_or(pictures.canvas),
            };
            let visual = match picture {
                EditJunctionPicture::Exterior(edge) => JunctionVisual::Exterior(*edge),
                EditJunctionPicture::Picture(picture) if picture.frame.is_none() => {
                    JunctionVisual::Background
                }
                EditJunctionPicture::Picture(picture) => {
                    match renderer.is_idle() {
                        Ok(true) => {}
                        Ok(false) => {
                            context.request_repaint_after(Duration::from_millis(16));
                            return;
                        }
                        Err(error) => {
                            fail_junction_stage(
                                &self.state,
                                &mut self.pair,
                                &mut self.render_error,
                                key,
                                error.to_string(),
                            );
                            return;
                        }
                    }
                    let rect = picture_rect(
                        egui::Rect::from_min_size(egui::Pos2::ZERO, size),
                        Some(canvas),
                    );
                    let raster = target_size(rect.size(), context.pixels_per_point());
                    if raster != key.raster[slot] {
                        fail_junction_stage(
                            &self.state,
                            &mut self.pair,
                            &mut self.render_error,
                            key,
                            "Junction raster changed during one layout pass.".into(),
                        );
                        return;
                    }
                    let target = match renderer.create_target(raster.0, raster.1) {
                        Ok(target) => Arc::new(target),
                        Err(error) => {
                            fail_junction_stage(
                                &self.state,
                                &mut self.pair,
                                &mut self.render_error,
                                key,
                                error.to_string(),
                            );
                            return;
                        }
                    };
                    let rendered = if let Some((width, height)) = picture.canvas {
                        match camera::render_layers(picture) {
                            Ok(layers) => renderer.render_composed(
                                picture.frame.as_ref().expect("source frame checked"),
                                &target,
                                picture.picture_context.as_deref(),
                                [width, height],
                                FitMode::Fit,
                                &layers,
                            ),
                            Err(error) => {
                                fail_junction_stage(
                                    &self.state,
                                    &mut self.pair,
                                    &mut self.render_error,
                                    key,
                                    error,
                                );
                                return;
                            }
                        }
                    } else {
                        renderer.render(
                            picture.frame.as_ref().expect("source frame checked"),
                            &target,
                            FitMode::Fit,
                        )
                    };
                    if let Err(error) = rendered {
                        fail_junction_stage(
                            &self.state,
                            &mut self.pair,
                            &mut self.render_error,
                            key,
                            error.to_string(),
                        );
                        return;
                    }
                    // Register retention immediately after the actual submit.
                    // Display/app drop may unregister the UI texture, but this
                    // queue-owned Arc holds both GPU targets until completion.
                    let submitted_target = Arc::clone(&target);
                    self.state
                        .queue
                        .on_submitted_work_done(move || drop(submitted_target));
                    let texture = self.state.renderer.write().register_native_texture(
                        &self.state.device,
                        target.display_view(),
                        eframe::wgpu::FilterMode::Linear,
                    );
                    self.pair.stage_slot(
                        slot,
                        JunctionVisual::Texture(
                            RegisteredJunctionTarget {
                                _target: target,
                                texture,
                                source: JunctionSourceEvidence {
                                    ordinal: picture.id,
                                    pts: picture
                                        .frame
                                        .as_ref()
                                        .expect("source frame checked")
                                        .metadata()
                                        .pts,
                                },
                            },
                            canvas,
                        ),
                        true,
                    );
                    context.request_repaint_after(Duration::from_millis(16));
                    return;
                }
            };
            self.pair.stage_slot(slot, visual, false);
        }

        if let JunctionPairCommit::Published(old) = self.pair.commit() {
            if let Some(old) = old {
                free_junction_pair(&self.state, *old);
            }
            self.worker_error = None;
            self.render_error = None;
            context.request_repaint();
        }
    }

    fn finish_or_discard_pending_stage(
        &mut self,
        renderer: &mut PictureRenderer,
        context: &egui::Context,
        _expected: &EditJunctionIdentity,
    ) {
        if !self.pair.has_gpu_pending() {
            self.discard_obsolete_stage();
            return;
        }
        match renderer.is_idle() {
            Ok(true) => {
                self.pair.gpu_drained();
                self.discard_obsolete_stage();
                context.request_repaint();
            }
            Ok(false) => context.request_repaint_after(Duration::from_millis(16)),
            Err(error) => {
                if let Some(key) = self.pair.stage_key().cloned() {
                    self.render_error = Some((key, error.to_string()));
                }
            }
        }
    }

    fn discard_obsolete_stage(&mut self) {
        let obsolete = self.pair.stage_key().is_some_and(|key| {
            junction_stage_is_obsolete(
                key,
                self.expected.as_ref(),
                self.reply.as_ref(),
                self.required_key.as_ref(),
            )
        });
        if obsolete && let Some(staged) = self.pair.discard_stage() {
            free_staged_junction_pair(&self.state, staged);
        }
    }
}

fn junction_stage_is_obsolete(
    stage: &PairKey,
    expected: Option<&EditJunctionIdentity>,
    reply: Option<&EditJunctionReply>,
    required_key: Option<&PairKey>,
) -> bool {
    required_key != Some(stage)
        || expected != Some(&stage.identity)
        || reply.is_none_or(|reply| reply.identity != stage.identity)
}

fn fail_junction_stage(
    state: &egui_wgpu::RenderState,
    pair: &mut JunctionPairState<JunctionVisual>,
    render_error: &mut Option<(PairKey, String)>,
    key: PairKey,
    error: String,
) {
    *render_error = Some((key, error));
    if let Some(staged) = pair.discard_stage() {
        free_staged_junction_pair(state, staged);
    }
}

impl Drop for JunctionDisplay {
    fn drop(&mut self) {
        // UI registrations end with the display. Each submitted allocation has
        // its independent queue callback owner until that submission completes.
        if let Some(pair) = self.pair.accepted.take() {
            free_junction_pair(&self.state, pair);
        }
        if let Some(pair) = self.pair.staged.take() {
            free_staged_junction_pair(&self.state, pair);
        }
    }
}

fn junction_reply_is_current(
    expected: Option<&EditJunctionIdentity>,
    reply: &EditJunctionReply,
) -> bool {
    expected == Some(&reply.identity)
}

fn junction_picture(pictures: &EditJunctionPictures, slot: usize) -> &EditJunctionPicture {
    if slot == 0 {
        &pictures.outgoing
    } else {
        &pictures.incoming
    }
}

fn junction_raster(
    context: &egui::Context,
    size: egui::Vec2,
    pictures: &EditJunctionPictures,
    slot: usize,
) -> (u32, u32) {
    let picture = junction_picture(pictures, slot);
    let canvas = match picture {
        EditJunctionPicture::Exterior(_) => pictures.canvas,
        EditJunctionPicture::Picture(picture) => picture.canvas.unwrap_or(pictures.canvas),
    };
    let rect = picture_rect(
        egui::Rect::from_min_size(egui::Pos2::ZERO, size),
        Some(canvas),
    );
    target_size(rect.size(), context.pixels_per_point())
}

struct JunctionLayout {
    captions: [egui::Rect; 2],
    pictures: [egui::Rect; 2],
    error: egui::Rect,
}

impl JunctionLayout {
    fn pictures_visible(&self, clip: egui::Rect) -> bool {
        self.pictures.iter().all(|picture| {
            picture.width() > 0.0 && picture.height() > 0.0 && clip.contains_rect(*picture)
        })
    }
}

fn allocate_junction_layout(ui: &mut egui::Ui, height: f32) -> JunctionLayout {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(0.0), height.max(0.0)),
        egui::Sense::hover(),
    );
    junction_layout(
        rect,
        ui.text_style_height(&egui::TextStyle::Body),
        ui.spacing().item_spacing,
    )
}

fn junction_layout(rect: egui::Rect, row_height: f32, spacing: egui::Vec2) -> JunctionLayout {
    let gap_x = spacing.x.max(0.0).min(rect.width());
    let gap_y = spacing.y.max(0.0).min(rect.height() / 2.0);
    let row_height = row_height.max(0.0).min((rect.height() - 2.0 * gap_y) / 2.0);
    let width = (rect.width() - gap_x) / 2.0;
    let picture_height = (rect.height() - 2.0 * (row_height + gap_y)).max(0.0);
    let captions = [0, 1].map(|slot| {
        egui::Rect::from_min_size(
            rect.min + egui::vec2(slot as f32 * (width + gap_x), 0.0),
            egui::vec2(width, row_height),
        )
    });
    let pictures = captions.map(|caption| {
        egui::Rect::from_min_size(
            egui::pos2(caption.left(), caption.bottom() + gap_y),
            egui::vec2(width, picture_height),
        )
    });
    let error = egui::Rect::from_min_size(
        egui::pos2(rect.left(), rect.bottom() - row_height),
        egui::vec2(rect.width(), row_height),
    );
    JunctionLayout {
        captions,
        pictures,
        error,
    }
}

fn junction_label(ui: &mut egui::Ui, rect: egui::Rect, text: &str, color: egui::Color32) {
    // This child never advances the parent's already allocated fixed region.
    // Its clip also contains text when the available height is unusually small.
    let mut label_ui = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    label_ui.set_clip_rect(rect.intersect(ui.clip_rect()));
    let response = label_ui.add_sized(
        rect.size(),
        egui::Label::new(egui::RichText::new(text).color(color))
            .truncate()
            .halign(egui::Align::LEFT),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, text));
    if !text.is_empty() {
        response.on_hover_text(text);
    }
}

fn junction_caption(identity: &EditJunctionIdentity, slot: usize) -> String {
    let side = match identity.side {
        crate::worker::JunctionSide::Before => "Before",
        crate::worker::JunctionSide::Proposed => "Proposed",
    };
    let role = match identity.role {
        crate::worker::JunctionRole::In => "In",
        crate::worker::JunctionRole::Out => "Out",
        crate::worker::JunctionRole::SlipIn => "Slip In",
        crate::worker::JunctionRole::SlipOut => "Slip Out",
        crate::worker::JunctionRole::Roll => "Roll",
    };
    let side_label = if slot == 0 { "outgoing" } else { "incoming" };
    let address = if slot == 0 {
        identity.outgoing
    } else {
        identity.incoming
    };
    let frame = address.map_or_else(
        || format!("no {side_label} frame"),
        |frame| format!("Edit {}", i128::from(frame.0) + 1),
    );
    format!("{side} · {role} · {side_label}: {frame}")
}

fn paint_junction_visual(ui: &egui::Ui, rect: egui::Rect, visual: &JunctionVisual) {
    match visual {
        JunctionVisual::Exterior(edge) => {
            let label = match edge {
                JunctionExterior::NoOutgoing => "No outgoing frame",
                JunctionExterior::NoIncoming => "No incoming frame",
            };
            ui.painter_at(rect).text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(14.0),
                style::muted(ui),
            );
        }
        JunctionVisual::Background => {}
        JunctionVisual::Texture(target, canvas) => {
            ui.painter().image(
                target.texture,
                picture_rect(rect, Some(*canvas)),
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
    }
}

fn free_junction_visual(state: &egui_wgpu::RenderState, visual: JunctionVisual) {
    if let JunctionVisual::Texture(target, _) = visual {
        state.renderer.write().free_texture(&target.texture);
    }
}

fn free_junction_pair(state: &egui_wgpu::RenderState, pair: AcceptedJunctionPair<JunctionVisual>) {
    for visual in pair.slots {
        free_junction_visual(state, visual);
    }
}

fn free_staged_junction_pair(
    state: &egui_wgpu::RenderState,
    pair: StagedJunctionPair<JunctionVisual>,
) {
    for visual in pair.slots.into_iter().flatten() {
        free_junction_visual(state, visual);
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
                    persisted_version: None,
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

    fn junction_identity(inspection: u64, boundary: i64) -> EditJunctionIdentity {
        use deadpan_playback::ContentIdentity;
        EditJunctionIdentity {
            session: 1,
            project: ProjectId::new("junction-display").unwrap(),
            base_revision: RevisionId::new("base").unwrap(),
            draft: 7,
            change: 3,
            content: ContentIdentity::Committed,
            proposal_revision: None,
            inspection,
            side: crate::worker::JunctionSide::Proposed,
            role: crate::worker::JunctionRole::In,
            boundary: ProjectFrame(boundary),
            outgoing: (boundary > 0).then(|| ProjectFrame(boundary - 1)),
            incoming: (boundary < 9).then_some(ProjectFrame(boundary)),
        }
    }

    fn pair_key(identity: EditJunctionIdentity, width: u32) -> PairKey {
        PairKey {
            identity,
            raster: [(width, 60), (width, 60)],
        }
    }

    #[test]
    fn junction_layout_fills_width_and_keeps_both_picture_rasters_equal() {
        let rect = egui::Rect::from_min_size(egui::pos2(30.0, 50.0), egui::vec2(1000.0, 400.0));
        let layout = junction_layout(rect, 20.0, egui::vec2(12.0, 6.0));
        assert!(layout.pictures_visible(rect));
        assert!(!layout.pictures_visible(egui::Rect::from_min_max(
            rect.min,
            egui::pos2(rect.right() - 1.0, rect.bottom()),
        )));
        assert_eq!(layout.captions[0].size(), egui::vec2(494.0, 20.0));
        assert_eq!(layout.captions[1].size(), layout.captions[0].size());
        assert_eq!(layout.pictures[0].size(), egui::vec2(494.0, 348.0));
        assert_eq!(layout.pictures[1].size(), layout.pictures[0].size());
        assert_eq!(layout.pictures[0].min, egui::pos2(30.0, 76.0));
        assert_eq!(layout.pictures[1].min, egui::pos2(536.0, 76.0));
        assert_eq!(layout.pictures[1].right(), rect.right());
        assert_eq!(
            layout.error,
            egui::Rect::from_min_size(egui::pos2(30.0, 430.0), egui::vec2(1000.0, 20.0))
        );
        for picture in layout.pictures {
            assert_eq!(picture.bottom() + 6.0, layout.error.top());
        }
    }

    #[test]
    fn junction_layout_bounds_tiny_or_empty_panels_without_negative_rectangles() {
        for size in [
            egui::Vec2::ZERO,
            egui::vec2(1.0, 1.0),
            egui::vec2(20.0, 12.0),
        ] {
            let rect = egui::Rect::from_min_size(egui::pos2(3.0, 7.0), size);
            let layout = junction_layout(rect, 20.0, egui::vec2(12.0, 6.0));
            assert!(!layout.pictures_visible(rect));
            for child in layout
                .captions
                .into_iter()
                .chain(layout.pictures)
                .chain([layout.error])
            {
                assert!(child.width() >= 0.0 && child.height() >= 0.0);
                assert!(rect.contains(child.min) && rect.contains(child.max));
            }
            assert_eq!(layout.pictures[0].size(), layout.pictures[1].size());
            assert!(layout.pictures[0].right() <= layout.pictures[1].left());
        }
    }

    #[test]
    fn junction_display_ignores_stale_decoded_success_and_error() {
        let old = junction_identity(1, 2);
        let current = junction_identity(2, 3);
        let pictures = || EditJunctionPictures {
            outgoing: EditJunctionPicture::Exterior(JunctionExterior::NoOutgoing),
            incoming: EditJunctionPicture::Exterior(JunctionExterior::NoIncoming),
            canvas: (320, 180),
        };
        assert!(!junction_reply_is_current(
            Some(&current),
            &EditJunctionReply {
                identity: old.clone(),
                pictures: Ok(pictures()),
            }
        ));
        assert!(!junction_reply_is_current(
            Some(&current),
            &EditJunctionReply {
                identity: old,
                pictures: Err("late decode failure".into()),
            }
        ));
        assert!(junction_reply_is_current(
            Some(&current),
            &EditJunctionReply {
                identity: current.clone(),
                pictures: Ok(pictures()),
            }
        ));
    }

    #[test]
    fn junction_pair_is_published_only_after_both_slots_and_gpu_drain() {
        let first = pair_key(junction_identity(1, 2), 100);
        let replacement = pair_key(junction_identity(2, 3), 120);
        let mut state = JunctionPairState::default();
        assert!(matches!(
            state.begin(first.clone()),
            BeginJunctionStage::Started(None)
        ));
        state.stage_slot(0, "outgoing-1", false);
        state.stage_slot(1, "incoming-1", true);
        assert!(matches!(state.commit(), JunctionPairCommit::NotReady));
        state.gpu_drained();
        assert!(matches!(
            state.commit(),
            JunctionPairCommit::Published(None)
        ));
        assert!(state.ready_for(&first));

        assert!(matches!(
            state.begin(replacement.clone()),
            BeginJunctionStage::Started(None)
        ));
        state.stage_slot(0, "outgoing-2", true);
        assert!(!state.ready_for(&replacement));
        assert!(
            state.ready_for(&first),
            "the whole prior pair remains accepted"
        );
        assert!(matches!(
            state.begin(pair_key(junction_identity(3, 4), 140)),
            BeginJunctionStage::Busy
        ));
        state.gpu_drained();
        state.stage_slot(1, "incoming-2", false);
        let JunctionPairCommit::Published(Some(old)) = state.commit() else {
            panic!("complete pair publishes together and returns the old pair");
        };
        assert_eq!(old.slots, ["outgoing-1", "incoming-1"]);
        assert!(state.ready_for(&replacement));
        assert_eq!(
            state.accepted().unwrap().slots,
            ["outgoing-2", "incoming-2"]
        );
    }

    #[test]
    fn drained_stage_for_a_prior_raster_is_discarded_even_for_the_same_identity() {
        let identity = junction_identity(1, 2);
        let staged = pair_key(identity.clone(), 200);
        let requested = pair_key(identity.clone(), 100);
        let reply = EditJunctionReply {
            identity: identity.clone(),
            pictures: Ok(EditJunctionPictures {
                outgoing: EditJunctionPicture::Exterior(JunctionExterior::NoOutgoing),
                incoming: EditJunctionPicture::Exterior(JunctionExterior::NoIncoming),
                canvas: (320, 180),
            }),
        };
        assert!(junction_stage_is_obsolete(
            &staged,
            Some(&identity),
            Some(&reply),
            Some(&requested),
        ));
        assert!(!junction_stage_is_obsolete(
            &requested,
            Some(&identity),
            Some(&reply),
            Some(&requested),
        ));
    }

    #[test]
    fn accepted_raster_return_polls_and_retires_the_submitted_resize_first() {
        let identity = junction_identity(1, 2);
        let accepted = pair_key(identity.clone(), 100);
        let resized = pair_key(identity, 200);
        let mut state = JunctionPairState::default();
        assert!(matches!(
            state.begin(accepted.clone()),
            BeginJunctionStage::Started(None)
        ));
        state.stage_slot(0, "accepted outgoing", false);
        state.stage_slot(1, "accepted incoming", false);
        assert!(matches!(
            state.commit(),
            JunctionPairCommit::Published(None)
        ));
        assert!(matches!(
            state.begin(resized.clone()),
            BeginJunctionStage::Started(None)
        ));
        state.stage_slot(0, "submitted resized outgoing", true);

        // This is the production transition before its accepted-key return.
        // Even though 100 is already displayed, the submitted 200 must poll.
        let mut polls = 0;
        let pending = state.prepare_key(&accepted, || {
            polls += 1;
            Ok::<_, &str>(false)
        });
        assert!(matches!(pending, Ok(JunctionPreparation::Pending)));
        assert_eq!(polls, 1);
        assert!(state.has_gpu_pending());
        assert_eq!(state.stage_key(), Some(&resized));
        assert_eq!(
            state.accepted().unwrap().slots,
            ["accepted outgoing", "accepted incoming"]
        );

        let completed = state.prepare_key(&accepted, || {
            polls += 1;
            Ok::<_, &str>(true)
        });
        let Ok(JunctionPreparation::Ready {
            accepted: true,
            discarded: Some(discarded),
        }) = completed
        else {
            panic!("completion retires the resize before reusing the accepted pair")
        };
        assert_eq!(polls, 2);
        assert_eq!(discarded.key, resized);
        assert_eq!(discarded.slots, [Some("submitted resized outgoing"), None]);
        assert!(!state.has_gpu_pending());
        assert!(state.stage_key().is_none());
        assert!(state.ready_for(&accepted));
        assert_eq!(
            state.accepted().unwrap().slots,
            ["accepted outgoing", "accepted incoming"]
        );

        assert!(matches!(
            state.prepare_key(&accepted, || -> Result<bool, &str> {
                panic!("a quiescent accepted pair needs no GPU poll")
            }),
            Ok(JunctionPreparation::Ready {
                accepted: true,
                discarded: None,
            })
        ));
    }

    #[test]
    fn accepted_raster_return_does_not_treat_a_poll_fault_as_completion() {
        let accepted = pair_key(junction_identity(1, 2), 100);
        let resized = pair_key(accepted.identity.clone(), 200);
        let mut state = JunctionPairState::default();
        assert!(matches!(
            state.begin(accepted.clone()),
            BeginJunctionStage::Started(None)
        ));
        state.stage_slot(0, "accepted outgoing", false);
        state.stage_slot(1, "accepted incoming", false);
        assert!(matches!(
            state.commit(),
            JunctionPairCommit::Published(None)
        ));
        assert!(matches!(
            state.begin(resized.clone()),
            BeginJunctionStage::Started(None)
        ));
        state.stage_slot(0, "submitted resized outgoing", true);
        assert!(matches!(
            state.prepare_key(&accepted, || Err("device poll failed")),
            Err("device poll failed")
        ));
        assert!(state.has_gpu_pending());
        assert_eq!(state.stage_key(), Some(&resized));
        assert!(state.discard_stage().is_none());
        assert!(state.ready_for(&accepted));
    }

    #[test]
    fn clearing_a_draft_removes_its_pair_but_keeps_a_submitted_stage() {
        let old = pair_key(junction_identity(1, 2), 100);
        let resized = pair_key(old.identity.clone(), 200);
        let new_draft = pair_key(
            EditJunctionIdentity {
                session: 2,
                draft: 8,
                ..junction_identity(1, 3)
            },
            120,
        );
        let mut state = JunctionPairState::default();
        assert!(matches!(
            state.begin(old),
            BeginJunctionStage::Started(None)
        ));
        state.stage_slot(0, "old outgoing", false);
        state.stage_slot(1, "old incoming", false);
        assert!(matches!(
            state.commit(),
            JunctionPairCommit::Published(None)
        ));
        assert!(matches!(
            state.begin(resized.clone()),
            BeginJunctionStage::Started(None)
        ));
        state.stage_slot(0, "submitted resize", true);

        // JunctionDisplay::clear uses this exact ownership transition.
        let cleared = state.clear();
        assert_eq!(
            cleared.accepted.unwrap().slots,
            ["old outgoing", "old incoming"]
        );
        assert!(cleared.staged.is_none());
        assert!(state.accepted().is_none());
        assert!(state.has_gpu_pending());
        assert!(matches!(
            state.prepare_key(&new_draft, || Ok::<_, &str>(false)),
            Ok(JunctionPreparation::Pending)
        ));
        assert!(state.accepted().is_none());

        let completed = state.prepare_key(&new_draft, || Ok::<_, &str>(true));
        let Ok(JunctionPreparation::Ready {
            accepted: false,
            discarded: Some(discarded),
        }) = completed
        else {
            panic!("new draft can prepare only after the old submitted stage drains")
        };
        assert_eq!(discarded.key, resized);
        assert_eq!(discarded.slots, [Some("submitted resize"), None]);
        assert!(state.accepted().is_none());
        assert!(state.stage_key().is_none());
    }

    #[test]
    fn junction_failed_or_resized_stage_keeps_complete_old_captions_and_pair() {
        let accepted = pair_key(junction_identity(1, 2), 100);
        let failed_resize = pair_key(junction_identity(1, 2), 200);
        let newer = pair_key(junction_identity(2, 3), 220);
        let mut state = JunctionPairState::default();
        assert!(matches!(
            state.begin(accepted.clone()),
            BeginJunctionStage::Started(None)
        ));
        state.stage_slot(0, "old outgoing", false);
        state.stage_slot(1, "old incoming", false);
        assert!(matches!(
            state.commit(),
            JunctionPairCommit::Published(None)
        ));
        assert!(matches!(
            state.begin(failed_resize.clone()),
            BeginJunctionStage::Started(None)
        ));
        state.stage_slot(0, "partial resize outgoing", true);
        assert!(matches!(state.commit(), JunctionPairCommit::NotReady));
        state.gpu_drained();
        let discarded = state.discard_stage().unwrap();
        assert_eq!(discarded.slots[0], Some("partial resize outgoing"));
        assert!(state.ready_for(&accepted));
        assert!(!state.ready_for(&failed_resize));

        assert!(matches!(
            state.begin(newer.clone()),
            BeginJunctionStage::Started(None)
        ));
        state.stage_slot(0, "new outgoing", false);
        let BeginJunctionStage::Started(Some(old_stage)) = state.begin(failed_resize.clone())
        else {
            panic!("resize discards only the drained partial stage")
        };
        assert_eq!(old_stage.slots[0], Some("new outgoing"));
        assert!(state.ready_for(&accepted));
        state.stage_slot(0, "retry outgoing", false);
        state.stage_slot(1, "retry incoming", false);
        assert!(matches!(
            state.commit(),
            JunctionPairCommit::Published(Some(_))
        ));
        assert!(state.ready_for(&failed_resize));
    }
}
