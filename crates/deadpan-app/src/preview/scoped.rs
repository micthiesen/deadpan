//! Inspector navigation has an explicit authoring address, separate from the
//! ordinary Sequence timeline and the concrete occurrence shown in the viewer.

use super::*;
use crate::navigation::command::{Entry, ScopeChoice};
use crate::project::scoped::Target;

pub(super) mod model;

const STRUCTURE_UNAVAILABLE: &str = "Inside Repeat and Retime contents, gain, Camera and pause audio are available. Return to the parent to change timing or copy a range.";

impl DeadpanApp {
    pub(super) fn reconcile_scoped(
        &mut self,
        before: Option<&Target>,
        receipt: Option<&crate::project::scoped::Commit>,
        saved_mark: Option<&crate::project::marks::Saved>,
    ) {
        let Some(state) = &mut self.scoped else {
            return;
        };
        let Some(workspace) = &self.workspace else {
            self.scoped = None;
            return;
        };
        if before.is_some_and(|before| {
            before.session == workspace.session
                && &before.revision == workspace.document.revision_id()
        }) {
            return;
        }
        let result = match receipt.filter(|receipt| {
            Some(&receipt.before) == before && &receipt.revision == workspace.document.revision_id()
        }) {
            Some(receipt) => state.reconcile(workspace, receipt),
            None => Ok(saved_mark.is_some_and(|saved| state.rebase_mark(workspace, saved))),
        };
        match result {
            Ok(true) => {}
            Ok(false) => self.scoped = None,
            Err(error) => {
                self.scoped = None;
                self.error = Some(error);
            }
        }
    }

    pub(super) fn scoped_target(&self) -> Result<Option<Target>, String> {
        self.scoped
            .as_ref()
            .map(|state| {
                let workspace = self.workspace.as_ref().ok_or("The project was closed.")?;
                let cursor = ProjectFrame(
                    i64::try_from(self.sequence_cursor).map_err(|error| error.to_string())?,
                );
                state.target(workspace, cursor)
            })
            .transpose()
    }

    pub(super) fn scoped_presentation(&self) -> Result<Option<model::Presentation>, String> {
        match (&self.scoped, &self.workspace) {
            (Some(state), Some(workspace)) => state.presentation(workspace),
            (Some(_), None) => Err("The project was closed.".into()),
            (None, _) => Ok(None),
        }
    }

    pub(super) fn inspected_node(&self) -> Option<&NodeId> {
        self.scoped
            .as_ref()
            .map(|state| &state.selected_target().node)
            .or(self.selected_beat.as_ref())
    }

    pub(super) fn inspected_target_matches(&self, node: &NodeId, target: Option<&Target>) -> bool {
        match target {
            Some(target) => self
                .scoped_target()
                .is_ok_and(|current| current.as_ref() == Some(target)),
            None => self.scoped.is_none() && self.selected_beat.as_ref() == Some(node),
        }
    }

    pub(super) fn refuse_scoped_structure(&mut self) -> bool {
        if self.view != View::Sequence {
            self.scoped = None;
        }
        if self.scoped.is_none() {
            return false;
        }
        self.error = Some(STRUCTURE_UNAVAILABLE.into());
        true
    }

    /// Opening a branch is entirely read-only. The service isolates a play only
    /// as part of the eventual atomic value edit.
    pub(super) fn enter_scoped(&mut self, context: &egui::Context) -> bool {
        if self.scoped.is_some() {
            let result = self
                .workspace
                .as_ref()
                .ok_or_else(|| "Open a project first.".to_owned())
                .and_then(|workspace| {
                    self.scoped
                        .as_mut()
                        .expect("scope checked")
                        .enter(workspace)
                });
            match result {
                Ok(true) => self.scoped_changed(context),
                Ok(false) => {
                    self.message = Some(
                        "This beat has no children. Gain, Camera and pause audio act on this beat."
                            .into(),
                    )
                }
                Err(error) => self.error = Some(error),
            }
            return true;
        }
        if self.view != View::Sequence {
            return false;
        }
        let Some(workspace) = &self.workspace else {
            return false;
        };
        let Some(root) = &self.selected_beat else {
            return false;
        };
        if !workspace.document.nodes().get(root).is_some_and(|node| {
            matches!(node.kind, NodeKind::Repeat { .. } | NodeKind::Retime { .. })
        }) {
            return false;
        }
        if self.macros.recording() {
            self.error = Some("Finish recording the macro before entering Repeat contents.".into());
            return true;
        }
        let result = i64::try_from(self.sequence_cursor)
            .map_err(|error| error.to_string())
            .and_then(|cursor| {
                model::State::new(
                    workspace,
                    self.sequence_scope.clone(),
                    root.clone(),
                    ProjectFrame(cursor),
                )
            });
        match result {
            Ok(state) => {
                self.scoped = Some(state);
                self.scoped_changed(context);
            }
            Err(error) => self.error = Some(error),
        }
        true
    }

