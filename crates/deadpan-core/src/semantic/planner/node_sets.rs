//! A union is captured once before any deletion. Its ordinary deletion leaves
//! run backwards through structural order in the same atomic Compound.

use super::*;

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: crate::SemanticSpeechProvider,
    P: FnMut(&ProjectDocument, super::PauseSite) -> Result<super::PauseProvider, EditError>,
{
    pub(super) fn capture_nodes(
        &mut self,
        trace_index: usize,
        register: RegisterName,
        nodes: &[NodeId],
        cut: bool,
    ) -> Result<(), EditError> {
        let target =
            crate::semantic::node_set::resolve_in_scope(&self.current, &self.context, nodes)?;
        if self
            .steps
            .len()
            .checked_add(1 + if cut { target.members.len() } else { 0 })
            .is_none_or(|count| count > MAX_COMPOUND_STEPS)
        {
            return Err(limit("node set exceeds resolved editing step limit"));
        }
        let parent = self.context.parent.clone();
        let (bounds, scope, scope_labels) = self.capture_scope(&parent)?;
        self.charge_step(true)?;
        let SemanticAllocation::Yank { capture_revision } =
            (self.allocate)(SemanticAllocationRequest::Yank {
                step_index: self.steps.len(),
            })?
        else {
            return Err(invalid("a node set capture requires a Yank allocation"));
        };
        self.reserve_revision(&capture_revision)?;
        let timing = AudioTimingId {
            allocation: capture_revision,
            ordinal: 0,
        };
        let canonical = target
            .members
            .iter()
            .map(|member| member.node.clone())
            .collect::<Vec<_>>();
        let set = Arc::new(crate::CapturedEditSet::capture(
            &self.current,
            &parent,
            &canonical,
            timing.clone(),
        )?);
        // Count the union's serialized payload too: one snapshot charge alone
        // does not bound repeated relationships across many closed components.
        charge(
            &mut self.captured_bytes,
            wire::size(set.as_ref(), MAX_COMPOUND_CAPTURE_BYTES)?,
            MAX_COMPOUND_CAPTURE_BYTES,
            "macro union capture byte limit",
        )?;
        let value = Arc::new(RegisterValue::EditedSet { set });
        self.writes.insert(register, value.clone());
        self.writes.insert(RegisterName::unnamed(), value.clone());
        self.steps.push(ResolvedStep::Yank {
            name: register,
            value,
        });
        if cut {
            self.context.visual_selection = None;
            for member in target.members.iter().rev() {
                self.charge_step(false)?;
                let SemanticAllocation::ParameterEdit { new_revision } =
                    (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                        step_index: self.steps.len(),
                    })?
                else {
                    return Err(invalid(
                        "a node set deletion requires a ParameterEdit allocation",
                    ));
                };
                self.reserve_revision(&new_revision)?;
                let timing = AudioTimingId {
                    allocation: new_revision.clone(),
                    ordinal: 0,
                };
                self.commit_leaf(LeafEdit::new(
                    new_revision,
                    Command::DeleteRipple {
                        node: member.node.clone(),
                        timing,
                    },
                )?)?;
            }
            self.context.cursor =
                ProjectFrame(target.members[0].range.start().0.min(self.bounds.1.0));
            // Keep a surviving containing navigation group; otherwise choose
            // the first remaining child at this boundary, preserving empties.
            let navigation = &target.members[0].navigation_child;
            self.context.selected_child = self
                .current
                .nodes()
                .contains_key(navigation)
                .then(|| navigation.clone())
                .or_else(|| {
                    self.child_ends
                        .iter()
                        .find(|(_, end)| *end >= self.context.cursor)
                        .map(|(node, _)| node.clone())
                });
            validate_context(&self.current, &self.context)?;
            validate_selection(&self.current, &self.context)?;
        }
        let trace = &mut self.trace[trace_index];
        trace.capture_set = Some(SemanticCaptureProvenance {
            timing,
            parent,
            bounds,
            scope,
            scope_labels,
        });
        trace.resolved_ranges = target.members.iter().map(|member| member.range).collect();
        trace.resolved_nodes = target.members;
        Ok(())
    }

    pub(super) fn prepare_set_paste(
        &mut self,
        parent: &NodeId,
        index: usize,
        set: &crate::CapturedEditSet,
    ) -> Result<(LeafEdit, NodeId, crate::FrameDuration), EditError> {
        let requirements = set.identity_requirements()?;
        let node_count = requirements
            .components
            .iter()
            .try_fold(1usize, |count, component| {
                count.checked_add(component.nodes)
            })
            .ok_or_else(|| limit("node set pasted identity count overflows"))?;
        if self
            .current
            .nodes()
            .len()
            .checked_add(node_count)
            .is_none_or(|count| count > crate::MAX_DOCUMENT_NODES)
            || self
                .current
                .marks()
                .len()
                .checked_add(requirements.marks)
                .is_none_or(|count| count > crate::MAX_DOCUMENT_MARKS)
        {
            return Err(limit("node set pasted identities exceed document limits"));
        }
        let SemanticAllocation::PasteEditedSet {
            new_revision,
            identities,
        } = (self.allocate)(SemanticAllocationRequest::PasteEditedSet {
            step_index: self.steps.len(),
            requirements: requirements.clone(),
        })?
        else {
            return Err(invalid(
                "node set paste requires a PasteEditedSet allocation",
            ));
        };
        if identities.components.len() != requirements.components.len()
            || identities.marks.len() != requirements.marks
            || identities
                .components
                .iter()
                .zip(&requirements.components)
                .any(|(ids, required)| {
                    ids.nodes.len() != required.nodes || ids.aliases.len() != required.aliases
                })
        {
            return Err(invalid(
                "node set paste requires exactly its preflight identity counts",
            ));
        }
        self.reserve_revision(&new_revision)?;
        self.reserve_node(&identities.group)?;
        for ids in &identities.components {
            for node in ids.nodes.iter().chain(&ids.aliases) {
                self.reserve_node(node)?;
            }
        }
        for mark in &identities.marks {
            if !self.marks.insert(mark.clone()) {
                return Err(identity("node set paste reuses a mark identity"));
            }
        }
        let selected = identities.group.clone();
        let timing = AudioTimingId {
            allocation: new_revision.clone(),
            ordinal: 0,
        };
        let edit = LeafEdit::new(
            new_revision,
            Command::SpliceEditSet {
                parent: parent.clone(),
                index,
                set: set.clone(),
                identities,
                timing,
            },
        )?;
        Ok((edit, selected, set.duration()))
    }
}
