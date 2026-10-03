//! Semantic programs prepare one complete transaction and its runtime copies
//! before the store publishes authored history or a new register bank.

use std::num::NonZeroU32;

use deadpan_core::{
    RegisterName, RegisterValue, SemanticAllocation, SemanticInstruction, SemanticPlan,
    SemanticProgram, SliceCaptureSelection, SplitIdentities, plan_semantic,
};
use deadpan_store::registers::RegisterBank;

use super::*;
use crate::project::macros::{Id, Operation, Outcome, Receipt, Update};
use crate::project::registers::{Bank, Value};
use crate::project::slice::{Captured, CopyId};

impl Service {
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
            Operation::Run {
                register,
                count,
                scope,
                context,
                ..
            } => {
                let register_name = macro_name(*register)?;
                let count_value = NonZeroU32::new(*count).ok_or("Macro count must be positive")?;
                let picture_plan = RenderPlan::compile(&document).map_err(display)?;
                let owner = scope.resolve_document(&document, &picture_plan)?;
                if owner.owner != &context.parent {
                    return Err("Macro target differs from the captured Sequence scope".into());
                }
                let invocation = SemanticProgram::new(vec![SemanticInstruction::Call {
                    register: register_name,
                    count: count_value,
                }])
                .map_err(display)?;
                let plan = plan_semantic(
                    &document,
                    context,
                    &invocation,
                    &stored.entries,
                    stored.version,
                    revision(),
                    |allocation| {
                        Ok(SemanticAllocation {
                            new_revision: revision(),
                            capture_revision: revision(),
                            split_identities: SplitIdentities {
                                nodes: (0..allocation.required_split_ids).map(|_| node()).collect(),
                            },
                        })
                    },
                )
                .map_err(display)?;
                let mut receipt = Receipt {
                    id: id.clone(),
                    bank_version: stored.version,
                    outcome: Outcome::Executed {
                        register: *register,
                        count: *count,
                        scope: scope.clone(),
                        cursor: plan.context.cursor,
                        selected: plan.selected_child.clone(),
                        committed: None,
                        refresh_error: None,
                    },
                };
                let Some(request) = &plan.request else {
                    self.saved_macro = Some((operation.clone(), receipt.clone()));
                    self.message = Some(format!("Macro {register} moved the Edit cursor"));
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
                    scope,
                    &document,
                    self.macro_runtime_bank(id)?,
                    &stored,
                    &preview.register_bank,
                    &plan,
                )?;
                let outcome = self
                    .writer()?
                    .commit_compound(request, None)
                    .map_err(display)?;
                // The request has authored leaves; commit_compound's contract
                // supplies the exact outer revision here, already prepared.
                let committed = CommittedEdit {
                    revision: request.new_revision.clone(),
                    selected_node: plan.selected_child.clone(),
                    preserve_cursor: false,
                    cursor: Some(plan.context.cursor),
                    scope: scope.clone(),
                    sound: None,
                    range_selection: None,
                };
                receipt.bank_version = outcome.register_bank.version;
                if let Outcome::Executed {
                    committed: result, ..
                } = &mut receipt.outcome
                {
                    *result = Some(Box::new(committed.clone()));
                }
                self.registers = Some(Arc::new(bank));
                self.committed = Some(committed);
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
                        self.message = Some(format!(
                            "Macro {register} saved as one edit; Undo restores it"
                        ))
                    }
                    Err(error) => {
                        let message = format!(
                            "Macro {register} saved, but the preview could not refresh: {error}. Reopen this project before editing or undoing."
                        );
                        if let Outcome::Executed { refresh_error, .. } = &mut receipt.outcome {
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
    scope: &SequenceScope,
    document: &ProjectDocument,
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
    let source_path: Vec<String> = std::iter::once("Your edit".to_owned())
        .chain(
            scope
                .groups()
                .iter()
                .map(|node| document.nodes()[node].label.clone()),
        )
        .collect();
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
            let SliceCaptureSelection::Range { range } = slice.selection() else {
                return Err("Macro frame cuts require an exact captured range".into());
            };
            let trace = plan
                .trace
                .iter()
                .find(|trace| {
                    matches!(trace.instruction, SemanticInstruction::CutFrames { .. })
                        && &trace.before_revision == slice.revision_id()
                        && &trace.before.parent == slice.parent()
                        && trace.resolved_range.as_ref() == Some(range)
                })
                .ok_or("Macro copy has no matching staged capture provenance")?;
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
                scope: scope.clone(),
                slice: slice.clone(),
                bounds: trace.before_scope,
                source_path: source_path.clone(),
                child_label: None,
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
