//! Persistent letter marks and a separate, bounded navigation trail.

use deadpan_core::{ExactRatio, ProjectId};

use super::*;
use crate::project::marks::{
    Id, Location, Operation, Outcome, Request, ResolvedLocation, Saved, Update,
};

mod history;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Capture {
    id: Id,
    location: Location,
    pane: Pane,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    session: u64,
    project: ProjectId,
    revision: RevisionId,
    location: ResolvedLocation,
}

impl Entry {
    fn available(&self, workspace: &Workspace) -> bool {
        if self.session != workspace.session || &self.project != workspace.document.project_id() {
            return false;
        }
        match &self.location {
            ResolvedLocation::Edit { .. } => &self.revision == workspace.document.revision_id(),
            ResolvedLocation::Original {
                asset,
                qualification,
                ordinal,
            } => workspace.sources.get(asset).is_some_and(|source| {
                source.receipt.id() == qualification
                    && source
                        .video_index
                        .as_ref()
                        .is_some_and(|index| *ordinal <= index.frames().len() as u64)
            }),
        }
    }
}

struct Pending {
    request: Request,
    departure: Capture,
    epoch: u64,
}

#[derive(Default)]
pub(super) struct State {
    pub prefix: Option<Result<Capture, String>>,
    pub command: Option<Result<Capture, String>>,
    pending: Option<Pending>,
    history: history::History<Entry>,
    exact: Option<(Capture, ResolvedLocation)>,
    epoch: u64,
    pub open: bool,
    entry: Option<Result<Capture, String>>,
    name: String,
    focus_pending: bool,
    return_focus: Option<(u64, Pane)>,
}

impl State {
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn reconcile(&mut self, workspace: Option<&Workspace>) {
        self.history
            .retain(|entry| workspace.is_some_and(|workspace| entry.available(workspace)));
        if self.exact.as_ref().is_some_and(|(capture, _)| {
            workspace.is_none_or(|workspace| !capture.entry().available(workspace))
        }) {
            self.exact = None;
        }
    }

    pub fn cancel_jump(&mut self) {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| matches!(pending.request.operation, Operation::Jump { .. }))
        {
            self.pending = None;
        }
    }

    pub fn navigated(&mut self, current: Option<&Capture>) {
        // Overflow invalidates pending input rather than allowing identity reuse.
        match self.epoch.checked_add(1) {
            Some(next) => self.epoch = next,
            None => {
                self.pending = None;
                self.epoch = 0;
            }
        }
        if self
            .exact
            .as_ref()
            .is_some_and(|(at, _)| current.is_none_or(|current| !current.same_position(at)))
        {
            self.exact = None;
        }
    }

    fn rebase(&mut self, saved: &Saved) {
        let update_capture = |capture: &mut Capture| {
            if same_base(&capture.id, &saved.id) {
                capture.id.revision = saved.revision.clone();
            }
        };
        self.history.for_each_mut(|entry| {
            if entry.session == saved.id.session
                && entry.project == saved.id.project
                && entry.revision == saved.id.revision
            {
                entry.revision = saved.revision.clone();
            }
        });
        if let Some((capture, _)) = &mut self.exact {
            update_capture(capture);
        }
        if let Some(Ok(capture)) = &mut self.entry {
            update_capture(capture);
        }
    }
}

fn same_base(left: &Id, right: &Id) -> bool {
    left.session == right.session
        && left.project == right.project
        && left.revision == right.revision
}

impl Capture {
    fn same_position(&self, other: &Self) -> bool {
        same_base(&self.id, &other.id) && self.location == other.location
    }

    #[cfg(feature = "ui-harness")]
    pub(super) fn location_matches_for_check(&self, other: &Self) -> bool {
        self.id.session == other.id.session
            && self.id.project == other.id.project
            && self.location == other.location
            && self.pane == other.pane
    }

    fn entry(&self) -> Entry {
        Entry {
            session: self.id.session,
            project: self.id.project.clone(),
            revision: self.id.revision.clone(),
            location: match &self.location {
                Location::Original {
                    asset,
                    qualification,
                    ordinal,
                } => ResolvedLocation::Original {
                    asset: asset.clone(),
                    qualification: qualification.clone(),
                    ordinal: *ordinal,
                },
                Location::Edit {
                    scope,
                    at,
                    selected,
                } => ResolvedLocation::Edit {
                    scope: scope.clone(),
                    selected: selected.clone(),
                    frame: *at,
                    exact_frame: ExactRatio::integer(at.0),
                },
            },
        }
    }
}

