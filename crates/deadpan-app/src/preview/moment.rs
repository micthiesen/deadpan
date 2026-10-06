//! Ephemeral half-open Original selection and shared copied-content controls.

use std::ops::Range;

use deadpan_core::SourceQualificationId;

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Identity {
    pub(super) session: u64,
    pub(super) asset: AssetId,
    pub(super) qualification: SourceQualificationId,
}

#[derive(Clone, Debug)]
pub(super) struct Copied {
    pub(super) identity: Identity,
    pub ordinals: Range<u64>,
}

/// Commands retain absence as well as presence of a range and register. A late
/// service reply cannot supply a different paste or placement destination.
#[derive(Clone)]
pub(super) struct PlacementTarget {
    pub base: Arc<Workspace>,
    pub scope: SequenceScope,
    pub parent: NodeId,
    pub cursor: u64,
    pub source_cursor: u64,
    pub pane: Pane,
    pub selected_beat: Option<NodeId>,
    pub copied: Option<copied::Content>,
    pub register: Option<char>,
    pub selection: edit_range::Selection,
    pub range: Option<deadpan_core::FrameRange>,
    pub macro_capture: Result<macros::Capture, String>,
}

impl PlacementTarget {
    /// Fast paste treats an explicitly empty Visual target as an error. Place
    /// slice has a separate destination chooser and does not use this guard.
    pub(super) fn check_nonempty_selection(&self) -> Result<(), String> {
        if self.selection.has_bounds() && self.range.is_none() {
            return Err("The Edit selection is empty. Extend it or clear it before pasting; no edit was made.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct Selection {
    identity: Option<Identity>,
    bounds: Option<(u64, u64)>,
    pub active: bool,
}

impl Selection {
    fn reconcile(&mut self, identity: Option<Identity>) {
        if self.identity != identity {
            self.cancel();
            self.identity = identity;
        }
    }

    fn toggle(&mut self, cursor: u64) {
        if self.active {
            self.active = false;
        } else {
            self.bounds = Some((cursor, cursor));
            self.active = true;
        }
    }

    pub fn move_to(&mut self, cursor: u64) {
        if self.active
            && let Some((_, head)) = &mut self.bounds
        {
            *head = cursor;
        }
    }

    pub fn range(&self) -> Option<Range<u64>> {
        let (anchor, head) = self.bounds?;
        (anchor != head).then_some(anchor.min(head)..anchor.max(head))
    }

    pub fn cancel(&mut self) {
        self.active = false;
        self.bounds = None;
    }

    fn copy(&mut self) -> Result<Copied, String> {
        let identity = self
            .identity
            .clone()
            .ok_or("Choose a registered Original first")?;
        let ordinals = self
            .range()
            .ok_or("Select a nonempty Original range before copying.")?;
        self.active = false;
        Ok(Copied { identity, ordinals })
    }
}

/// Paste without a selected beat is defined only for an empty current group.
fn paste_slot(rows: &[BeatRow], selected: Option<&NodeId>, before: bool) -> Result<usize, String> {
    if rows.is_empty() {
        return Ok(0);
    }
    rows.iter()
        .position(|row| Some(&row.id) == selected)
        .map(|index| index + usize::from(!before))
        .ok_or_else(|| "Select a beat in the current group before pasting".into())
}

// Geometry is approximate, but always represents measured elapsed source time.
// Convert differences before floats so large signed PTS origins lose no pixels.
fn boundary_fraction(index: &deadpan_core::SourceFrameIndex, boundary: u64) -> f64 {
    let start = i128::from(index.frames()[0].pts);
    let end = i128::from(index.terminal_end());
    let at = usize::try_from(boundary)
        .ok()
        .and_then(|n| index.frames().get(n))
        .map_or(end, |frame| i128::from(frame.pts));
    (at - start) as f64 / (end - start) as f64
}

fn nearest_boundary(index: &deadpan_core::SourceFrameIndex, fraction: f64) -> u64 {
    let fraction = fraction.clamp(0.0, 1.0);
    let next = index
        .frames()
        .partition_point(|frame| boundary_fraction(index, frame.identity.0) < fraction)
        as u64;
    let previous = next.saturating_sub(1);
    if fraction - boundary_fraction(index, previous) < boundary_fraction(index, next) - fraction {
        previous
    } else {
        next
    }
}

impl DeadpanApp {
    pub(super) fn capture_placement_target(&self) -> Result<PlacementTarget, String> {
        if self.copied.is_pending() {
            return Err("Wait for the copy to finish saving before placing it.".into());
        }
        if self.view != View::Sequence
            || self.sound_focused()
            || self.pane == Pane::Sounds
            || self.event_focused()
        {
            return Err("Return to Your edit (:sequence) before placing a slice.".into());
        }
        let base = self.workspace.clone().ok_or("Open a project first.")?;
        let parent = self.sequence_scope.resolve(&base)?.owner.clone();
        Ok(PlacementTarget {
            base,
            scope: self.sequence_scope.clone(),
            parent,
            cursor: self.sequence_cursor,
            source_cursor: self.source_cursor,
            pane: self.pane,
            selected_beat: self.selected_beat.clone(),
            copied: self.copied.selected_content().cloned(),
            register: self.copied.selected(),
            selection: self.edit_range.clone(),
            range: self.selected_edit_range(),
            macro_capture: self.capture_macro_target(),
        })
    }

    pub(super) fn check_placement_target(&self, target: &PlacementTarget) -> Result<(), String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("The captured edit is closed.")?;
        if workspace.session != target.base.session
            || workspace.document.project_id() != target.base.document.project_id()
            || workspace.document.revision_id() != target.base.document.revision_id()
            || self.sequence_scope != target.scope
            || target.scope.resolve(workspace)?.owner != &target.parent
        {
            return Err(
                "The captured Edit destination changed. Enter the command again; no edit was made."
                    .into(),
            );
        }
        if let Some(copied) = &target.copied {
            copied.check(workspace)?;
        }
        Ok(())
    }

    pub(super) fn moment_identity(&self) -> Option<Identity> {
        if self.raw_source.is_some() {
            return None;
        }
        let workspace = self.workspace.as_ref()?;
        let asset = self.selected_source.as_ref()?;
        let source = workspace.sources.get(asset)?;
        source.video_index.as_ref()?;
        Some(Identity {
            session: workspace.session,
            asset: asset.clone(),
            qualification: source.receipt.id().clone(),
        })
    }

    pub(super) fn reconcile_moment(&mut self) {
        self.moment.reconcile(self.moment_identity());
        self.copied.reconcile(self.workspace.as_deref());
        if self.view != View::Source {
            self.moment.active = false;
        }
    }

    pub(super) fn visual_moment(&mut self) {
        self.bindings.clear();
        self.reconcile_moment();
        if self.view != View::Source || self.moment.identity.is_none() {
            self.error = Some("Use :source to select a moment from the registered Original".into());
            return;
        }
        self.pause_playback();
        self.moment.toggle(self.source_cursor);
        self.error = None;
        self.message = Some(if self.moment.active {
            format!(
                "Move with {} to select time. {} copies; {} finishes; {} cancels.",
                self.editor_pair(EditorKey::FramePrevious, EditorKey::FrameNext, "/"),
                self.editor_key(EditorKey::Copy),
                self.editor_key(EditorKey::Visual),
                self.editor_key(EditorKey::Escape)
            )
        } else if self.moment.range().is_some() {
            format!(
                "Selection retained. {} copies this moment.",
                self.editor_key(EditorKey::Copy)
            )
        } else {
            format!(
                "Empty selection. Press {} and move with {} before copying.",
                self.editor_key(EditorKey::Visual),
                self.editor_pair(EditorKey::FramePrevious, EditorKey::FrameNext, "/")
            )
        });
    }

    pub(super) fn copy_moment(&mut self, destination: Option<char>) {
        self.bindings.clear();
        self.reconcile_moment();
        if self.view != View::Source {
            self.error = Some("Select and copy a moment in Original (:source)".into());
            return;
        }
        let copied = match self.moment.clone().copy() {
            Ok(copied) => copied,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(serial) = self.next_serial() else {
            return;
        };
        let Some(workspace) = &self.workspace else {
            self.error = Some("Open a project before copying.".into());
            return;
        };
        let id = crate::project::slice::CopyId {
            session: workspace.session,
            project: workspace.document.project_id().clone(),
            source_revision: workspace.document.revision_id().clone(),
            request: serial,
            persisted_version: None,
        };
        let request = crate::project::registers::OriginalRequest {
            id: id.clone(),
            register: destination,
            asset: copied.identity.asset,
            qualification: copied.identity.qualification,
            ordinals: copied.ordinals,
        };
        match self
            .service
            .submit(ProjectRequest::CaptureOriginal(request))
        {
            Ok(()) => {
                self.copied.expect_original(id, self.moment.clone());
                self.error = None;
                self.message = Some("Saving Original copy…".into());
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn receive_original_copy(
        &mut self,
        update: Option<crate::project::registers::OriginalUpdate>,
        unrefreshed_commit: bool,
    ) {
        let Some((selection, result)) =
            update.and_then(|update| self.copied.receive_original(update))
        else {
            return;
        };
        match result {
            Ok(()) => {
                if unrefreshed_commit {
                    return;
                }
                if self.moment == selection {
                    self.moment.active = false;
                }
                self.error = None;
                self.message = Some(format!(
                    "Moment copied and saved. Return to Your edit (:sequence): :splice previews placement; {} replaces an Edit selection or pastes beside a beat.",
                    self.editor_pair(EditorKey::PasteAfter, EditorKey::PasteBefore, "/")
                ));
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn paste_moment(&mut self, before: bool) {
        self.bindings.clear();
        self.reconcile_moment();
        let target = self.capture_placement_target();
        self.paste_captured_moment(before, target);
    }

    pub(super) fn paste_captured_moment(
        &mut self,
        before: bool,
        target: Result<PlacementTarget, String>,
    ) {
        if self.record_macro_paste(before, &target) {
            return;
        }
        self.copied.clear_selection();
        let result = (|| {
            let target = target?;
            self.check_placement_target(&target)?;
            target.check_nonempty_selection()?;
            let copied = target.copied.as_ref().ok_or_else(|| {
                if let Some(name) = target.register {
                    return format!(
                        "Register {name} is empty. Copy or cut into it first; no edit was made."
                    );
                }
                format!(
                    "Copy a range first: {} in Original or Your edit.",
                    self.editor_copy_recipe()
                )
            })?;
            let destination = if let Some(range) = target.range {
                crate::project::splice::Destination::Replace { range }
            } else {
                crate::project::splice::Destination::Slot(paste_slot(
                    &self.beat_rows,
                    target.selected_beat.as_ref(),
                    before,
                )?)
            };
            Ok::<_, String>(match copied {
                copied::Content::Original(copied) => {
                    ProjectRequest::PasteMoment(crate::project::MomentPaste {
                        expected_session: target.base.session,
                        expected_revision: target.base.document.revision_id().clone(),
                        asset: copied.identity.asset.clone(),
                        qualification: copied.identity.qualification.clone(),
                        ordinals: copied.ordinals.clone(),
                        scope: target.scope,
                        parent: target.parent,
                        destination,
                    })
                }
                copied::Content::Edited(copied) => {
                    ProjectRequest::PasteEditedSlice(crate::project::slice::Paste {
                        expected_session: target.base.session,
                        expected_revision: target.base.document.revision_id().clone(),
                        copied: copied.clone(),
                        scope: target.scope,
                        parent: target.parent,
                        destination,
                    })
                }
                copied::Content::Macro(_) => return Err(copied::MACRO_PASTE_ERROR.into()),
            })
        })();
        match result {
            Ok(request) => {
                self.stop_playback();
                self.submit(request);
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn moment_controls(&mut self, ui: &mut egui::Ui) {
        if self.view == View::Source && self.moment_identity().is_some() {
            let Some(source) = self.workspace.as_ref().and_then(|workspace| {
                workspace
                    .sources
                    .get(self.selected_source.as_ref()?)
                    .cloned()
            }) else {
                return;
            };
            let Some(index) = source.video_index.as_ref() else {
                return;
            };
            let length = self.source_length();
            let range = self.moment.range();
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), 26.0),
                egui::Sense::click_and_drag(),
            );
            let x = |boundary: u64| {
                rect.left() + rect.width() * boundary_fraction(index, boundary) as f32
            };
            ui.painter().rect_filled(rect, 3.0, style::PANEL);
            // Pauses as quiet bands along the bottom, shots as hairline ticks,
            // from marks cached with the timeline. Bands closer than a pixel
            // join and ticks within a pixel of the previous one are skipped,
            // so a long Original paints at most about one shape per pixel.
            let marks = self.source_analysis_marks();
            let clip = ui.clip_rect().intersect(rect);
            let mut band: Option<(f32, f32)> = None;
            let paint_band = |ui: &egui::Ui, (left, right): (f32, f32)| {
                if right >= clip.left() && left <= clip.right() {
                    ui.painter().rect_filled(
                        egui::Rect::from_min_max(
                            egui::pos2(left, rect.bottom() - 6.0),
                            egui::pos2(right.max(left + 1.0), rect.bottom() - 1.0),
                        ),
                        0.0,
                        style::PAUSE_BAND,
                    );
                }
            };
            for pause in &marks.pauses {
                let (left, right) = (x(pause.start), x(pause.end));
                band = match band {
                    Some((from, to)) if left <= to + 1.0 => Some((from, to.max(right))),
                    Some(previous) => {
                        paint_band(ui, previous);
                        Some((left, right))
                    }
                    None => Some((left, right)),
                };
            }
            if let Some(last) = band {
                paint_band(ui, last);
            }
            let mut last_tick = f32::NEG_INFINITY;
            for shot in &marks.shots {
                let at = x(*shot);
                if at < last_tick + 1.0 || at < clip.left() || at > clip.right() {
                    continue;
                }
                last_tick = at;
                ui.painter().line_segment(
                    [
                        egui::pos2(at, rect.top() + 3.0),
                        egui::pos2(at, rect.bottom() - 3.0),
                    ],
                    egui::Stroke::new(1.0, style::SHOT_TICK),
                );
            }
            if let Some(range) = &range {
                let selected = egui::Rect::from_min_max(
                    egui::pos2(x(range.start), rect.top()),
                    egui::pos2(x(range.end), rect.bottom()),
                );
                ui.painter().rect_filled(selected, 2.0, style::SELECTED);
                ui.painter().rect_stroke(
                    selected,
                    2.0,
                    egui::Stroke::new(1.0, style::LAVENDER),
                    egui::StrokeKind::Inside,
                );
            }
            ui.painter().line_segment(
                [
                    egui::pos2(x(self.source_cursor), rect.top()),
                    egui::pos2(x(self.source_cursor), rect.bottom()),
                ],
                egui::Stroke::new(2.0, style::CURSOR),
            );
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Slider,
                    true,
                    format!(
                        "Original temporal range, cursor {} of {}. {}",
                        self.source_cursor,
                        length,
                        range_label(range.as_ref(), self.moment.active, &self.bindings)
                    ),
                )
            });
            if (response.clicked() || response.dragged())
                && let Some(pointer) = response.interact_pointer_pos()
            {
                self.source_cursor =
                    nearest_boundary(index, f64::from((pointer.x - rect.left()) / rect.width()));
                self.moment.move_to(self.source_cursor);
                self.pane = Pane::Viewer;
                ui.memory_mut(|memory| memory.request_focus(pane_id(Pane::Viewer)));
                self.request_picture(false);
            }
            let label = range_label(range.as_ref(), self.moment.active, &self.bindings);
            accessibility::full_text(
                ui.add(
                    egui::Label::new(egui::RichText::new(&label).color(style::LAVENDER)).truncate(),
                ),
                &label,
            )
            .on_hover_text(label);
        } else if self.view == View::Sequence
            && let Some(copied) = self.copied.selected_content()
        {
            let replacement_label = self.selected_edit_range().map(|_| {
                if self.edit_range.object().is_some() {
                    "Replace object"
                } else {
                    "Replace range"
                }
            });
            let label = copied.label();
            let is_macro = matches!(copied, copied::Content::Macro(_));
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(style::LAVENDER, label);
                if is_macro {
                    ui.weak(format!(
                        "Run with {} then the register name.",
                        self.editor_key(EditorKey::MacroExecute)
                    ));
                    return;
                }
                if ui
                    .add_enabled(
                        !self.service.is_busy(),
                        egui::Button::new("Place slice…  :splice"),
                    )
                    .clicked()
                {
                    self.open_splice(ui.ctx());
                }
                if ui
                    .add_enabled(
                        !self.service.is_busy(),
                        egui::Button::new(format!(
                            "{}  {}",
                            replacement_label.unwrap_or("Paste after"),
                            self.editor_key(EditorKey::PasteAfter)
                        ))
                        .wrap(),
                    )
                    .clicked()
                {
                    self.pane = Pane::Viewer;
                    self.paste_moment(false);
                }
                if ui
                    .add_enabled(
                        !self.service.is_busy(),
                        egui::Button::new(format!(
                            "{}  {}",
                            replacement_label.unwrap_or("Paste before"),
                            self.editor_key(EditorKey::PasteBefore)
                        ))
                        .wrap(),
                    )
                    .clicked()
                {
                    self.pane = Pane::Viewer;
                    self.paste_moment(true);
                }
            });
        }
    }

    pub(super) fn moment_inspector(&mut self, ui: &mut egui::Ui) {
        let layout = self.workspace_layout(ui);
        egui::Panel::right("workspace-inspector").resizable(false)
            .default_size(layout.inspector).min_size(layout.inspector).max_size(layout.inspector)
            .frame(style::panel()).show(ui, |ui| {
                let heading = pane_heading(ui, "ORIGINAL MOMENT", self.pane == Pane::Inspector);
                if pane_focus(ui, Pane::Inspector, heading.rect, "Original moment inspector pane").has_focus() {
                    self.pane = Pane::Inspector;
                }
                ui.separator();
                egui::ScrollArea::vertical().id_salt("moment-inspector").show(ui, |ui| {
                    ui.label(egui::RichText::new("Select time. Reuse it.").size(17.0));
                    if let Some(range) = self.moment.range() {
                        inspector_value(ui, "In · included", &range.start.to_string());
                        inspector_value(ui, "Out · excluded", &range.end.to_string());
                        inspector_value(ui, "Original frames", &(range.end - range.start).to_string());
                        if let Some(seconds) = self.moment_seconds(&range) {
                            inspector_value(ui, "Measured duration", &format!("{seconds:.3} s"));
                        }
                        ui.weak("The Out boundary is excluded. Original frame count and project duration can differ.");
                    } else {
                        ui.weak(if self.moment.active {
                            format!("Move with {} to select a nonempty range. {} finishes; {} cancels.", self.editor_pair(EditorKey::FramePrevious, EditorKey::FrameNext, "/"), self.editor_key(EditorKey::Visual), self.editor_key(EditorKey::Escape))
                        } else {
                            format!("Press {}, then move with {}. Counts work: {} selects 24 presentation frames.", self.editor_key(EditorKey::Visual), self.editor_pair(EditorKey::FramePrevious, EditorKey::FrameNext, "/"), self.editor_counted(EditorKey::FrameNext, 24))
                        });
                    }
                    ui.add_space(8.0);
                    if ui.add(egui::Button::new(format!("{}  {}", if self.moment.active { "Finish selection" } else { "Select moment" }, self.editor_key(EditorKey::Visual))).wrap()).clicked() { self.pane = Pane::Inspector; self.visual_moment(); }
                    if ui.add_enabled(self.moment.range().is_some(), egui::Button::new(format!("Copy moment  {}", self.editor_key(EditorKey::Copy))).wrap().fill(style::SELECTED)).clicked() { self.pane = Pane::Inspector; self.copy_slice(); }
                    if ui.add(egui::Button::new(format!("Cancel selection  {}", self.editor_key(EditorKey::Escape))).wrap()).clicked() { self.pane = Pane::Inspector; self.moment.cancel(); self.cancel_register_choice(); }
                    ui.separator();
                    ui.weak("Copies are saved in this project. Copying does not change Your edit or its history.");
                    if let Some(copied) = self.copied.selected_content() {
                        ui.colored_label(style::LAVENDER, copied.label());
                        if matches!(copied, copied::Content::Macro(_)) {
                            ui.weak(format!("Return to Your edit, then {} and the register name to run the macro.", self.editor_key(EditorKey::MacroExecute)));
                        } else {
                            ui.weak(format!("Return to Your edit, choose a beat, then {} after or {} before. One paste, one undo.", self.editor_key(EditorKey::PasteAfter), self.editor_key(EditorKey::PasteBefore)));
                        }
                        if ui.button("Your edit  :sequence").clicked() {
                            self.view.set(View::Sequence, &mut self.message);
                            self.pane = Pane::Sequence;
                            self.request_picture(false);
                            ui.memory_mut(|memory| memory.request_focus(pane_id(Pane::Sequence)));
                        }
                    }
                });
            });
    }

    /// Display-only elapsed seconds from two measured endpoints. No scan,
    /// source qualification or frame-count-to-rate approximation on the UI.
    fn moment_seconds(&self, range: &Range<u64>) -> Option<f64> {
        let workspace = self.workspace.as_ref()?;
        let index = workspace
            .sources
            .get(self.selected_source.as_ref()?)?
            .video_index
            .as_ref()?;
        let start = index.interval(SourceFrameId(range.start)).ok()?.0;
        let end = index
            .interval(SourceFrameId(range.end.checked_sub(1)?))
            .ok()?
            .1;
        let ticks = i128::from(end) - i128::from(start);
        Some(
            ticks as f64 * f64::from(index.time_base().numerator())
                / f64::from(index.time_base().denominator()),
        )
    }
}

fn range_label(range: Option<&Range<u64>>, active: bool, bindings: &Bindings) -> String {
    range.map_or_else(
        || {
            if active {
                format!(
                    "Original time · {} extends selection",
                    key_labels::pair(
                        bindings,
                        EditorKey::FramePrevious,
                        EditorKey::FrameNext,
                        "/"
                    )
                )
            } else {
                format!(
                    "Original time · {} starts selection",
                    bindings.key_label(EditorKey::Visual)
                )
            }
        },
        |range| {
            format!(
                "In {} · Out {} excluded · {} original frames",
                range.start,
                range.end,
                range.end - range.start
            )
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(session: u64) -> Identity {
        Identity {
            session,
            asset: AssetId::new("original").unwrap(),
            qualification: SourceQualificationId::new("a".repeat(64)).unwrap(),
        }
    }

    #[test]
    fn temporal_bar_uses_measured_vfr_time_and_snaps_to_actual_boundaries() {
        use deadpan_core::{
            IndexedSourceFrame, SourceFrameIndex, SourceTimeBase, TerminalProvenance,
        };
        for origin in [-100, i64::MAX - 200] {
            let index = SourceFrameIndex::new(
                AssetId::new("original").unwrap(),
                SourceTimeBase::new(1, 1000).unwrap(),
                [0, 10, 90]
                    .into_iter()
                    .enumerate()
                    .map(|(n, offset)| IndexedSourceFrame {
                        identity: SourceFrameId(n as u64),
                        pts: origin + offset,
                        reported_duration: None,
                        keyframe: true,
                        seek_from: None,
                        decode_timestamp: None,
                    })
                    .collect(),
                origin + 100,
                TerminalProvenance::Explicit,
            )
            .unwrap();
            assert_eq!(boundary_fraction(&index, 1), 0.1);
            assert_eq!(boundary_fraction(&index, 2), 0.9);
            assert_eq!(nearest_boundary(&index, 0.0), 0);
            assert_eq!(nearest_boundary(&index, 0.3), 1);
            assert_eq!(nearest_boundary(&index, 0.7), 2);
            assert_eq!(nearest_boundary(&index, 1.0), 3);
        }
    }

    #[test]
    fn selection_is_half_open_reversible_and_copy_survives_cancel() {
        let mut state = Selection::default();
        state.reconcile(Some(identity(1)));
        state.toggle(84);
        assert!(state.copy().is_err());
        state.move_to(60);
        assert_eq!(state.range(), Some(60..84));
        let copied = state.copy().unwrap();
        assert!(!state.active);
        state.move_to(100);
        assert_eq!(state.range(), Some(60..84));
        state.cancel();
        assert_eq!(state.range(), None);
        assert_eq!(copied.ordinals, 60..84);
    }

    #[test]
    fn finish_freezes_range_and_identity_change_discards_selection() {
        let mut state = Selection::default();
        state.reconcile(Some(identity(1)));
        state.toggle(0);
        state.move_to(240);
        state.toggle(240);
        state.move_to(12);
        assert_eq!(state.range(), Some(0..240));
        state.reconcile(Some(identity(2)));
        assert_eq!(state.range(), None);
        assert!(!state.active);
    }

    #[test]
    fn paste_slot_requires_selection_and_keeps_explicit_group_edges() {
        let rows = vec![BeatRow {
            id: NodeId::new("beat").unwrap(),
            label: "Beat".into(),
            kind: "Original".into(),
            start: 120,
            frames: 24,
        }];
        assert_eq!(paste_slot(&rows, Some(&rows[0].id), true), Ok(0));
        assert_eq!(paste_slot(&rows, Some(&rows[0].id), false), Ok(1));
        assert!(paste_slot(&rows, None, false).is_err());
        assert_eq!(paste_slot(&[], None, false), Ok(0));
    }
}
