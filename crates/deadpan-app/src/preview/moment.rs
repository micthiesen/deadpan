//! Ephemeral half-open Original selection and an identity-bound copy register.

use std::ops::Range;

use deadpan_core::SourceQualificationId;

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Identity {
    session: u64,
    asset: AssetId,
    qualification: SourceQualificationId,
}

#[derive(Clone, Debug)]
pub(super) struct Copied {
    identity: Identity,
    pub ordinals: Range<u64>,
}

#[derive(Default)]
pub(super) struct Selection {
    identity: Option<Identity>,
    bounds: Option<(u64, u64)>,
    pub active: bool,
    pub copied: Option<Copied>,
}

impl Selection {
    pub(super) fn copied_audio_selection(&self) -> Option<crate::project::RoomToneSelection> {
        let copied = self.copied.as_ref()?;
        Some(crate::project::RoomToneSelection::Original {
            asset: copied.identity.asset.clone(),
            qualification: copied.identity.qualification.clone(),
            ordinals: copied.ordinals.clone(),
        })
    }
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

    fn copy(&mut self) -> Result<(), String> {
        let identity = self
            .identity
            .clone()
            .ok_or("Choose a registered Original first")?;
        let ordinals = self
            .range()
            .ok_or("Select a nonempty Original range: v, then h/l, then y")?;
        self.copied = Some(Copied { identity, ordinals });
        self.active = false;
        Ok(())
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
        if self.moment.copied.as_ref().is_some_and(|copied| {
            self.workspace.as_ref().is_none_or(|workspace| {
                workspace.session != copied.identity.session
                    || workspace
                        .sources
                        .get(&copied.identity.asset)
                        .is_none_or(|source| source.receipt.id() != &copied.identity.qualification)
            })
        }) {
            self.moment.copied = None;
        }
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
        self.message = Some(
            if self.moment.active {
                "Move with h/l to select time. y copies; v finishes; Esc cancels."
            } else {
                if self.moment.range().is_some() {
                    "Selection retained. y copies this moment."
                } else {
                    "Empty selection. Press v and move with h/l before copying."
                }
            }
            .into(),
        );
    }

    pub(super) fn copy_moment(&mut self) {
        self.bindings.clear();
        self.reconcile_moment();
        if self.view != View::Source {
            self.error = Some("Select and copy a moment in Original (:source)".into());
            return;
        }
        match self.moment.copy() {
            Ok(()) => {
                self.error = None;
                self.message = Some("Moment copied. Return to Your edit (:sequence), then p after or P before a beat.".into());
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn paste_moment(&mut self, before: bool) {
        self.bindings.clear();
        self.reconcile_moment();
        if self.view != View::Sequence {
            self.message =
                Some("Return to Your edit (:sequence) to paste beside the selected beat.".into());
            return;
        }
        let result = (|| {
            let copied = self
                .moment
                .copied
                .as_ref()
                .ok_or("Copy an Original range first: :source, v, h/l, y")?;
            let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
            let scope = self.sequence_scope.resolve(workspace)?;
            let index = paste_slot(&self.beat_rows, self.selected_beat.as_ref(), before)?;
            Ok::<_, String>(ProjectRequest::PasteMoment(crate::project::MomentPaste {
                expected_session: workspace.session,
                expected_revision: workspace.document.revision_id().clone(),
                asset: copied.identity.asset.clone(),
                qualification: copied.identity.qualification.clone(),
                ordinals: copied.ordinals.clone(),
                scope: self.sequence_scope.clone(),
                parent: scope.owner.clone(),
                index,
            }))
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
                        range_label(range.as_ref(), self.moment.active)
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
            let label = range_label(range.as_ref(), self.moment.active);
            ui.add(egui::Label::new(egui::RichText::new(&label).color(style::LAVENDER)).truncate())
                .on_hover_text(label);
        } else if self.view == View::Sequence
            && let Some(copied) = &self.moment.copied
        {
            let label = format!(
                "Copied Original [{}..{})",
                copied.ordinals.start, copied.ordinals.end
            );
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(style::LAVENDER, label);
                if ui
                    .add_enabled(!self.service.is_busy(), egui::Button::new("Paste after  p"))
                    .clicked()
                {
                    self.pane = Pane::Viewer;
                    self.paste_moment(false);
                }
                if ui
                    .add_enabled(
                        !self.service.is_busy(),
                        egui::Button::new("Paste before  P"),
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
                            "Move with h/l to select a nonempty range. v finishes; Esc cancels."
                        } else {
                            "Press v, then move with h/l. Counts work: 24l selects 24 presentation frames."
                        });
                    }
                    ui.add_space(8.0);
                    if ui.button(if self.moment.active { "Finish selection  v" } else { "Select moment  v" }).clicked() { self.pane = Pane::Inspector; self.visual_moment(); }
                    if ui.add_enabled(self.moment.range().is_some(), egui::Button::new("Copy moment  y").fill(style::SELECTED)).clicked() { self.pane = Pane::Inspector; self.copy_moment(); }
                    if ui.button("Cancel selection  Esc").clicked() { self.pane = Pane::Inspector; self.moment.cancel(); }
                    ui.separator();
                    ui.weak("Your Original stays intact. Copying does not change the project.");
                    if let Some(copied) = &self.moment.copied {
                        ui.colored_label(style::LAVENDER, format!("Copied [{}..{})", copied.ordinals.start, copied.ordinals.end));
                        ui.weak("Return to Your edit, choose a beat, then p after or P before. One paste, one undo.");
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

fn range_label(range: Option<&Range<u64>>, active: bool) -> String {
    range.map_or_else(
        || {
            if active {
                "Original time · h/l extends selection".into()
            } else {
                "Original time · v starts selection".into()
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
        state.copy().unwrap();
        assert!(!state.active);
        state.move_to(100);
        assert_eq!(state.range(), Some(60..84));
        state.cancel();
        assert_eq!(state.range(), None);
        assert_eq!(state.copied.unwrap().ordinals, 60..84);
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
