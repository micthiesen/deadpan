//! Explicit, captured-target pause sound drafts: room tone or a custom sound
//! (a catalog sound or an Original range, once or looped, with fades).
//! Preview never authors a Hold.

use deadpan_core::{AudioSample, HoldAudio, SourceAudio, SourceSpan, SourceTimestamp};

use super::*;
use crate::project::{PreparedRoomTone, RoomToneFailure, RoomToneSelection};
use crate::transport::{ContentRef, Domain};

const IN_ID: &str = "room-tone-in";
const OUT_ID: &str = "room-tone-out";
const FOCUS_ID: &str = "room-tone-focus";
const FADE_IN_ID: &str = "custom-sound-fade-in";
const FADE_OUT_ID: &str = "custom-sound-fade-out";

#[derive(Clone)]
struct Target {
    session: u64,
    revision: RevisionId,
    scope: SequenceScope,
    node: NodeId,
    cursor: ProjectFrame,
    label: String,
    frames: i64,
    /// Which recipe the edit changes: `node` is the selected beat, which for
    /// a fragment or Repeat gap differs from the recipe's owner.
    pause: deadpan_core::PauseSoundTarget,
    scoped: Option<crate::project::scoped::Target>,
    /// The inspector's shared gap row: the one-based play this Repeat gap
    /// follows. Apply gives that play its own copy (`SetPlayGap`).
    play_gap: Option<u32>,
}

impl Target {
    fn heading_for(&self, custom: bool) -> String {
        let heading = self.heading();
        if custom {
            heading.replacen("Room tone", "Custom sound", 1)
        } else {
            heading.to_owned()
        }
    }

    fn heading(&self) -> &'static str {
        if self.play_gap.is_some() {
            return "Room tone for this play's gap";
        }
        match &self.pause {
            deadpan_core::PauseSoundTarget::Hold {
                fragment: false, ..
            } => "Room tone for pause",
            deadpan_core::PauseSoundTarget::Hold { fragment: true, .. } => {
                "Room tone for pause fragment"
            }
            deadpan_core::PauseSoundTarget::RepeatGap { .. } => "Room tone for Repeat gaps",
        }
    }

    fn scope_note(&self) -> &'static str {
        if self.play_gap.is_some() {
            return "Apply gives this play its own copy of the shared gap with this sound; the other plays keep the shared gap. One edit, one undo.";
        }
        match &self.pause {
            deadpan_core::PauseSoundTarget::Hold {
                fragment: false, ..
            } => "Apply changes only this pause's sound. One edit, one undo.",
            deadpan_core::PauseSoundTarget::Hold { fragment: true, .. } => {
                "Apply changes only this fragment's sound; the rest of the split pause keeps its own. One edit, one undo."
            }
            deadpan_core::PauseSoundTarget::RepeatGap { .. } => {
                "Apply changes every gap without its own pause; gaps with their own pause keep their sound. One edit, one undo."
            }
        }
    }

    fn edit(&self, audio: HoldAudio) -> ProjectEdit {
        match &self.scoped {
            Some(target) => ProjectEdit::Scoped {
                target: Box::new(target.clone()),
                edit: deadpan_core::ScopedNodeEdit::SetHoldAudio { audio },
            },
            None => ProjectEdit::HoldAudio {
                node: self.node.clone(),
                audio,
            },
        }
    }
}

pub(super) struct CommandTarget {
    target: Result<Target, String>,
    selection: Option<RoomToneSelection>,
    copied: Option<RoomToneSelection>,
    /// The pause's saved custom sound options, when it has one.
    custom: Option<CustomOptions>,
}

/// A custom sound's playback options, as authored on the Hold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CustomOptions {
    looped: bool,
    fade_in_ms: u32,
    fade_out_ms: u32,
}

/// One audio-only catalog sound a custom pause sound can start from.
#[derive(Clone)]
struct CatalogSound {
    label: String,
    source: SourceAudio,
    qualification: deadpan_core::SourceQualificationId,
}

