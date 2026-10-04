//! Grouping changes only structural ownership. Semantic selectors are resolved
//! once against the current staged Sequence and never retained as absolute IDs.

use super::*;

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: FnMut(&ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError>,
    P: FnMut(&ProjectDocument, ProjectFrame) -> Result<super::PauseProvider, EditError>,
{
    pub(super) fn group(
        &mut self,
        trace_index: usize,
        selector: SemanticSelector,
        label: &str,
    ) -> Result<(), EditError> {
        crate::validate_group_label(label)?;
        let target = self.resolve_selector(selector)?;
        let selection = target.selection()?.clone();
        let plan = self.current.group_selection(&target.parent, &selection)?;
        self.charge_step(false)?;
        let allocation = (self.allocate)(SemanticAllocationRequest::Group {
            step_index: self.steps.len(),
            required_split_ids: plan.required_split_ids,
        })?;
        let SemanticAllocation::Group {
            new_revision,
            identities,
        } = allocation
        else {
            return Err(invalid("macro grouping requires a Group allocation"));
        };
        if identities.split.nodes.len() != plan.required_split_ids {
            return Err(invalid("macro Group requires its exact Split identities"));
        }
        self.reserve_revision(&new_revision)?;
        for node in std::iter::once(&identities.group).chain(&identities.split.nodes) {
            self.reserve_node(node)?;
        }
        let selected = identities.group.clone();
        let timing = AudioTimingId {
            allocation: new_revision.clone(),
            ordinal: 0,
        };
        let edit = LeafEdit::new(
            new_revision,
            Command::GroupSelection {
                parent: target.parent.clone(),
                selection: selection.clone(),
                label: label.into(),
                identities,
                timing,
            },
        )?;
        self.apply_group_step(edit)?;
        self.continue_target(&target, plan.range.start(), Some(selected))?;
        self.trace[trace_index].resolved_parent = Some(target.parent);
        self.trace[trace_index].resolved_range = Some(plan.range);
        self.trace[trace_index].resolved_selection = Some(selection);
        Ok(())
    }

    pub(super) fn ungroup(&mut self, trace_index: usize) -> Result<(), EditError> {
        if self.context.visual_selection.is_some() {
            return Err(invalid(
                "clear the Visual selection before ungrouping a selected Sequence",
            ));
        }
        let selected = self.context.selected_child.clone().ok_or_else(|| {
            EditError::new(
                EditErrorCode::SelectionUnavailable,
                "select a Sequence to ungroup",
            )
        })?;
        let (slot, range) =
            crate::group_selection::child_range(&self.current, &self.context.parent, &selected)?;
        if !matches!(
            self.current.nodes()[&selected].kind,
            NodeKind::Sequence { .. }
        ) {
            return Err(EditError::new(
                EditErrorCode::WrongNodeKind,
                "ungroup requires a Sequence",
            ));
        }
        self.charge_step(false)?;
        let allocation = (self.allocate)(SemanticAllocationRequest::Ungroup {
            step_index: self.steps.len(),
        })?;
        let SemanticAllocation::Ungroup { new_revision } = allocation else {
            return Err(invalid("macro ungrouping requires an Ungroup allocation"));
        };
        self.reserve_revision(&new_revision)?;
        let edit = LeafEdit::new(
            new_revision,
            Command::Ungroup {
                node: selected.clone(),
            },
        )?;
        self.apply_group_step(edit)?;
        self.refresh_children()?;
        // The same slot is the first promoted child, or the following sibling
        // for an empty group. Preserve independently visitable empty siblings.
        self.context.selected_child = self
            .child_ends
            .get(slot)
            .or_else(|| self.child_ends.last())
            .map(|(node, _)| node.clone());
        self.context.cursor = range.start();
        self.trace[trace_index].resolved_range = Some(range);
        self.trace[trace_index].resolved_parent = Some(self.context.parent.clone());
        self.trace[trace_index].resolved_selection =
            Some(SliceCaptureSelection::Child { node: selected });
        Ok(())
    }

    fn apply_group_step(&mut self, edit: LeafEdit) -> Result<(), EditError> {
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
        Ok(())
    }
}
