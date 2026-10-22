//! Inspector navigation has an explicit authoring address, separate from the
//! ordinary Sequence timeline and the concrete occurrence shown in the viewer.

use super::*;
use crate::navigation::command::{Entry, ScopeChoice};
use crate::project::scoped::Target;

pub(super) mod model;

const STRUCTURE_UNAVAILABLE: &str = "Inside Repeat and Retime contents, gain, Camera, pause audio and AI pictures are available. Repeat pauses follow each level's All plays, This play or selected plays choice. Pause length supports All plays at every level or This play at every level; AI pauses require one play at every level. Other timing edits and range copying require the parent.";

impl DeadpanApp {
    pub(super) fn focus_scoped_pane(&mut self, pane: Pane) {
        if self.pane != pane {
            self.revoke_scoped_continuation();
            self.pane = pane;
        }
    }
    pub(super) fn revoke_scoped_continuation(&mut self) {
        if let Some(next) = self.scoped_navigation.checked_add(1) {
            self.scoped_navigation = next;
        } else {
            // Exhaustion must never make an old receipt current again.
            self.scoped = None;
            self.error = Some("Scoped navigation generation exhausted; restart Deadpan.".into());
        }
    }
    pub(super) fn insert_scoped_pause(
        &mut self,
        input: navigation::duration::DurationInput,
        black: bool,
        ai: bool,
        target: Target,
    ) {
        let request = (|| {
            if self.view != View::Sequence
                || matches!(self.pane, Pane::Sources | Pane::Sounds)
                || self.scoped_target()?.as_ref() != Some(&target)
            {
                return Err(
                    "The pause insertion scope changed. Enter the command again; no edit was made."
                        .into(),
                );
            }
            if self.macros.recording() || self.macros.is_pending() {
                return Err("Finish the macro before inserting a scoped pause.".into());
            }
            let workspace = self.workspace.as_ref().ok_or("Open a project first.")?;
            if target.also.is_empty() {
                target.pause_instance(workspace)?;
            } else {
                target.validate(workspace)?;
            }
            let duration = input.resolve(workspace.document.presentation_basis().frame_rate)?;
            Ok(ProjectRequest::Edit {
                expected_session: target.session,
                expected_revision: target.revision.clone(),
                scope: target.scope.clone(),
                cursor: target.cursor,
                edit: ProjectEdit::ScopedPause {
                    target: Box::new(target),
                    duration,
                    black,
                    ai,
                },
            })
        })();
        match request {
            Ok(request) => {
                self.submit(request);
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn reconcile_scoped(
        &mut self,
        before: Option<&Target>,
        receipt: Option<(&crate::project::scoped::Commit, Option<ProjectFrame>)>,
        saved_mark: Option<&crate::project::marks::Saved>,
        mark_metadata_from: Option<&RevisionId>,
        saved_target: Option<&crate::project::targets::Saved>,
        saved_interpretation: Option<&crate::project::SourceInterpretationSaved>,
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
        let result = match receipt.filter(|(receipt, _)| {
            Some(&receipt.before) == before && &receipt.revision == workspace.document.revision_id()
        }) {
            Some((receipt, cursor)) => state.reconcile(workspace, receipt, cursor),
            None => Ok(mark_metadata_from
                .is_some_and(|before| state.rebase_mark_metadata(workspace, before))
                || saved_mark.is_some_and(|saved| state.rebase_mark(workspace, saved))
                || saved_target.is_some_and(|saved| state.rebase_target(workspace, saved))
                || saved_interpretation
                    .is_some_and(|saved| state.rebase_source_interpretation(workspace, saved))),
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
                let mut target = state.target(workspace, cursor)?;
                target.navigation = self.scoped_navigation;
                Ok(target)
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

    /// Reuse ordinary Hold parameter entry, retaining the captured play when
    /// command mode opens. Admission still runs against that exact snapshot.
    pub(super) fn scoped_hold_parameter(&self) -> Option<String> {
        self.scoped
            .as_ref()?
            .hold_parameter(self.workspace.as_ref()?)
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
                self.focus_scoped_pane(Pane::Sequence);
                self.message = Some("Returned to the parent beat.".into());
                context.memory_mut(|memory| memory.request_focus(pane_id(Pane::Sequence)));
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn scoped_changed(&mut self, context: &egui::Context) {
        self.revoke_scoped_continuation();
        self.macros.revoke_cursor();
        if self.macros.recording() {
            self.cancel_macro_recording();
        }
        self.ai_stop_preview();
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

    /// `]r`/`[r`. Inside contents it steps the nearest Repeat's plays; on a
    /// selected Repeat it opens play 1 (or its last play) read-only.
    pub(super) fn scoped_play(&mut self, forward: bool, count: u32, context: &egui::Context) {
        if self.scoped.is_none() {
            let repeat = self.view == View::Sequence
                && self
                    .workspace
                    .as_ref()
                    .zip(self.selected_beat.as_ref())
                    .is_some_and(|(workspace, node)| {
                        workspace
                            .document
                            .nodes()
                            .get(node)
                            .is_some_and(|node| matches!(node.kind, NodeKind::Repeat { .. }))
                    });
            if !repeat {
                self.error = Some(
                    "]r and [r step through a Repeat's plays. Select a Repeat beat first.".into(),
                );
                return;
            }
            if !self.enter_scoped(context) || self.scoped.is_none() {
                return;
            }
            // Opening starts at All plays: N]r opens play N, N[r the Nth play
            // from the end (both default to one).
            self.scoped_play(true, if forward { count } else { u32::MAX }, context);
            if !forward && count > 1 && self.scoped.is_some() {
                self.scoped_play(false, count - 1, context);
            }
            return;
        }
        let result = self
            .workspace
            .as_ref()
            .ok_or_else(|| "Open a project first.".to_owned())
            .and_then(|workspace| {
                self.scoped
                    .as_mut()
                    .expect("scope checked")
                    .step_play(workspace, forward, count)
            });
        match result {
            Ok(true) => self.scoped_changed(context),
            Ok(false) => {
                self.message = Some(if forward {
                    "Already at the last play.".into()
                } else {
                    "Already at All plays.".into()
                })
            }
            Err(error) => self.error = Some(error),
        }
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
                ScopeChoice::Plays(plays) => state.switch_plays(workspace, &plays),
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
        let blocked = matches!(
            command,
            Ok(Entry::Splice | Entry::Slip(_) | Entry::Trim(_) | Entry::Group { .. })
        ) || matches!(command, Ok(Entry::Action(action)) if scoped_structural_action(*action));
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
        let duration_entry = action == Action::EnterGroup && self.scoped_hold_parameter().is_some();
        let permission = if duration_entry {
            Action::Command
        } else {
            action
        };
        if (self.macros.recording() || duration_entry) && !self.macro_action_allowed(permission) {
            return true;
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
            Action::EnterGroup => { self.open_inspector_parameter(context); }
            Action::Play { forward, count } => self.scoped_play(forward, count, context),
            Action::LeaveGroup => self.scoped_leave(context),
            Action::Beat { forward, count } => {
                let result = self.workspace.as_ref().ok_or_else(|| "Open a project first.".to_owned())
                    .and_then(|workspace| self.scoped.as_mut().expect("scope checked").step(workspace, forward, count));
                match result { Ok(()) => self.scoped_changed(context), Err(error) => self.error = Some(error) }
            }
            Action::Step { .. } | Action::First | Action::Last => {
                match self.scoped_presentation() {
                    Ok(Some(presentation)) => {
                        self.revoke_scoped_continuation();
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
                self.revoke_scoped_continuation();
                if self.macros.recording() {
                    self.cancel_macro_recording();
                }
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
                    ui.colored_label(style::muted(ui), error);
                    return;
                }
            };
            ui.spacing_mut().item_spacing.y = 4.0;
            if let Some(draft) = &self.gain {
                if let Some(row) = rows.iter().find(|row| row.selected) {
                    let label = format!("{} · {}", row.label, self.beat_scope_label());
                    ui.horizontal(|ui| {
                        ui.weak(format!("{} OWNER", draft.editor_name().to_ascii_uppercase()));
                        accessibility::full_text(
                            ui.add(egui::Label::new(&label).truncate()),
                            &label,
                        )
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
                    accessibility::full_text(
                        ui.add(egui::Label::new(egui::RichText::new(&label).strong()).truncate()),
                        &label,
                    )
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
                self.focus_scoped_pane(Pane::Sequence);
            }
            if let Some(choice) = choice {
                if let Some(plays) = choice.label.strip_prefix("plays ") {
                    let plays = plays.split('/').next().unwrap_or(plays);
                    ui.label(
                        egui::RichText::new(format!(
                            "Plays {plays} edited together  :scope plays"
                        ))
                        .strong(),
                    )
                    .on_hover_text(
                        "Gain, Camera, pause insertion and pause-audio changes apply to this beat in every selected play, in one Undo.",
                    );
                }
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
                    let previous =
                        format!("Previous  {}", self.editor_key(EditorKey::PlayPrevious));
                    if ui
                        .add_enabled(choice.one_based.is_some(), egui::Button::new(previous))
                        .on_hover_text("Previous play; before play 1 it returns to All plays")
                        .clicked()
                    {
                        self.scoped_play(false, 1, ui.ctx());
                    }
                    let next = format!("Next  {}", self.editor_key(EditorKey::PlayNext));
                    if ui
                        .add_enabled(
                            choice.one_based.is_none_or(|play| play < choice.plays),
                            egui::Button::new(next),
                        )
                        .on_hover_text("Next play; from All plays it opens play 1")
                        .clicked()
                    {
                        self.scoped_play(true, 1, ui.ctx());
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
        let hold_parameter = self.scoped_hold_parameter();
        let composite = matches!(
            node.kind,
            NodeKind::Sequence { .. } | NodeKind::Repeat { .. } | NodeKind::Retime { .. }
        );
        let scope = state.scope_label(workspace).unwrap_or_else(|error| error);
        let repeated = !state.selected_target().repeats.is_empty();
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
                if pane_focus(ui, Pane::Inspector, heading.rect, "Nested beat inspector pane").has_focus() { self.focus_scoped_pane(Pane::Inspector); }
                ui.separator();
                style::side_scroll(ui, egui::ScrollArea::vertical().id_salt("scoped-inspector-details").animated(false), |ui| {
                    style::ink_padded_label(ui, style::semibold(label).size(16.0), None);
                    ui.colored_label(style::LAVENDER, scope);
                    if let Some(interval) = interval { ui.small(interval); }
                    ui.small(if repeated {
                        "All plays changes the shared definition. Existing overrides keep their own changes."
                    } else {
                        "Changes apply to this beat inside the Retime."
                    });
                    if let Some(reason) = reason { ui.add_space(8.0); ui.weak(reason); ui.small("The viewer retains its last displayed picture."); }
                    let ready = !self.service.is_busy() && !self.dialogs.is_open();
                    ui.add_space(4.0);
                    if composite && ui.add(style::row_action(ui, "Enter contents", self.editor_key(EditorKey::EnterGroup)).fill(style::SELECTED)).clicked() { self.enter_scoped(ui.ctx()); }
                    if hold_parameter.is_some()
                        && ui.add_enabled(ready, style::row_action(ui, "Change duration…", self.editor_key(EditorKey::EnterGroup))).clicked() {
                        self.open_inspector_parameter(ui.ctx());
                    }
                    if hold && hold_parameter.is_none() {
                        ui.small("Pause length needs a visible ordinary Hold inside fixed Edit Retimes, or This play at every Repeat level with no Retime ancestor or attached sounds.");
                    }
                    ui.add_space(8.0);
                    ui.label(style::section_title("EDIT", false));
                    if ui.add_enabled(ready, style::row_action(ui, "Camera…", self.editor_key(EditorKey::Camera))).clicked() {
                        self.framing_action(navigation::FramingAction::EnterCamera, ui.ctx());
                    }
                    if ui.add_enabled(ready, style::row_action(ui, "Insert pause", self.editor_key(EditorKey::Hold)))
                        .on_hover_text("Insert 0.5 s at the cursor in the selected Repeat scopes. Each selected play captures its own freeze picture.").clicked() {
                        self.edit(BeatEdit::InsertHold(navigation::duration::DurationInput::half_seconds(1)));
                    }
                    if ui.add_enabled(ready, style::row_action(ui, "Insert AI pause", self.editor_key(EditorKey::GenerateAi)))
                        .on_hover_text("Save the pause in This play, then request pictures. Accepting a candidate is a separate edit.").clicked() {
                        self.edit(BeatEdit::InsertAiHold(navigation::duration::DurationInput::half_seconds(1)));
                    }
                    if ui.add_enabled(ready, style::row_action(ui, "Insert pause of…", ":hold")).clicked() {
                        self.open_command("hold 0.5s".into(), ui.ctx());
                    }
                    if hold { self.ai_inspector(ui, ready); self.hold_audio_controls(ui, ready); }
                    self.gain_inspector(ui, ready);
                    ui.add_space(4.0);
                    ui.small(format!("{} children · {} enter · {} parent", self.editor_pair(EditorKey::BeatPrevious, EditorKey::BeatNext, "/"), self.editor_key(EditorKey::EnterGroup), self.editor_key(EditorKey::LeaveGroup)));
                    ui.small(STRUCTURE_UNAVAILABLE);
                });
            });
    }
}

fn scoped_structural_action(action: Action) -> bool {
    if matches!(
        action,
        Action::Edit(
            BeatEdit::InsertHold(_)
                | BeatEdit::InsertBlack(_)
                | BeatEdit::InsertAiHold(_)
                | BeatEdit::HoldDuration(_)
        )
    ) {
        return false;
    }
    matches!(
        action,
        Action::Insert
            | Action::Group
            | Action::Ungroup
            | Action::Explode
            | Action::Duplicate
            | Action::OfferInsert
            | Action::Edit(_)
            | Action::Trim
            | Action::VisualMoment
            | Action::CopyMoment
            | Action::Operator { .. }
            | Action::Repeat { .. }
            | Action::DeleteSelection
            | Action::DeleteFrames(_)
            | Action::PasteMoment { .. }
    )
}
