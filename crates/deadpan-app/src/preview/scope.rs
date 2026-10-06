//! Ephemeral Sequence navigation. Picture and edit cursors keep the root clock.

use super::*;

impl DeadpanApp {
    pub(super) fn beat_scope_label(&self) -> String {
        if let (Some(state), Some(workspace)) = (&self.scoped, &self.workspace) {
            return state.scope_label(workspace).unwrap_or_else(|error| error);
        }
        if self.sequence_scope.groups().is_empty() {
            "Root beat".into()
        } else {
            "Group beat".into()
        }
    }

    /// The selected visible beat is a Repeat, whose plays `]r`/`[r` open.
    pub(super) fn selected_repeat(&self) -> bool {
        self.selected_beat.as_ref().is_some_and(|id| {
            self.beat_rows.iter().any(|row| &row.id == id)
                && self.workspace.as_ref().is_some_and(|workspace| {
                    matches!(
                        workspace.document.nodes().get(id).map(|node| &node.kind),
                        Some(NodeKind::Repeat { .. })
                    )
                })
        })
    }

    pub(super) fn selected_group(&self) -> bool {
        self.selected_beat.as_ref().is_some_and(|id| {
            self.beat_rows.iter().any(|row| &row.id == id)
                && self.workspace.as_ref().is_some_and(|workspace| {
                    matches!(
                        workspace.document.nodes().get(id).map(|node| &node.kind),
                        Some(
                            NodeKind::Sequence { .. }
                                | NodeKind::Repeat { .. }
                                | NodeKind::Retime { .. }
                        )
                    )
                })
        })
    }

