//! Captured, reversible native gain drafts and same-window comparisons.

use deadpan_core::{AudioSample, AudioTreatments, GainDb};
use deadpan_playback::{Snapshot, Window};

use super::*;
use crate::gain::GainEdit;
use crate::project::gain::{Proposal, ProposalId, ProposalUpdate, Target};
use crate::transport::Domain;

mod controls;
mod waveform;
use controls::FocusReveal;

const FOCUS_ID: &str = "gain-draft-focus";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Choice {
    Before,
    Draft,
}

#[derive(Clone, Copy)]
enum DraftAction {
    Apply,
    Cancel,
    Play,
    Restart,
    Choose(Choice),
}

pub(super) struct Draft {
    target: Target,
    base: Arc<Snapshot>,
    token: u64,
    change: u64,
    edit: GainEdit,
    controls: controls::Controls,
    waveform: waveform::Display,
    pending: Option<ProposalId>,
    requested: Option<u64>,
    prepared: Option<Arc<Snapshot>>,
    error: Option<String>,
    domain: Domain,
    window: Window,
    pub(super) position: AudioSample,
    choice: Choice,
    label: String,
    source_cursor: u64,
    pane: Pane,
    key: Option<DraftAction>,
    focus_pending: bool,
    cancel_focus: Option<egui::Id>,
    applying: bool,
}

impl Draft {
    #[cfg(feature = "ui-harness")]
    pub(super) fn prepared_snapshot(&self) -> Option<&Arc<Snapshot>> {
        self.prepared.as_ref()
    }

    #[cfg(feature = "ui-harness")]
    pub(super) fn window(&self) -> Window {
        self.window
    }

    #[cfg(feature = "ui-harness")]
    pub(super) fn waveform_state(
        &self,
    ) -> (
        Option<deadpan_playback::WaveformTicket>,
        deadpan_playback::WaveformStatus,
        Option<&Arc<deadpan_audio::DefinitionWaveform>>,
    ) {
        (
            self.waveform.ticket,
            self.waveform.status,
            self.waveform.data.as_ref(),
        )
    }

    #[cfg(feature = "ui-harness")]
    pub(super) fn receive_waveform_for_check(&mut self, update: deadpan_playback::WaveformUpdate) {
        self.waveform.receive(&self.target, update);
    }

    fn proposal(&self) -> Proposal {
        Proposal {
            target: self.target.clone(),
            draft: self.token,
            change: self.change,
            treatments: self.edit.recipe().clone(),
        }
    }

    fn receive(&mut self, update: ProposalUpdate) {
        if self.pending.as_ref() != Some(&update.id) {
            return;
        }
        self.pending = None;
        if update.id != self.proposal().id() {
            return;
        }
        match update.result {
            Ok(snapshot)
                if snapshot.session == self.target.session
                    && snapshot.document.project_id() == &self.target.project
                    && snapshot.content
                        == (deadpan_playback::ContentIdentity::Proposed {
                            base_revision: self.target.revision.clone(),
                            draft: self.token,
                            change: self.change,
                        }) =>
            {
                self.prepared = Some(snapshot);
                self.error = None;
            }
            Ok(_) => {
                self.prepared = None;
                self.error = Some("Prepared gain content did not match this draft.".into());
            }
            Err(error) => {
                self.prepared = None;
                self.error = Some(error);
            }
        }
    }

    fn changed(&mut self) -> Result<(), String> {
        self.change = self
            .change
            .checked_add(1)
            .ok_or("Gain draft change identities exhausted.")?;
        self.prepared = None;
        self.error = None;
        Ok(())
    }

    fn selected_snapshot(&self) -> Option<Arc<Snapshot>> {
        match self.choice {
            Choice::Before => Some(self.base.clone()),
            Choice::Draft => self.prepared.clone(),
        }
    }
}

