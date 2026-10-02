//! Explicit, captured-target room-tone drafts. Preview never authors a Hold.

use deadpan_core::{AudioSample, HoldAudio, SourceAudio, SourceSpan, SourceTimestamp};

use super::*;
use crate::project::{PreparedRoomTone, RoomToneFailure, RoomToneSelection};
use crate::transport::{ContentRef, Domain};

const IN_ID: &str = "room-tone-in";
const OUT_ID: &str = "room-tone-out";
const FOCUS_ID: &str = "room-tone-focus";

#[derive(Clone)]
struct Target {
    session: u64,
    revision: RevisionId,
    scope: SequenceScope,
    node: NodeId,
    cursor: ProjectFrame,
    label: String,
    frames: i64,
}

pub(super) struct CommandTarget {
    target: Result<Target, String>,
    selection: Option<RoomToneSelection>,
    copied: Option<RoomToneSelection>,
}

#[derive(Clone, Copy)]
enum SheetAction {
    Apply,
    Prepare,
    UseCopied,
    Play,
    Loop,
    Cancel,
}

pub(super) struct Draft {
    target: Target,
    pub(super) prepared: Option<PreparedRoomTone>,
    pending: Option<u64>,
    basis: Option<(SourceAudio, deadpan_core::SourceQualificationId)>,
    copied: Option<RoomToneSelection>,
    start: String,
    end: String,
    error: Option<String>,
    key: Option<SheetAction>,
    focus_pending: bool,
    pub(super) cursor: u64,
}

impl Draft {
    fn receive_failure(&mut self, failure: RoomToneFailure) {
        if self.pending == Some(failure.ticket)
            && self.target.session == failure.session
            && self.target.revision == failure.revision
        {
            self.pending = None;
            self.error = Some(failure.error);
        }
    }
    fn source(&self) -> Result<SourceAudio, String> {
        let (basis, _) = self.basis.as_ref().ok_or("Preparing the source range…")?;
        let parse = |value: &str| {
            value.trim().parse::<i64>().map_err(|_| "Enter whole source samples, including a minus sign only for a signed source origin.".to_owned())
        };
        let span = SourceSpan::new(
            SourceTimestamp {
                ticks: parse(&self.start)?,
                time_base: basis.span.start().time_base,
            },
            SourceTimestamp {
                ticks: parse(&self.end)?,
                time_base: basis.span.start().time_base,
            },
        )
        .map_err(|error| error.to_string())?;
        if span.start().ticks >= span.end().ticks {
            return Err("Out must be later than In; the Out sample is excluded.".into());
        }
        Ok(SourceAudio {
            asset: basis.asset.clone(),
            span,
        })
    }

    fn ready(&self) -> bool {
        self.pending.is_none()
            && self
                .prepared
                .as_ref()
                .is_some_and(|prepared| self.source().is_ok_and(|source| source == prepared.source))
    }
}