    pub(super) fn enter_group(&mut self, context: &egui::Context) {
        if self.enter_scoped(context) {
            return;
        }
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
        if self.scoped.is_some() {
            self.scoped_leave(context);
            return;
        }
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
        if self.scoped.is_some() {
            return;
        }
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

    pub(super) fn sequence_heading(
        &mut self,
        ui: &mut egui::Ui,
        compact_sounds_heading: bool,
    ) -> egui::Response {
        let (heading, destination) = if compact_sounds_heading {
            let (heading, destination, sounds) = draw_compact_heading(
                ui,
                self.pane == Pane::Sequence,
                SoundsHeading {
                    focused: self.pane == Pane::Sounds,
                    count: self
                        .workspace
                        .as_ref()
                        .map_or(0, |w| w.document.sounds().len()),
                },
                &self.scope_labels,
                self.beat_rows.len(),
                self.scope_end - self.scope_start,
                &self.bindings,
            );
            if pane_focus(ui, Pane::Sounds, sounds.rect, "Placed sounds pane").has_focus()
                && (self.pane != Pane::Sounds || self.view != View::Sequence)
            {
                self.focus_events(ui.ctx());
            }
            (heading, destination)
        } else {
            draw_heading(
                ui,
                self.pane == Pane::Sequence,
                self.focused_workflow(),
                &self.scope_labels,
                self.beat_rows.len(),
                self.scope_end - self.scope_start,
                &self.bindings,
            )
        };
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

#[derive(Default)]
struct SoundsHeading {
    focused: bool,
    count: usize,
}

fn draw_compact_heading(
    ui: &mut egui::Ui,
    focused: bool,
    sound_state: SoundsHeading,
    labels: &[String],
    beats: usize,
    frames: u64,
    bindings: &Bindings,
) -> (egui::Response, Option<usize>, egui::Response) {
    let text = format!(
        "PLACED SOUNDS {} · {}{}",
        sound_state.count,
        if sound_state.count == 0 {
            format!("{} place", bindings.key_label(EditorKey::PlaceSound))
        } else {
            ":sounds".into()
        },
        if sound_state.focused { " · FOCUS" } else { "" }
    );
    // A long configured path may use several lines, but cannot take the
    // breadcrumb's entire width or escape the viewport.
    let available = ui.available_width().max(1.0);
    let sounds = egui::WidgetText::from(egui::RichText::new(text).size(11.0).strong().color(
        if sound_state.focused {
            style::LAVENDER
        } else {
            style::muted(ui)
        },
    ))
    .into_galley(
        ui,
        Some(egui::TextWrapMode::Wrap),
        (available * 0.55).max(1.0),
        egui::TextStyle::Body,
    );
    let gap = 8.0;
    let width = (ui.available_width() - sounds.size().x - gap).max(0.0);
    // Anchor both columns before drawing. A breadcrumb's minimum scroll width
    // must not grow the left allocation and push the sound label off-screen.
    let origin = ui.next_widget_position();
    let left_rect =
        egui::Rect::from_min_max(origin, egui::pos2(origin.x + width, ui.max_rect().bottom()));
    let mut left = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("compact-beat-heading")
            .max_rect(left_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    left.set_clip_rect(ui.clip_rect().intersect(left_rect));
    let (heading, destination) =
        draw_heading(&mut left, focused, true, labels, beats, frames, bindings);
    let height = left.min_rect().height().max(sounds.size().y);
    let sounds_rect = egui::Rect::from_min_size(
        egui::pos2(
            origin.x + width + gap,
            origin.y + (height - sounds.size().y) * 0.5,
        ),
        sounds.size(),
    );
    let mut right = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("compact-sounds-heading")
            .max_rect(sounds_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    let sounds = right
        .label(sounds)
        .on_hover_text(if sound_state.count == 0 {
            format!(
                "Choose a catalog sound, then place it with {}. Picture length stays the same.",
                bindings.key_label(EditorKey::PlaceSound),
            )
        } else {
            "Open placed sounds in Your edit · :sounds".into()
        });
    ui.advance_cursor_after_rect(egui::Rect::from_min_size(
        origin,
        egui::vec2(available, height),
    ));
    (heading, destination, sounds)
}

/// Shared with CPU-only layout checks; the caller owns navigation and repaint.
pub(super) fn draw_heading(
    ui: &mut egui::Ui,
    focused: bool,
    single_original: bool,
    labels: &[String],
    beats: usize,
    frames: u64,
    bindings: &Bindings,
) -> (egui::Response, Option<usize>) {
    let mut destination = None;
    let depth = labels.len();
    let heading = ui
        .horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let heading = ui.label(style::section_title("BEATS", focused));
            if focused {
                style::focus_pill(ui);
            }
            if depth > 0
                && ui
                    .add(
                        egui::Button::new(format!(
                            "‹  {}",
                            bindings.key_label(EditorKey::LeaveGroup)
                        ))
                        .wrap(),
                    )
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
                            // The current level is plain text; only its
                            // ancestors are navigation targets.
                            if index == depth {
                                ui.label(style::semibold(label));
                                continue;
                            }
                            if ui
                                .add(
                                    egui::Button::new(egui::RichText::new(label).weak())
                                        .frame_when_inactive(false),
                                )
                                .on_hover_text(format!(
                                    "View {label}. {} opens a selected group; {} returns.",
                                    bindings.key_label(EditorKey::EnterGroup),
                                    bindings.key_label(EditorKey::LeaveGroup)
                                ))
                                .clicked()
                            {
                                destination = Some(index);
                            }
                        }
                        ui.add_space(4.0);
                        ui.weak(format!("{beats} beats · {frames} f"));
                    });
                });
            heading
        })
        .inner;
    (heading, destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_paths_paint_fully_in_both_compact_heading_controls() {
        let bindings = Bindings::from_json(
            &serde_json::to_vec(&serde_json::json!({
                "version": 1,
                "key_mode": "logical",
                "bindings": [
                    { "action": "group.leave", "keys": [vec!["F11"; 16]] },
                    { "action": "sound.place", "keys": [vec!["F12"; 16]] }
                ]
            }))
            .unwrap(),
        )
        .unwrap();
        let parent = format!("‹  {}", bindings.key_label(EditorKey::LeaveGroup));
        let sounds = format!(
            "PLACED SOUNDS 0 · {} place",
            bindings.key_label(EditorKey::PlaceSound)
        );
        for width in [360.0, 600.0] {
            let context = egui::Context::default();
            style::apply(&context);
            let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 300.0));
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(viewport),
                    ..Default::default()
                },
                |ui| {
                    draw_compact_heading(
                        ui,
                        true,
                        SoundsHeading::default(),
                        &["Nested group".into()],
                        2,
                        120,
                        &bindings,
                    );
                },
            );
            output.textures_delta.clear();
            for expected in [&parent, &sounds] {
                let matching = output
                    .shapes
                    .iter()
                    .filter_map(|clipped| {
                        let egui::Shape::Text(text) = &clipped.shape else {
                            return None;
                        };
                        (text.galley.text() == expected).then_some((clipped, text))
                    })
                    .collect::<Vec<_>>();
                assert_eq!(matching.len(), 1, "Missing full key teaching: {expected}");
                let (clipped, text) = matching[0];
                assert!(
                    clipped.clip_rect.contains_rect(text.visual_bounding_rect()),
                    "{expected}: painted {:?}, clip {:?}, viewport {viewport:?}",
                    text.visual_bounding_rect(),
                    clipped.clip_rect
                );
                assert!(
                    viewport.contains_rect(text.visual_bounding_rect()),
                    "{expected}: painted {:?}, viewport {viewport:?}",
                    text.visual_bounding_rect()
                );
            }
        }
    }

    #[test]
    fn compact_empty_sounds_stays_visible_beside_long_nested_breadcrumbs() {
        for sounds_focused in [false, true] {
            let context = egui::Context::default();
            style::apply(&context);
            let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 128.0));
            let labels = vec![
                "A deliberately long outer group label".into(),
                "Another deliberately long nested group label".into(),
            ];
            let mut targets = None;
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(viewport),
                    ..Default::default()
                },
                |ui| {
                    let (beats, _, sounds) = draw_compact_heading(
                        ui,
                        !sounds_focused,
                        SoundsHeading {
                            focused: sounds_focused,
                            count: 0,
                        },
                        &labels,
                        100_000,
                        u64::MAX,
                        &Bindings::default(),
                    );
                    targets = Some((beats.rect, sounds.rect));
                },
            );
            output.textures_delta.clear();
            let (beats, sounds) = targets.unwrap();
            assert!(!beats.intersects(sounds));
            assert!(viewport.contains_rect(beats));
            assert!(viewport.contains_rect(sounds));
            for clipped in &output.shapes {
                let egui::Shape::Text(text) = &clipped.shape else {
                    continue;
                };
                if !text.galley.text().starts_with("PLACED SOUNDS 0 · ,s place") {
                    let painted = clipped.clip_rect.intersect(text.visual_bounding_rect());
                    if painted.is_positive() {
                        assert!(painted.right() <= sounds.left());
                    }
                }
            }
            let labels = output
                .shapes
                .iter()
                .filter_map(|clipped| {
                    let egui::Shape::Text(text) = &clipped.shape else {
                        return None;
                    };
                    text.galley
                        .text()
                        .starts_with("PLACED SOUNDS 0 · ,s place")
                        .then_some((clipped, text))
                })
                .collect::<Vec<_>>();
            assert_eq!(labels.len(), 1);
            let (clipped, text) = labels[0];
            assert!(clipped.clip_rect.contains_rect(text.visual_bounding_rect()));
            assert!(viewport.contains_rect(text.visual_bounding_rect()));
            assert!(clipped.clip_rect.contains_rect(sounds));
            assert_eq!(text.galley.text().contains("FOCUS"), sounds_focused);
        }
    }

    #[test]
    fn compact_empty_sounds_does_not_cover_parent_group_navigation() {
        let context = egui::Context::default();
        style::apply(&context);
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 128.0));
        let labels = vec!["Outer group".into(), "Nested group".into()];
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(viewport),
                ..Default::default()
            },
            |ui| {
                draw_compact_heading(
                    ui,
                    true,
                    SoundsHeading::default(),
                    &labels,
                    2,
                    120,
                    &Bindings::default(),
                );
            },
        );
        output.textures_delta.clear();
        let back = output
            .shapes
            .iter()
            .find_map(|clipped| {
                let egui::Shape::Text(text) = &clipped.shape else {
                    return None;
                };
                (text.galley.text() == "‹  Backspace")
                    .then_some(text.visual_bounding_rect().center())
            })
            .expect("The parent-group action remains painted");
        let mut destination = None;
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(viewport),
                events: vec![
                    egui::Event::PointerMoved(back),
                    egui::Event::PointerButton {
                        pos: back,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                    egui::Event::PointerButton {
                        pos: back,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ui| {
                let (_, next, sounds) = draw_compact_heading(
                    ui,
                    true,
                    SoundsHeading::default(),
                    &labels,
                    2,
                    120,
                    &Bindings::default(),
                );
                let sounds = pane_focus(ui, Pane::Sounds, sounds.rect, "Placed sounds pane");
                assert!(!sounds.clicked());
                destination = next;
            },
        );
        output.textures_delta.clear();
        assert_eq!(destination, Some(1));
    }
}
