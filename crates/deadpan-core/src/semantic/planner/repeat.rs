//! Repeat resolves its selector once on the staged document and authors one
//! leaf. It neither reads nor writes a copied-content register.

use super::*;

impl<F, R> Planner<'_, F, R>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
{
    pub(super) fn repeat(
        &mut self,
        trace_index: usize,
        selector: SemanticSelector,
        plays: u32,
    ) -> Result<(), EditError> {
        self.charge_step(false)?;
        let selection = self.resolve_selector(selector)?;
        let plan = self
            .current
            .repeat_selection(&self.context.parent, &selection, plays)?;
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
                parent: self.context.parent.clone(),
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
        self.context.cursor = plan.range.start();
        self.context.selected_child = Some(selected);
        self.context.visual_selection = None;
        self.refresh_children()?;
        self.trace[trace_index].resolved_selection = Some(selection);
        self.trace[trace_index].resolved_range = Some(plan.range);
        Ok(())
    }
}