impl DeadpanApp {
    pub(super) fn room_tone_render_edit(
        &self,
    ) -> Result<Option<super::render::PreviewEdit>, String> {
        let draft = self
            .room_tone
            .as_ref()
            .ok_or("Room tone preview is no longer open.")?;
        self.check_hold_target(&draft.target)?;
        if !draft.ready() {
            return Err(draft
                .error
                .clone()
                .unwrap_or_else(|| "Prepare the room tone range before committing it.".into()));
        }
        let audio = HoldAudio::RoomTone {
            source: draft.prepared.as_ref().expect("ready draft").source.clone(),
        };
        if self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.document.nodes().get(&draft.target.node))
            .is_some_and(
                |node| matches!(&node.kind, NodeKind::Hold { recipe } if recipe.audio == audio),
            )
        {
            return Ok(None);
        }
        Ok(Some(super::render::PreviewEdit {
            session: draft.target.session,
            revision: draft.target.revision.clone(),
            cursor: draft.target.cursor,
            scope: draft.target.scope.clone(),
            edit: ProjectEdit::HoldAudio {
                node: draft.target.node.clone(),
                audio,
            },
        }))
    }

    pub(super) fn capture_hold_command(&self) -> CommandTarget {
        let target = (|| {
            if self.view != View::Sequence || self.event_focused() || self.sound_focused() {
                return Err(
                    "Select an ordinary pause in Your edit before opening its audio controls."
                        .into(),
                );
            }
            let workspace = self.workspace.as_ref().ok_or("Open a project first.")?;
            let node = self.selected_beat.as_ref().ok_or("Select a pause first.")?;
            if !self
                .sequence_scope
                .resolve(workspace)?
                .children
                .contains(node)
            {
                return Err("The selected pause is outside the displayed group.".into());
            }
            let beat = workspace
                .document
                .nodes()
                .get(node)
                .ok_or("The pause is unavailable.")?;
            let NodeKind::Hold { recipe } = &beat.kind else {
                return Err("Select an ordinary Hold. Repeat gaps and fragments need their own occurrence controls.".into());
            };
            Ok(Target {
                session: workspace.session,
                revision: workspace.document.revision_id().clone(),
                scope: self.sequence_scope.clone(),
                node: node.clone(),
                cursor: ProjectFrame(
                    i64::try_from(self.sequence_cursor).map_err(|e| e.to_string())?,
                ),
                label: beat.label.clone(),
                frames: recipe.duration.frames(),
            })
        })();
        let copied = self.copied.copied_audio_selection();
        let selection = (|| {
            let target = target.as_ref().ok()?;
            let workspace = self.workspace.as_ref()?;
            let NodeKind::Hold { recipe } = &workspace.document.nodes().get(&target.node)?.kind
            else {
                return None;
            };
            let HoldAudio::RoomTone { source } = &recipe.audio else {
                return None;
            };
            let registered = workspace.sources.get(&source.asset)?;
            Some(RoomToneSelection::Exact {
                source: source.clone(),
                qualification: registered.receipt.id().clone(),
            })
        })()
        .or_else(|| copied.clone());
        CommandTarget {
            target,
            selection,
            copied,
        }
    }

    fn check_hold_target(&self, target: &Target) -> Result<(), String> {
        if self.workspace.as_ref().is_none_or(|workspace| {
            workspace.session != target.session
                || workspace.document.revision_id() != &target.revision
        }) || self.sequence_scope != target.scope
            || self.selected_beat.as_ref() != Some(&target.node)
            || i64::try_from(self.sequence_cursor).ok() != Some(target.cursor.0)
            || self.view != View::Sequence
        {
            return Err("The pause, cursor or project changed. Open its audio controls again; no edit was made.".into());
        }
        Ok(())
    }

    pub(super) fn open_room_tone(
        &mut self,
        captured: Option<CommandTarget>,
        context: &egui::Context,
    ) {
        let result = (|| {
            let captured = captured.ok_or("No pause was captured when command entry opened.")?;
            let target = captured.target?;
            self.check_hold_target(&target)?;
            let selection = captured.selection.ok_or_else(|| format!("Copy a quiet Original range first: :source, {}. Return to Your edit and select a pause, then :room-tone.", self.editor_copy_recipe()))?;
            Ok::<_, String>((target, selection, captured.copied))
        })();
        let (target, selection, copied) = match result {
            Ok(pair) => pair,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        self.stop_playback();
        self.cancel_camera();
        self.help_open = false;
        self.bindings.clear();
        self.service.set_preview_active(true);
        self.room_tone = Some(Draft {
            target,
            prepared: None,
            pending: None,
            basis: None,
            copied,
            start: String::new(),
            end: String::new(),
            error: None,
            key: None,
            focus_pending: true,
            cursor: 0,
        });
        self.prepare_room_tone(selection);
        context.request_repaint();
    }

    fn prepare_room_tone(&mut self, selection: RoomToneSelection) {
        self.stop_playback();
        let Some(ticket) = self.next_serial() else {
            return;
        };
        let Some(draft) = self.room_tone.as_mut() else {
            return;
        };
        draft.prepared = None;
        draft.cursor = 0;
        draft.error = None;
        let request = ProjectRequest::PrepareRoomTone {
            expected_session: draft.target.session,
            expected_revision: draft.target.revision.clone(),
            ticket,
            selection,
        };
        match self.service.submit(request) {
            Ok(()) => draft.pending = Some(ticket),
            Err(error) => {
                draft.pending = None;
                draft.error = Some(error);
            }
        }
    }

    pub(super) fn receive_room_tone(
        &mut self,
        prepared: Option<PreparedRoomTone>,
        failure: Option<RoomToneFailure>,
    ) {
        let Some(draft) = self.room_tone.as_mut() else {
            return;
        };
        if let Some(prepared) = prepared {
            if draft.pending != Some(prepared.ticket)
                || draft.target.session != prepared.session
                || draft.target.revision != prepared.revision
            {
                return;
            }
            draft.pending = None;
            draft.start = prepared.source.span.start().ticks.to_string();
            draft.end = prepared.source.span.end().ticks.to_string();
            draft.basis = Some((
                prepared.source.clone(),
                prepared.audition.qualification_id().clone(),
            ));
            draft.prepared = Some(prepared);
            draft.error = None;
        } else if let Some(failure) = failure {
            draft.receive_failure(failure);
        }
    }

    pub(super) fn reconcile_room_tone(&mut self, context: &egui::Context) {
        if let Some(draft) = &self.room_tone
            && let Err(error) = self.check_hold_target(&draft.target)
        {
            self.close_room_tone(context);
            self.error = Some(error);
        }
    }

    pub(super) fn close_room_tone(&mut self, context: &egui::Context) {
        self.stop_playback();
        self.room_tone = None;
        self.bindings.clear();
        context.memory_mut(|memory| memory.request_focus(pane_id(self.pane)));
        // Modal focus ownership is retained through egui's end_pass. Paint a
        // pass without the sheet now, so the very next key can enter a command
        // instead of losing its field focus to the just-closed modal layer.
        context.request_discard("room-tone sheet closed");
        context.request_repaint();
    }

    pub(super) fn silence_hold(&mut self, captured: Option<CommandTarget>) {
        let result = (|| {
            let target = captured
                .ok_or("No pause was captured when command entry opened.")?
                .target?;
            self.check_hold_target(&target)?;
            Ok::<_, String>(hold_request(target, HoldAudio::Silence))
        })();
        match result {
            Ok(request) => {
                self.submit(request);
            }
            Err(error) => self.error = Some(error),
        }
    }

    /// Modal routing precedes the editor's normal keys. Text and focused button
    /// activation remain native; final text is consumed before a deferred Apply.
    pub(super) fn room_tone_keyboard(&mut self, context: &egui::Context) {
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
        let field = context.memory(|memory| {
            [IN_ID, OUT_ID]
                .iter()
                .any(|id| memory.has_focus(egui::Id::new(id)))
        });
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
                repeat: false,
                ..
            } = event
            else {
                continue;
            };
            use navigation::room_tone::RoomToneKey;
            if self.bindings.clone().route_event(
                key,
                physical_key,
                modifiers,
                field,
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
            let action =
                navigation::room_tone::route_key(key, modifiers, field, background, false, false)
                    .map(|key| match key {
                        RoomToneKey::Apply => SheetAction::Apply,
                        RoomToneKey::Cancel => SheetAction::Cancel,
                        RoomToneKey::Play => SheetAction::Play,
                        RoomToneKey::Loop => SheetAction::Loop,
                    });
            if let Some(action) = action {
                if let Some(draft) = &mut self.room_tone {
                    draft.key = Some(action);
                }
                context.input_mut(|input| {
                    input.consume_key(modifiers, key);
                });
                break;
            }
        }
    }

    pub(super) fn room_tone_sheet(&mut self, context: &egui::Context) {
        let Some(mut draft) = self.room_tone.take() else {
            return;
        };
        let mut action = draft.key.take();
        if matches!(action, Some(SheetAction::Cancel)) {
            self.close_room_tone(context);
            return;
        }
        let mut changed = false;
        let busy = self.service.is_busy();
        let ready = draft.ready();
        let source_label = draft
            .basis
            .as_ref()
            .and_then(|(source, _)| self.workspace.as_ref()?.sources.get(&source.asset))
            .map(|source| source.label.clone());
        let width = (context.content_rect().width() - 64.0).clamp(260.0, 520.0);
        egui::Modal::new(egui::Id::new("room-tone-sheet"))
            .frame(egui::Frame::popup(&context.style_of(egui::Theme::Dark)).inner_margin(24).corner_radius(10))
            .show(context, |ui| {
                ui.set_width(width);
                let heading = ui.horizontal(|ui| {
                    ui.heading("Room tone for pause");
                    ui.colored_label(style::LAVENDER, "DRAFT");
                }).response;
                let focus = ui.interact(heading.rect, egui::Id::new(FOCUS_ID), egui::Sense::focusable_noninteractive());
                if std::mem::take(&mut draft.focus_pending) { focus.request_focus(); }
                ui.label(format!("{} · {} f · picture and duration stay intact", draft.target.label, draft.target.frames));
                ui.separator();
                egui::ScrollArea::vertical().id_salt("room-tone-details").max_height((context.content_rect().height() - 225.0).max(160.0)).show(ui, |ui| {
                    ui.label(egui::RichText::new(source_label.as_deref().unwrap_or("Original range")).strong());
                    ui.weak("Listen for quiet words. Choose non-speech material; nothing is detected or normalized automatically.");
                    if ui.add_enabled(!busy && draft.copied.is_some(), egui::Button::new("Use copied Original range")).clicked() {
                        action = Some(SheetAction::UseCopied);
                    }
                    if let Some((source, _)) = &draft.basis {
                        let rate = source.span.start().time_base.denominator();
                        ui.label(format!("Source samples · {rate} Hz · Out excluded"));
                        ui.columns(2, |columns| {
                            let label = columns[0].label("Room tone In sample");
                            changed |= columns[0].add(egui::TextEdit::singleline(&mut draft.start).id(egui::Id::new(IN_ID)).return_key(None).desired_width(f32::INFINITY)).labelled_by(label.id).changed();
                            let label = columns[1].label("Room tone Out sample");
                            changed |= columns[1].add(egui::TextEdit::singleline(&mut draft.end).id(egui::Id::new(OUT_ID)).return_key(None).desired_width(f32::INFINITY)).labelled_by(label.id).changed();
                        });
                        if let Ok(source) = draft.source() {
                            let rate = f64::from(rate);
                            ui.colored_label(style::LAVENDER, format!("Source {:.3}–{:.3} s · {:.3} s selected", source.span.start().ticks as f64 / rate, source.span.end().ticks as f64 / rate, (i128::from(source.span.end().ticks) - i128::from(source.span.start().ticks)) as f64 / rate));
                        }
                        if ui.add_enabled(!busy && draft.source().is_ok() && (!ready || changed), egui::Button::new("Prepare range")).clicked() { action = Some(SheetAction::Prepare); }
                    }
                    if draft.pending.is_some() { ui.weak("Preparing the exact source range…"); }
                    else if !ready || changed { ui.weak("Prepare the range before audition or Apply."); }
                    if let Some(error) = &draft.error { ui.colored_label(ui.visuals().error_fg_color, error); }
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        let play_label = match self.transport.as_ref().map(|run| run.phase) {
                            Some(deadpan_playback::Phase::Preparing) => "Cancel source  ·  Space",
                            Some(_) => "Pause source  ·  Space",
                            None => "Play source  ·  Space",
                        };
                        if ui.add_enabled(ready && !changed && !busy, egui::Button::new(play_label)).clicked() { action = Some(SheetAction::Play); }
                        if ui.add_enabled(ready && !changed && !busy, egui::Button::new("Loop source  ·  Shift+Space")).clicked() { action = Some(SheetAction::Loop); }
                    });
                    let total = draft.prepared.as_ref().map_or(0, |p| p.audition.duration_samples().0.max(0) as u64);
                    let status = self.transport.as_ref().map_or("Stopped", |run| if run.phase == deadpan_playback::Phase::Preparing { "Preparing" } else { "Playing" });
                    ui.monospace(format!("Source preview · {status} · {} / {}", playback::sound_time(draft.cursor), playback::sound_time(total)));
                    ui.weak("Source preview has no lead-in or follow-through. After Apply, audition the pause to hear its crossfaded loop.");
                });
                ui.separator();
                ui.weak("Apply changes only this pause's sound. One edit, one undo.");
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(ready && !changed && !busy, egui::Button::new("Apply room tone  ·  Enter").fill(style::SELECTED)).clicked() { action = Some(SheetAction::Apply); }
                    if ui.button("Cancel  ·  Esc").clicked() { action = Some(SheetAction::Cancel); }
                });
                ui.weak("Tab / Shift+Tab moves through controls. Escape discards the draft.");
            });
        if changed {
            draft.prepared = None;
            draft.pending = None;
            draft.error = None;
            draft.cursor = 0;
            self.stop_playback();
        }
        self.service.set_preview_active(true);
        self.room_tone = Some(draft);
        if let Some(action) = action {
            self.room_tone_action(action, context);
        }
    }

    fn room_tone_action(&mut self, action: SheetAction, context: &egui::Context) {
        if matches!(action, SheetAction::Cancel) {
            self.close_room_tone(context);
            return;
        }
        let Some(draft) = self.room_tone.as_ref() else {
            return;
        };
        if let Err(error) = self.check_hold_target(&draft.target) {
            self.close_room_tone(context);
            self.error = Some(error);
            return;
        }
        if self.service.is_busy() {
            return;
        }
        if matches!(action, SheetAction::UseCopied) {
            if let Some(selection) = draft.copied.clone() {
                self.prepare_room_tone(selection);
            }
            return;
        }
        if matches!(action, SheetAction::Prepare)
            || (matches!(action, SheetAction::Apply) && !draft.ready())
        {
            let result = draft.source().and_then(|source| {
                let qualification = draft
                    .basis
                    .as_ref()
                    .ok_or("No source range is available.")?
                    .1
                    .clone();
                Ok(RoomToneSelection::Exact {
                    source,
                    qualification,
                })
            });
            match result {
                Ok(selection) => self.prepare_room_tone(selection),
                Err(error) => {
                    if let Some(draft) = &mut self.room_tone {
                        draft.error = Some(error);
                    }
                }
            }
            return;
        }
        if !draft.ready() {
            return;
        }
        match action {
            SheetAction::Apply => {
                let target = draft.target.clone();
                let source = draft.prepared.as_ref().expect("ready draft").source.clone();
                if self.submit(hold_request(target, HoldAudio::RoomTone { source })) {
                    self.close_room_tone(context);
                }
            }
            SheetAction::Play | SheetAction::Loop => {
                self.preview_room_tone(matches!(action, SheetAction::Loop))
            }
            SheetAction::Prepare | SheetAction::UseCopied | SheetAction::Cancel => {}
        }
    }

    fn preview_room_tone(&mut self, looping: bool) {
        if self.transport.is_some() {
            self.receive_playback();
            self.pause_playback();
            return;
        }
        let Some(draft) = &self.room_tone else {
            return;
        };
        let Some(prepared) = &draft.prepared else {
            return;
        };
        let domain = Domain::AudioRange(prepared.audition.clone());
        let Ok(window) = deadpan_playback::Window::new(
            AudioSample(0),
            prepared.audition.duration_samples(),
            looping,
        ) else {
            return;
        };
        let resumed = self
            .resume
            .as_ref()
            .and_then(|resume| {
                let workspace = self.workspace.as_ref()?;
                resume.sample_for_domain(
                    ContentRef {
                        session: workspace.session,
                        project: workspace.document.project_id(),
                        revision: &draft.target.revision,
                        content: &deadpan_playback::ContentIdentity::Committed,
                    },
                    &domain,
                    &window,
                    draft.cursor,
                )
            })
            .filter(|sample| looping || *sample < window.end());
        self.start_domain_playback(domain, window, resumed.unwrap_or(AudioSample(0)));
    }

    pub(super) fn hold_audio_controls(&mut self, ui: &mut egui::Ui, ready: bool) {
        ui.add_space(8.0);
        if let Some(workspace) = &self.workspace
            && let Some(node) = self
                .selected_beat
                .as_ref()
                .and_then(|id| workspace.document.nodes().get(id))
            && let NodeKind::Hold { recipe } = &node.kind
            && let HoldAudio::RoomTone { source } = &recipe.audio
        {
            let label = workspace
                .sources
                .get(&source.asset)
                .map_or(source.asset.as_str(), |s| s.label.as_str());
            ui.colored_label(style::LAVENDER, "Room tone");
            ui.label(label);
            ui.small(format!(
                "Samples [{}..{}) · {} Hz",
                source.span.start().ticks,
                source.span.end().ticks,
                source.span.start().time_base.denominator()
            ));
            ui.weak("Crossfades stay inside this pause.");
        }
        if ui
            .add_enabled(ready, egui::Button::new("Room tone…  ·  :room-tone"))
            .clicked()
        {
            self.open_room_tone(Some(self.capture_hold_command()), ui.ctx());
        }
        if ui
            .add_enabled(ready, egui::Button::new("Use silence  ·  :hold-silence"))
            .clicked()
        {
            self.silence_hold(Some(self.capture_hold_command()));
        }
        ui.weak(format!(
            "Copy a quiet Original range with {}. Silence retains explicit sound permissions.",
            self.editor_copy_recipe()
        ));
    }
}