#[derive(Clone, Copy)]
enum SheetAction {
    Apply,
    Prepare,
    UseCopied,
    UseSound(usize),
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
    waveform: RangeWaveform,
    /// Custom sound instead of room tone.
    pub(super) custom: bool,
    pub(super) looped: bool,
    fade_in: String,
    fade_out: String,
    sounds: Vec<CatalogSound>,
}

/// The prepared range's measured stereo waveform, as its audition plays it.
#[derive(Default)]
struct RangeWaveform {
    ticket: Option<deadpan_playback::WaveformTicket>,
    data: Option<Arc<deadpan_audio::EditWaveform>>,
    status: Option<deadpan_playback::WaveformStatus>,
    error: Option<String>,
}

impl Draft {
    /// Waveform status and measured end, for replay checks.
    #[cfg(feature = "ui-harness")]
    pub(super) fn waveform_for_check(&self) -> (Option<String>, Option<i64>, Option<String>) {
        (
            self.waveform.status.map(|status| format!("{status:?}")),
            self.waveform
                .data
                .as_ref()
                .map(|data| data.measured_end().0),
            self.waveform.error.clone(),
        )
    }

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

    /// The Hold audio Apply authors from the prepared range.
    fn audio(&self) -> Result<HoldAudio, String> {
        let source = self
            .prepared
            .as_ref()
            .ok_or("Prepare the source range first.")?
            .source
            .clone();
        if !self.custom {
            return Ok(HoldAudio::RoomTone { source });
        }
        Ok(HoldAudio::Custom {
            source,
            looped: self.looped,
            fade_in_ms: parse_fade(&self.fade_in, "Fade in")?,
            fade_out_ms: parse_fade(&self.fade_out, "Fade out")?,
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
        if draft.target.play_gap.is_some() {
            return Err(
                "Apply this play's gap sound first, then render; a render preview cannot isolate the gap."
                    .into(),
            );
        }
        if !draft.ready() {
            return Err(draft
                .error
                .clone()
                .unwrap_or_else(|| "Prepare the source range before committing it.".into()));
        }
        let audio = draft.audio()?;
        if self
            .workspace
            .as_ref()
            .and_then(|workspace| draft.target.pause.recipe(&workspace.document))
            .is_some_and(|recipe| recipe.audio == audio)
        {
            return Ok(None);
        }
        Ok(Some(super::render::PreviewEdit {
            session: draft.target.session,
            revision: draft.target.revision.clone(),
            cursor: draft.target.cursor,
            scope: draft.target.scope.clone(),
            edit: draft.target.edit(audio),
        }))
    }

    pub(super) fn capture_hold_command(&self) -> CommandTarget {
        let target = (|| {
            if self.view != View::Sequence || self.event_focused() || self.sound_focused() {
                return Err(
                    "Select a pause in Your edit before opening its audio controls.".into(),
                );
            }
            let workspace = self.workspace.as_ref().ok_or("Open a project first.")?;
            let scoped = self.scoped_target()?;
            let node = scoped
                .as_ref()
                .map(|target| &target.target.node)
                .or(self.selected_beat.as_ref())
                .ok_or("Select a pause first.")?;
            if scoped.is_none()
                && !self
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
            if let Some((recipe, play, repeat)) = scoped.as_ref().and(self.scoped_shared_gap()) {
                return Ok(Target {
                    session: workspace.session,
                    revision: workspace.document.revision_id().clone(),
                    scope: self.sequence_scope.clone(),
                    node: node.clone(),
                    cursor: ProjectFrame(
                        i64::try_from(self.sequence_cursor).map_err(|e| e.to_string())?,
                    ),
                    label: format!("Gap after play {play}"),
                    frames: recipe.duration.frames(),
                    pause: deadpan_core::PauseSoundTarget::RepeatGap { node: repeat },
                    scoped,
                    play_gap: Some(play),
                });
            }
            let pause = if scoped.is_some() {
                // One play's pause sound goes through occurrence isolation,
                // which edits authored Holds only.
                if !matches!(beat.kind, NodeKind::Hold { .. }) {
                    return Err("Inside one Repeat play, select an ordinary pause. Split fragments and Repeat gaps take room tone from Your edit, outside Repeat contents.".into());
                }
                deadpan_core::PauseSoundTarget::Hold {
                    node: node.clone(),
                    fragment: false,
                }
            } else {
                workspace.document.pause_sound_target(node).ok_or_else(|| {
                    if let NodeKind::Repeat { iterations, .. } = &beat.kind {
                        format!("This Repeat has no gap between plays. Add one first, for example :repeat {} gap=500ms.", iterations.len())
                    } else {
                        "Select a pause, a split pause fragment or a Repeat with a gap between plays.".to_owned()
                    }
                })?
            };
            let recipe = pause
                .recipe(&workspace.document)
                .ok_or("The pause is unavailable.")?;
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
                pause,
                scoped,
                play_gap: None,
            })
        })();
        let copied = self.copied.copied_audio_selection();
        let selection = (|| {
            let target = target.as_ref().ok()?;
            let workspace = self.workspace.as_ref()?;
            let recipe = target.pause.recipe(&workspace.document)?;
            let (HoldAudio::RoomTone { source } | HoldAudio::Custom { source, .. }) = &recipe.audio
            else {
                return None;
            };
            let registered = workspace.sources.get(&source.asset)?;
            Some(RoomToneSelection::Exact {
                source: source.clone(),
                qualification: registered.receipt.id().clone(),
            })
        })()
        .or_else(|| copied.clone());
        let custom = target.as_ref().ok().and_then(|target| {
            let recipe = target.pause.recipe(&self.workspace.as_ref()?.document)?;
            match recipe.audio {
                HoldAudio::Custom {
                    looped,
                    fade_in_ms,
                    fade_out_ms,
                    ..
                } => Some(CustomOptions {
                    looped,
                    fade_in_ms,
                    fade_out_ms,
                }),
                _ => None,
            }
        });
        CommandTarget {
            target,
            selection,
            copied,
            custom,
        }
    }

