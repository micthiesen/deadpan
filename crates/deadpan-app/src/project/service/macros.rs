//! Semantic programs prepare one complete transaction and its runtime copies
//! before the store publishes authored history or a new register bank.

use std::num::NonZeroU32;

use deadpan_core::{
    RegisterName, RegisterValue, SemanticInstruction, SemanticPlan, SemanticProgram,
    SemanticSelector, SliceCaptureSelection,
};
use deadpan_store::registers::RegisterBank;

use super::*;
use crate::project::macros::{Id, Operation, Outcome, Receipt, Update};
use crate::project::registers::{Bank, Value};
use crate::project::slice::{Captured, CopyId};

impl Service {
    /// Remote macros use explicit document coordinates. Build every runtime
    /// copy before committing, without creating a native cursor continuation.
    pub(super) fn prepare_remote_macro_registers(
        &mut self,
        prepared: &deadpan_cli::macros::Prepared,
    ) -> Result<Option<Arc<Bank>>> {
        use deadpan_cli::macros::Operation as RemoteOperation;

        if prepared.request.dry_run
            || prepared
                .plan
                .as_ref()
                .is_some_and(|plan| plan.request.is_none())
        {
            return Ok(None);
        }
        self.refresh_registers()?;
        let id = Id {
            session: self.session,
            project: prepared.request.project_id.clone(),
            revision: prepared.request.expected_revision.clone(),
            bank_version: prepared.bank.version,
            // Used only to prepare persisted runtime copies. No native request
            // or receipt is published for this remote operation.
            request: 1,
        };
        let runtime = self.macro_runtime_bank(&id)?;
        if runtime.entries.len() != prepared.bank.entries.len()
            || prepared.bank.entries.iter().any(|(name, value)| {
                runtime
                    .entries
                    .get(&name.as_char())
                    .is_none_or(|runtime| !matches_runtime(value, runtime, &id))
            })
        {
            return Err("Remote macro input differs from its runtime register content".into());
        }
        let bank = match &prepared.request.operation {
            RemoteOperation::Save { register, program } => {
                let mut bank = runtime.clone();
                bank.version = prepared.final_bank.version;
                bank.entries
                    .insert(register.as_char(), Value::Macro(program.clone()));
                bank
            }
            RemoteOperation::Run { .. } => {
                let plan = prepared.plan.as_ref().ok_or("Remote macro has no plan")?;
                prepare_runtime_bank(&id, runtime, &prepared.bank, &prepared.final_bank, plan)?
            }
        };
        Ok(Some(Arc::new(bank)))
    }

    pub(super) fn macro_command(&mut self, operation: Operation) {
        let result = match &self.saved_macro {
            Some((previous, receipt)) if previous == &operation => Ok(receipt.clone()),
            Some((previous, _)) if previous.id() == operation.id() => {
                Err("The macro identity was already used with different parameters".into())
            }
            _ => self.perform_macro(&operation),
        };
        self.macros = Some(Update {
            id: operation.id().clone(),
            result,
        });
    }

