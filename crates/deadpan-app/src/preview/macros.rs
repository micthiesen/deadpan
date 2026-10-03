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
    pub(super) fn group_instruction(
        &self,
        label: Option<String>,
    ) -> Result<SemanticInstruction, String> {
        if self.scope.resolve(&self.base)?.owner != &self.context.parent {
            return Err(
                "The captured Group target differs from its ordinary Sequence scope.".into(),
            );
        }
        groups::instruction(&self.base.document, &self.context, label)
    }

    pub(super) fn repeat_count_instruction(
        &self,
        plays: NonZeroU32,
    ) -> Result<SemanticInstruction, String> {
        if self.scope.resolve(&self.base)?.owner != &self.context.parent {
            return Err(
                "The captured Repeat target differs from its ordinary Sequence scope.".into(),
            );
        }
        repeats::repeat_count_instruction(&self.base.document, &self.context, plays)
    }

    pub(super) fn repeat_selector(&self) -> deadpan_core::SemanticSelector {
        if self.context.visual_selection.is_some() {
            deadpan_core::SemanticSelector::VisualSelection
        } else {
            deadpan_core::SemanticSelector::SelectedBeat
        }
    }

    pub(super) fn repeat_target(&self) -> Result<repeat_queue::Target, String> {
        if self.context.visual_selection.is_some() {
            return Err("Clear the Visual range before changing an existing Repeat count.".into());
        }
        Ok(repeat_queue::Target {
            session: self.base.session,
            revision: self.base.document.revision_id().clone(),
            scope: self.scope.clone(),
            node: self
                .context
                .selected_child
                .clone()
                .ok_or("Select a beat before repeating it.")?,
            cursor: self.context.cursor,
            pane: self.pane,
        })
    }

    pub(super) fn matches(&self, app: &DeadpanApp) -> bool {
        self.matches_without_selection(app) && app.selected_beat == self.context.selected_child
    }

    fn matches_without_selection(&self, app: &DeadpanApp) -> bool {
        app.copied.bank_version() == Some(self.bank_version)
            && self.matches_without_bank_or_selection(app)
    }

    fn matches_without_bank_or_selection(&self, app: &DeadpanApp) -> bool {
        app.workspace.as_ref().is_some_and(|workspace| {
            workspace.session == self.base.session
                && workspace.document.project_id() == self.base.document.project_id()
                && workspace.document.revision_id() == self.base.document.revision_id()
        }) && app.sequence_scope == self.scope
            && i64::try_from(app.sequence_cursor).ok() == Some(self.context.cursor.0)
            && app.pane == self.pane
            && app.view == View::Sequence
            && app.capture_visual_selection().as_ref() == Ok(&self.context.visual_selection)
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
    command_recording: bool,
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

    pub fn capture_command(&mut self) {
        self.command_recording = self.recording();
    }

    pub fn session_changed(&mut self) {
        // Command text can outlive an asynchronous project switch. Retain its
        // recording ownership so submission refuses instead of targeting it anew.
        self.recording = None;
        self.pending = None;
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
        if self.scoped.is_some() {
            return Err(
                "Return to the parent before recording or applying structural edits.".into(),
            );
        }
        if let Some(error) = self.frame_delete_blocked() {
            return Err(error.into());
        }
        if self.view != View::Sequence
            || matches!(self.pane, Pane::Sources | Pane::Sounds)
            || self.sound_focused()
            || self.event_focused()
        {
            return Err("Focus Your edit before using this action.".into());
        }
        let base = self
            .workspace
            .clone()
            .ok_or("Open a project before using this action.")?;
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
            selected_child: self.selected_beat.clone(),
            visual_selection: self.capture_visual_selection()?,
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
                | Action::Beat { .. }
                | Action::First
                | Action::Last
                | Action::VisualMoment
                | Action::DeleteSelection
                | Action::Edit(BeatEdit::Delete)
                | Action::DeleteFrames(_)
                | Action::Operator { .. }
                | Action::Repeat { .. }
                | Action::Group
                | Action::Ungroup
                | Action::Edit(BeatEdit::WrapRepeat(_))
                | Action::Edit(BeatEdit::Repeat(_))
                | Action::CopyMoment
                | Action::PasteMoment { .. }
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
            self.error = Some("This action cannot be recorded yet. Macros support frame and beat motions, group boundaries, Visual selections, cuts, copies, Repeat wraps and count changes, grouping, ungrouping, register pastes and named calls. Save or cancel recording first.".into());
            return false;
        }
        if matches!(
            action,
            Action::Step { .. }
                | Action::Beat { .. }
                | Action::First
                | Action::Last
                | Action::VisualMoment
                | Action::DeleteSelection
                | Action::Edit(BeatEdit::Delete)
                | Action::DeleteFrames(_)
                | Action::Operator { .. }
                | Action::Repeat { .. }
                | Action::Group
                | Action::Ungroup
                | Action::Edit(BeatEdit::WrapRepeat(_))
                | Action::Edit(BeatEdit::Repeat(_))
                | Action::CopyMoment
                | Action::PasteMoment { .. }
                | Action::RepeatLast
                | Action::MacroExecute { .. }
        ) || (action == Action::Escape
            && !self.macros.is_pending()
            && self.edit_selection() != navigation::EditSelection::None)
        {
            if self.macros.instruction_count() < deadpan_core::MAX_SEMANTIC_PROGRAM_INSTRUCTIONS {
                return true;
            }
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
        let was_recording = std::mem::take(&mut self.macros.command_recording);
        self.reconcile_macro_recording();
        if was_recording
            && !self.macros.recording()
            && !matches!(
                command,
                Ok(navigation::command::Entry::Action(Action::MacroCancel))
            )
        {
            self.error = Some("Recording stopped because its context changed. Start the command again; no action was performed.".into());
            return false;
        }
        match command {
            Ok(navigation::command::Entry::Group { .. }) => {
                self.macro_action_allowed(Action::Group)
            }
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
                        "Recording @{name}: motions, Visual selections, cuts, copies, pastes and named calls. {} saves; Esc clears a selection, otherwise cancels.",
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
                return Err("The recording is empty. Record a motion, selection, cut, copy, paste or macro call before saving.".into());
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

    pub(super) fn record_macro_local(&mut self, instruction: SemanticInstruction) {
        if self.macros.recording() {
            self.append_macro_instruction(instruction);
        }
    }

    pub(super) fn record_macro_escape(&mut self) {
        if !self.macros.recording() {
            if let Some(pending) = &mut self.macros.pending {
                pending.owns_cursor = false;
            }
            return;
        }
        if !self.macros.is_pending() && self.edit_selection() != navigation::EditSelection::None {
            self.edit_range.clear();
            self.record_macro_local(SemanticInstruction::ClearSelection);
            self.message =
                Some("Visual selection cleared and recorded. Esc now cancels recording.".into());
        } else {
            self.cancel_macro_recording();
            if let Some(pending) = &mut self.macros.pending {
                pending.owns_cursor = false;
            }
        }
    }

    /// Returns true when the recorder owns this copy, including a refused or
    /// still-pending request. Ordinary copies keep their existing route.
    pub(super) fn record_macro_yank(
        &mut self,
        destination: Option<char>,
        target: Option<Result<Capture, String>>,
    ) -> bool {
        if !self.macros.recording() && !self.macros.is_pending() {
            return false;
        }
        if self.macros.is_pending() {
            self.error = Some("Wait for the pending macro action to finish.".into());
            return true;
        }
        let target = target.unwrap_or_else(|| self.capture_macro_target());
        self.copied.begin_write();
        let instruction = target.as_ref().map_err(Clone::clone).and_then(|target| {
            let register = register_name(destination)?;
            Ok(if target.context.visual_selection.is_some() {
                SemanticInstruction::YankSelection { register }
            } else {
                SemanticInstruction::YankBeat { register }
            })
        });
        self.apply_recorded_instruction(target, instruction);
        true
    }

    pub(super) fn record_macro_delete(
        &mut self,
        destination: Option<char>,
        target: Option<Result<Capture, String>>,
    ) -> bool {
        if !self.macros.recording() && !self.macros.is_pending() {
            return false;
        }
        let target = target.unwrap_or_else(|| self.capture_macro_target());
        self.copied.begin_write();
        let instruction = target.as_ref().map_err(Clone::clone).and_then(|target| {
            let register = register_name(destination)?;
            Ok(if target.context.visual_selection.is_some() {
                SemanticInstruction::CutSelection { register }
            } else {
                SemanticInstruction::Cut {
                    selector: deadpan_core::SemanticSelector::SelectedBeat,
                    register,
                }
            })
        });
        self.apply_recorded_instruction(target, instruction);
        true
    }

    pub(super) fn record_macro_paste(
        &mut self,
        before: bool,
        target: &Result<moment::PlacementTarget, String>,
    ) -> bool {
        if !self.macros.recording() && !self.macros.is_pending() {
            return false;
        }
        if self.macros.is_pending() {
            self.error = Some("Wait for the pending macro action to finish.".into());
            return true;
        }
        self.copied.clear_selection();
        let captured = target
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|target| target.macro_capture.clone());
        let instruction = target.as_ref().map_err(Clone::clone).and_then(|target| {
            target.check_nonempty_selection()?;
            match &target.copied {
                None => {
                    return Err(target.register.map_or_else(
                        || "Copy a beat or Original range before recording a paste.".into(),
                        |name| format!("Register {name} is empty. Copy or cut into it first; no edit was made."),
                    ));
                }
                Some(copied::Content::Macro(_)) => return Err(copied::MACRO_PASTE_ERROR.into()),
                Some(content) => content.check(&target.base)?,
            }
            let register = register_name(target.register)?;
            Ok(if target.selection.has_bounds() {
                SemanticInstruction::ReplaceSelection { register }
            } else {
                SemanticInstruction::Paste { register, before }
            })
        });
        self.apply_recorded_instruction(captured, instruction);
        true
    }

    pub(super) fn apply_recorded_instruction(
        &mut self,
        target: Result<Capture, String>,
        instruction: Result<SemanticInstruction, String>,
    ) {
        self.apply_semantic_instruction(target, instruction, None);
    }

    pub(super) fn repeat_last_edit(&mut self) {
        let uses_register = self.last_edit_uses_register();
        let invocation = (|| {
            let captured = self.capture_macro_target()?;
            let snapshot = self
                .semantic
                .snapshot()
                .ok_or("Make a picture cut or Repeat before repeating an edit.")?;
            let edit = snapshot.edit_for(&captured.base)?;
            if matches!(
                edit.operation,
                crate::project::semantic::RepeatableEdit::Ungroup
            ) && captured.context.visual_selection.is_some()
            {
                return Err("Clear the Visual range before repeating Ungroup.".into());
            }
            if matches!(
                edit.operation,
                crate::project::semantic::RepeatableEdit::SetRepeatPlays { .. }
            ) && captured.context.visual_selection.is_some()
            {
                return Err(
                    "Clear the Visual range before repeating a Repeat count change.".into(),
                );
            }
            match &captured.context.visual_selection {
                Some(selection) if selection.anchor == selection.head => {
                    return Err(if edit.uses_register() {
                        "The Edit selection is empty. Move a boundary before repeating the cut."
                    } else {
                        "The Edit selection is empty. Move a boundary before repeating the edit."
                    }
                    .into());
                }
                None if matches!(
                    edit.operation,
                    crate::project::semantic::RepeatableEdit::Cut(
                        crate::project::semantic::RepeatableCut::Selector(
                            deadpan_core::SemanticSelector::VisualSelection
                        )
                    ) | crate::project::semantic::RepeatableEdit::Repeat {
                        selector: deadpan_core::SemanticSelector::VisualSelection,
                        ..
                    } | crate::project::semantic::RepeatableEdit::Group {
                        selector: deadpan_core::SemanticSelector::VisualSelection,
                        ..
                    }
                ) =>
                {
                    return Err(if edit.uses_register() {
                        "Select a new Visual range before repeating this cut."
                    } else {
                        "Select a new Visual range before repeating this edit."
                    }
                    .into());
                }
                _ => {}
            }
            let register = register_name(self.copied.selected_override().unwrap_or(edit.register))?;
            let instruction = edit.instruction(&captured.context, register);
            Ok((captured, instruction, snapshot.version))
        })();
        self.cancel_repeats("semantic repetition was requested");
        self.bindings.clear();
        // The override belongs to this attempt, including refusals. A saved
        // result still arrives through the immutable request and durable bank.
        if uses_register {
            self.copied.begin_write();
        }
        match invocation {
            Ok((capture, instruction, version)) => {
                self.apply_semantic_instruction(Ok(capture), Ok(instruction), Some(version));
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn apply_semantic_instruction(
        &mut self,
        target: Result<Capture, String>,
        instruction: Result<SemanticInstruction, String>,
        repeat_version: Option<u64>,
    ) {
        let result = (|| {
            if self.macros.is_pending() || self.service.is_busy() || self.copied.is_pending() {
                return Err("Wait for the pending project action before another edit.".into());
            }
            let captured = target?;
            let instruction = instruction?;
            if !captured.matches(self)
                || self
                    .macros
                    .recording
                    .as_ref()
                    .is_some_and(|recording| !recording.expected.matches(self))
            {
                return Err("The captured editing context changed. Start the command again; no edit was made.".into());
            }
            if self.macros.instruction_count() >= deadpan_core::MAX_SEMANTIC_PROGRAM_INSTRUCTIONS {
                return Err(
                    "The macro has reached 1024 instructions. Save or cancel recording first."
                        .into(),
                );
            }
            let Some(serial) = self.next_serial() else {
                return Ok(());
            };
            let operation = protocol::Operation::Apply {
                id: captured.id(serial),
                instruction: instruction.clone(),
                scope: captured.scope.clone(),
                context: captured.context.clone(),
                repeat_version,
            };
            self.stop_playback();
            if self.submit(ProjectRequest::Macro(operation.clone())) {
                self.macros.pending = Some(Pending {
                    operation,
                    capture: captured,
                    instruction: self.macros.recording().then_some(instruction),
                    owns_cursor: true,
                });
                self.message = Some("Saving action…".into());
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.error = Some(error);
        }
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
            recording.cut = Some((request.id.clone(), attempt.operation.instruction(register)));
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
            }
            | protocol::Outcome::Applied {
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
            Err(error) => {
                // An ordinary refresh may fill an absent selection from the
                // cursor. A refused owned action preserves its captured absence.
                if pending.owns_cursor && pending.capture.matches_without_selection(self) {
                    self.selected_beat = pending.capture.context.selected_child.clone();
                }
                self.error = Some(error);
            }
            Ok(receipt) => {
                let visible = completion_visible(
                    &receipt,
                    self.workspace.as_ref().map(|workspace| {
                        (
                            workspace.session,
                            workspace.document.project_id(),
                            workspace.document.revision_id(),
                        )
                    }),
                    self.copied.bank_version(),
                );
                let completed_message = match &receipt.outcome {
                    protocol::Outcome::Executed {
                        register,
                        count,
                        committed,
                        ..
                    } => format!(
                        "Ran Macro @{register} × {count}{}.",
                        if committed.is_some() {
                            " · one Undo"
                        } else if receipt.bank_version != receipt.id.bank_version {
                            " · registers saved"
                        } else {
                            " · cursor and selection only"
                        },
                    ),
                    protocol::Outcome::Applied { .. } => format!(
                        "Action completed{}.",
                        if self.macros.recording() {
                            " and recorded"
                        } else {
                            ""
                        },
                    ),
                    protocol::Outcome::Saved { .. } => String::new(),
                };
                match receipt.outcome {
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
                        scope,
                        cursor,
                        selected,
                        visual_selection,
                        committed,
                        refresh_error,
                        ..
                    }
                    | protocol::Outcome::Applied {
                        scope,
                        cursor,
                        selected,
                        visual_selection,
                        committed,
                        refresh_error,
                    } => {
                        if refresh_error.is_some() || !visible {
                            if pending.owns_cursor
                                && pending.capture.matches_without_bank_or_selection(self)
                            {
                                self.selected_beat = pending.capture.context.selected_child.clone();
                            }
                            self.cancel_macro_recording();
                            self.message = refresh_error.or_else(|| Some("The action completed, but its edit is no longer visible. Recording was stopped.".into()));
                            return;
                        }
                        if !pending.owns_cursor {
                            self.cancel_macro_recording();
                            self.message = Some("Action completed after the editing context changed. Its final cursor and selection were not applied.".into());
                            return;
                        }
                        // Ownership was captured before receive installed the bank.
                        // A successful yank changes that bank without a revision,
                        // so comparing against the entry bank here would reject it.
                        let cursor =
                            u64::try_from(cursor.0).expect("validated nonnegative macro cursor");
                        let picture_changed =
                            self.sequence_scope != scope || self.sequence_cursor != cursor;
                        let selection_changed = self.selected_beat != selected;
                        self.sequence_scope = scope;
                        self.sequence_cursor = cursor;
                        self.selected_beat = selected;
                        self.reveal_beat |= selection_changed;
                        if let Err(error) = self.restore_macro_visual_selection(visual_selection) {
                            self.cancel_macro_recording();
                            self.error = Some(error);
                            return;
                        }
                        if committed.is_none() && picture_changed {
                            self.request_picture(false);
                        }
                        if let Some(instruction) = pending.instruction {
                            self.append_macro_instruction(instruction);
                        }
                        self.message = Some(completed_message);
                    }
                }
            }
        }
    }
}

fn register_name(register: Option<char>) -> Result<RegisterName, String> {
    register.map_or(Ok(RegisterName::unnamed()), |name| {
        RegisterName::new(name).map_err(|error| error.to_string())
    })
}

fn completion_visible(
    receipt: &protocol::Receipt,
    workspace: Option<(u64, &deadpan_core::ProjectId, &RevisionId)>,
    bank_version: Option<u64>,
) -> bool {
    let revision = receipt
        .committed()
        .map_or(&receipt.id.revision, |commit| &commit.revision);
    workspace.is_some_and(|(session, project, visible_revision)| {
        session == receipt.id.session
            && project == &receipt.id.project
            && visible_revision == revision
            && bank_version == Some(receipt.bank_version)
    })
}

#[cfg(test)]
mod tests;