    fn check_hold_target(&self, target: &Target) -> Result<(), String> {
        if self.workspace.as_ref().is_none_or(|workspace| {
            workspace.session != target.session
                || workspace.document.revision_id() != &target.revision
        }) || self.sequence_scope != target.scope
            || !self.inspected_target_matches(&target.node, target.scoped.as_ref())
            || i64::try_from(self.sequence_cursor).ok() != Some(target.cursor.0)
            || self.view != View::Sequence
        {
            return Err("The pause, cursor or project changed. Open its audio controls again; no edit was made.".into());
        }
        Ok(())
    }

    /// Commands and inspector buttons share the same recording/pending guard.
    /// Command entry alone permits navigation away from a pending scoped edit;
    /// opening or applying an audio draft must still wait for that edit.
    pub(super) fn hold_audio_entry_allowed(&mut self) -> bool {
        let recording = self.macros.recording();
        if !self.macro_action_allowed(Action::Command) {
            return false;
        }
        if self.refuse_duration_recording_draft() {
            return false;
        }
        if self.macros.is_pending() {
            self.error = Some("Wait for the pending macro action to finish.".into());
            return false;
        }
        if recording && !self.macros.recording() {
            self.error = Some("Recording stopped because its context changed. Start the command again; no action was performed.".into());
            return false;
        }
        true
    }

    fn uses_semantic_hold_audio(&self, target: &Target) -> bool {
        self.macros.recording()
            || target.scoped.as_ref().is_some_and(|scoped| {
                scoped.has_single_play_authority()
                    && self.edit_selection() == navigation::EditSelection::None
                    && !matches!(self.pane, Pane::Sources | Pane::Sounds)
                    && !self.sound_focused()
                    && !self.event_focused()
            })
    }

