//! Incremental pure planning. A host commits the returned Compound only after
//! validating its frozen bank, live revision and every ordinary leaf boundary.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use crate::{
    AudioTimingId, CapturedEditSlice, Command, CommandRequest, EditError, EditErrorCode,
    FrameRange, LeafEdit, MAX_COMPOUND_CAPTURE_BYTES, MAX_COMPOUND_DOCUMENT_BYTES,
    MAX_COMPOUND_STEPS, MAX_DOCUMENT_JSON_BYTES, NodeId, NodeKind, ProjectDocument, ProjectFrame,
    RegisterName, RegisterValue, ResolvedStep, ResolvedTransaction, RevisionId,
    SemanticInstruction, SemanticProgram, SplitIdentities, compound::wire,
};

use super::{MAX_SEMANTIC_CALL_DEPTH, MAX_SEMANTIC_INSTRUCTION_FUEL};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticContext {
    pub parent: NodeId,
    /// Absolute Edit boundary, including either endpoint of the ordinary scope.
    pub cursor: ProjectFrame,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticAllocationRequest {
    pub cut_index: usize,
    pub required_split_ids: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticAllocation {
    pub new_revision: RevisionId,
    /// Scratch capture timing allocation, distinct from all editing allocations.
    pub capture_revision: RevisionId,
    pub split_identities: SplitIdentities,
}

/// Ordered instruction-entry trace. Call rows precede their expanded bodies and
/// include the final context after all their repetitions. Motions that clamp to
/// the same boundary still consume one fuel unit and retain a trace row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticTrace {
    pub instruction: SemanticInstruction,
    pub before_revision: RevisionId,
    pub before_scope: FrameRange,
    pub before: SemanticContext,
    pub after: SemanticContext,
    pub resolved_range: Option<FrameRange>,
    pub depth: usize,
}

#[derive(Debug)]
pub struct SemanticPlan {
    /// Motion-only programs produce no authored request or revision change.
    pub request: Option<CommandRequest>,
    pub document: ProjectDocument,
    pub context: SemanticContext,
    /// Right-hand direct child, or the final child at the scope's end.
    pub selected_child: Option<NodeId>,
    /// Final writes only, including the unnamed alias of each named cut.
    pub register_writes: BTreeMap<RegisterName, Arc<RegisterValue>>,
    pub trace: Vec<SemanticTrace>,
}

/// Resolve and apply each instruction once against the preceding staged state.
/// The allocator supplies identities only; it must not publish authored state.
/// On any error the input document and bank remain untouched. The host retains
/// responsibility for historical identity uniqueness and measured media admission.
pub fn plan_semantic(
    document: &ProjectDocument,
    context: &SemanticContext,
    program: &SemanticProgram,
    bank: &BTreeMap<RegisterName, Arc<RegisterValue>>,
    expected_bank_version: u64,
    new_revision: RevisionId,
    allocate: impl FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
) -> Result<SemanticPlan, EditError> {
    program.validate()?;
    document.validate()?;
    crate::command::check_revision(
        document,
        document.project_id(),
        document.revision_id(),
        &new_revision,
    )?;
    let bounds = scope_bounds(document, &context.parent)?;
    if context.cursor < bounds.0 || context.cursor > bounds.1 {
        return Err(EditError::new(
            EditErrorCode::SelectionUnavailable,
            "the macro cursor is outside its current Sequence",
        ));
    }
    let document_bytes = wire::size(document, MAX_DOCUMENT_JSON_BYTES)?;
    let mut revisions: BTreeSet<_> = document
        .audio_bindings()
        .allocation_ids()
        .into_iter()
        .cloned()
        .collect();
    revisions.extend(
        document
            .audio_lineage()
            .values()
            .map(|lineage| lineage.allocation.clone()),
    );
    for node in document.nodes().values() {
        if let NodeKind::Repeat { iterations, .. } = &node.kind {
            revisions.extend(
                iterations
                    .segments()
                    .map(|(revision, _, _)| revision.clone()),
            );
        }
    }
    revisions.insert(document.revision_id().clone());
    if !revisions.insert(new_revision.clone()) {
        return Err(identity(
            "macro outer revision reuses an existing allocation",
        ));
    }
    let mut planner = Planner {
        current: document.clone(),
        context: context.clone(),
        bounds,
        bank,
        inputs: BTreeMap::new(),
        writes: BTreeMap::new(),
        steps: Vec::new(),
        trace: Vec::new(),
        calls: Vec::new(),
        nodes: document.nodes().keys().cloned().collect(),
        revisions,
        document_bytes,
        captured_bytes: 0,
        allocate,
    };
    planner.execute(program)?;
    let selected_child = selected_child(&planner.current, &planner.context, planner.bounds)?;
    let request = if planner.steps.is_empty() {
        None
    } else {
        planner.current.revision_id = new_revision.clone();
        charge(
            &mut planner.document_bytes,
            wire::size(&planner.current, MAX_DOCUMENT_JSON_BYTES)?,
            MAX_COMPOUND_DOCUMENT_BYTES,
            "macro staged document byte limit",
        )?;
        Some(CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision,
            command: Command::Compound {
                transaction: ResolvedTransaction::new(
                    expected_bank_version,
                    planner.inputs,
                    planner.steps,
                )?,
            },
        })
    };
    Ok(SemanticPlan {
        request,
        document: planner.current,
        context: planner.context,
        selected_child,
        register_writes: planner.writes,
        trace: planner.trace,
    })
}

