//! Ephemeral Sequence navigation. Picture and edit cursors keep the root clock.

use super::*;

impl DeadpanApp {
    pub(super) fn beat_scope_label(&self) -> &'static str {
        if self.sequence_scope.groups().is_empty() {
            "Root beat"
        } else {
            "Group beat"
        }
    }

    pub(super) fn selected_group(&self) -> bool {
        self.selected_beat.as_ref().is_some_and(|id| {
            self.beat_rows.iter().any(|row| &row.id == id)
                && self.workspace.as_ref().is_some_and(|workspace| {
                    matches!(
                        workspace.document.nodes().get(id).map(|node| &node.kind),
                        Some(NodeKind::Sequence { .. })
                    )
                })
        })
    }

    pub(super) fn enter_group(&mut self, context: &egui::Context) {
        self.bindings.clear();
        if self.view != View::Sequence {
            self.message = Some("Return to Your edit (:sequence) to enter a group.".into());
            return;
        }
        let (Some(workspace), Some(node)) = (&self.workspace, &self.selected_beat) else {
            self.error = Some("Select a Sequence group to enter.".into());
            return;
        };
        match self.sequence_scope.descend(workspace, node) {
            Ok(scope) => self.change_scope(scope, None, context),
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn leave_group(&mut self, context: &egui::Context) {
        self.bindings.clear();
        if self.view != View::Sequence {
            self.message = Some("Return to Your edit (:sequence) to navigate groups.".into());
            return;
        }
        let Some(parent) = self.sequence_scope.parent() else {
            self.message = Some("Already viewing the full edit.".into());
            return;
        };
        let exited = self.sequence_scope.groups().last().cloned();
        self.change_scope(parent, exited, context);
    }

    fn change_scope(
        &mut self,
        scope: SequenceScope,
        selected: Option<NodeId>,
        context: &egui::Context,
    ) {
        self.cancel_camera();
        self.stop_playback();
        self.bindings.clear();
        self.sequence_scope = scope;
        self.rebuild_rows();
        let (cursor, selected) = selection::after_scope_change(
            &self.beat_rows,
            self.scope_start..=self.scope_end,
            self.sequence_cursor,
            selected.as_ref(),
        );
        self.sequence_cursor = cursor;
        self.selected_beat = selected.map(|index| self.beat_rows[index].id.clone());
        self.view.set(View::Sequence, &mut self.message);
        self.pane = Pane::Sequence;
        self.reveal_beat = true;
        self.error = None;
        self.message = None;
        self.request_picture(false);
        context.memory_mut(|memory| memory.request_focus(pane_id(Pane::Sequence)));
        context.request_repaint();
    }

    /// Explicit transport completion follows the heard cursor. Context-preserving
    /// stops (command entry, help, inspector actions) deliberately do not call this.
    pub(super) fn follow_playhead_scope(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let scope = self
            .sequence_scope
            .enclosing_cursor(workspace, self.sequence_cursor);
        if scope != self.sequence_scope {
            self.sequence_scope = scope;
            self.bindings.clear();
            self.rebuild_rows();
        }
    }

    pub(super) fn scope_clock_label(&self) -> String {
        if self.sequence_scope.groups().is_empty() {
            format!(
                "Edit boundary {} / {}",
                self.sequence_cursor,
                self.sequence_length()
            )
        } else if (self.scope_start..=self.scope_end).contains(&self.sequence_cursor) {
            format!(
                "Group {} / {} f · Edit {} f",
                self.sequence_cursor - self.scope_start,
                self.scope_end - self.scope_start,
                self.sequence_cursor
            )
        } else {
            format!("Cursor outside group · Edit {} f", self.sequence_cursor)
        }
    }

    pub(super) fn sequence_heading(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let (heading, destination) = draw_heading(
            ui,
            self.pane == Pane::Sequence,
            self.focused_workflow(),
            &self.scope_labels,
            self.beat_rows.len(),
            self.scope_end - self.scope_start,
        );
        if let Some(depth) = destination {
            let mut scope = self.sequence_scope.clone();
            let mut exited = None;
            while scope.groups().len() > depth {
                exited = scope.groups().last().cloned();
                let Some(parent) = scope.parent() else {
                    break;
                };
                scope = parent;
            }
            self.change_scope(scope, exited, ui.ctx());
        }
        heading
    }
}

/// Shared with CPU-only layout checks; the caller owns navigation and repaint.
pub(super) fn draw_heading(
    ui: &mut egui::Ui,
    focused: bool,
    single_original: bool,
    labels: &[String],
    beats: usize,
    frames: u64,
) -> (egui::Response, Option<usize>) {
    let mut destination = None;
    let depth = labels.len();
    let heading = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let heading = ui.label(egui::RichText::new("BEATS").size(13.0).strong());
            if focused {
                ui.label(
                    egui::RichText::new("FOCUS")
                        .size(9.0)
                        .color(style::LAVENDER),
                );
            }
            if depth > 0
                && ui
                    .button("‹  Backspace")
                    .on_hover_text("Return to the parent group; keep the project cursor.")
                    .clicked()
            {
                destination = Some(depth - 1);
            }
            egui::ScrollArea::horizontal()
                .id_salt("sequence-breadcrumbs")
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        let root = if single_original {
                            "Your edit"
                        } else {
                            "Sequence"
                        };
                        for (index, label) in std::iter::once(root)
                            .chain(labels.iter().map(String::as_str))
                            .enumerate()
                        {
                            if index > 0 {
                                ui.weak("›");
                            }
                            if ui
                                .add_enabled(
                                    index != depth,
                                    egui::Button::new(label).selected(index == depth),
                                )
                                .on_hover_text(format!(
                                    "View {label}. Enter opens a selected group; Backspace returns."
                                ))
                                .clicked()
                                && index != depth
                            {
                                destination = Some(index);
                            }
                        }
                        ui.weak(format!("{beats} beats · {frames} f"));
                    });
                });
            heading
        })
        .inner;
    (heading, destination)
}