    fn hold_audio_capture(&self, target: &Target) -> Result<Option<macros::Capture>, String> {
        self.check_hold_target(target)?;
        // Existing All/mixed, Visual and unfocused nonrecording contexts
        // retain their direct captured-target workflow.
        if !self.uses_semantic_hold_audio(target) {
            return Ok(None);
        }
        let captured = self.capture_macro_target()?;
        if let Some(scoped) = &target.scoped {
            if !captured.captures_scope(scoped) {
                return Err(
                    "The pause's captured play changed; open its audio controls again.".into(),
                );
            }
        } else {
            self.recorded_context(
                target.session,
                &target.revision,
                &target.node,
                target.cursor,
            )?;
        }
        Ok(Some(captured))
    }

    pub(super) fn open_room_tone(
        &mut self,
        captured: Option<CommandTarget>,
        context: &egui::Context,
    ) {
        self.open_pause_sound(captured, context, false);
    }

    /// Open the pause sound sheet: room tone, or a custom sound when `custom`
    /// is asked for or the pause already has one.
    pub(super) fn open_pause_sound(
        &mut self,
        captured: Option<CommandTarget>,
        context: &egui::Context,
        custom: bool,
    ) {
        if !self.hold_audio_entry_allowed() {
            return;
        }
        let sounds = self.catalog_sounds();
        let result = (|| {
            let captured = captured.ok_or("No pause was captured when command entry opened.")?;
            let target = captured.target?;
            self.hold_audio_capture(&target)?;
            let options = captured.custom;
            let custom = custom || options.is_some();
            let selection = match captured.selection {
                Some(selection) => Some(selection),
                // A custom sound can start from a catalog sound instead.
                None if custom && !sounds.is_empty() => None,
                None => {
                    return Err(format!(
                        "Copy a quiet Original range first: :source, {}. Return to Your edit and select a pause, then :room-tone.{}",
                        self.editor_copy_recipe(),
                        if custom {
                            " Or add a sound to the catalog."
                        } else {
                            ""
                        }
                    ));
                }
            };
            Ok::<_, String>((
                target,
                selection,
                captured.copied,
                custom,
                options.unwrap_or_default(),
            ))
        })();
        let (target, selection, copied, custom, options) = match result {
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
            waveform: RangeWaveform::default(),
            custom,
            looped: options.looped,
            fade_in: options.fade_in_ms.to_string(),
            fade_out: options.fade_out_ms.to_string(),
            sounds,
        });
        if let Some(selection) = selection {
            self.prepare_room_tone(selection);
        }
        context.request_repaint();
    }

    /// Audio-only catalog sounds with their complete measured audio.
    fn catalog_sounds(&self) -> Vec<CatalogSound> {
        let Some(workspace) = &self.workspace else {
            return Vec::new();
        };
        workspace
            .sources
            .iter()
            .filter_map(|(asset, registered)| {
                let (label, source) = deadpan_cli::sound_events::catalog_source(
                    &workspace.document,
                    &registered.receipt,
                    asset,
                )
                .ok()?;
                Some(CatalogSound {
                    label,
                    source,
                    qualification: registered.receipt.id().clone(),
                })
            })
            .collect()
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
            self.request_room_tone_waveform();
        } else if let Some(failure) = failure {
            draft.receive_failure(failure);
        }
    }

    /// Measure the prepared range off the UI, on the playback engine's idle
    /// analysis lane, exactly as its source audition plays it.
    fn request_room_tone_waveform(&mut self) {
        self.cancel_room_tone_waveform();
        let Some(snapshot) = self
            .workspace
            .as_ref()
            .map(|workspace| Arc::new(workspace.playback_snapshot()))
        else {
            return;
        };
        let Some(draft) = self.room_tone.as_mut() else {
            return;
        };
        let Some(prepared) = &draft.prepared else {
            return;
        };
        match self.playback.request_range_waveform(
            snapshot,
            prepared.audition.clone(),
            deadpan_audio::WaveformLimits::default(),
        ) {
            Ok(ticket) => draft.waveform.ticket = Some(ticket),
            Err(error) => draft.waveform.error = Some(error.to_string()),
        }
    }

    fn cancel_room_tone_waveform(&mut self) {
        if let Some(draft) = self.room_tone.as_mut() {
            if let Some(ticket) = draft.waveform.ticket.take() {
                self.playback.cancel_waveform(ticket);
            }
            draft.waveform = RangeWaveform::default();
        }
    }

    /// The sheet is modal, so no other Edit waveform consumer is open.
    fn receive_room_tone_waveform(&mut self) {
        let Some(ticket) = self
            .room_tone
            .as_ref()
            .and_then(|draft| draft.waveform.ticket)
        else {
            return;
        };
        let Some(update) = self.playback.poll_edit_waveform() else {
            return;
        };
        let Some(draft) = self.room_tone.as_mut() else {
            return;
        };
        if update.ticket != ticket {
            return;
        }
        if update.waveform.is_some()
            || update.status == deadpan_playback::WaveformStatus::Unavailable
        {
            draft.waveform.data = update.waveform;
        }
        draft.waveform.status = Some(update.status);
        draft.waveform.error = update.error;
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
        self.cancel_room_tone_waveform();
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
        if !self.hold_audio_entry_allowed() {
            return;
        }
        let result = (|| {
            let target = captured
                .ok_or("No pause was captured when command entry opened.")?
                .target?;
            let capture = self.hold_audio_capture(&target)?;
            Ok::<_, String>((target, capture))
        })();
        match result {
            Ok((_, Some(capture))) => self.apply_recorded_instruction(
                Ok(capture),
                Ok(deadpan_core::SemanticInstruction::SetHoldAudio {
                    audio: HoldAudio::Silence,
                }),
            ),
            Ok((target, None)) => {
                self.submit(hold_request(target, HoldAudio::Silence));
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
            [IN_ID, OUT_ID, FADE_IN_ID, FADE_OUT_ID]
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
        self.receive_room_tone_waveform();
        let Some(mut draft) = self.room_tone.take() else {
            return;
        };
        let mut action = draft.key.take();
        if matches!(action, Some(SheetAction::Cancel)) {
            self.close_room_tone(context);
            return;
        }
        let mut changed = false;
        let mut options_changed = false;
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
                let title = draft.target.heading_for(draft.custom);
                super::accessibility::dialog(ui, &title);
                ui.set_width(width);
                let heading = ui.horizontal(|ui| {
                    ui.heading(&title);
                    ui.colored_label(style::LAVENDER, "DRAFT");
                }).response;
                let focus = ui.interact(heading.rect, egui::Id::new(FOCUS_ID), egui::Sense::focusable_noninteractive());
                if std::mem::take(&mut draft.focus_pending) { focus.request_focus(); }
                ui.label(format!("{} · {} f · picture and duration stay intact", draft.target.label, draft.target.frames));
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Sound");
                    options_changed |= ui.selectable_value(&mut draft.custom, false, "Room tone").changed();
                    options_changed |= ui.selectable_value(&mut draft.custom, true, "Custom sound").changed();
                });
                egui::ScrollArea::vertical().id_salt("room-tone-details").max_height((context.content_rect().height() - 260.0).max(160.0)).show(ui, |ui| {
                    ui.label(egui::RichText::new(source_label.as_deref().unwrap_or(if draft.custom { "Choose a sound" } else { "Original range" })).strong());
                    if draft.custom {
                        ui.weak("Any catalog sound or Original range plays from the pause's start at its natural speed. It never runs past the pause.");
                    } else {
                        ui.weak("Listen for quiet words. Choose non-speech material; nothing is detected or normalized automatically.");
                    }
                    if ui.add_enabled(!busy && draft.copied.is_some(), egui::Button::new("Use copied Original range")).clicked() {
                        action = Some(SheetAction::UseCopied);
                    }
                    if draft.custom {
                        for (index, sound) in draft.sounds.iter().enumerate() {
                            if ui.add_enabled(!busy, egui::Button::new(format!("Use sound: {}", sound.label))).clicked() {
                                action = Some(SheetAction::UseSound(index));
                            }
                        }
                        if draft.sounds.is_empty() {
                            ui.weak("No catalog sounds yet. Add one from Sources, or use a copied Original range.");
                        }
                        options_changed |= ui.checkbox(&mut draft.looped, "Loop until the pause ends (crossfaded like room tone)").changed();
                        ui.columns(2, |columns| {
                            let label = columns[0].label("Fade in (ms)");
                            options_changed |= columns[0].add(egui::TextEdit::singleline(&mut draft.fade_in).id(egui::Id::new(FADE_IN_ID)).return_key(None).desired_width(f32::INFINITY)).labelled_by(label.id).changed();
                            let label = columns[1].label("Fade out (ms)");
                            options_changed |= columns[1].add(egui::TextEdit::singleline(&mut draft.fade_out).id(egui::Id::new(FADE_OUT_ID)).return_key(None).desired_width(f32::INFINITY)).labelled_by(label.id).changed();
                        });
                        ui.weak(if draft.looped { "Fades start at the pause's start and end at its end." } else { "Fades start at the pause's start and end where the sound ends, or at the pause's end if that comes first. Silence follows." });
                    }
                    if let Some((source, _)) = &draft.basis {
                        let rate = source.span.start().time_base.denominator();
                        ui.label(format!("Source samples · {rate} Hz · Out excluded"));
                        ui.columns(2, |columns| {
                            let label = columns[0].label(if draft.custom { "Sound In sample" } else { "Room tone In sample" });
                            changed |= columns[0].add(egui::TextEdit::singleline(&mut draft.start).id(egui::Id::new(IN_ID)).return_key(None).desired_width(f32::INFINITY)).labelled_by(label.id).changed();
                            let label = columns[1].label(if draft.custom { "Sound Out sample" } else { "Room tone Out sample" });
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
                            Some(deadpan_playback::Phase::Preparing) => "Cancel source",
                            Some(_) => "Pause source",
                            None => "Play source",
                        };
                        if ui.add_enabled(ready && !changed && !busy, style::action(play_label, "Space")).clicked() { action = Some(SheetAction::Play); }
                        if ui.add_enabled(ready && !changed && !busy, style::action("Loop source", "Shift+Space")).clicked() { action = Some(SheetAction::Loop); }
                    });
                    let total = draft.prepared.as_ref().map_or(0, |p| p.audition.duration_samples().0.max(0) as u64);
                    let status = self.transport.as_ref().map_or("Stopped", |run| if run.phase == deadpan_playback::Phase::Preparing { "Preparing" } else { "Playing" });
                    ui.monospace(format!("Source preview · {status} · {} / {}", playback::sound_time(draft.cursor), playback::sound_time(total)));
                    if draft.prepared.is_some() && !changed {
                        range_waveform(ui, &draft.waveform, draft.cursor, total);
                    }
                    ui.weak(if draft.custom { "Source preview plays the range without fades. After Apply, audition the pause to hear it with its fades." } else { "Source preview has no lead-in or follow-through. After Apply, audition the pause to hear its crossfaded loop." });
                });
                ui.separator();
                ui.weak(draft.target.scope_note());
                ui.horizontal_wrapped(|ui| {
                    let apply = if draft.custom { "Apply custom sound" } else { "Apply room tone" };
                    if ui.add_enabled(ready && !changed && !busy, style::action(apply, crate::navigation::registry::mode_label("room-tone.apply")).fill(style::SELECTED)).clicked() { action = Some(SheetAction::Apply); }
                    if ui.add(style::action("Cancel", crate::navigation::registry::mode_label("room-tone.cancel"))).clicked() { action = Some(SheetAction::Cancel); }
                });
                ui.weak("Tab / Shift+Tab moves through controls. Escape discards the draft.");
            });
        if options_changed && !changed {
            // Choosing the kind, loop or fades keeps the prepared range.
            draft.error = None;
        }
        if changed {
            if let Some(ticket) = draft.waveform.ticket.take() {
                self.playback.cancel_waveform(ticket);
            }
            draft.waveform = RangeWaveform::default();
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
            let refusal = "The pause sound did not change because another project command is still in progress. Try again when it finishes.";
            if let Some(draft) = &mut self.room_tone {
                draft.error = Some(refusal.into());
            }
            return;
        }
        if matches!(action, SheetAction::UseCopied) {
            if let Some(selection) = draft.copied.clone() {
                self.prepare_room_tone(selection);
            }
            return;
        }
        if let SheetAction::UseSound(index) = action {
            if let Some(sound) = draft.sounds.get(index).cloned() {
                self.prepare_room_tone(RoomToneSelection::Exact {
                    source: sound.source,
                    qualification: sound.qualification,
                });
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
                let audio = match draft.audio() {
                    Ok(audio) => audio,
                    Err(error) => {
                        if let Some(draft) = &mut self.room_tone {
                            draft.error = Some(error);
                        }
                        return;
                    }
                };
                if !self.hold_audio_entry_allowed() {
                    return;
                }
                // Record only the exact prepared span. Closing the modal must
                // restore the same captured Hold before capture and dispatch:
                // ordinary capture correctly refuses an open preview modal.
                if self.uses_semantic_hold_audio(&target) {
                    self.close_room_tone(context);
                    let capture = self.hold_audio_capture(&target).and_then(|capture| {
                        capture.ok_or_else(|| {
                            "The pause's captured play changed; open its audio controls again."
                                .to_owned()
                        })
                    });
                    self.apply_recorded_instruction(
                        capture,
                        Ok(deadpan_core::SemanticInstruction::SetHoldAudio { audio }),
                    );
                    return;
                }
                if self.submit(hold_request(target, audio)) {
                    self.close_room_tone(context);
                }
            }
            SheetAction::Play | SheetAction::Loop => {
                self.preview_room_tone(matches!(action, SheetAction::Loop))
            }
            SheetAction::Prepare
            | SheetAction::UseCopied
            | SheetAction::UseSound(_)
            | SheetAction::Cancel => {}
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
        let pause = self.inspected_pause_sound();
        let gap = matches!(
            pause,
            Some(deadpan_core::PauseSoundTarget::RepeatGap { .. })
        );
        ui.label(style::section_title(
            if gap { "GAP SOUND" } else { "PAUSE SOUND" },
            false,
        ));
        if self.scoped_shared_gap().is_some() {
            ui.weak("Shared with the other plays. A change gives this play its own copy.");
        } else if gap {
            ui.weak("Default gaps between plays. A gap with its own pause keeps its sound.");
        }
        if let Some(workspace) = &self.workspace
            && let Some(recipe) = pause
                .as_ref()
                .and_then(|pause| pause.recipe(&workspace.document))
            && let HoldAudio::Custom {
                source,
                looped,
                fade_in_ms,
                fade_out_ms,
            } = &recipe.audio
        {
            let label = workspace
                .sources
                .get(&source.asset)
                .map_or(source.asset.as_str(), |s| s.label.as_str());
            ui.colored_label(style::LAVENDER, "Custom sound");
            ui.label(label);
            ui.small(format!(
                "{} · fade in {fade_in_ms} ms · fade out {fade_out_ms} ms",
                if *looped {
                    "Loops to the end"
                } else {
                    "Plays once"
                }
            ));
        }
        if let Some(workspace) = &self.workspace
            && let Some(recipe) = pause
                .as_ref()
                .and_then(|pause| pause.recipe(&workspace.document))
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
            ui.weak(if gap {
                "Each gap loops from its own start; crossfades stay inside it."
            } else {
                "Crossfades stay inside this pause."
            });
        }
        if ui
            .add_enabled(ready, style::row_action(ui, "Room tone…", ":room-tone"))
            .clicked()
        {
            self.open_room_tone(Some(self.capture_hold_command()), ui.ctx());
        }
        if ui
            .add_enabled(ready, style::row_action(ui, "Custom sound…", ":room-tone"))
            .clicked()
        {
            self.open_pause_sound(Some(self.capture_hold_command()), ui.ctx(), true);
        }
        if ui
            .add_enabled(ready, style::row_action(ui, "Use silence", ":hold-silence"))
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

