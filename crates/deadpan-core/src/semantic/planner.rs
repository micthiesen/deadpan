//! Incremental pure planning. A host commits the returned Compound only after
//! validating its frozen bank, live revision and every ordinary leaf boundary.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use crate::{
    AudioTimingId, CapturedEditSlice, Command, CommandRequest, EditError, EditErrorCode,
    FrameRange, LeafEdit, MAX_COMPOUND_CAPTURE_BYTES, MAX_COMPOUND_DOCUMENT_BYTES,
    MAX_COMPOUND_STEPS, MAX_DOCUMENT_JSON_BYTES, MarkId, NodeId, NodeKind, ProjectDocument,
    ProjectFrame, RegisterName, RegisterValue, ResolvedStep, ResolvedTransaction, RevisionId,
    SemanticInstruction, SemanticProgram, SliceCaptureSelection, SliceIdentityRequirements,
    SlicePasteIdentities, SourceNode, SplitIdentities, compound::wire,
};

use super::{MAX_SEMANTIC_CALL_DEPTH, MAX_SEMANTIC_INSTRUCTION_FUEL};

mod content;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticContext {
    pub parent: NodeId,
    /// Absolute Edit boundary, including either endpoint of the ordinary scope.
    pub cursor: ProjectFrame,
    /// Explicit direct-child selection, independent of the cursor. None means
    /// no selected beat, including when nonempty children surround the cursor.
    pub selected_child: Option<NodeId>,
}

