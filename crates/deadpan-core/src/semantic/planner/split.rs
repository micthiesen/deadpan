//! `s`/`:split` as a semantic instruction, so a macro records it and `.`
//! repeats it: the same `Split` the native edit authors, resolved against the
//! staged document at the cursor inside the selected direct child.

use super::*;

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: FnMut(&ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError>,
    P: FnMut(&ProjectDocument, super::PauseSite) -> Result<super::PauseProvider, EditError>,
{
    pub(super) fn split(&mut self, trace_index: usize) -> Result<(), EditError> {
        if self.context.visual_selection.is_some() {
            return Err(invalid(
                "clear the Visual selection before splitting a beat",
            ));
        }
        let selected = self.context.selected_child.clone().ok_or_else(|| {
            EditError::new(
                EditErrorCode::SelectionUnavailable,
                "select a beat before splitting it",
            )
        })?;
        let Some(&slot) = self.child_indices.get(&selected) else {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "the beat must be a direct child of the current Sequence",
            ));
        };
        let start = slot
            .checked_sub(1)
            .map_or(self.bounds.0, |previous| self.child_ends[previous].1);
        let end = self.child_ends[slot].1;
        let cursor = self.context.cursor;
        if cursor.0 <= start.0 || cursor.0 >= end.0 {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "move inside the selected beat to split it; existing boundaries need no split",
            ));
        }
        let at =
            crate::FrameDuration::new(cursor.0 - start.0).map_err(crate::DocumentError::from)?;
        // The native pool: one identity per node of the beat, plus wrappers.
        let mut pending = vec![selected.clone()];
        let mut nodes = 3_usize;
        while let Some(id) = pending.pop() {
            nodes = nodes
                .checked_add(1)
                .filter(|count| *count <= crate::MAX_DOCUMENT_NODES)
                .ok_or_else(|| limit("split exceeds the document node limit"))?;
            pending.extend(self.current.children(&id).cloned());
        }
        self.charge_step(false)?;
        let SemanticAllocation::Split {
            new_revision,
            identities,
        } = (self.allocate)(SemanticAllocationRequest::Split {
            step_index: self.steps.len(),
            nodes,
        })?
        else {
            return Err(invalid("a split requires a Split allocation"));
        };
        self.reserve_revision(&new_revision)?;
        for node in &identities.nodes {
            self.reserve_node(node)?;
        }
        self.commit_leaf(LeafEdit::new(
            new_revision,
            Command::Split {
                node: selected,
                at,
                identities,
            },
        )?)?;
        // Select the fragment that starts at the cursor.
        let right = self
            .child_ends
            .iter()
            .enumerate()
            .find(|(slot, _)| {
                slot.checked_sub(1)
                    .map_or(self.bounds.0, |previous| self.child_ends[previous].1)
                    == cursor
            })
            .map(|(_, (node, _))| node.clone())
            .ok_or_else(|| invalid("the split left no fragment at the cursor"))?;
        let right_end = self.child_ends[self.child_indices[&right]].1;
        self.context.selected_child = Some(right.clone());
        self.trace[trace_index].resolved_parent = Some(self.context.parent.clone());
        self.trace[trace_index].resolved_selection =
            Some(SliceCaptureSelection::Child { node: right });
        self.trace[trace_index].resolved_range =
            Some(FrameRange::new(cursor, right_end).map_err(crate::DocumentError::from)?);
        Ok(())
    }
}