fn hold_request(target: Target, audio: HoldAudio) -> ProjectRequest {
    ProjectRequest::Edit {
        expected_session: target.session,
        expected_revision: target.revision,
        cursor: target.cursor,
        scope: target.scope,
        edit: ProjectEdit::HoldAudio {
            node: target.node,
            audio,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> Draft {
        let base = deadpan_core::SourceTimeBase::new(1, 44_100).unwrap();
        Draft {
            target: Target {
                session: 3,
                revision: RevisionId::new("current").unwrap(),
                scope: SequenceScope::default(),
                node: NodeId::new("pause").unwrap(),
                cursor: ProjectFrame(11),
                label: "Pause".into(),
                frames: 11,
            },
            prepared: None,
            pending: Some(20),
            basis: Some((
                SourceAudio {
                    asset: AssetId::new("original").unwrap(),
                    span: SourceSpan::new(
                        SourceTimestamp {
                            ticks: -441,
                            time_base: base,
                        },
                        SourceTimestamp {
                            ticks: 441,
                            time_base: base,
                        },
                    )
                    .unwrap(),
                },
                deadpan_core::SourceQualificationId::new("a".repeat(64)).unwrap(),
            )),
            copied: None,
            start: "-441".into(),
            end: "441".into(),
            error: None,
            key: None,
            focus_pending: false,
            cursor: 0,
        }
    }

    #[test]
    fn delayed_failures_cannot_consume_a_newer_range_ticket_or_context() {
        let mut draft = draft();
        for (ticket, session, revision) in [(19, 3, "current"), (20, 2, "current"), (20, 3, "old")]
        {
            draft.receive_failure(RoomToneFailure {
                ticket,
                session,
                revision: RevisionId::new(revision).unwrap(),
                error: "old failure".into(),
            });
            assert_eq!(draft.pending, Some(20));
            assert!(draft.error.is_none());
        }
        draft.receive_failure(RoomToneFailure {
            ticket: 20,
            session: 3,
            revision: RevisionId::new("current").unwrap(),
            error: "range outside source".into(),
        });
        assert_eq!(draft.pending, None);
        assert_eq!(draft.error.as_deref(), Some("range outside source"));
    }

    #[test]
    fn native_sample_fields_keep_signed_source_units_and_reject_invalid_ranges() {
        let mut draft = draft();
        let source = draft.source().unwrap();
        assert_eq!(source.span.start().ticks, -441);
        assert_eq!(source.span.end().ticks, 441);
        assert_eq!(
            source.span.start().time_base,
            deadpan_core::SourceTimeBase::new(1, 44_100).unwrap()
        );
        for (start, end) in [
            ("0", "0"),
            ("2", "1"),
            ("0.5", "100"),
            ("9223372036854775808", "100"),
            ("dd", "100"),
        ] {
            draft.start = start.into();
            draft.end = end.into();
            assert!(draft.source().is_err(), "{start}..{end}");
        }
    }
}