impl DeadpanApp {
    pub(super) fn gain_render_edit(&self) -> Result<Option<super::render::PreviewEdit>, String> {
        let draft = self
            .gain
            .as_ref()
            .ok_or("Gain preview is no longer open.")?;
        self.check_gain_target(&draft.target)?;
        if draft.applying || draft.prepared.is_none() || !draft.controls.ready(&draft.edit) {
            return Err(draft.error.clone().unwrap_or_else(|| {
                "Finish and prepare the Gain preview before committing it.".into()
            }));
        }
        if draft.target.entry == *draft.edit.recipe() {
            return Ok(None);
        }
        Ok(Some(super::render::PreviewEdit {
            session: draft.target.session,
            revision: draft.target.revision.clone(),
            cursor: draft.target.cursor,
            scope: draft.target.scope.clone(),
            edit: draft.target.edit(draft.edit.recipe().clone()),
        }))
    }

    pub(super) fn capture_gain_target(&self) -> Result<Target, String> {
        if self.view != View::Sequence
            || matches!(self.pane, Pane::Sources | Pane::Sounds)
            || self.event_focused()
            || self.sound_focused()
        {
            return Err("Select a beat in Your edit before changing its gain. Placed sounds use :sound-gain.".into());
        }
        let workspace = self.workspace.as_ref().ok_or("Open a project first.")?;
        let scoped = self.scoped_target()?;
        let node = scoped
            .as_ref()
            .map(|target| &target.target.node)
            .or(self.selected_beat.as_ref())
            .ok_or("Select a beat first.")?;
        let owner = workspace
            .document
            .nodes()
            .get(node)
            .ok_or("The selected beat is unavailable.")?;
        let target = Target {
            session: workspace.session,
            project: workspace.document.project_id().clone(),
            revision: workspace.document.revision_id().clone(),
            scope: self.sequence_scope.clone(),
            node: node.clone(),
            cursor: ProjectFrame(i64::try_from(self.sequence_cursor).map_err(|e| e.to_string())?),
            entry: owner.audio_treatments.clone(),
            scoped,
        };
        if target.scoped.is_none() {
            target.validate(workspace)?;
        }
        Ok(target)
    }

    fn check_gain_target(&self, target: &Target) -> Result<(), String> {
        target.validate(self.workspace.as_ref().ok_or("The project was closed.")?)?;
        if !self.inspected_target_matches(&target.node, target.scoped.as_ref())
            || self.sequence_scope != target.scope
            || self.view != View::Sequence
            || i64::try_from(self.sequence_cursor).ok() != Some(target.cursor.0)
            || matches!(self.pane, Pane::Sources | Pane::Sounds)
            || self.event_focused()
            || self.sound_focused()
        {
            return Err(
                "The selected beat or Edit cursor changed. Reopen Gain; no edit was made.".into(),
            );
        }
        Ok(())
    }

    fn commit_gain(&mut self, target: Target, treatments: AudioTreatments) {
        if let Err(error) = self.check_gain_target(&target) {
            self.error = Some(error);
            return;
        }
        if treatments == target.entry {
            self.message = Some("Gain unchanged. No edit was made.".into());
            return;
        }
        self.stop_playback();
        let edit = target.edit(treatments);
        self.submit(ProjectRequest::Edit {
            expected_session: target.session,
            expected_revision: target.revision,
            cursor: target.cursor,
            scope: target.scope,
            edit,
        });
    }

