//! Placed sounds keep a distinct selection and never retarget the retained beat.

use deadpan_core::{AudioEdgePolicy, AudioSample, ExactRatio, FrameDuration, SoundId};

use super::*;
use crate::navigation::SoundAction;
use crate::project::ProjectSoundEdit;
use crate::project::sound::{PauseTarget, pause_target};

pub(super) struct Inspection {
    session: u64,
    revision: RevisionId,
    id: SoundId,
    onset: AudioSample,
    position: String,
    duration: String,
    routed: bool,
    cursor: u64,
    pause: Result<PauseTarget, String>,
}

pub(super) struct CommandTarget {
    session: u64,
    revision: RevisionId,
    event: SoundId,
    initial_command: String,
    cursor: u64,
    pause: Result<PauseTarget, String>,
}

impl DeadpanApp {
    pub(super) fn compact_sound_layout(&self, context: &egui::Context) -> bool {
        self.view == View::Sequence
            && context.input(|input| input.content_rect().height() < 700.0)
            && self
                .workspace
                .as_ref()
                .is_some_and(|w| !w.document.sounds().is_empty())
    }

    pub(super) fn capture_sound_command(&self, command: &str) -> Option<CommandTarget> {
        if !self.event_focused() {
            return None;
        }
        let workspace = self.workspace.as_ref()?;
        Some(CommandTarget {
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            event: self.selected_event.clone()?,
            initial_command: command.into(),
            cursor: self.sequence_cursor,
            pause: cursor_pause(
                workspace,
                self.selected_event.as_ref()?,
                self.sequence_cursor,
            ),
        })
    }

    pub(super) fn unchanged_sound_position(&self, target: &CommandTarget) -> bool {
        !target.initial_command.is_empty() && target.initial_command == self.command
    }

    pub(super) fn check_sound_command(&self, target: &CommandTarget) -> Result<(), String> {
        if !self.event_focused()
            || self.workspace.as_ref().is_none_or(|workspace| {
                workspace.session != target.session
                    || workspace.document.revision_id() != &target.revision
            })
            || self.selected_event.as_ref() != Some(&target.event)
        {
            return Err("The sound or project changed while entering this command. Open its parameters again; no edit was made.".into());
        }
        Ok(())
    }

    pub(super) fn captured_sound_allowance(&mut self, target: CommandTarget, allowed: bool) {
        let result = (|| {
            self.check_sound_command(&target)?;
            if self.sequence_cursor != target.cursor {
                return Err(
                    "The Edit cursor changed while entering the pause command; no edit was made."
                        .into(),
                );
            }
            let pause = target.pause?;
            pause.validate_change(allowed)?;
            Ok(ProjectRequest::SoundEdit {
                expected_session: target.session,
                expected_revision: target.revision,
                edit: ProjectSoundEdit::Allowance {
                    id: target.event,
                    issuer: pause.issuer,
                    at: pause.at,
                    allowed,
                },
            })
        })();
        match result {
            Ok(request) => {
                self.submit(request);
            }
            Err(error) => self.error = Some(error),
        }
    }
    pub(super) fn event_focused(&self) -> bool {
        self.view == View::Sequence
            && matches!(self.pane, Pane::Sounds | Pane::Inspector)
            && self.selected_event.is_some()
    }

    pub(super) fn focus_events(&mut self, context: &egui::Context) {
        self.stop_playback();
        self.cancel_camera();
        self.bindings.clear();
        self.pane = Pane::Sounds;
        self.view.set(View::Sequence, &mut self.message);
        self.reconcile_events();
        self.request_picture(false);
        context.memory_mut(|m| m.request_focus(pane_id(Pane::Sounds)));
    }

    pub(super) fn reconcile_events(&mut self) {
        if self.selected_event.as_ref().is_some_and(|id| {
            self.workspace
                .as_ref()
                .is_none_or(|w| !w.document.sounds().contains_key(id))
        }) {
            self.selected_event = None;
        }
        if self.pane == Pane::Sounds && self.selected_event.is_none() {
            self.selected_event = self
                .workspace
                .as_ref()
                .and_then(|w| w.document.sounds().keys().next().cloned());
            self.reveal_event = true;
        }
        let Some((workspace, id)) = self.workspace.as_ref().zip(self.selected_event.as_ref())
        else {
            self.sound_inspection = None;
            return;
        };
        if self.sound_inspection.as_ref().is_some_and(|inspection| {
            inspection.session == workspace.session
                && &inspection.revision == workspace.document.revision_id()
                && &inspection.id == id
                && inspection.cursor == self.sequence_cursor
        }) {
            return;
        }
        self.sound_inspection = inspect(workspace, id, self.sequence_cursor).ok();
    }