/// Frozen register contents and the version the host must admit at commit.
#[derive(Debug, Clone, Copy)]
pub struct SemanticRegisterBank<'a> {
    pub entries: &'a BTreeMap<RegisterName, Arc<RegisterValue>>,
    pub version: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticAllocationRequest {
    Cut {
        step_index: usize,
        required_split_ids: usize,
    },
    Yank {
        step_index: usize,
    },
    PasteEdited {
        step_index: usize,
        requirements: SliceIdentityRequirements,
    },
    PasteOriginal {
        step_index: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticAllocation {
    Cut {
        new_revision: RevisionId,
        capture_revision: RevisionId,
        split_identities: SplitIdentities,
    },
    Yank {
        capture_revision: RevisionId,
    },
    PasteEdited {
        new_revision: RevisionId,
        identities: SlicePasteIdentities,
    },
    PasteOriginal {
        new_revision: RevisionId,
        node: NodeId,
    },
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
    /// Exact staged direct-child label for a YankBeat capture.
    pub captured_child_label: Option<String>,
    pub depth: usize,
}

#[derive(Debug)]
pub struct SemanticPlan {
    /// Motion-only programs produce no request. A Yank-only request updates the
    /// bank without changing `document`'s revision or creating authored history.
    pub request: Option<CommandRequest>,
    pub document: ProjectDocument,
    pub context: SemanticContext,
    /// Final explicit selection, also retained in `context.selected_child`.
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
    registers: SemanticRegisterBank<'_>,
    new_revision: RevisionId,
    allocate: impl FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    resolve_original: impl FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
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
    validate_selection(document, context)?;
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
        child_ends: child_ends(document, &context.parent, bounds)?,
        bank: registers.entries,
        inputs: BTreeMap::new(),
        writes: BTreeMap::new(),
        steps: Vec::new(),
        trace: Vec::new(),
        calls: Vec::new(),
        nodes: occupied_nodes(document),
        marks: document.marks().keys().cloned().collect(),
        revisions,
        document_bytes,
        captured_bytes: 0,
        allocate,
        resolve_original,
    };
    planner.execute(program)?;
    let selected_child = planner.context.selected_child.clone();
    let request = if planner.steps.is_empty() {
        None
    } else {
        if planner.steps.iter().any(|step| step.edit().is_some()) {
            planner.current.revision_id = new_revision.clone();
            charge(
                &mut planner.document_bytes,
                wire::size(&planner.current, MAX_DOCUMENT_JSON_BYTES)?,
                MAX_COMPOUND_DOCUMENT_BYTES,
                "macro staged document byte limit",
            )?;
        }
        Some(CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision,
            command: Command::Compound {
                transaction: ResolvedTransaction::new(
                    registers.version,
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

struct Planner<'a, F, R> {
    current: ProjectDocument,
    context: SemanticContext,
    bounds: (ProjectFrame, ProjectFrame),
    /// Updated only after an authored leaf. Frame motions use a binary search
    /// instead of walking the entire staged document for every instruction.
    child_ends: Vec<(NodeId, ProjectFrame)>,
    bank: &'a BTreeMap<RegisterName, Arc<RegisterValue>>,
    inputs: BTreeMap<RegisterName, Option<Arc<RegisterValue>>>,
    writes: BTreeMap<RegisterName, Arc<RegisterValue>>,
    steps: Vec<ResolvedStep>,
    trace: Vec<SemanticTrace>,
    calls: Vec<RegisterName>,
    nodes: BTreeSet<NodeId>,
    marks: BTreeSet<MarkId>,
    revisions: BTreeSet<RevisionId>,
    document_bytes: usize,
    captured_bytes: usize,
    allocate: F,
    resolve_original: R,
}

impl<F, R> Planner<'_, F, R>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
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
                captured_child_label: None,
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
                    self.context.selected_child =
                        selected_child(&self.child_ends, self.context.cursor, self.bounds);
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
                SemanticInstruction::YankBeat { register } => {
                    let (range, label) = self.yank(*register)?;
                    self.trace[index].resolved_range = Some(range);
                    self.trace[index].captured_child_label = Some(label);
                }
                SemanticInstruction::Paste { register, before } => {
                    self.trace[index].resolved_range = Some(self.paste(*register, *before)?);
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
        let direct_steps = program
            .instructions()
            .iter()
            .filter(|instruction| {
                matches!(
                    instruction,
                    SemanticInstruction::CutFrames { .. }
                        | SemanticInstruction::YankBeat { .. }
                        | SemanticInstruction::Paste { .. }
                )
            })
            .count();
        let minimum_steps = usize::try_from(count)
            .ok()
            .and_then(|count| count.checked_mul(direct_steps))
            .ok_or_else(|| limit("macro count exceeds resolved editing steps"))?;
        if minimum_steps > MAX_COMPOUND_STEPS - self.steps.len() {
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
        let allocation = (self.allocate)(SemanticAllocationRequest::Cut {
            step_index: self.steps.len(),
            required_split_ids: preflight.required_ids,
        })?;
        let SemanticAllocation::Cut {
            new_revision,
            capture_revision,
            split_identities,
        } = allocation
        else {
            return Err(invalid("macro cut requires a Cut allocation"));
        };
        if split_identities.nodes.len() != preflight.required_ids {
            return Err(invalid(
                "macro cut requires exactly its preflight Split identities",
            ));
        }
        for revision in [&new_revision, &capture_revision] {
            if !self.revisions.insert(revision.clone()) {
                return Err(identity("macro reuses a revision or capture allocation"));
            }
        }
        for node in &split_identities.nodes {
            if !self.nodes.insert(node.clone()) {
                return Err(identity("macro reuses a Split node identity"));
            }
        }
        let slice = Arc::new(CapturedEditSlice::capture(
            &self.current,
            &self.context.parent,
            range,
            AudioTimingId {
                allocation: capture_revision,
                ordinal: 0,
            },
        )?);
        let delete = LeafEdit::new(
            new_revision.clone(),
            Command::DeleteRange {
                parent: self.context.parent.clone(),
                range,
                identities: split_identities,
                timing: AudioTimingId {
                    allocation: new_revision,
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
        self.child_ends = child_ends(&self.current, &self.context.parent, self.bounds)?;
        self.context.selected_child =
            selected_child(&self.child_ends, self.context.cursor, self.bounds);
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

fn validate_selection(
    document: &ProjectDocument,
    context: &SemanticContext,
) -> Result<(), EditError> {
    let NodeKind::Sequence { children } = &document.nodes()[&context.parent].kind else {
        unreachable!("scope_bounds admitted an ordinary Sequence")
    };
    if context
        .selected_child
        .as_ref()
        .is_some_and(|selected| !children.contains(selected))
    {
        return Err(EditError::new(
            EditErrorCode::SelectionUnavailable,
            "the selected macro beat is not a direct child of its Sequence",
        ));
    }
    Ok(())
}

fn occupied_nodes(document: &ProjectDocument) -> BTreeSet<NodeId> {
    let mut nodes: BTreeSet<_> = document.nodes().keys().cloned().collect();
    nodes.extend(
        document
            .audio_lineage()
            .values()
            .map(|lineage| lineage.origin.clone()),
    );
    for layout in document.audio_bindings().timings.values() {
        nodes.extend(layout.nodes().keys().cloned());
        nodes.extend(
            layout
                .audio_lineage()
                .values()
                .map(|lineage| lineage.origin.clone()),
        );
    }
    nodes
}

fn child_ends(
    document: &ProjectDocument,
    parent: &NodeId,
    bounds: (ProjectFrame, ProjectFrame),
) -> Result<Vec<(NodeId, ProjectFrame)>, EditError> {
    let NodeKind::Sequence { children } = &document.nodes()[parent].kind else {
        unreachable!("the ordinary Sequence scope was already admitted")
    };
    let durations = document.durations()?;
    let mut start = bounds.0.0;
    let mut result = Vec::with_capacity(children.len());
    for child in children {
        let end = start
            .checked_add(durations[child].frames())
            .ok_or_else(|| {
                EditError::new(
                    EditErrorCode::TimingOverflow,
                    "macro child boundary overflow",
                )
            })?;
        result.push((child.clone(), ProjectFrame(end)));
        start = end;
    }
    Ok(result)
}

fn selected_child(
    child_ends: &[(NodeId, ProjectFrame)],
    cursor: ProjectFrame,
    bounds: (ProjectFrame, ProjectFrame),
) -> Option<NodeId> {
    if cursor == bounds.1 {
        return child_ends.last().map(|(node, _)| node.clone());
    }
    child_ends
        .get(child_ends.partition_point(|(_, end)| *end <= cursor))
        .map(|(node, _)| node.clone())
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