    pub(super) fn gain_step(&mut self, delta: i32, context: &egui::Context) {
        if self.pane == Pane::Sounds || self.event_focused() {
            self.sound_action(navigation::SoundAction::GainStep(delta), context);
            return;
        }
        let result = self.capture_gain_target().and_then(|target| {
            let mut edit = GainEdit::new(target.entry.clone());
            edit.adjust_trim(delta)?;
            Ok((target, edit.recipe().clone()))
        });
        match result {
            Ok((target, recipe)) => self.commit_gain(target, recipe),
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn gain_mute(&mut self, target: Option<Result<Target, String>>) {
        let result = target
            .unwrap_or_else(|| Err("No gain target was captured on command entry.".into()))
            .and_then(|target| {
                let mut edit = GainEdit::new(target.entry.clone());
                edit.set_muted(!edit.muted())?;
                Ok((target, edit.recipe().clone()))
            });
        match result {
            Ok((target, recipe)) => self.commit_gain(target, recipe),
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn gain_command(
        &mut self,
        target: Option<Result<Target, String>>,
        value: Option<GainDb>,
        context: &egui::Context,
    ) {
        let target =
            target.unwrap_or_else(|| Err("No gain target was captured on command entry.".into()));
        let target = match target.and_then(|target| {
            self.check_gain_target(&target)?;
            Ok(target)
        }) {
            Ok(target) => target,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        if let Some(value) = value {
            let mut edit = GainEdit::new(target.entry.clone());
            match edit.set_trim(value) {
                Ok(()) => self.commit_gain(target, edit.recipe().clone()),
                Err(error) => self.error = Some(error),
            }
            return;
        }
        let result = (|| {
            let workspace = self.workspace.as_ref().ok_or("Open a project first.")?;
            let range = if target.scoped.is_some() {
                self.scoped_presentation()?.ok_or("This definition has no visible play to audition. Use :gain with a value to change its gain.")?.frames
            } else {
                let row = self
                    .beat_rows
                    .iter()
                    .find(|row| row.id == target.node)
                    .ok_or("Selected beat has no visible span.")?;
                row.start
                    ..row
                        .start
                        .checked_add(row.frames)
                        .ok_or("Gain audition range overflowed.")?
            };
            let domain = Domain::Sequence {
                rate: workspace.document.presentation_basis().frame_rate,
                frames: workspace.plan.duration().frames(),
            };
            let window =
                domain.selection_window(range, AudioSample(24_000), AudioSample(36_000))?;
            let owner_frames = workspace
                .plan
                .node_duration(&target.node)
                .ok_or("Gain owner has no duration.")?
                .frames();
            Ok::<_, String>((
                Arc::new(workspace.playback_snapshot()),
                domain,
                window,
                owner_frames,
                workspace.document.nodes()[&target.node].label.clone(),
            ))
        })();
        let (base, domain, window, frames, label) = match result {
            Ok(value) => value,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let Some(token) = self.next_serial() else {
            return;
        };
        self.stop_playback();
        self.cancel_camera();
        self.help_open = false;
        self.bindings.clear();
        self.error = None;
        let edit = GainEdit::new(target.entry.clone());
        let controls = controls::Controls::new(&edit, frames);
        let mut waveform = waveform::Display::default();
        waveform.request(&self.playback, &base, &target.node);
        self.service.set_preview_active(true);
        self.gain = Some(Draft {
            target,
            base,
            token,
            change: 1,
            edit,
            controls,
            waveform,
            pending: None,
            requested: None,
            prepared: None,
            error: None,
            domain,
            window,
            position: window.start(),
            choice: Choice::Draft,
            label,
            source_cursor: self.source_cursor,
            pane: self.pane,
            key: None,
            focus_pending: true,
            cancel_focus: None,
            applying: false,
        });
        context.request_repaint();
    }

    pub(super) fn receive_gain(&mut self, update: Option<ProposalUpdate>) {
        if let (Some(draft), Some(update)) = (&mut self.gain, update) {
            draft.receive(update);
        }
    }

    pub(super) fn receive_gain_waveform(&mut self) {
        if let Some(update) = self.playback.poll_waveform()
            && let Some(draft) = &mut self.gain
        {
            draft.waveform.receive(&draft.target, update);
        }
    }

    pub(super) fn cancel_gain_waveform(&mut self) {
        if let Some(draft) = &mut self.gain {
            draft.waveform.cancel(&self.playback);
        }
    }

    pub(super) fn finish_gain_commit(&mut self, update: &crate::project::ProjectUpdate) {
        let Some(draft) = &mut self.gain else {
            return;
        };
        if !draft.applying {
            return;
        }
        if update
            .committed
            .as_ref()
            .is_some_and(|committed| match &draft.target.scoped {
                Some(target) => committed
                    .scoped
                    .as_ref()
                    .is_some_and(|receipt| &receipt.before == target),
                None => {
                    committed.scoped.is_none()
                        && committed.selected_node.as_ref() == Some(&draft.target.node)
                }
            })
        {
            draft.waveform.cancel(&self.playback);
            self.gain = None;
        } else if let Some(error) = &update.error {
            draft.applying = false;
            draft.error = Some(error.clone());
        }
    }

    pub(super) fn dispatch_gain_proposal(&mut self, context: &egui::Context) {
        let Some(draft) = &mut self.gain else {
            return;
        };
        if draft.applying || draft.pending.is_some() || draft.requested == Some(draft.change) {
            return;
        }
        if self.service.is_busy() {
            context.request_repaint_after(Duration::from_millis(16));
            return;
        }
        let proposal = draft.proposal();
        let id = proposal.id();
        match self.service.submit(ProjectRequest::PrepareGain(proposal)) {
            Ok(()) => {
                draft.pending = Some(id);
                draft.requested = Some(draft.change);
            }
            Err(error) => {
                draft.error = Some(error);
                draft.requested = Some(draft.change);
            }
        }
    }

    pub(super) fn reconcile_gain(&mut self, context: &egui::Context) {
        let failure = self
            .gain
            .as_ref()
            .and_then(|draft| self.check_gain_target(&draft.target).err());
        if let Some(error) = failure {
            self.stop_playback();
            self.cancel_gain_waveform();
            self.gain = None;
            self.error = Some(error);
            context.request_repaint();
        }
    }

    pub(super) fn close_gain(&mut self, context: &egui::Context) {
        self.stop_playback();
        self.cancel_gain_waveform();
        if let Some(draft) = self.gain.take()
            && self
                .workspace
                .as_ref()
                .is_some_and(|workspace| draft.target.validate(workspace).is_ok())
        {
            self.source_cursor = draft.source_cursor;
            self.sequence_cursor = draft.target.cursor.0 as u64;
            self.selected_beat = Some(
                draft
                    .target
                    .scoped
                    .as_ref()
                    .map_or(draft.target.node, |target| target.root.clone()),
            );
            self.sequence_scope = draft.target.scope;
            self.view = View::Sequence;
            self.pane = draft.pane;
            context.memory_mut(|memory| memory.request_focus(pane_id(self.pane)));
            self.request_picture(false);
        }
        self.bindings.clear();
        context.request_repaint();
    }

    pub(super) fn gain_keyboard(&mut self, context: &egui::Context) {
        if help_scroll::defer_popup_input(
            context,
            self.dialogs.is_open(),
            &mut self.ime_composing,
            &mut self.bindings,
        ) {
            return;
        }
        let events = context.input(|input| input.events.clone());
        help_scroll::observe_composition(&events, &mut self.ime_composing);
        self.bindings.clear();
        if self.ime_composing
            || events
                .iter()
                .any(|event| matches!(event, egui::Event::Ime(_)))
            || pointer_focus_transition(&events)
        {
            return;
        }
        let background = context.memory(|memory| {
            memory
                .focused()
                .is_none_or(|id| id == egui::Id::new(FOCUS_ID) || id == pane_id(self.pane))
        });
        for event in events {
            let egui::Event::Key {
                key,
                physical_key,
                modifiers,
                pressed: true,
                repeat,
                ..
            } = event
            else {
                continue;
            };
            if key == egui::Key::Tab
                && (modifiers.is_none() || modifiers.shift_only())
                && let Some(draft) = &self.gain
                && !draft.applying
            {
                let focused = context.memory(|memory| memory.focused());
                let heading = egui::Id::new(FOCUS_ID);
                let destination = if modifiers.shift && focused == Some(heading) {
                    draft.cancel_focus
                } else if !modifiers.shift
                    && draft.cancel_focus.is_some()
                    && focused == draft.cancel_focus
                {
                    Some(heading)
                } else {
                    None
                };
                if let Some(destination) = destination {
                    context.memory_mut(|memory| {
                        memory.move_focus(egui::FocusDirection::None);
                        memory.request_focus(destination);
                    });
                    context.input_mut(|input| {
                        input.consume_key(modifiers, key);
                    });
                    context.request_repaint();
                    break;
                }
            }
            if repeat {
                continue;
            }
            if self.bindings.clone().route_event(
                key,
                physical_key,
                modifiers,
                controls::Controls::text_focused(context),
                false,
                false,
                true,
                navigation::EditSelection::None,
            ) == Some(Action::Render)
            {
                self.render.requested = true;
                context.input_mut(|input| {
                    input.consume_key(modifiers, key);
                });
                break;
            }
            let action = navigation::gain::route_key(
                key,
                modifiers,
                controls::Controls::text_focused(context),
                background,
                false,
                false,
            )
            .map(|action| match action {
                navigation::gain::GainKey::Cancel => DraftAction::Cancel,
                navigation::gain::GainKey::Apply => DraftAction::Apply,
                navigation::gain::GainKey::Play => DraftAction::Play,
            });
            if let Some(action) = action {
                if let Some(draft) = &mut self.gain {
                    draft.key = Some(action);
                }
                context.input_mut(|input| {
                    input.consume_key(modifiers, key);
                });
                break;
            }
        }
    }

    pub(super) fn gain_panel(&mut self, ui: &mut egui::Ui) {
        let panel = egui::Panel::bottom("gain-draft-panel")
            .resizable(false)
            .show_separator_line(true)
            .frame(style::panel());
        let Some(mut draft) = self.gain.take() else {
            // Keep its widget ID without letting frame margins move the
            // remaining workspace over the footer when no draft is open.
            panel
                .exact_size(0.0)
                .frame(egui::Frame::NONE)
                .show_separator_line(false)
                .show(ui, |_| {});
            return;
        };
        let mut action = draft.key.take();
        let mut changed = false;
        let mut retry_waveform = false;
        let height = (ui.ctx().content_rect().height() * 0.34).clamp(224.0, 300.0);
        panel.exact_size(height).show(ui, |ui| {
            let title_width = (ui.available_width() - 350.0).clamp(120.0, 480.0);
            let heading = ui
                .horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(title_width, 20.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!("Gain · {}", draft.label)).strong(),
                                )
                                .truncate(),
                            )
                            .on_hover_text(&draft.label);
                        },
                    );
                    ui.colored_label(style::LAVENDER, "UNSAVED DRAFT");
                    ui.weak("Owner-output frames · Out excluded");
                })
                .response;
            let focus = ui.interact(heading.rect, egui::Id::new(FOCUS_ID), egui::Sense::click());
            focus.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    !draft.applying,
                    "Gain draft keyboard focus",
                )
            });
            if std::mem::take(&mut draft.focus_pending) || focus.clicked() {
                focus.request_focus();
            }
            if focus.has_focus() {
                ui.painter().hline(
                    heading.rect.x_range(),
                    heading.rect.bottom(),
                    egui::Stroke::new(1.0, style::LAVENDER),
                );
            }
            // A fixed outer panel and one bounded scroller keep the
            // comparison/Apply row painted and reachable after resize.
            egui::ScrollArea::vertical()
                .id_salt("gain-draft-scroll")
                .max_height((ui.available_height() - 54.0).max(30.0))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if draft.pending.is_some() || draft.requested != Some(draft.change) {
                        ui.weak("Preparing the latest draft…");
                    }
                    let status = self.transport.as_ref().map_or("Paused", |run| {
                        if run.phase == deadpan_playback::Phase::Preparing {
                            "Preparing audio"
                        } else {
                            "Playing"
                        }
                    });
                    ui.weak(format!(
                        "{} · {status} · {} · loop {}–{}",
                        if draft.choice == Choice::Before {
                            "Before"
                        } else {
                            "Draft"
                        },
                        playback::sound_time(draft.position.0.max(0) as u64),
                        playback::sound_time(draft.window.start().0 as u64),
                        playback::sound_time(draft.window.end().0 as u64)
                    ));
                    ui.add_enabled_ui(!draft.applying, |ui| {
                        changed = draft
                            .controls
                            .show(ui, &mut draft.edit, |ui, owner_frames| {
                                retry_waveform |= draft.waveform.show(ui, owner_frames);
                            });
                    });
                    if let Some(error) = &draft.error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                });
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                for (choice, label) in [(Choice::Before, "Before"), (Choice::Draft, "Draft")] {
                    if ui.selectable_label(draft.choice == choice, label).clicked() {
                        action = Some(DraftAction::Choose(choice));
                    }
                }
                let playing = self.transport.is_some();
                if ui
                    .add_enabled(
                        !draft.applying
                            && (playing
                                || (draft.selected_snapshot().is_some()
                                    && draft.controls.ready(&draft.edit))),
                        egui::Button::new(if playing {
                            "Pause · Space"
                        } else {
                            "Audition · Space"
                        }),
                    )
                    .clicked()
                {
                    action = Some(DraftAction::Play);
                }
                if ui
                    .add_enabled(
                        !draft.applying
                            && draft.selected_snapshot().is_some()
                            && draft.controls.ready(&draft.edit),
                        egui::Button::new("Restart loop"),
                    )
                    .clicked()
                {
                    action = Some(DraftAction::Restart);
                }
                if ui
                    .add_enabled(
                        !draft.applying
                            && !self.service.is_busy()
                            && draft.prepared.is_some()
                            && draft.controls.ready(&draft.edit)
                            && !changed,
                        style::action("Apply", "Enter").fill(style::SELECTED),
                    )
                    .clicked()
                {
                    action = Some(DraftAction::Apply);
                }
                let cancel = ui.add_enabled(!draft.applying, style::action("Cancel", "Esc"));
                draft.cancel_focus = Some(cancel.id);
                if cancel.clicked() {
                    action = Some(DraftAction::Cancel);
                }
            });
        });
        if retry_waveform && !draft.applying {
            draft
                .waveform
                .request(&self.playback, &draft.base, &draft.target.node);
            draft.focus_pending = true;
        }
        if changed {
            if let Some(run) = &self.transport
                && let Ok(position) = run.content_sample()
            {
                draft.position = position;
            }
            self.stop_playback();
            if let Err(error) = draft.changed() {
                draft.error = Some(error);
            }
        }
        self.service.set_preview_active(true);
        self.gain = Some(draft);
        if let Some(action) = action {
            self.gain_action(action, ui.ctx());
        }
    }

    fn gain_action(&mut self, action: DraftAction, context: &egui::Context) {
        if self.gain.as_ref().is_some_and(|draft| draft.applying) {
            return;
        }
        if matches!(action, DraftAction::Cancel) {
            self.close_gain(context);
            return;
        }
        let Some(draft) = &self.gain else {
            return;
        };
        if matches!(action, DraftAction::Choose(choice) if choice == draft.choice) {
            return;
        }
        if let Err(error) = self.check_gain_target(&draft.target) {
            self.error = Some(error);
            return;
        }
        if matches!(action, DraftAction::Restart) && !draft.controls.ready(&draft.edit) {
            return;
        }
        if matches!(action, DraftAction::Apply) {
            if self.service.is_busy()
                || draft.prepared.is_none()
                || !draft.controls.ready(&draft.edit)
            {
                return;
            }
            let target = draft.target.clone();
            let recipe = draft.edit.recipe().clone();
            if target.entry == recipe {
                self.close_gain(context);
                self.message = Some("Gain unchanged. No edit was made.".into());
                return;
            }
            let edit = target.edit(recipe);
            if self.submit(ProjectRequest::Edit {
                expected_session: target.session,
                expected_revision: target.revision,
                cursor: target.cursor,
                scope: target.scope,
                edit,
            }) && let Some(draft) = &mut self.gain
            {
                draft.waveform.cancel(&self.playback);
                draft.applying = true;
            }
            return;
        }
        let running = self.transport.is_some();
        let heard = self
            .transport
            .as_ref()
            .and_then(|run| run.content_sample().ok());
        let Some(draft) = &mut self.gain else {
            return;
        };
        if let Some(position) = heard {
            draft.position = position;
        }
        if let DraftAction::Choose(choice) = action {
            draft.choice = choice;
        }
        if matches!(action, DraftAction::Restart) {
            draft.position = draft.window.start();
        }
        let start = if draft.position >= draft.window.end() {
            draft.window.start()
        } else {
            draft.position
        };
        let snapshot = draft.selected_snapshot();
        let domain = draft.domain.clone();
        let window = draft.window;
        let ready = draft.controls.ready(&draft.edit);
        self.stop_playback();
        let should_play = matches!(action, DraftAction::Restart)
            || matches!(action, DraftAction::Play) && !running
            || matches!(action, DraftAction::Choose(_)) && running;
        if should_play
            && ready
            && let Some(snapshot) = snapshot
        {
            self.start_snapshot_playback(snapshot, domain, window, start);
        }
    }

    pub(super) fn gain_inspector(&mut self, ui: &mut egui::Ui, ready: bool) {
        let Ok(target) = self.capture_gain_target() else {
            return;
        };
        let proposed = self
            .gain
            .as_ref()
            .filter(|draft| draft.target.node == target.node);
        let edit = GainEdit::new(
            proposed
                .map_or(&target.entry, |draft| draft.edit.recipe())
                .clone(),
        );
        let previewing = proposed.is_some();
        ui.add_space(8.0);
        ui.label(style::section_title("GAIN", false));
        ui.horizontal(|ui| {
            ui.label(
                style::semibold(format!("{} dB", crate::gain::format_db(edit.trim()))).size(20.0),
            );
            ui.weak(if previewing { "unsaved" } else { "this beat" });
        });
        ui.add_enabled_ui(ready && self.gain.is_none(), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if ui
                    .add(style::action("−3 dB", self.editor_key(EditorKey::GainDown)))
                    .reveal_on_focus()
                    .clicked()
                {
                    self.gain_step(-3000, ui.ctx());
                }
                if ui
                    .add(style::action("+3 dB", self.editor_key(EditorKey::GainUp)))
                    .reveal_on_focus()
                    .clicked()
                {
                    self.gain_step(3000, ui.ctx());
                }
                if ui
                    .button(if edit.muted() { "Unmute" } else { "Mute" })
                    .on_hover_text(":gain-mute · true silence")
                    .reveal_on_focus()
                    .clicked()
                {
                    self.gain_mute(Some(Ok(target.clone())));
                }
            });
            if ui
                .add(style::row_action(ui, "Edit envelope…", ":gain"))
                .reveal_on_focus()
                .clicked()
            {
                self.gain_command(Some(Ok(target.clone())), None, ui.ctx());
            }
        });
        ui.weak(format!(
            "{} envelopes · {} mute ranges. Placed sounds keep their own gain.",
            edit.envelopes().len(),
            edit.mute_ranges().len()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::{Command, CommandRequest, FrameRate, ProjectDocument, ProjectId};
    use deadpan_store::ProjectStore;
    use std::collections::BTreeMap;

    fn fixture() -> (tempfile::TempDir, ProjectStore, Draft) {
        let scratch = tempfile::tempdir().unwrap();
        let document = Arc::new(
            ProjectDocument::new_automatic(
                ProjectId::new("gain-ui").unwrap(),
                RevisionId::new("base").unwrap(),
                NodeId::new("root").unwrap(),
            )
            .unwrap(),
        );
        let store = ProjectStore::create(&scratch.path().join("gain.deadpan"), &document).unwrap();
        let base = Arc::new(Snapshot::committed(
            7,
            document.clone(),
            BTreeMap::new(),
            store.original_import_handle().unwrap(),
        ));
        let edit = GainEdit::new(AudioTreatments::default());
        let draft = Draft {
            target: Target {
                session: 7,
                project: document.project_id().clone(),
                revision: document.revision_id().clone(),
                scope: SequenceScope::default(),
                node: document.root().clone(),
                cursor: ProjectFrame(0),
                entry: edit.recipe().clone(),
                scoped: None,
            },
            base,
            token: 10,
            change: 1,
            controls: controls::Controls::new(&edit, 10),
            waveform: waveform::Display::default(),
            edit,
            pending: None,
            requested: None,
            prepared: None,
            error: None,
            domain: Domain::Sequence {
                rate: FrameRate::new(30, 1).unwrap(),
                frames: 10,
            },
            window: Window::new(AudioSample(0), AudioSample(16000), true).unwrap(),
            position: AudioSample(137),
            choice: Choice::Draft,
            label: "Fixture".into(),
            source_cursor: 4,
            pane: Pane::Sequence,
            key: None,
            focus_pending: false,
            cancel_focus: None,
            applying: false,
        };
        (scratch, store, draft)
    }

    #[test]
    fn replaced_gain_proposals_coalesce_and_stale_errors_cannot_consume_new_request() {
        let (_scratch, _store, mut draft) = fixture();
        let old = draft.proposal().id();
        draft.pending = Some(old.clone());
        draft.requested = Some(1);
        draft.edit.adjust_trim(3000).unwrap();
        draft.changed().unwrap();
        draft.edit.adjust_trim(3000).unwrap();
        draft.changed().unwrap();
        assert_eq!(draft.change, 3);
        assert_eq!(draft.pending, Some(old.clone()));
        draft.receive(ProposalUpdate {
            id: old.clone(),
            result: Err("old failure".into()),
        });
        assert!(draft.pending.is_none());
        assert!(draft.error.is_none());
        assert!(draft.prepared.is_none());
        let current = draft.proposal().id();
        draft.pending = Some(current.clone());
        for case in 0..5 {
            let mut stale = current.clone();
            match case {
                0 => stale = old.clone(),
                1 => stale.session += 1,
                2 => stale.project = ProjectId::new("stale-project").unwrap(),
                3 => stale.revision = RevisionId::new("stale-revision").unwrap(),
                _ => stale.draft += 1,
            }
            draft.receive(ProposalUpdate {
                id: stale,
                result: Err("stale".into()),
            });
            assert_eq!(draft.pending, Some(current.clone()));
            assert!(draft.error.is_none());
        }
        draft.receive(ProposalUpdate {
            id: current,
            result: Err("current failure".into()),
        });
        assert!(draft.pending.is_none());
        assert_eq!(draft.error.as_deref(), Some("current failure"));
        assert_eq!(draft.edit.trim().millidecibels(), 6000);
        assert_eq!(draft.position, AudioSample(137));
    }

    #[test]
    fn tagged_gain_success_admits_only_current_proposed_content() {
        let (_scratch, store, mut draft) = fixture();
        let id = draft.proposal().id();
        draft.pending = Some(id.clone());
        draft.receive(ProposalUpdate {
            id: id.clone(),
            result: Ok(draft.base.clone()),
        });
        assert!(draft.prepared.is_none());
        assert!(draft.error.is_some());
        let request = CommandRequest {
            project_id: draft.target.project.clone(),
            expected_revision: draft.target.revision.clone(),
            new_revision: RevisionId::new("proposal").unwrap(),
            command: Command::SetAudioTreatments {
                node: draft.target.node.clone(),
                treatments: AudioTreatments::from_clip_gain(deadpan_core::ClipGain::default()),
            },
        };
        let document = Arc::new(
            store
                .preview(&request)
                .unwrap()
                .forward
                .apply(&draft.base.document)
                .unwrap(),
        );
        let proposal =
            Arc::new(Snapshot::proposed(&draft.base, document, draft.token, draft.change).unwrap());
        draft.pending = Some(id.clone());
        draft.receive(ProposalUpdate {
            id,
            result: Ok(proposal.clone()),
        });
        assert!(draft.error.is_none());
        assert!(Arc::ptr_eq(&draft.selected_snapshot().unwrap(), &proposal));
        draft.choice = Choice::Before;
        assert!(Arc::ptr_eq(
            &draft.selected_snapshot().unwrap(),
            &draft.base
        ));
        draft.choice = Choice::Draft;
        draft.changed().unwrap();
        assert!(draft.selected_snapshot().is_none());
        assert_eq!(*draft.base.document, store.snapshot().unwrap());
    }
}