    pub(super) fn sound_action(&mut self, action: SoundAction, context: &egui::Context) {
        let result = self.prepare_sound_action(action, context);
        match result {
            Ok(Some(edit)) => {
                let Some(workspace) = &self.workspace else {
                    return;
                };
                self.submit(ProjectRequest::SoundEdit {
                    expected_session: workspace.session,
                    expected_revision: workspace.document.revision_id().clone(),
                    edit,
                });
            }
            Ok(None) => {}
            Err(error) => self.error = Some(error),
        }
    }

    fn prepare_sound_action(
        &mut self,
        action: SoundAction,
        context: &egui::Context,
    ) -> Result<Option<ProjectSoundEdit>, String> {
        if action == SoundAction::Focus {
            self.focus_events(context);
            return Ok(None);
        }
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("Open a project before placing sounds.")?;
        if action == SoundAction::Place {
            let asset = self
                .selected_sound
                .clone()
                .ok_or("Choose a sound in the sound-effects catalog first.")?;
            let cursor = i64::try_from(self.sequence_cursor)
                .map_err(|_| "Edit cursor exceeds the supported range.")?;
            let at = workspace
                .document
                .presentation_basis()
                .frame_rate
                .audio_boundary(ProjectFrame(cursor))
                .map_err(|e| e.to_string())?;
            // Pure typed validation supplies immediate fit feedback. The service
            // independently resolves the same recipe against captured revision.
            crate::project::sound::placement(workspace, &asset, at)?;
            return Ok(Some(ProjectSoundEdit::Place { asset, at }));
        }
        if !self.event_focused() {
            return Err(
                "Select a placed sound first. Use :sounds or the Placed sounds pane.".into(),
            );
        }
        let id = self
            .selected_event
            .clone()
            .ok_or("Select a placed sound first.")?;
        let event = workspace
            .document
            .sounds()
            .get(&id)
            .ok_or("The selected sound no longer exists.")?;
        let (gain, start_edge, end_edge) = match action {
            SoundAction::Allowance(allowed) => {
                let target = cursor_pause(workspace, &id, self.sequence_cursor)?;
                target.validate_change(allowed)?;
                return Ok(Some(ProjectSoundEdit::Allowance {
                    id,
                    issuer: target.issuer,
                    at: target.at,
                    allowed,
                }));
            }
            SoundAction::Move(at) => return Ok(Some(ProjectSoundEdit::Move { id, at })),
            SoundAction::Delete => return Ok(Some(ProjectSoundEdit::Delete { id })),
            SoundAction::Gain(value) => (value, event.start_edge, event.end_edge),
            SoundAction::GainStep(delta) => (
                event
                    .gain_millidecibels
                    .checked_add(delta)
                    .ok_or("Sound gain exceeds the supported range.")?,
                event.start_edge,
                event.end_edge,
            ),
            SoundAction::Edges(edge) => (event.gain_millidecibels, edge, edge),
            SoundAction::Place | SoundAction::Focus => unreachable!("handled above"),
        };
        Ok(Some(ProjectSoundEdit::Update {
            id,
            gain_millidecibels: gain,
            start_edge,
            end_edge,
        }))
    }