impl DeadpanApp {
    /// The pause sound the inspected beat names. Inside one Repeat play only
    /// an ordinary Hold qualifies.
    pub(super) fn inspected_pause_sound(&self) -> Option<deadpan_core::PauseSoundTarget> {
        let workspace = self.workspace.as_ref()?;
        let node = self.inspected_node()?;
        if let Some((_, _, repeat)) = self.scoped_shared_gap() {
            return Some(deadpan_core::PauseSoundTarget::RepeatGap { node: repeat });
        }
        if self.scoped.is_some() {
            return matches!(
                workspace.document.nodes().get(node)?.kind,
                NodeKind::Hold { .. }
            )
            .then(|| deadpan_core::PauseSoundTarget::Hold {
                node: node.clone(),
                fragment: false,
            });
        }
        workspace.document.pause_sound_target(node)
    }
}

/// Whole milliseconds, optionally ending in `ms`; empty means no fade.
fn parse_fade(value: &str, name: &str) -> Result<u32, String> {
    let value = value.trim();
    let digits = value.strip_suffix("ms").unwrap_or(value).trim();
    if digits.is_empty() {
        return Ok(0);
    }
    digits
        .parse::<u32>()
        .ok()
        .filter(|ms| *ms <= deadpan_core::MAX_CUSTOM_FADE_MS)
        .ok_or_else(|| format!("{name} is whole milliseconds from 0 to 10000."))
}