impl DeadpanApp {
    pub(super) fn capture_mark(&self) -> Result<Capture, String> {
        if self.sound_focused() || self.pane == Pane::Sounds || self.event_focused() {
            return Err(
                "Focus Original or Your edit before setting or jumping to a picture mark.".into(),
            );
        }
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("Open a project before using marks.")?;
        let location = match self.view {
            View::Source => {
                if self.raw_source.is_some() {
                    return Err("Register this Original before marking it.".into());
                }
                let asset = self
                    .selected_source
                    .as_ref()
                    .ok_or("Select an Original first.")?;
                let source = workspace
                    .sources
                    .get(asset)
                    .ok_or("The selected Original is unavailable.")?;
                let index = source
                    .video_index
                    .as_ref()
                    .ok_or("This source has no measured video boundaries.")?;
                if self.source_cursor > index.frames().len() as u64 {
                    return Err("Original cursor is outside its measured boundaries.".into());
                }
                Location::Original {
                    asset: asset.clone(),
                    qualification: source.receipt.id().clone(),
                    ordinal: self.source_cursor,
                }
            }
            View::Sequence => {
                let scope = self.sequence_scope.resolve(workspace)?;
                if !(scope.start..=scope.end).contains(&self.sequence_cursor) {
                    return Err("Move into the displayed group before marking its position.".into());
                }
                Location::Edit {
                    scope: self.sequence_scope.clone(),
                    at: ProjectFrame(
                        i64::try_from(self.sequence_cursor)
                            .map_err(|_| "Edit cursor exceeds the supported frame range.")?,
                    ),
                    selected: self.selected_beat.clone(),
                }
            }
        };
        Ok(Capture {
            id: Id {
                ticket: 0,
                session: workspace.session,
                project: workspace.document.project_id().clone(),
                revision: workspace.document.revision_id().clone(),
            },
            location,
            pane: self.pane,
        })
    }

    pub(super) fn begin_mark_prefix(&mut self) {
        self.cancel_repeats("a mark prefix was entered");
        self.marks.cancel_jump();
        self.pause_playback();
        self.marks.prefix = Some(self.capture_mark());
    }

