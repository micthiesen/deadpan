//! Native semantic recording. Only completed commands enter a recorded body.

use std::num::NonZeroU32;

use deadpan_core::{RegisterName, SemanticContext, SemanticInstruction, SemanticProgram};

use super::*;
use crate::project::{macros as protocol, slice};

#[derive(Clone)]
pub(super) struct Capture {
    base: Arc<Workspace>,
    scope: SequenceScope,
    context: SemanticContext,
    bank_version: u64,
    pane: Pane,
}

impl Capture {
    fn matches(&self, app: &DeadpanApp) -> bool {
        app.workspace.as_ref().is_some_and(|workspace| {
            workspace.session == self.base.session
                && workspace.document.project_id() == self.base.document.project_id()
                && workspace.document.revision_id() == self.base.document.revision_id()
        }) && app.sequence_scope == self.scope
            && i64::try_from(app.sequence_cursor).ok() == Some(self.context.cursor.0)
            && app.copied.bank_version() == Some(self.bank_version)
            && app.pane == self.pane
            && app.view == View::Sequence
            && app.edit_selection() == navigation::EditSelection::None
            && !app.sound_focused()
            && !app.event_focused()
            && app.frame_delete_blocked().is_none()
    }

    fn id(&self, request: u64) -> protocol::Id {
        protocol::Id {
            session: self.base.session,
            project: self.base.document.project_id().clone(),
            revision: self.base.document.revision_id().clone(),
            bank_version: self.bank_version,
            request,
        }
    }
}

struct Recording {
    name: char,
    expected: Capture,
    instructions: Vec<SemanticInstruction>,
    cut: Option<(slice::CopyId, SemanticInstruction)>,
}

struct Pending {
    operation: protocol::Operation,
    capture: Capture,
    instruction: Option<SemanticInstruction>,
    owns_cursor: bool,
}

#[derive(Default)]
pub(super) struct State {
    recording: Option<Recording>,
    pending: Option<Pending>,
}

impl State {
    pub fn recording(&self) -> bool {
        self.recording.is_some()
    }

    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
            || self
                .recording
                .as_ref()
                .is_some_and(|recording| recording.cut.is_some())
    }

    pub fn recording_name(&self) -> Option<char> {
        self.recording.as_ref().map(|recording| recording.name)
    }

    pub fn instruction_count(&self) -> usize {
        self.recording
            .as_ref()
            .map_or(0, |recording| recording.instructions.len())
    }

    pub fn label(&self) -> Option<String> {
        self.recording_name().map(|name| {
            format!(
                "RECORDING @{name} · {} instructions",
                self.instruction_count()
            )
        })
    }
}

impl DeadpanApp {
    pub(super) fn capture_macro_target(&self) -> Result<Capture, String> {
        if let Some(error) = self.frame_delete_blocked() {
            return Err(error.into());
        }
        if self.view != View::Sequence
            || matches!(self.pane, Pane::Sources | Pane::Sounds)
            || self.sound_focused()
            || self.event_focused()
            || self.edit_selection() != navigation::EditSelection::None
        {
            return Err(
                "Focus Your edit and clear the Visual selection before using a macro.".into(),
            );
        }
        let base = self
            .workspace
            .clone()
            .ok_or("Open a project before using a macro.")?;
        let scope = self.sequence_scope.clone();
        let owner = scope.resolve(&base)?;
        if !(owner.start..=owner.end).contains(&self.sequence_cursor) {
            return Err("The Edit cursor is outside this group.".into());
        }
        let context = SemanticContext {
            parent: owner.owner.clone(),
            cursor: ProjectFrame(
                i64::try_from(self.sequence_cursor).map_err(|error| error.to_string())?,
            ),
        };
        Ok(Capture {
            base,
            scope,
            context,
            bank_version: self
                .copied
                .bank_version()
                .ok_or("The project registers are still loading.")?,
            pane: self.pane,
        })
    }

    pub(super) fn reconcile_macro_recording(&mut self) {
        let moved = self
            .macros
            .pending
            .as_ref()
            .is_some_and(|pending| !pending.capture.matches(self));
        if moved && let Some(pending) = &mut self.macros.pending {
            pending.owns_cursor = false;
        }
        if self
            .macros
            .recording
            .as_ref()
            .is_some_and(|recording| !recording.expected.matches(self))
        {
            self.macros.recording = None;
            self.bindings.set_macro_recording(false);
            self.message = Some("Macro recording cancelled because its project, registers or editing context changed. The previous macro is unchanged.".into());
        }
    }