    fn perform_macro(&mut self, operation: &Operation) -> Result<Receipt> {
        self.observe_semantic();
        let id = operation.id();
        let (document, stored) = self.check_macro_context(id)?;
        match operation {
            Operation::Save {
                register, program, ..
            } => {
                let name = macro_name(*register)?;
                program.validate().map_err(display)?;
                let mut bank = self.macro_runtime_bank(id)?.clone();
                bank.version = next_bank_version(stored.version)?;
                bank.entries
                    .insert(*register, Value::Macro(program.clone()));
                let saved = self
                    .writer()?
                    .save_macro(
                        &id.project,
                        &id.revision,
                        id.bank_version,
                        name,
                        program.clone(),
                    )
                    .map_err(display)?;
                // The store owns version advancement. All runtime preparation
                // above is fallible; after success publication cannot fail.
                bank.version = saved.version;
                self.registers = Some(Arc::new(bank));
                let receipt = Receipt {
                    id: id.clone(),
                    bank_version: saved.version,
                    outcome: Outcome::Saved {
                        register: *register,
                        instructions: program.instructions().len(),
                    },
                };
                self.saved_macro = Some((operation.clone(), receipt.clone()));
                self.message = Some(format!("Macro {register} saved"));
                self.error = None;
                Ok(receipt)
            }
            Operation::Run { scope, context, .. } | Operation::Apply { scope, context, .. } => {
                if let Operation::Apply {
                    instruction,
                    repeat_version: Some(version),
                    ..
                } = operation
                {
                    self.check_semantic_repeat(*version, context, instruction)?;
                }
                let (instruction, label) = match operation {
                    Operation::Run {
                        register, count, ..
                    } => (
                        SemanticInstruction::Call {
                            register: macro_name(*register)?,
                            count: NonZeroU32::new(*count).ok_or("Macro count must be positive")?,
                        },
                        format!("Macro {register}"),
                    ),
                    Operation::Apply { instruction, .. } => (
                        instruction.clone(),
                        match instruction {
                            SemanticInstruction::YankBeat { .. } => "Copy beat",
                            SemanticInstruction::Yank {
                                selector: SemanticSelector::SelectedBeat,
                                ..
                            } => "Copy beat",
                            SemanticInstruction::YankSelection { .. } => "Copy selection",
                            SemanticInstruction::CutFrames { .. } => "Cut frames",
                            SemanticInstruction::CutSelection { .. } => "Cut selection",
                            SemanticInstruction::Cut {
                                selector: SemanticSelector::SelectedBeat,
                                ..
                            } => "Cut beat",
                            SemanticInstruction::Yank { .. } => "Copy selection",
                            SemanticInstruction::Cut { .. } => "Cut selection",
                            SemanticInstruction::ReplaceSelection { .. } => "Replace selection",
                            SemanticInstruction::Paste { .. } => "Paste register",
                            SemanticInstruction::Repeat { .. } => "Wrap Repeat",
                            SemanticInstruction::SetRepeatPlays { .. } => "Set Repeat plays",
                            SemanticInstruction::SetRepeat { .. } => "Change Repeat",
                            SemanticInstruction::SetRoomTone { .. } => "Set room tone",
                            SemanticInstruction::Group { .. } => "Group beats",
                            SemanticInstruction::Ungroup => "Ungroup beats",
                            SemanticInstruction::Gag { .. } => "Apply gag",
                            SemanticInstruction::InsertPause { .. } => "Insert pause",
                            SemanticInstruction::SetFraming { .. } => "Frame beat",
                            _ => "Recorded action",
                        }
                        .to_owned(),
                    ),
                    Operation::Save { .. } => unreachable!("save handled separately"),
                };
                let picture_plan = RenderPlan::compile(&document).map_err(display)?;
                let owner = scope.resolve_document(&document, &picture_plan)?;
                if owner.owner != &context.parent {
                    return Err("Macro target differs from the captured Sequence scope".into());
                }
                let invocation = SemanticProgram::new(vec![instruction]).map_err(display)?;
                let plan = deadpan_cli::macros::plan_program(
                    self.store.as_ref().ok_or("Open a project first")?,
                    &document,
                    &stored,
                    context,
                    &invocation,
                    revision(),
                )
                .map_err(display)?;
                let final_picture_plan = RenderPlan::compile(&plan.document).map_err(display)?;
                let final_scope = SequenceScope::from_historical_parent(
                    &plan.document,
                    &final_picture_plan,
                    &plan.context.parent,
                )?;
                let mut receipt = Receipt {
                    id: id.clone(),
                    bank_version: stored.version,
                    outcome: match operation {
                        Operation::Run {
                            register, count, ..
                        } => Outcome::Executed {
                            register: *register,
                            count: *count,
                            scope: final_scope.clone(),
                            cursor: plan.context.cursor,
                            selected: plan.selected_child.clone(),
                            visual_selection: plan.context.visual_selection.clone(),
                            committed: None,
                            refresh_error: None,
                        },
                        Operation::Apply { .. } => Outcome::Applied {
                            scope: final_scope.clone(),
                            cursor: plan.context.cursor,
                            selected: plan.selected_child.clone(),
                            visual_selection: plan.context.visual_selection.clone(),
                            committed: None,
                            refresh_error: None,
                        },
                        Operation::Save { .. } => unreachable!("save handled separately"),
                    },
                };
                let Some(request) = &plan.request else {
                    self.saved_macro = Some((operation.clone(), receipt.clone()));
                    self.message = Some(format!("{label} updated the Edit cursor and selection"));
                    self.error = None;
                    return Ok(receipt);
                };
                let preview = self
                    .store
                    .as_ref()
                    .ok_or("Open a project first")?
                    .preview_compound(request)
                    .map_err(display)?;
                let bank = prepare_runtime_bank(
                    id,
                    self.macro_runtime_bank(id)?,
                    &stored,
                    &preview.register_bank,
                    &plan,
                )?;
                let outcome = self
                    .writer()?
                    .commit_compound(request, None)
                    .map_err(display)?;
                if let Some(saved) = &outcome.committed
                    && let Operation::Apply { instruction, .. } = operation
                    && let Some(edit) =
                        crate::project::semantic::LastEdit::from_instruction(instruction)
                {
                    self.semantic.prove(
                        id.session,
                        &id.project,
                        &id.revision,
                        &saved.revision_id,
                        semantic::Change::Replace(edit),
                    );
                }
                // A yank-only program saves its bank without a new timeline
                // revision. Only the store's authored receipt grants a native
                // edit continuation; never infer one from a Compound request.
                let committed = outcome.committed.map(|saved| CommittedEdit {
                    scoped: None,
                    revision: saved.revision_id,
                    selected_node: plan.selected_child.clone(),
                    preserve_cursor: false,
                    cursor: Some(plan.context.cursor),
                    scope: final_scope,
                    sound: None,
                    range_selection: None,
                });
                receipt.bank_version = outcome.register_bank.version;
                if let Outcome::Executed {
                    committed: result, ..
                }
                | Outcome::Applied {
                    committed: result, ..
                } = &mut receipt.outcome
                {
                    *result = committed.clone().map(Box::new);
                }
                self.registers = Some(Arc::new(bank));
                if let Some(committed) = &committed {
                    self.committed = Some(committed.clone());
                }
                self.saved_macro = Some((operation.clone(), receipt.clone()));
                #[cfg(test)]
                {
                    self.render_preview_refresh_failure = self
                        .shared
                        .render_commit_refresh_failure
                        .swap(false, Ordering::AcqRel);
                }
                match self.refresh() {
                    Ok(()) => {
                        self.message = Some(if committed.is_some() {
                            format!("{label} saved as one edit; Undo restores it")
                        } else {
                            format!("{label} saved to registers; timeline history is unchanged")
                        })
                    }
                    Err(error) => {
                        let message = format!(
                            "{label} saved, but the preview could not refresh: {error}. Reopen this project before editing or undoing."
                        );
                        if let Outcome::Executed { refresh_error, .. }
                        | Outcome::Applied { refresh_error, .. } = &mut receipt.outcome
                        {
                            *refresh_error = Some(message.clone());
                        }
                        self.message = Some(message);
                    }
                }
                self.error = None;
                self.saved_macro = Some((operation.clone(), receipt.clone()));
                Ok(receipt)
            }
        }
    }

