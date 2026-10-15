//! `,v`/`:trim` as a semantic instruction: the native combined Trim's
//! accepted In/Out/Slip/Roll values, resolved against the selected Source
//! and its literal right sibling in the staged document.

use super::*;
use crate::{AudioTimingId, SourceTrimCapture, SourceTrimIntent, SourceTrimResources};

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: FnMut(&ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError>,
    P: FnMut(&ProjectDocument, super::PauseSite) -> Result<super::PauseProvider, EditError>,
{
    pub(super) fn trim(
        &mut self,
        trace_index: usize,
        intent: SourceTrimIntent,
    ) -> Result<(), EditError> {
        if self.context.visual_selection.is_some() {
            return Err(invalid("clear the Visual selection before trimming a beat"));
        }
        if intent.is_zero() {
            return Err(invalid("a trim must change In, Out, Slip or Roll"));
        }
        let selected = self.context.selected_child.clone().ok_or_else(|| {
            EditError::new(
                EditErrorCode::SelectionUnavailable,
                "select a beat before trimming it",
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
        let right = self.child_ends.get(slot + 1).map(|(node, _)| node.clone());
        let parent = self.context.parent.clone();
        let resolved = self
            .current
            .source_trim_edit(&parent, &selected, right.as_ref(), intent)?;
        self.charge_step(false)?;
        let SemanticAllocation::Trim {
            new_revision,
            target_wrapper,
            right_wrapper,
            split,
            fillers,
        } = (self.allocate)(SemanticAllocationRequest::Trim {
            step_index: self.steps.len(),
            target_wrapper: resolved.required_target_wrapper,
            right_wrapper: resolved.required_right_wrapper,
            split_nodes: resolved.required_split_nodes,
            filler_nodes: resolved.required_filler_nodes,
        })?
        else {
            return Err(invalid("a trim requires a Trim allocation"));
        };
        self.reserve_revision(&new_revision)?;
        for node in target_wrapper
            .iter()
            .chain(&right_wrapper)
            .chain(&split.nodes)
            .chain(&fillers)
        {
            self.reserve_node(node)?;
        }
        let timing = (resolved.capture != SourceTrimCapture::None).then(|| AudioTimingId {
            allocation: new_revision.clone(),
            ordinal: 0,
        });
        self.commit_leaf(LeafEdit::new(
            new_revision,
            Command::ApplySourceTrim {
                parent,
                node: selected,
                right,
                intent,
                resources: SourceTrimResources {
                    target_wrapper,
                    right_wrapper,
                    split,
                    fillers,
                    timing,
                },
            },
        )?)?;
        // The trimmed beat (or its new crop) keeps its slot and its start.
        let (result, end) = self.child_ends[slot].clone();
        self.context.cursor = start;
        self.context.selected_child = Some(result.clone());
        self.trace[trace_index].resolved_parent = Some(self.context.parent.clone());
        self.trace[trace_index].resolved_selection =
            Some(SliceCaptureSelection::Child { node: result });
        self.trace[trace_index].resolved_range =
            Some(FrameRange::new(start, end).map_err(crate::DocumentError::from)?);
        Ok(())
    }
}