    pub(super) fn cancel_macro_recording(&mut self) {
        if self
            .macros
            .pending
            .as_ref()
            .is_some_and(|pending| matches!(pending.operation, protocol::Operation::Save { .. }))
        {
            self.error = Some("The macro save is already queued. Wait for its result.".into());
            return;
        }
        if self.macros.recording.take().is_some() {
            self.message = Some("Macro recording cancelled. The previous macro is unchanged; completed edits remain undoable.".into());
        }
        self.bindings.set_macro_recording(false);
    }

    pub(super) fn macro_action_allowed(&mut self, action: Action) -> bool {
        self.reconcile_macro_recording();
        if self.macros.is_pending()
            && !matches!(
                action,
                Action::Invalid(_) | Action::OfferInsert | Action::MacroCancel | Action::Escape
            )
        {
            self.error = Some("Wait for the pending macro action to finish.".into());
            return false;
        }
        if !self.macros.recording() {
            return true;
        }
        if !matches!(
            action,
            Action::Step { .. }
                | Action::DeleteFrames(_)
                | Action::RepeatLast
                | Action::MacroExecute { .. }
                | Action::MacroStop
                | Action::MacroCancel
                | Action::SelectRegister(_)
                | Action::Command
                | Action::Escape
                | Action::Invalid(_)
                | Action::OfferInsert
        ) {
            self.error = Some("This action cannot be recorded yet. Macros support frame motions, frame cuts and named macro calls. Save or cancel recording first.".into());
            return false;
        }
        if matches!(
            action,
            Action::Step { .. }
                | Action::DeleteFrames(_)
                | Action::RepeatLast
                | Action::MacroExecute { .. }
        ) && self.macros.instruction_count() >= deadpan_core::MAX_SEMANTIC_PROGRAM_INSTRUCTIONS
        {
            self.error = Some(
                "The macro has reached 1024 instructions. Save or cancel recording first.".into(),
            );
            return false;
        }
        true
    }

    pub(super) fn macro_command_allowed(
        &mut self,
        command: &Result<navigation::command::Entry, String>,
    ) -> bool {
        match command {
            Ok(navigation::command::Entry::Action(action)) => self.macro_action_allowed(*action),
            Ok(_) if self.macros.recording() || self.macros.is_pending() => {
                self.error =
                    Some("This command cannot be recorded. Save or cancel the macro first.".into());
                false
            }
            _ => true,
        }
    }

    pub(super) fn macro_request_allowed(&mut self, request: &ProjectRequest) -> bool {
        if !self.macros.recording() && !self.macros.is_pending() {
            return true;
        }
        if !self.macros.is_pending()
            && matches!(
                request,
                ProjectRequest::CutFrames { .. } | ProjectRequest::Macro(_)
            )
        {
            return true;
        }
        self.error = Some("Finish the macro action, then save or cancel recording before another project command.".into());
        false
    }