    fn check_macro_context(&mut self, id: &Id) -> Result<(ProjectDocument, RegisterBank)> {
        if id.session == 0 || id.request == 0 {
            return Err("Macro identities must be nonzero".into());
        }
        self.check_context(id.session, &id.revision)?;
        if self
            .workspace
            .as_ref()
            .is_none_or(|workspace| workspace.document.project_id() != &id.project)
        {
            return Err("Macro belongs to another project".into());
        }
        if self.pending_session_change.is_some() {
            return Err("Project session is changing; start the macro again after it opens".into());
        }
        let store = self.store.as_ref().ok_or("Open a project first")?;
        let document = store.snapshot().map_err(display)?;
        if document.project_id() != &id.project {
            return Err("Macro belongs to another project".into());
        }
        if document.revision_id() != &id.revision {
            return Err(
                "The saved project changed. Reopen it before running or saving macros.".into(),
            );
        }
        let stored = store.registers().map_err(display)?;
        if stored.version != id.bank_version {
            return Err("The register bank changed; start the macro request again".into());
        }
        self.refresh_registers()?;
        let runtime = self.macro_runtime_bank(id)?;
        if runtime.entries.len() != stored.entries.len()
            || stored.entries.iter().any(|(name, value)| {
                runtime
                    .entries
                    .get(&name.as_char())
                    .is_none_or(|runtime| !matches_runtime(value, runtime, id))
            })
        {
            return Err("Macro input differs from its runtime register content".into());
        }
        Ok((document, stored))
    }

    fn macro_runtime_bank(&self, id: &Id) -> Result<&Bank> {
        self.registers
            .as_deref()
            .filter(|bank| {
                bank.session == id.session
                    && bank.project == id.project
                    && bank.version == id.bank_version
            })
            .ok_or_else(|| "The runtime register bank differs from the macro request".into())
    }
}

fn macro_name(register: char) -> Result<RegisterName> {
    if !register.is_ascii_lowercase() {
        return Err("A macro requires a named register a-z".into());
    }
    RegisterName::new(register).map_err(display)
}

