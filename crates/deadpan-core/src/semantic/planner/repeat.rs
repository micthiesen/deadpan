//! Repeat resolves its selector once on the staged document and authors one
//! leaf. It neither reads nor writes a copied-content register.

use super::*;

impl<F, R> Planner<'_, F, R>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
{
    pub(super) fn set_repeat_plays(
        &mut self,
        trace_index: usize,
        plays: u32,
    ) -> Result<(), EditError> {
        if self.context.visual_selection.is_some() {
            return Err(invalid(
                "clear the Visual selection before setting Repeat total plays",
            ));
        }
        let selected = self.context.selected_child.clone().ok_or_else(|| {
            EditError::new(
                EditErrorCode::SelectionUnavailable,
                "select an existing Repeat before setting total plays",
            )
        })?;
        let slot = self.child_indices.get(&selected).copied().ok_or_else(|| {
            EditError::new(
                EditErrorCode::SelectionUnavailable,
                "the selected Repeat must be a direct child of the current Sequence",
            )
        })?;
        if !matches!(
            self.current.nodes()[&selected].kind,
            NodeKind::Repeat { .. }
        ) {
            return Err(EditError::new(
                EditErrorCode::WrongNodeKind,
                "set-repeat-plays requires an existing Repeat",
            ));
        }
        let start = slot
            .checked_sub(1)
            .map_or(self.bounds.0, |previous| self.child_ends[previous].1);
        let range =
            FrameRange::new(start, self.child_ends[slot].1).map_err(crate::DocumentError::from)?;
        self.charge_step(false)?;
        let allocation = (self.allocate)(SemanticAllocationRequest::SetRepeatPlays {
            step_index: self.steps.len(),
        })?;
        let SemanticAllocation::SetRepeatPlays { new_revision } = allocation else {
            return Err(invalid(
                "macro count setting requires a SetRepeatPlays allocation",
            ));
        };
        self.reserve_revision(&new_revision)?;
        let timing = AudioTimingId {
            allocation: new_revision.clone(),
            ordinal: 0,
        };
        // Even an unchanged count authors the supplied fresh revision, matching
        // the ordinary setter. Existing Repeat identities and clocks survive.
        let edit = LeafEdit::new(
            new_revision,
            Command::SetRepeatPlays {
                node: selected.clone(),
                plays,
                timing,
            },
        )?;
        let applied = crate::apply(&self.current, &edit.request(&self.current))?;
        let next = applied.forward.apply(&self.current)?;
        charge(
            &mut self.document_bytes,
            wire::size(&next, MAX_DOCUMENT_JSON_BYTES)?,
            MAX_COMPOUND_DOCUMENT_BYTES,
            "macro staged document byte limit",
        )?;
        self.current = next;
        self.steps.push(ResolvedStep::Edit { edit });
        self.context.cursor = start;
        self.context.selected_child = Some(selected.clone());
        self.refresh_children()?;
        self.trace[trace_index].resolved_selection =
            Some(SliceCaptureSelection::Child { node: selected });
        self.trace[trace_index].resolved_range = Some(range);
        self.trace[trace_index].resolved_parent = Some(self.context.parent.clone());
        Ok(())
    }

    pub(super) fn repeat(
        &mut self,
        trace_index: usize,
        selector: SemanticSelector,
        plays: u32,
    ) -> Result<(), EditError> {
        self.charge_step(false)?;
        let target = self.resolve_selector(selector)?;
        let selection = target.selection()?.clone();
        let plan = self
            .current
            .repeat_selection(&target.parent, &selection, plays)?;
        let allocation = (self.allocate)(SemanticAllocationRequest::Repeat {
            step_index: self.steps.len(),
            required_split_ids: plan.required_split_ids,
            needs_group: plan.needs_group,
        })?;
        let SemanticAllocation::Repeat {
            new_revision,
            identities,
        } = allocation
        else {
            return Err(invalid("macro repeat requires a Repeat allocation"));
        };
        if identities.group.is_some() != plan.needs_group
            || identities.split.nodes.len() != plan.required_split_ids
        {
            return Err(invalid(
                "macro Repeat requires its exact group and Split identities",
            ));
        }
        self.reserve_revision(&new_revision)?;
        for node in std::iter::once(&identities.repeat)
            .chain(identities.group.iter())
            .chain(&identities.split.nodes)
        {
            self.reserve_node(node)?;
        }
        let selected = identities.repeat.clone();
        let timing = AudioTimingId {
            allocation: new_revision.clone(),
            ordinal: 0,
        };
        let edit = LeafEdit::new(
            new_revision,
            Command::RepeatSelection {
                parent: target.parent.clone(),
                selection: selection.clone(),
                plays,
                identities,
                timing,
            },
        )?;
        let applied = crate::apply(&self.current, &edit.request(&self.current))?;
        let next = applied.forward.apply(&self.current)?;
        charge(
            &mut self.document_bytes,
            wire::size(&next, MAX_DOCUMENT_JSON_BYTES)?,
            MAX_COMPOUND_DOCUMENT_BYTES,
            "macro staged document byte limit",
        )?;
        self.current = next;
        self.steps.push(ResolvedStep::Edit { edit });
        self.continue_target(&target, plan.range.start(), Some(selected))?;
        self.trace[trace_index].resolved_parent = Some(target.parent);
        self.trace[trace_index].resolved_selection = Some(selection);
        self.trace[trace_index].resolved_range = Some(plan.range);
        Ok(())
    }
}