    pub(super) fn mark_action(
        &mut self,
        action: Action,
        captured: Option<Result<Capture, String>>,
    ) {
        self.bindings.clear();
        self.marks.pending = None;
        let capture = match captured.unwrap_or_else(|| self.capture_mark()) {
            Ok(capture) => capture,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let operation = match action {
            Action::SetMark(letter) => Operation::Set {
                letter,
                location: capture.location.clone(),
            },
            Action::JumpMark(letter) => Operation::Jump { letter },
            Action::DeleteMark(letter) => Operation::Delete { letter },
            _ => return,
        };
        let Some(ticket) = self.next_serial() else {
            return;
        };
        let request = Request {
            id: Id {
                ticket,
                ..capture.id.clone()
            },
            operation,
        };
        match self.service.submit(ProjectRequest::Marks(request.clone())) {
            Ok(()) => {
                self.marks.pending = Some(Pending {
                    request,
                    departure: capture,
                    epoch: self.marks.epoch,
                });
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn rebase_mark_metadata(&mut self, update: &Update) {
        let Some(saved) = &update.saved else {
            return;
        };
        if !self.workspace.as_ref().is_some_and(|workspace| {
            workspace.session == saved.id.session
                && workspace.document.project_id() == &saved.id.project
                && workspace.document.revision_id() == &saved.revision
        }) {
            return;
        }
        self.edit_range.rebase_mark(saved);
        self.marks.rebase(saved);
    }

    pub(super) fn receive_marks(&mut self, update: Update, context: &egui::Context) {
        let Some(reply) = update.reply else {
            return;
        };
        let Some(pending) = &self.marks.pending else {
            return;
        };
        if pending.request.id != reply.id {
            return;
        }
        let pending = self.marks.pending.take().expect("matched mark reply");
        match reply.result {
            Err(error) => self.error = Some(error),
            Ok(Outcome::Saved(saved)) => {
                self.error = None;
                self.message = Some(saved.refresh_error.unwrap_or_else(|| {
                    if matches!(pending.request.operation, Operation::Delete { .. }) {
                        format!("Mark {} removed and saved. u restores it.", saved.letter)
                    } else {
                        format!(
                            "Mark {} saved. '{} returns to it; u restores the previous mark.",
                            saved.letter, saved.letter
                        )
                    }
                }));
            }
            Ok(Outcome::Jumped(location)) => {
                if pending.epoch != self.marks.epoch
                    || self.capture_mark().as_ref() != Ok(&pending.departure)
                {
                    self.message = Some("Mark resolved, but navigation changed while it was loading. Jump again to use the current position.".into());
                    return;
                }
                let mut destination = pending.departure.entry();
                destination.location = location;
                let departure = self.current_jump_entry(&pending.departure);
                if let Err(error) = self.apply_jump(&destination, context) {
                    self.error = Some(error);
                    return;
                }
                self.marks.history.jump(departure, &destination);
                self.describe_jump(&destination);
                if self.marks.open {
                    self.close_marks(context);
                }
            }
        }
    }

    fn current_jump_entry(&self, capture: &Capture) -> Entry {
        let mut entry = capture.entry();
        if let Some((at, location)) = &self.marks.exact
            && at.same_position(capture)
        {
            entry.location = location.clone();
        }
        entry
    }

    pub(super) fn jump_history(&mut self, forward: bool, context: &egui::Context) {
        self.bindings.clear();
        self.marks.pending = None;
        let Some(destination) = self.marks.history.target(forward).cloned() else {
            self.message = Some(
                if forward {
                    "No later jump."
                } else {
                    "No earlier jump."
                }
                .into(),
            );
            return;
        };
        let departure = match self.capture_mark() {
            Ok(capture) => self.current_jump_entry(&capture),
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        if let Err(error) = self.apply_jump(&destination, context) {
            self.error = Some(error);
            return;
        }
        self.marks.history.complete(forward, departure);
        self.describe_jump(&destination);
    }

    fn apply_jump(&mut self, entry: &Entry, context: &egui::Context) -> Result<(), String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("Open a project before jumping.")?;
        if workspace.session != entry.session || workspace.document.project_id() != &entry.project {
            return Err("This jump belongs to another project session.".into());
        }
        match &entry.location {
            ResolvedLocation::Original {
                asset,
                qualification,
                ordinal,
            } => {
                let source = workspace
                    .sources
                    .get(asset)
                    .ok_or("This jump's Original is unavailable.")?;
                let index = source
                    .video_index
                    .as_ref()
                    .ok_or("This Original has no measured video boundaries.")?;
                if source.receipt.id() != qualification || *ordinal > index.frames().len() as u64 {
                    return Err("This jump's Original qualification or boundary changed.".into());
                }
            }
            ResolvedLocation::Edit {
                scope,
                selected,
                frame,
                ..
            } => {
                if workspace.document.revision_id() != &entry.revision {
                    return Err("The edit changed since this history position. Use a saved mark to follow edited content.".into());
                }
                let view = scope.resolve(workspace)?;
                let frame = u64::try_from(frame.0).map_err(|_| "Jump boundary is negative.")?;
                if !(view.start..=view.end).contains(&frame)
                    || selected
                        .as_ref()
                        .is_some_and(|node| !view.children.contains(node))
                {
                    return Err("This jump's exact group or child is no longer available.".into());
                }
            }
        }
        // Everything above is read-only. Failed admission must preserve both
        // cursors, selections, playback, scope and the complete history trail.
        self.scoped = None;
        self.stop_playback();
        self.cancel_camera();
        self.bindings.clear();
        self.selected_sound = None;
        self.selected_event = None;
        self.sound_inspection = None;
        match &entry.location {
            ResolvedLocation::Original { asset, ordinal, .. } => {
                self.raw_source = None;
                self.selected_source = Some(asset.clone());
                self.view.set(View::Source, &mut self.message);
                self.source_cursor = *ordinal;
                self.reconcile_moment();
                self.moment.move_to(*ordinal);
                self.reveal_source = true;
                self.pane = Pane::Viewer;
            }
            ResolvedLocation::Edit {
                scope,
                selected,
                frame,
                ..
            } => {
                self.sequence_scope = scope.clone();
                self.view.set(View::Sequence, &mut self.message);
                self.rebuild_rows();
                self.sequence_cursor = frame.0 as u64;
                self.selected_beat = selected.clone();
                self.reconcile_edit_range();
                self.edit_range.move_to(self.sequence_cursor);
                self.reveal_beat = true;
                self.pane = Pane::Sequence;
            }
        }
        self.request_picture(false);
        self.marks.exact = self
            .capture_mark()
            .ok()
            .map(|capture| (capture, entry.location.clone()));
        self.error = None;
        context.memory_mut(|memory| memory.request_focus(pane_id(self.pane)));
        context.request_repaint();
        Ok(())
    }

    fn describe_jump(&mut self, entry: &Entry) {
        self.message = Some(match &entry.location {
            ResolvedLocation::Original { ordinal, .. } => {
                format!("Original boundary {ordinal}. Ctrl O goes back; Ctrl I goes forward.")
            }
            ResolvedLocation::Edit {
                frame, exact_frame, ..
            } if *exact_frame != ExactRatio::integer(frame.0) => format!(
                "Mark at exact Edit boundary {}/{} f; shown at frame {} after ties-to-even rounding. Ctrl O goes back.",
                exact_frame.numerator(),
                exact_frame.denominator(),
                frame.0
            ),
            ResolvedLocation::Edit { frame, .. } => format!(
                "Edit boundary {}. Ctrl O goes back; Ctrl I goes forward.",
                frame.0
            ),
        });
    }

    pub(super) fn open_marks(&mut self, context: &egui::Context) {
        self.pause_playback();
        self.bindings.clear();
        self.marks.prefix = None;
        self.marks.entry = Some(self.capture_mark());
        self.marks.name = "a".into();
        self.marks.open = true;
        self.marks.focus_pending = true;
        context.request_repaint();
    }

    pub(super) fn marks_keyboard(&mut self, context: &egui::Context) -> bool {
        if let Some((frame, pane)) = self.marks.return_focus
            && context.cumulative_frame_nr() > frame
        {
            context.memory_mut(|memory| memory.request_focus(pane_id(pane)));
            self.marks.return_focus = None;
        }
        if !self.marks.open {
            return false;
        }
        let events = context.input(|input| input.events.clone());
        let composition = self.ime_composing
            || events
                .iter()
                .any(|event| matches!(event, egui::Event::Ime(_)));
        help_scroll::observe_composition(&events, &mut self.ime_composing);
        self.bindings.clear();
        if !composition
            && !self.ime_composing
            && !self.service.is_busy()
            && self.marks.pending.is_none()
        {
            let forward = events
                .iter()
                .enumerate()
                .find_map(|(index, event)| match event {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        repeat: false,
                        modifiers,
                        ..
                    } => {
                        let companion = match events.get(index + 1) {
                            Some(egui::Event::Text(text)) => Some(text.as_str()),
                            _ => None,
                        };
                        crate::navigation::mode_key(*key, *modifiers, companion)
                            .and_then(|(logical, modifiers)| {
                                crate::navigation::panels::marks_key(logical, modifiers)
                            })
                            .map(|forward| (forward, *key, *modifiers))
                    }
                    _ => None,
                });
            if let Some((forward, key, modifiers)) = forward {
                context.input_mut(|input| {
                    input.consume_key(modifiers, key);
                });
                self.jump_history(forward, context);
                if self.error.is_none() {
                    self.close_marks(context);
                }
            }
        }
        context.input_mut(|input| input.events.retain(|event| {
            !matches!(event, egui::Event::Key { key: egui::Key::Enter | egui::Key::Space | egui::Key::Escape, repeat, .. }
                if composition || self.ime_composing || *repeat)
        }));
        true
    }

    pub(super) fn marks_window(&mut self, context: &egui::Context) {
        if !self.marks.open {
            return;
        }
        let rows = self
            .workspace
            .as_ref()
            .map(|workspace| {
                ('a'..='z')
                    .chain('A'..='Z')
                    .filter_map(|letter| {
                        let id = crate::project::marks::mark_id(letter).ok()?;
                        let mark = workspace.document.marks().get(&id)?;
                        if mark.label != letter.to_string() {
                            return None;
                        }
                        let domain = if matches!(
                            mark.boundary.coordinate,
                            deadpan_core::Anchor::Source { .. }
                        ) {
                            "Original"
                        } else {
                            "Your edit"
                        };
                        let state = if mark
                            .bindings()
                            .any(|binding| binding.state == deadpan_core::MarkState::Bound)
                        {
                            "Saved"
                        } else {
                            "Unresolved"
                        };
                        Some((letter, domain, state))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let busy = self.service.is_busy() || self.marks.is_pending();
        let mut action = None;
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("marks-window")).show(context, |ui| {
            super::accessibility::dialog(ui, "Marks");
            ui.set_width(540.0_f32.min((context.content_rect().width() - 64.0).max(240.0)));
            ui.heading("Marks");
            ui.weak("m + letter saves · ' + letter jumps · uppercase letters are separate marks");
            ui.weak("Ctrl O goes back · Ctrl I goes forward · Tab moves between controls");
            if let Some(capture) = &self.marks.entry {
                match capture {
                    Ok(capture) => { ui.label(match &capture.location {
                        Location::Original { ordinal, .. } => format!("Captured Original boundary {ordinal}"),
                        Location::Edit { at, .. } => format!("Captured Edit boundary {}", at.0),
                    }); }
                    Err(error) => { ui.colored_label(ui.visuals().error_fg_color, error); }
                }
            }
            ui.horizontal(|ui| {
                let label = ui.label("Letter");
                let field = ui.add(egui::TextEdit::singleline(&mut self.marks.name).id_salt("mark-letter").char_limit(1).desired_width(42.0)).labelled_by(label.id);
                if std::mem::take(&mut self.marks.focus_pending) { field.request_focus(); }
                let letter = self.marks.name.chars().next().filter(char::is_ascii_alphabetic);
                if ui.add_enabled(!busy && letter.is_some(), egui::Button::new("Save this position")).clicked() {
                    action = letter.map(Action::SetMark);
                }
                if ui.add(style::action("Back to editor", "Esc")).clicked() { close = true; }
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(!busy && self.marks.history.target(false).is_some(), style::action("Back", "Ctrl O")).clicked() {
                    action = Some(Action::JumpHistory { forward: false });
                }
                if ui.add_enabled(!busy && self.marks.history.target(true).is_some(), style::action("Forward", "Ctrl I")).clicked() {
                    action = Some(Action::JumpHistory { forward: true });
                }
                if busy { crate::preview::accessibility::busy(ui); ui.weak("Resolving mark…"); }
            });
            if let Some(error) = &self.error { ui.colored_label(ui.visuals().error_fg_color, error); }
            if let Some(message) = &self.message { ui.label(message); }
            ui.separator();
            egui::ScrollArea::vertical().id_salt("mark-rows").max_height((context.content_rect().height() - 300.0).max(120.0)).show(ui, |ui| {
                if rows.is_empty() { ui.label("No saved letter marks. Save this position or return to the editor and type ma."); }
                for (letter, domain, state) in rows {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(letter.to_string()).monospace().color(style::LAVENDER));
                        ui.label(domain);
                        ui.weak(state);
                        let jump = ui.add_enabled(!busy, egui::Button::new(format!("Jump '{letter}")));
                        if jump.gained_focus() { jump.scroll_to_me(Some(egui::Align::Center)); }
                        if jump.clicked() { action = Some(Action::JumpMark(letter)); }
                        let remove = ui.add_enabled(!busy, egui::Button::new(format!("Remove {letter}")));
                        if remove.gained_focus() { remove.scroll_to_me(Some(egui::Align::Center)); }
                        if remove.clicked() { action = Some(Action::DeleteMark(letter)); }
                    });
                }
            });
        });
        close |= modal.should_close() && !self.ime_composing;
        if let Some(action) = action {
            match action {
                Action::JumpHistory { forward } => {
                    self.jump_history(forward, context);
                    close = self.error.is_none();
                }
                _ => {
                    let capture = self.marks.entry.clone();
                    self.mark_action(action, capture);
                    // Resolution remains asynchronous. Leave the list visible
                    // on failure, then close only after an admitted jump.
                }
            }
        }
        if close {
            self.close_marks(context);
        }
    }

    fn close_marks(&mut self, context: &egui::Context) {
        self.marks.open = false;
        self.marks.return_focus = Some((context.cumulative_frame_nr(), self.pane));
        context.request_discard("marks closed");
        context.request_repaint();
    }
}