struct Planner<'a, F> {
    current: ProjectDocument,
    context: SemanticContext,
    bounds: (ProjectFrame, ProjectFrame),
    bank: &'a BTreeMap<RegisterName, Arc<RegisterValue>>,
    inputs: BTreeMap<RegisterName, Option<Arc<RegisterValue>>>,
    writes: BTreeMap<RegisterName, Arc<RegisterValue>>,
    steps: Vec<ResolvedStep>,
    trace: Vec<SemanticTrace>,
    calls: Vec<RegisterName>,
    nodes: BTreeSet<NodeId>,
    revisions: BTreeSet<RevisionId>,
    document_bytes: usize,
    captured_bytes: usize,
    allocate: F,
}

impl<F> Planner<'_, F>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
{
    fn execute(&mut self, program: &SemanticProgram) -> Result<(), EditError> {
        for instruction in program.instructions() {
            if self.trace.len() == MAX_SEMANTIC_INSTRUCTION_FUEL {
                return Err(limit("macro instruction fuel exhausted"));
            }
            let index = self.trace.len();
            self.trace.push(SemanticTrace {
                instruction: instruction.clone(),
                before_revision: self.current.revision_id().clone(),
                before_scope: FrameRange::new(self.bounds.0, self.bounds.1)
                    .map_err(crate::DocumentError::from)?,
                before: self.context.clone(),
                after: self.context.clone(),
                resolved_range: None,
                depth: self.calls.len(),
            });
            match instruction {
                SemanticInstruction::MoveFrames { forward, count } => {
                    // Compute distance to the bound first so a huge motion can
                    // clamp safely without overflowing the absolute frame clock.
                    let cursor = self.context.cursor.0;
                    let distance = if *forward {
                        self.bounds.1.0 - cursor
                    } else {
                        cursor - self.bounds.0.0
                    };
                    let amount = distance.min(i64::from(count.get()));
                    self.context.cursor = ProjectFrame(if *forward {
                        cursor + amount
                    } else {
                        cursor - amount
                    });
                }
                SemanticInstruction::CutFrames {
                    operation,
                    register,
                } => {
                    let range = operation.resolve(
                        &self.current,
                        &self.context.parent,
                        self.context.cursor,
                    )?;
                    self.cut(*register, range)?;
                    self.trace[index].resolved_range = Some(range);
                }
                SemanticInstruction::Call { register, count } => {
                    self.call(*register, count.get())?;
                }
            }
            self.trace[index].after = self.context.clone();
        }
        Ok(())
    }

    fn call(&mut self, register: RegisterName, count: u32) -> Result<(), EditError> {
        if self.calls.contains(&register) {
            return Err(invalid("recursive macro call is forbidden"));
        }
        if self.calls.len() == MAX_SEMANTIC_CALL_DEPTH {
            return Err(limit("macro call depth exceeds 16"));
        }
        let value = if let Some(value) = self.writes.get(&register) {
            value.clone()
        } else {
            let value = self.bank.get(&register).cloned();
            self.inputs.insert(register, value.clone());
            value.ok_or_else(|| invalid("the called macro register is empty"))?
        };
        let RegisterValue::Macro { program } = value.as_ref() else {
            return Err(invalid(
                "the called register contains copied content, not a macro",
            ));
        };
        // Each repetition must execute this many instructions even when its
        // motions are no-ops. Reject enormous counts without entering the loop.
        let minimum = usize::try_from(count)
            .ok()
            .and_then(|count| count.checked_mul(program.instructions().len()))
            .ok_or_else(|| limit("macro count exceeds instruction fuel"))?;
        if minimum > MAX_SEMANTIC_INSTRUCTION_FUEL - self.trace.len() {
            return Err(limit("macro count exceeds remaining instruction fuel"));
        }
        let direct_cuts = program
            .instructions()
            .iter()
            .filter(|instruction| matches!(instruction, SemanticInstruction::CutFrames { .. }))
            .count();
        let minimum_cuts = usize::try_from(count)
            .ok()
            .and_then(|count| count.checked_mul(direct_cuts))
            .ok_or_else(|| limit("macro count exceeds resolved editing steps"))?;
        if minimum_cuts > MAX_COMPOUND_STEPS - self.steps.len() {
            return Err(limit("macro count exceeds 1024 resolved editing steps"));
        }
        // The selected Arc stays frozen for every counted repetition, even if
        // this body writes over its own register. A later Call reads that write.
        self.calls.push(register);
        for _ in 0..count {
            self.execute(program)?;
        }
        self.calls.pop();
        Ok(())
    }