fn next_bank_version(version: u64) -> Result<u64> {
    version
        .checked_add(1)
        .filter(|version| *version <= i64::MAX as u64)
        .ok_or_else(|| "Register versions are exhausted".into())
}

fn matches_runtime(value: &RegisterValue, runtime: &Value, id: &Id) -> bool {
    match (value, runtime) {
        (
            RegisterValue::Original {
                asset,
                qualification,
                ordinals,
                ..
            },
            Value::Original {
                asset: found_asset,
                qualification: found_qualification,
                ordinals: found_ordinals,
            },
        ) => {
            asset == found_asset
                && qualification == found_qualification
                && ordinals == found_ordinals
        }
        (RegisterValue::Edited { slice }, Value::Edited(copied)) => {
            copied.id().session == id.session
                && copied.id().project == id.project
                && copied.id().source_revision == *slice.revision_id()
                && copied.slice() == slice
        }
        (RegisterValue::Macro { program }, Value::Macro(found)) => program == found,
        _ => false,
    }
}

fn prepare_runtime_bank(
    id: &Id,
    runtime: &Bank,
    stored: &RegisterBank,
    preview: &RegisterBank,
    plan: &SemanticPlan,
) -> Result<Bank> {
    let mut expected = stored.entries.clone();
    expected.extend(plan.register_writes.clone());
    let expected_version = if plan.register_writes.is_empty() {
        stored.version
    } else {
        next_bank_version(stored.version)?
    };
    if preview.entries != expected || preview.version != expected_version {
        return Err("Macro preview register writes differ from the prepared semantic plan".into());
    }
    let mut entries = BTreeMap::new();
    let mut prepared: Vec<(Arc<RegisterValue>, Value)> = Vec::new();
    for (name, value) in &preview.entries {
        let entry = if stored.entries.get(name) == Some(value) {
            runtime
                .entries
                .get(&name.as_char())
                .filter(|entry| matches_runtime(value, entry, id))
                .ok_or("Macro input differs from its runtime register content")?
                .clone()
        } else if let Some((_, prepared)) = prepared.iter().find(|(previous, _)| previous == value)
        {
            prepared.clone()
        } else if let Some((slot, _)) = stored
            .entries
            .iter()
            .find(|(_, previous)| *previous == value)
        {
            let existing = runtime
                .entries
                .get(&slot.as_char())
                .filter(|entry| matches_runtime(value, entry, id))
                .ok_or("Macro input differs from its runtime register content")?;
            existing.clone()
        } else {
            let RegisterValue::Edited { slice } = value.as_ref() else {
                return Err("Macro plan introduced an unexpected register content type".into());
            };
            let selection = slice.selection();
            let trace = plan
                .trace
                .iter()
                .find(|trace| {
                    trace.capture.as_ref().is_some_and(|capture| {
                        &capture.timing == slice.capture_timing()
                            && &capture.parent == slice.parent()
                    }) && &trace.before_revision == slice.revision_id()
                        && trace.resolved_parent.as_ref() == Some(slice.parent())
                        && trace.resolved_selection.as_ref() == Some(selection)
                })
                .ok_or("Macro copy has no matching staged capture provenance")?;
            let capture = trace
                .capture
                .as_ref()
                .ok_or("Macro copy has no staged capture provenance")?;
            let scope = SequenceScope::from_staged_capture(capture)?;
            let source_path: Vec<String> = std::iter::once("Your edit".to_owned())
                .chain(capture.scope_labels.iter().cloned())
                .collect();
            let child_label = match selection {
                SliceCaptureSelection::Range { .. } | SliceCaptureSelection::Children { .. } => {
                    None
                }
                SliceCaptureSelection::Child { .. } => Some(
                    trace
                        .captured_child_label
                        .clone()
                        .ok_or("Macro beat copy has no staged child label")?,
                ),
            };
            let request = u64::try_from(prepared.len())
                .map_err(display)?
                .checked_add(1)
                .ok_or("Macro copy identities exhausted")?;
            Value::Edited(Arc::new(Captured {
                id: CopyId {
                    session: id.session,
                    project: id.project.clone(),
                    source_revision: slice.revision_id().clone(),
                    request,
                    persisted_version: Some(preview.version),
                },
                scope,
                slice: slice.clone(),
                bounds: capture.bounds,
                source_path,
                child_label,
            }))
        };
        prepared.push((value.clone(), entry.clone()));
        entries.insert(name.as_char(), entry);
    }
    Ok(Bank {
        session: id.session,
        project: id.project.clone(),
        version: preview.version,
        entries,
    })
}