    pub(super) fn scoped_leave(&mut self, context: &egui::Context) {
        let result = self
            .workspace
            .as_ref()
            .ok_or_else(|| "Open a project first.".to_owned())
            .and_then(|workspace| {
                self.scoped
                    .as_mut()
                    .ok_or("No nested contents are open.")?
                    .leave(workspace)
            });
        match result {
            Ok(true) => self.scoped_changed(context),
            Ok(false) => {
                self.cancel_camera();
                self.stop_playback();
                self.scoped = None;
                self.bindings.clear();
                self.pane = Pane::Sequence;
                self.message = Some("Returned to the parent beat.".into());
                context.memory_mut(|memory| memory.request_focus(pane_id(Pane::Sequence)));
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn scoped_changed(&mut self, context: &egui::Context) {
        self.cancel_camera();
        self.stop_playback();
        self.bindings.clear();
        self.edit_range.clear();
        self.pane = Pane::Sequence;
        self.reveal_beat = true;
        self.view = View::Sequence;
        self.error = None;
        match self.scoped_presentation() {
            Ok(Some(presentation)) => {
                if !presentation.frames.contains(&self.sequence_cursor) {
                    self.sequence_cursor = presentation.frames.start;
                }
                self.request_picture(false);
                self.message = None;
            }
            Ok(None) => {
                self.message = self.workspace.as_ref().and_then(|workspace| {
                    self.scoped
                        .as_ref()?
                        .no_picture_reason(workspace)
                        .ok()
                        .flatten()
                });
            }
            Err(error) => self.error = Some(error),
        }
        context.memory_mut(|memory| memory.request_focus(pane_id(Pane::Sequence)));
        context.request_repaint();
    }

    fn scoped_choose(&mut self, choice: ScopeChoice, context: &egui::Context) {
        let result = (|| {
            let workspace = self.workspace.as_ref().ok_or("Open a project first.")?;
            let state = self
                .scoped
                .as_mut()
                .ok_or("Enter a Repeat before choosing its scope.")?;
            match choice {
                ScopeChoice::All => state.switch_all(workspace),
                ScopeChoice::Play(play) => state.switch_play(workspace, play),
            }
        })();
        match result {
            Ok(()) => self.scoped_changed(context),
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn scoped_command(
        &mut self,
        choice: ScopeChoice,
        captured: Option<Result<Option<Target>, String>>,
        context: &egui::Context,
    ) {
        let valid = captured
            .and_then(Result::ok)
            .flatten()
            .is_some_and(|target| {
                self.inspected_target_matches(&target.target.node, Some(&target))
            });
        if !valid {
            self.error = Some(
                "Enter a Repeat, then open :scope again to capture its current contents.".into(),
            );
            return;
        }
        self.scoped_choose(choice, context);
    }

    pub(super) fn scoped_command_blocked(&mut self, command: &Result<Entry, String>) -> bool {
        if self.view != View::Sequence {
            self.scoped = None;
        }
        if self.scoped.is_none() {
            return false;
        }
        let blocked = matches!(command, Ok(Entry::Splice | Entry::Slip(_) | Entry::Trim(_)))
            || matches!(command, Ok(Entry::Action(action)) if scoped_structural_action(*action));
        if blocked {
            self.error = Some(STRUCTURE_UNAVAILABLE.into());
        }
        blocked
    }

    pub(super) fn scoped_action(&mut self, action: Action, context: &egui::Context) -> bool {
        if self.view != View::Sequence {
            self.scoped = None;
        }
        if self.scoped.is_none() {
            return false;
        }
        if matches!(self.pane, Pane::Sources | Pane::Sounds)
            || self.sound_focused()
            || self.event_focused()
        {
            return false;
        }
        if scoped_structural_action(action) {
            self.error = Some(STRUCTURE_UNAVAILABLE.into());
            return true;
        }
        match action {
            Action::EnterGroup => { self.enter_scoped(context); }
            Action::LeaveGroup => self.scoped_leave(context),
            Action::Beat { forward, count } => {
                let result = self.workspace.as_ref().ok_or_else(|| "Open a project first.".to_owned())
                    .and_then(|workspace| self.scoped.as_mut().expect("scope checked").step(workspace, forward, count));
                match result { Ok(()) => self.scoped_changed(context), Err(error) => self.error = Some(error) }
            }
            Action::Step { .. } | Action::First | Action::Last => {
                match self.scoped_presentation() {
                    Ok(Some(presentation)) => {
                        self.cancel_camera(); self.stop_playback(); self.bindings.clear();
                        let end = presentation.frames.end.saturating_sub(1);
                        self.sequence_cursor = match action {
                            Action::First => presentation.frames.start,
                            Action::Last => end,
                            Action::Step { forward: true, count } => self.sequence_cursor.saturating_add(u64::from(count)).clamp(presentation.frames.start, end),
                            Action::Step { forward: false, count } => self.sequence_cursor.saturating_sub(u64::from(count)).clamp(presentation.frames.start, end),
                            _ => unreachable!(),
                        };
                        self.request_picture(false);
                    }
                    Ok(None) => self.error = Some("This definition has no sampled picture in the selected play. Choose a visible play to seek.".into()),
                    Err(error) => self.error = Some(error),
                }
            }
            Action::Escape if !self.help_open => {
                self.scoped = None;
                self.cancel_camera();
                // Keep the ordinary Escape path's playback, register and
                // selection cleanup after leaving this inspector.
                return false;
            }
            _ => return false,
        }
        true
    }

    pub(super) fn scoped_timeline(&mut self, ui: &mut egui::Ui) {
        let mut layout = self.workspace_layout(ui);
        if self.gain.is_some() {
            layout.beats = 42.0;
        }
        let details = (|| {
            let workspace = self.workspace.as_ref().ok_or("Open a project first.")?;
            let state = self.scoped.as_ref().ok_or("No nested contents are open.")?;
            Ok::<_, String>((
                state.breadcrumb(workspace)?,
                state.rows(workspace)?,
                state.repeat_choice(workspace)?,
            ))
        })();
        style::beat_panel(layout).show(ui, |ui| {
            let (breadcrumb, rows, choice) = match details {
                Ok(details) => details,
                Err(error) => {
                    ui.colored_label(style::MUTED, error);
                    return;
                }
            };
            ui.spacing_mut().item_spacing.y = 4.0;
            if self.gain.is_some() {
                if let Some(row) = rows.iter().find(|row| row.selected) {
                    let label = format!("{} · {}", row.label, self.beat_scope_label());
                    ui.horizontal(|ui| {
                        ui.weak("GAIN OWNER");
                        ui.add(egui::Label::new(&label).truncate())
                            .on_hover_text(label);
                    });
                }
                return;
            }
            let heading = ui
                .horizontal(|ui| {
                    if ui
                        .small_button(format!(
                            "Parent  {}",
                            self.editor_key(EditorKey::LeaveGroup)
                        ))
                        .clicked()
                    {
                        self.scoped_leave(ui.ctx());
                    }
                    let label = breadcrumb.join(" › ");
                    ui.add(egui::Label::new(egui::RichText::new(&label).strong()).truncate())
                        .on_hover_text(label)
                })
                .inner;
            if pane_focus(
                ui,
                Pane::Sequence,
                heading.rect,
                "Nested beat contents pane",
            )
            .has_focus()
            {
                self.pane = Pane::Sequence;
            }
            if let Some(choice) = choice {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .selectable_label(choice.one_based.is_none(), "All plays  :scope all")
                        .on_hover_text(format!(
                            "Edit the shared definition of {}.",
                            choice.owner_label
                        ))
                        .clicked()
                    {
                        self.scoped_choose(ScopeChoice::All, ui.ctx());
                    }
                    let play = choice.one_based.unwrap_or(1);
                    if ui
                        .selectable_label(
                            choice.one_based.is_some(),
                            format!("This play {play}/{}  :scope play N", choice.plays),
                        )
                        .on_hover_text(format!("Edit only play {play} of {}.", choice.owner_label))
                        .clicked()
                    {
                        self.scoped_choose(ScopeChoice::Play(play), ui.ctx());
                    }
                    if ui
                        .add_enabled(play > 1, egui::Button::new("Previous"))
                        .on_hover_text("Previous play")
                        .clicked()
                    {
                        self.scoped_choose(ScopeChoice::Play(play - 1), ui.ctx());
                    }
                    if ui
                        .add_enabled(play < choice.plays, egui::Button::new("Next"))
                        .on_hover_text("Next play")
                        .clicked()
                    {
                        self.scoped_choose(ScopeChoice::Play(play + 1), ui.ctx());
                    }
                });
            }
            let mut picked = None;
            let mut scroll = egui::ScrollArea::vertical()
                .id_salt("scoped-beat-list")
                .min_scrolled_height(0.0)
                .max_height(ui.available_height().max(0.0));
            if std::mem::take(&mut self.reveal_beat)
                && let Some(index) = rows.iter().position(|row| row.selected)
            {
                scroll = scroll
                    .vertical_scroll_offset(index as f32 * (28.0 + ui.spacing().item_spacing.y));
            }
            scroll.show_rows(ui, 28.0, rows.len(), |ui, range| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                for index in range {
                    let row = &rows[index];
                    let text = format!("{}. {} · {}", index + 1, row.label, row.kind);
                    let response = ui
                        .push_id((&row.target.node, index), |ui| {
                            ui.selectable_label(row.selected, text)
                        })
                        .inner;
                    if response.on_hover_text(&row.label).clicked() {
                        picked = Some(index);
                    }
                }
            });
            if let Some(index) = picked {
                let result = self
                    .workspace
                    .as_ref()
                    .ok_or_else(|| "Open a project first.".to_owned())
                    .and_then(|workspace| {
                        self.scoped
                            .as_mut()
                            .ok_or("No nested contents are open.")?
                            .select(workspace, index)
                    });
                match result {
                    Ok(()) => self.scoped_changed(ui.ctx()),
                    Err(error) => self.error = Some(error),
                }
            }
        });
    }

    pub(super) fn scoped_inspector(&mut self, ui: &mut egui::Ui) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let Some(state) = &self.scoped else {
            return;
        };
        let Some(node) = workspace
            .document
            .nodes()
            .get(&state.selected_target().node)
        else {
            return;
        };
        let label = node.label.clone();
        let hold = matches!(node.kind, NodeKind::Hold { .. });
        let composite = matches!(
            node.kind,
            NodeKind::Sequence { .. } | NodeKind::Repeat { .. } | NodeKind::Retime { .. }
        );
        let scope = state.scope_label(workspace).unwrap_or_else(|error| error);
        let reason = state.no_picture_reason(workspace).ok().flatten();
        let interval = state.visible_range(workspace).ok().flatten().map(|range| {
            let boundary = |value: deadpan_core::ExactRatio| {
                if value.denominator() == 1 {
                    value.numerator().to_string()
                } else {
                    format!("{}/{}", value.numerator(), value.denominator())
                }
            };
            format!(
                "Edit interval [{}..{}) f",
                boundary(range.start),
                boundary(range.end)
            )
        });
        let layout = self.workspace_layout(ui);
        egui::Panel::right("workspace-inspector").resizable(false)
            .default_size(layout.inspector).min_size(layout.inspector).max_size(layout.inspector)
            .frame(style::panel()).show(ui, |ui| {
                let heading = pane_heading(ui, "INSPECTOR", self.pane == Pane::Inspector);
                if pane_focus(ui, Pane::Inspector, heading.rect, "Nested beat inspector pane").has_focus() { self.pane = Pane::Inspector; }
                ui.separator();
                egui::ScrollArea::vertical().id_salt("scoped-inspector-details").show(ui, |ui| {
                    ui.heading(label);
                    ui.colored_label(style::LAVENDER, scope);
                    if let Some(interval) = interval { ui.small(interval); }
                    ui.small("All plays changes the shared definition. Existing overrides keep their own changes.");
                    if let Some(reason) = reason { ui.add_space(8.0); ui.weak(reason); ui.small("The viewer retains its last displayed picture."); }
                    let ready = !self.service.is_busy() && !self.dialogs.is_open();
                    if composite && ui.button(format!("Enter contents  {}", self.editor_key(EditorKey::EnterGroup))).clicked() { self.enter_scoped(ui.ctx()); }
                    self.gain_inspector(ui, ready);
                    if hold { self.hold_audio_controls(ui, ready); }
                    if ui.add_enabled(ready, egui::Button::new(format!("Camera…  {}", self.editor_key(EditorKey::Camera)))).clicked() {
                        self.framing_action(navigation::FramingAction::EnterCamera, ui.ctx());
                    }
                    ui.separator();
                    ui.small(format!("{} children · {} enter · {} parent", self.editor_pair(EditorKey::BeatPrevious, EditorKey::BeatNext, "/"), self.editor_key(EditorKey::EnterGroup), self.editor_key(EditorKey::LeaveGroup)));
                    ui.small(STRUCTURE_UNAVAILABLE);
                });
            });
    }
}

fn scoped_structural_action(action: Action) -> bool {
    matches!(
        action,
        Action::Insert
            | Action::OfferInsert
            | Action::Edit(_)
            | Action::Trim
            | Action::VisualMoment
            | Action::CopyMoment
            | Action::Operator { .. }
            | Action::Repeat { .. }
            | Action::DeleteSelection
            | Action::DeleteFrames(_)
            | Action::RepeatLast
            | Action::PasteMoment { .. }
            | Action::MacroRecord(_)
            | Action::MacroExecute { .. }
    )
}