    pub(super) fn macro_action(&mut self, action: Action, target: Option<Result<Capture, String>>) {
        if action == Action::MacroCancel {
            self.cancel_macro_recording();
            return;
        }
        if action == Action::MacroStop {
            self.save_recording();
            return;
        }
        let result = (|| {
            if self.macros.is_pending() || self.service.is_busy() || self.copied.is_pending() {
                return Err("Wait for the pending project action before using a macro.".into());
            }
            let captured =
                target.ok_or("Start the macro binding again to capture its context.")??;
            if !captured.matches(self) {
                return Err("The captured macro context changed. Start the command again; no macro was run.".into());
            }
            match action {
                Action::MacroRecord(name) => {
                    RegisterName::new(name).map_err(|error| error.to_string())?;
                    if self.macros.recording() {
                        return Err("Save or cancel the current recording first.".into());
                    }
                    self.stop_playback();
                    self.macros.recording = Some(Recording {
                        name,
                        expected: captured,
                        instructions: Vec::new(),
                        cut: None,
                    });
                    self.bindings.set_macro_recording(true);
                    self.message = Some(format!(
                        "Recording @{name}: frame motions, frame cuts and named calls. {} saves; Esc cancels.",
                        self.editor_key(EditorKey::MacroRecord)
                    ));
                }
                Action::MacroExecute { register, count } => {
                    let instruction = SemanticInstruction::Call {
                        register: RegisterName::new(register).map_err(|error| error.to_string())?,
                        count: NonZeroU32::new(count).ok_or("A macro count must be positive.")?,
                    };
                    let Some(serial) = self.next_serial() else {
                        return Ok(());
                    };
                    let operation = protocol::Operation::Run {
                        id: captured.id(serial),
                        register,
                        count,
                        scope: captured.scope.clone(),
                        context: captured.context.clone(),
                    };
                    if self.submit(ProjectRequest::Macro(operation.clone())) {
                        self.macros.pending = Some(Pending {
                            operation,
                            capture: captured,
                            instruction: self.macros.recording().then_some(instruction),
                            owns_cursor: true,
                        });
                        self.message = Some("Running macro…".into());
                    }
                }
                _ => unreachable!("only macro actions enter macro dispatch"),
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.error = Some(error);
        }
    }

    fn save_recording(&mut self) {
        let result = (|| {
            if self.macros.is_pending() || self.service.is_busy() {
                return Err("Wait for the pending action before saving the macro.".into());
            }
            let recording = self
                .macros
                .recording
                .as_ref()
                .ok_or("No macro is being recorded.")?;
            if !recording.expected.matches(self) {
                return Err("The recording context changed. Cancel it and record again.".into());
            }
            if recording.instructions.is_empty() {
                return Err("The recording is empty. Record a frame motion, cut or macro call before saving.".into());
            }
            let program = Arc::new(
                SemanticProgram::new(recording.instructions.clone())
                    .map_err(|error| error.to_string())?,
            );
            let capture = recording.expected.clone();
            let register = recording.name;
            let Some(serial) = self.next_serial() else {
                return Ok(());
            };
            let operation = protocol::Operation::Save {
                id: capture.id(serial),
                register,
                program,
            };
            if self.submit(ProjectRequest::Macro(operation.clone())) {
                self.macros.pending = Some(Pending {
                    operation,
                    capture,
                    instruction: None,
                    owns_cursor: true,
                });
                self.message = Some("Saving macro…".into());
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.error = Some(error);
        }
    }

    pub(super) fn record_macro_motion(&mut self, forward: bool, count: u32) {
        if !self.macros.recording() {
            return;
        }
        let Some(count) = NonZeroU32::new(count) else {
            return;
        };
        self.append_macro_instruction(SemanticInstruction::MoveFrames { forward, count });
    }

    fn append_macro_instruction(&mut self, instruction: SemanticInstruction) {
        let capture = self.capture_macro_target();
        if let Some(recording) = &mut self.macros.recording {
            match capture {
                Ok(capture) => {
                    recording.instructions.push(instruction);
                    recording.expected = capture;
                }
                Err(error) => {
                    self.macros.recording = None;
                    self.error = Some(format!("Recording stopped: {error}"));
                    self.bindings.set_macro_recording(false);
                }
            }
        }
    }

    pub(super) fn record_macro_cut(
        &mut self,
        request: &slice::CaptureRequest,
        attempt: Option<&crate::project::semantic::CutAttempt>,
    ) {
        if let (Some(recording), Some(attempt)) = (&mut self.macros.recording, attempt) {
            let register = request
                .register
                .map_or(Ok(RegisterName::unnamed()), RegisterName::new)
                .expect("captured register was validated");
            recording.cut = Some((
                request.id.clone(),
                SemanticInstruction::CutFrames {
                    operation: attempt.operation.clone(),
                    register,
                },
            ));
        }
    }

    /// Called before replacing the visible workspace. A delayed macro cannot
    /// retarget a cursor which the user has since moved with the pointer.
    pub(super) fn prepare_macro_update(&mut self, update: &mut crate::project::ProjectUpdate) {
        let owns_cursor = self
            .macros
            .pending
            .as_ref()
            .is_some_and(|pending| pending.capture.matches(self));
        if let Some(pending) = &mut self.macros.pending {
            pending.owns_cursor &= owns_cursor;
        }
        let saved = update.saved_macro.as_ref();
        if let Some(receipt) = saved {
            let owned = self.macros.pending.as_ref().is_some_and(|pending| {
                pending.operation.id() == &receipt.id && pending.owns_cursor
            });
            if let Some(commit) = receipt.committed() {
                let visible = update.workspace.as_ref().is_some_and(|workspace| {
                    selection::commit_matches_visible(
                        commit,
                        workspace.session,
                        workspace.document.project_id(),
                        workspace.document.revision_id(),
                    )
                });
                if owned && visible {
                    update.committed = Some(commit.clone());
                } else if update
                    .committed
                    .as_ref()
                    .is_some_and(|current| current.revision == commit.revision)
                {
                    update.committed = None;
                }
            }
            if let protocol::Outcome::Executed {
                refresh_error: Some(error),
                ..
            } = &receipt.outcome
                && update.workspace.as_ref().is_some_and(|workspace| {
                    workspace.session == receipt.id.session
                        && workspace.document.revision_id() == &receipt.id.revision
                })
            {
                update.message = Some(error.clone());
            }
        }
    }

    pub(super) fn receive_macro_cut(
        &mut self,
        update: Option<&slice::CutUpdate>,
        saved: Option<&slice::CutReceipt>,
    ) {
        let Some((id, _)) = self
            .macros
            .recording
            .as_ref()
            .and_then(|recording| recording.cut.as_ref())
        else {
            return;
        };
        let result = saved
            .filter(|receipt| receipt.copied.id() == id)
            .map(Ok)
            .or_else(|| {
                update
                    .filter(|update| &update.request.id == id)
                    .map(|update| update.result.as_ref())
            });
        let Some(result) = result else {
            return;
        };
        let instruction = self
            .macros
            .recording
            .as_mut()
            .and_then(|recording| recording.cut.take())
            .expect("matching recorded cut")
            .1;
        match result {
            Ok(receipt)
                if receipt.refresh_error.is_none()
                    && self.workspace.as_ref().is_some_and(|workspace| {
                        workspace.document.revision_id() == &receipt.committed.revision
                    }) =>
            {
                self.append_macro_instruction(instruction)
            }
            Ok(_) => {
                self.cancel_macro_recording();
                self.message = Some("The cut was saved, but recording stopped because its refreshed edit is unavailable. Reopen the project before continuing. The previous macro is unchanged.".into());
            }
            Err(error) => self.error = Some(format!("Cut was not recorded: {error}")),
        }
    }

    pub(super) fn receive_macro(
        &mut self,
        update: Option<protocol::Update>,
        saved: Option<protocol::Receipt>,
    ) {
        let Some(pending) = self.macros.pending.as_ref() else {
            return;
        };
        let id = pending.operation.id();
        let result = saved
            .filter(|receipt| &receipt.id == id)
            .map(Ok)
            .or_else(|| {
                update
                    .filter(|update| &update.id == id)
                    .map(|update| update.result)
            });
        let Some(result) = result else {
            return;
        };
        let pending = self.macros.pending.take().expect("matched pending macro");
        match result {
            Err(error) => self.error = Some(error),
            Ok(receipt) => match receipt.outcome {
                protocol::Outcome::Saved {
                    register,
                    instructions,
                } => {
                    self.macros.recording = None;
                    self.bindings.set_macro_recording(false);
                    self.message = Some(format!(
                        "Saved Macro @{register} · {instructions} instructions. Use {}{register} to run it.",
                        self.editor_key(EditorKey::MacroExecute)
                    ));
                }
                protocol::Outcome::Executed {
                    register,
                    count,
                    scope,
                    cursor,
                    selected,
                    committed,
                    refresh_error,
                    ..
                } => {
                    let revision = committed
                        .as_ref()
                        .map_or(&receipt.id.revision, |commit| &commit.revision);
                    let visible = self.workspace.as_ref().is_some_and(|workspace| {
                        workspace.session == receipt.id.session
                            && workspace.document.project_id() == &receipt.id.project
                            && workspace.document.revision_id() == revision
                    });
                    if refresh_error.is_some() || !visible {
                        self.cancel_macro_recording();
                        self.message = refresh_error.or_else(|| Some("The macro completed, but its edit is no longer visible. Recording was stopped.".into()));
                        return;
                    }
                    if !pending.owns_cursor {
                        self.cancel_macro_recording();
                        self.message = Some("Macro completed after you moved the cursor. Its final cursor was not applied.".into());
                        return;
                    }
                    if committed.is_none() && pending.capture.matches(self) {
                        self.sequence_scope = scope;
                        self.sequence_cursor =
                            u64::try_from(cursor.0).expect("validated nonnegative macro cursor");
                        self.selected_beat = selected;
                        self.reveal_beat = true;
                        self.request_picture(false);
                    }
                    if let Some(instruction) = pending.instruction {
                        self.append_macro_instruction(instruction);
                    }
                    self.message = Some(format!(
                        "Ran Macro @{register} × {count}{}.",
                        if committed.is_some() {
                            " · one Undo"
                        } else {
                            " · cursor only"
                        }
                    ));
                }
            },
        }
    }
}