fn hold_request(target: Target, audio: HoldAudio) -> ProjectRequest {
    let edit = target.edit(audio);
    ProjectRequest::Edit {
        expected_session: target.session,
        expected_revision: target.revision,
        cursor: target.cursor,
        scope: target.scope,
        edit,
    }
}

/// The prepared range's stereo peaks with the source preview position: what
/// the user is choosing, so quiet words show before Apply.
fn range_waveform(ui: &mut egui::Ui, waveform: &RangeWaveform, cursor: u64, total: u64) {
    use deadpan_playback::WaveformStatus;
    const LABEL: &str = "Room tone range waveform · stereo · before limiter";
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 64.0), egui::Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, LABEL));
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(28, 31, 38));
    if let Some(data) = &waveform.data {
        super::timing::paint_peaks(&painter, rect, data, |sample| {
            rect.left() + rect.width() * (sample as f32 / total.max(1) as f32).clamp(0.0, 1.0)
        });
    }
    let x = |sample: i64| {
        rect.left() + rect.width() * (sample as f32 / total.max(1) as f32).clamp(0.0, 1.0)
    };
    let state = match (waveform.status, &waveform.error) {
        (_, Some(error)) => Some(error.as_str()),
        (Some(WaveformStatus::Complete), None) => None,
        (Some(WaveformStatus::Measuring), None) => Some("Measuring…"),
        _ => Some("Waiting for idle audio"),
    };
    if let Some(state) = state {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            state,
            egui::FontId::proportional(11.0),
            egui::Color32::from_gray(177),
        );
    }
    let at = x(i64::try_from(cursor).unwrap_or(i64::MAX));
    painter.line_segment(
        [egui::pos2(at, rect.top()), egui::pos2(at, rect.bottom())],
        egui::Stroke::new(1.5, style::SAVED),
    );
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
                pause: deadpan_core::PauseSoundTarget::Hold {
                    node: NodeId::new("pause").unwrap(),
                    fragment: false,
                },
                scoped: None,
                play_gap: None,
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
            waveform: RangeWaveform::default(),
            custom: false,
            looped: false,
            fade_in: String::new(),
            fade_out: String::new(),
            sounds: Vec::new(),
        }
    }

    #[test]
    fn custom_fades_are_whole_milliseconds_within_ten_seconds() {
        assert_eq!(parse_fade("", "Fade in"), Ok(0));
        assert_eq!(parse_fade(" 25ms ", "Fade in"), Ok(25));
        assert_eq!(parse_fade("10000", "Fade in"), Ok(10_000));
        for invalid in ["10001", "-1", "2.5", "1s"] {
            assert!(parse_fade(invalid, "Fade out").is_err(), "{invalid}");
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