    fn cut(&mut self, register: RegisterName, range: FrameRange) -> Result<(), EditError> {
        if self.steps.len() == MAX_COMPOUND_STEPS {
            return Err(limit("macro exceeds 1024 resolved editing steps"));
        }
        let size = wire::size(&self.current, MAX_DOCUMENT_JSON_BYTES)?;
        charge(
            &mut self.document_bytes,
            size,
            MAX_COMPOUND_DOCUMENT_BYTES,
            "macro staged document byte limit",
        )?;
        charge(
            &mut self.captured_bytes,
            size,
            MAX_COMPOUND_CAPTURE_BYTES,
            "macro captured document byte limit",
        )?;
        let preflight = self.current.range_deletion(&self.context.parent, range)?;
        let allocation = (self.allocate)(SemanticAllocationRequest {
            cut_index: self.steps.len(),
            required_split_ids: preflight.required_ids,
        })?;
        if allocation.split_identities.nodes.len() != preflight.required_ids {
            return Err(invalid(
                "macro cut requires exactly its preflight Split identities",
            ));
        }
        for revision in [&allocation.new_revision, &allocation.capture_revision] {
            if !self.revisions.insert(revision.clone()) {
                return Err(identity("macro reuses a revision or capture allocation"));
            }
        }
        for node in &allocation.split_identities.nodes {
            if !self.nodes.insert(node.clone()) {
                return Err(identity("macro reuses a Split node identity"));
            }
        }
        let slice = Arc::new(CapturedEditSlice::capture(
            &self.current,
            &self.context.parent,
            range,
            AudioTimingId {
                allocation: allocation.capture_revision,
                ordinal: 0,
            },
        )?);
        let delete = LeafEdit::new(
            allocation.new_revision.clone(),
            Command::DeleteRange {
                parent: self.context.parent.clone(),
                range,
                identities: allocation.split_identities,
                timing: AudioTimingId {
                    allocation: allocation.new_revision,
                    ordinal: 0,
                },
            },
        )?;
        let applied = crate::apply(&self.current, &delete.request(&self.current))?;
        let next = applied.forward.apply(&self.current)?;
        charge(
            &mut self.document_bytes,
            wire::size(&next, MAX_DOCUMENT_JSON_BYTES)?,
            MAX_COMPOUND_DOCUMENT_BYTES,
            "macro staged document byte limit",
        )?;
        let value = Arc::new(RegisterValue::Edited {
            slice: slice.clone(),
        });
        self.writes.insert(register, value.clone());
        self.writes.insert(RegisterName::unnamed(), value);
        self.steps.push(ResolvedStep::Cut {
            name: register,
            slice,
            delete,
        });
        self.current = next;
        self.context.cursor = range.start();
        self.bounds = scope_bounds(&self.current, &self.context.parent)?;
        Ok(())
    }
}

fn scope_bounds(
    document: &ProjectDocument,
    parent: &NodeId,
) -> Result<(ProjectFrame, ProjectFrame), EditError> {
    let start = document.source_splice_boundary(parent, 0)?;
    let NodeKind::Sequence { children } = &document.nodes()[parent].kind else {
        unreachable!("source_splice_boundary admitted an ordinary Sequence")
    };
    let end = document.source_splice_boundary(parent, children.len())?;
    Ok((start, end))
}

fn selected_child(
    document: &ProjectDocument,
    context: &SemanticContext,
    bounds: (ProjectFrame, ProjectFrame),
) -> Result<Option<NodeId>, EditError> {
    let NodeKind::Sequence { children } = &document.nodes()[&context.parent].kind else {
        unreachable!("the ordinary Sequence scope was already admitted")
    };
    if context.cursor == bounds.1 {
        return Ok(children.last().cloned());
    }
    let durations = document.durations()?;
    let mut start = bounds.0.0;
    for child in children {
        let end = start
            .checked_add(durations[child].frames())
            .ok_or_else(|| {
                EditError::new(
                    EditErrorCode::TimingOverflow,
                    "macro child boundary overflow",
                )
            })?;
        if start <= context.cursor.0 && context.cursor.0 < end {
            return Ok(Some(child.clone()));
        }
        start = end;
    }
    Ok(None)
}

fn charge(
    total: &mut usize,
    amount: usize,
    maximum: usize,
    message: &str,
) -> Result<(), EditError> {
    *total = total
        .checked_add(amount)
        .filter(|value| *value <= maximum)
        .ok_or_else(|| limit(message))?;
    Ok(())
}

fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}
fn identity(message: &str) -> EditError {
    EditError::new(EditErrorCode::IdentityConflict, message)
}
fn limit(message: &str) -> EditError {
    EditError::new(EditErrorCode::LimitExceeded, message)
}

#[cfg(test)]
mod tests;