    pub(super) fn step_event(&mut self, forward: bool, count: u32, context: &egui::Context) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let ids = workspace
            .document
            .sounds()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return;
        }
        let next = self
            .selected_event
            .as_ref()
            .and_then(|id| ids.iter().position(|other| other == id))
            .map_or(if forward { 0 } else { ids.len() - 1 }, |index| {
                navigation::boundary_step(index as u64, ids.len() as u64 - 1, forward, count)
                    as usize
            });
        self.select_event(ids[next].clone(), context);
    }

    fn select_event(&mut self, id: SoundId, context: &egui::Context) {
        self.selected_event = Some(id);
        self.reveal_event = true;
        self.focus_events(context);
    }

    pub(super) fn move_event(&mut self, forward: bool, count: u32, _context: &egui::Context) {
        let result = (|| {
            if !self.event_focused() {
                return Err("Select a placed sound before moving it.".into());
            }
            let workspace = self.workspace.as_ref().ok_or("Open a project first.")?;
            let inspection = self
                .sound_inspection
                .as_ref()
                .ok_or("Select a placed sound first.")?;
            if inspection.routed {
                return Err("This sound follows timeline cuts. Its placement cannot be moved yet; gain and edges remain editable.".into());
            }
            Ok(ProjectRequest::SoundEdit {
                expected_session: workspace.session,
                expected_revision: workspace.document.revision_id().clone(),
                edit: ProjectSoundEdit::Nudge {
                    id: inspection.id.clone(),
                    frames: if forward {
                        i64::from(count)
                    } else {
                        -i64::from(count)
                    },
                },
            })
        })();
        match result {
            Ok(request) => {
                self.submit(request);
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn open_sound_position(&mut self, context: &egui::Context) -> bool {
        if !self.event_focused() {
            return false;
        }
        if let Some(inspection) = &self.sound_inspection {
            if inspection.routed {
                self.message = Some("Placement follows timeline cuts. Change gain or edges while retaining those edits.".into());
            } else {
                self.open_command(format!("sound-at {}", inspection.onset.0), context);
            }
        }
        true
    }

    pub(super) fn placed_sounds(&mut self, ui: &mut egui::Ui) {
        if self.gain.is_some() {
            // Preserve the panel's place in the ID tree while its inactive
            // list gives the comparison picture room above the gain editor.
            egui::Panel::bottom("workspace-placed-sounds")
                .resizable(false)
                .exact_size(0.0)
                .frame(egui::Frame::NONE)
                .show(ui, |_| {});
            return;
        }
        let sounds = self
            .workspace
            .as_ref()
            .map(|w| {
                w.document
                    .sounds()
                    .iter()
                    .map(|(id, event)| {
                        let position = if w.document.sound_routes().contains_key(id) {
                            "Edited timing".to_owned()
                        } else {
                            event_onset(w, event).map_or_else(
                                |_| "Position unavailable".into(),
                                |at| format!("{} samples", at.0),
                            )
                        };
                        (
                            id.clone(),
                            event.label.clone(),
                            format!("{position} · {} dB", gain_label(event.gain_millidecibels)),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        // Keep the empty pane in the tree, so native focus and automatic IDs
        // have the same lifetime before and after the first placement.
        let compact = self.compact_sound_layout(ui.ctx());
        let height = if sounds.is_empty() {
            64.0
        } else if compact {
            94.0
        } else {
            102.0
        };
        egui::Panel::bottom("workspace-placed-sounds").resizable(false)
            .default_size(height).min_size(height).max_size(height).frame(style::compact_panel())
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                if compact {
                    ui.spacing_mut().interact_size.y = 24.0;
                    ui.spacing_mut().button_padding.y = 4.0;
                }
                let heading = ui.horizontal(|ui| {
                    let heading = pane_heading(ui, "PLACED SOUNDS", self.pane == Pane::Sounds);
                    ui.weak(sounds.len().to_string());
                    heading
                }).inner;
                if pane_focus(ui, Pane::Sounds, heading.rect, "Placed sounds pane").has_focus()
                    && (self.pane != Pane::Sounds || self.view != View::Sequence) {
                    self.focus_events(ui.ctx());
                }
                if sounds.is_empty() {
                    ui.weak("Choose a catalog sound, then place it with ,s. Picture length stays the same.");
                } else {
                    let selected = self.selected_event.clone();
                    let reveal = std::mem::take(&mut self.reveal_event);
                    egui::ScrollArea::vertical().id_salt("placed-sounds-list").show(ui, |ui| {
                        for (id, label, detail) in &sounds {
                            let response = ui.add_sized([ui.available_width(), if compact { 24.0 } else { 27.0 }],
                                egui::Button::new(format!("♫  {label}    {detail}")).selected(selected.as_ref() == Some(id)));
                            if reveal && selected.as_ref() == Some(id) { response.scroll_to_me(Some(egui::Align::Center)); }
                            if response.clicked() { self.select_event(id.clone(), ui.ctx()); }
                        }
                    });
                }
            });
    }

    pub(super) fn event_inspector(&mut self, ui: &mut egui::Ui) {
        self.reconcile_events();
        let Some((workspace, id)) = self.workspace.as_ref().zip(self.selected_event.as_ref())
        else {
            return;
        };
        let Some(event) = workspace.document.sounds().get(id).cloned() else {
            return;
        };
        let Some(info) = &self.sound_inspection else {
            return;
        };
        let (position, duration, routed) =
            (info.position.clone(), info.duration.clone(), info.routed);
        let allowance_entry = self.command_open
            && matches!(
                navigation::command::parse(&self.command),
                Ok(navigation::command::Entry::Action(Action::Sound(
                    SoundAction::Allowance(_)
                )))
            );
        let (pause, pause_cursor) = if allowance_entry {
            self.sound_command_target.as_ref().map_or_else(
                || {
                    (
                        Err("No sound and pause were captured when command entry opened.".into()),
                        self.sequence_cursor,
                    )
                },
                |target| (target.pause.clone(), target.cursor),
            )
        } else {
            (info.pause.clone(), info.cursor)
        };
        let layout = self.workspace_layout(ui);
        egui::Panel::right("workspace-inspector").resizable(false).default_size(layout.inspector)
            .min_size(layout.inspector).max_size(layout.inspector).frame(style::panel()).show(ui, |ui| {
                let heading = pane_heading(ui, "SOUND EVENT", self.pane == Pane::Inspector);
                if pane_focus(ui, Pane::Inspector, heading.rect, "Selected sound inspector pane").has_focus() { self.pane = Pane::Inspector; }
                ui.separator();
                egui::ScrollArea::vertical().id_salt("sound-event-inspector").show(ui, |ui| {
                    ui.label(egui::RichText::new(&event.label).size(17.0).color(style::LAVENDER));
                    ui.weak("Your edit · picture length stays the same");
                    ui.add_space(8.0);
                    inspector_value(ui, "Start", &position);
                    inspector_value(ui, "Duration", &duration);
                    inspector_value(ui, "Gain", &format!("{} dB", gain_label(event.gain_millidecibels)));
                    let ready = !self.service.is_busy() && !self.dialogs.is_open() && !self.command_open;
                    ui.add_space(8.0);
                    ui.label("Fine position · 48 kHz samples");
                    if ui.add_enabled(ready && !routed, egui::Button::new("Change position  ·  Enter").min_size(egui::vec2(ui.available_width(), 28.0))).clicked() {
                        self.pane = Pane::Inspector;
                        self.open_sound_position(ui.ctx());
                    }
                    if routed { ui.small("Placement follows timeline cuts. Gain and edges retain those cuts; moving this sound is not available yet."); }
                    if ui.add_enabled(ready, egui::Button::new("Change gain  ·  + / −").min_size(egui::vec2(ui.available_width(), 28.0))).clicked() {
                        self.pane = Pane::Inspector;
                        self.open_command(format!("sound-gain {}", gain_label(event.gain_millidecibels)), ui.ctx());
                    }
                    ui.label("Edges");
                    ui.horizontal(|ui| {
                        for (label, edge) in [("Soft", AudioEdgePolicy::Automatic), ("Hard", AudioEdgePolicy::Hard)] {
                            if ui.add_enabled(ready, egui::Button::new(label).selected(event.start_edge == edge && event.end_edge == edge)).clicked() {
                                self.sound_action(SoundAction::Edges(edge), ui.ctx());
                            }
                        }
                    });
                    ui.add_space(8.0);
                    ui.label("Pause in Edit frame");
                    ui.small(format!("{}Edit frame {} · selected sound only", if allowance_entry { "Captured " } else { "" }, pause_cursor));
                    match &pause {
                        Ok(target) => {
                            ui.label(&target.label);
                            ui.small(if target.allowed { "This sound is allowed in this exact pause occurrence." } else { "This pause silences this sound." });
                            if !target.selected_support {
                                ui.colored_label(style::CURSOR, "No retained sound selection in this pause within the Edit frame. Allowing cannot fill a timing gap.");
                            }
                            let (label, command, allowed) = if target.allowed {
                                ("Silence this sound in pause", ":sound-silence", false)
                            } else {
                                ("Allow this sound in pause", ":sound-allow", true)
                            };
                            if ui.add_enabled(ready && (target.allowed || target.selected_support), egui::Button::new(label).wrap()).clicked() {
                                self.pane = Pane::Inspector;
                                self.sound_action(SoundAction::Allowance(allowed), ui.ctx());
                            }
                            ui.monospace(command);
                        }
                        Err(reason) => { ui.colored_label(style::MUTED, reason); }
                    }
                    ui.small("Move the Edit cursor to a pause to choose it. Only this sound's permission changes.");
                    ui.separator();
                    if ui.add_enabled(ready, egui::Button::new("Remove sound  ·  dd")).clicked() { self.sound_action(SoundAction::Delete, ui.ctx()); }
                    if ui.add_enabled(ready, egui::Button::new("Undo  ·  u")).clicked() { self.history(false); }
                });
            });
    }
}

pub(super) fn command_hint(command: &str) -> Option<&'static str> {
    let command = command.trim();
    let verb = command
        .strip_prefix(':')
        .unwrap_or(command)
        .split_whitespace()
        .next()?;
    match verb.to_ascii_lowercase().as_str() {
        "sound-at" => Some("Position: whole 48 kHz samples · 48,000 = 1 second"),
        "sound-gain" => Some("Gain: −96 to +24 dB · up to 3 decimal places"),
        "sound-edges" => Some("Edges: soft or hard · retained timing stays intact"),
        "sound-delete" => Some("Remove selected sound · picture time stays intact"),
        "sound-allow" | "sound-silence" => {
            Some("Captured Edit-cursor pause · selected sound only · no timing gaps filled")
        }
        "sound-place" => {
            Some("Place complete catalog sound at Edit cursor · no added picture time")
        }
        _ => None,
    }
}

fn event_onset(
    workspace: &Workspace,
    event: &deadpan_core::SoundEvent,
) -> Result<AudioSample, String> {
    let rate = workspace.document.presentation_basis().frame_rate;
    let start = event
        .mapping
        .selection_frames_with_offset(FrameDuration::ZERO, event.offset, rate)
        .map_err(|e| e.to_string())?
        .start;
    let sample = start
        .checked_mul(
            ExactRatio::new(
                48_000 * i128::from(rate.denominator()),
                i128::from(rate.numerator()),
            )
            .map_err(|e| e.to_string())?,
        )
        .and_then(ExactRatio::round_even)
        .map_err(|e| e.to_string())?;
    i64::try_from(sample)
        .map(AudioSample)
        .map_err(|_| "Sound position exceeds the sample range.".into())
}

fn cursor_pause(workspace: &Workspace, id: &SoundId, cursor: u64) -> Result<PauseTarget, String> {
    let at = i64::try_from(cursor)
        .map(ProjectFrame)
        .map_err(|_| "Edit cursor exceeds the supported range.")?;
    pause_target(workspace, id, at)
}

fn inspect(workspace: &Workspace, id: &SoundId, cursor: u64) -> Result<Inspection, String> {
    let sound = workspace.plan.root_sound(id).map_err(|e| e.to_string())?;
    let onset = sound.audible_samples().start;
    let routed = workspace.document.sound_routes().contains_key(id);
    let position = if routed {
        "Follows timeline edits".into()
    } else {
        format!("{} samples", onset.0)
    };
    let duration = format!(
        "{:.3} s{}",
        (sound.audible_samples().end.0 - onset.0) as f64 / 48_000.0,
        if routed { " recipe" } else { "" }
    );
    Ok(Inspection {
        session: workspace.session,
        revision: workspace.document.revision_id().clone(),
        id: id.clone(),
        onset,
        position,
        duration,
        routed,
        cursor,
        pause: cursor_pause(workspace, id, cursor),
    })
}

fn gain_label(value: i32) -> String {
    let absolute = value.unsigned_abs();
    let mut label = format!(
        "{}{}.{:03}",
        if value < 0 { "-" } else { "" },
        absolute / 1000,
        absolute % 1000
    );
    while label.ends_with('0') {
        label.pop();
    }
    if label.ends_with('.') {
        label.pop();
    }
    label
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_labels_preserve_millidecibels_for_command_entry() {
        for (value, expected) in [
            (0, "0"),
            (-3000, "-3"),
            (-1, "-0.001"),
            (24_000, "24"),
            (-96_000, "-96"),
            (1250, "1.25"),
        ] {
            assert_eq!(gain_label(value), expected);
        }
    }
}
