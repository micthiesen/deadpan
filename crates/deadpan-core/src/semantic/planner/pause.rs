//! Pause insertion and framing replacement: the macro forms of `,h`/`:hold`
//! and `,c`/`,z`. The host resolves a pause's frozen picture; the planner
//! authors the same InsertTime and SetFraming commands as the native edits.

use super::*;
use crate::{AudioTimingId, Framing, HoldAudio, HoldRecipe};

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: FnMut(&ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError>,
    P: FnMut(&ProjectDocument, ProjectFrame) -> Result<super::PauseProvider, EditError>,
{
    pub(super) fn insert_pause(
        &mut self,
        trace_index: usize,
        length: crate::PauseLength,
        black: bool,
    ) -> Result<(), EditError> {
        if self.context.visual_selection.is_some() {
            return Err(invalid(
                "clear the Visual selection before inserting a pause",
            ));
        }
        let duration = length.resolve(self.current.presentation_basis().frame_rate)?;
        let at = self.context.cursor;
        let target = self.current.insert_time_target(at)?;
        if !self.within_scope(&target.parent) {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "this boundary belongs to an enclosing group; insert the pause from that group",
            ));
        }
        // Black punctuation samples no picture and captures no framing.
        let provider = if black {
            super::PauseProvider {
                video: crate::HoldVideo::Background,
                picture_context: None,
            }
        } else {
            (self.resolve_pause)(&self.current, at)?
        };
        self.charge_step(false)?;
        let required = target.split.as_ref().map_or(0, |split| split.required_ids);
        let SemanticAllocation::InsertPause {
            new_revision,
            id,
            split,
        } = (self.allocate)(SemanticAllocationRequest::InsertPause {
            step_index: self.steps.len(),
            required_split_ids: required,
        })?
        else {
            return Err(invalid("macro pause requires a pause allocation"));
        };
        if split.nodes.len() != required {
            return Err(invalid("macro pause requires its exact Split identities"));
        }
        self.reserve_revision(&new_revision)?;
        for node in std::iter::once(&id).chain(&split.nodes) {
            self.reserve_node(node)?;
        }
        let edit = LeafEdit::new(
            new_revision.clone(),
            Command::InsertTime {
                at,
                hold: HoldRecipe {
                    duration,
                    video: provider.video,
                    audio: HoldAudio::Silence,
                    picture_context: provider.picture_context,
                },
                id: id.clone(),
                identities: split,
                timing: AudioTimingId {
                    allocation: new_revision,
                    ordinal: 0,
                },
            },
        )?;
        self.commit_leaf(edit)?;
        // The cursor stays at the pause; a hidden Hold selects its visible
        // enclosing child, as natively.
        self.context.cursor = at;
        self.context.selected_child = if self.child_indices.contains_key(&id) {
            Some(id)
        } else {
            selected_child(&self.child_ends, at, self.bounds)
        };
        let end =
            ProjectFrame(at.0.checked_add(duration.frames()).ok_or_else(|| {
                EditError::new(EditErrorCode::TimingOverflow, "pause end overflows")
            })?);
        self.trace[trace_index].resolved_parent = Some(target.parent);
        self.trace[trace_index].resolved_range =
            Some(FrameRange::new(at, end).map_err(crate::DocumentError::from)?);
        Ok(())
    }

    pub(super) fn set_framing(
        &mut self,
        trace_index: usize,
        framing: Option<Framing>,
    ) -> Result<(), EditError> {
        if self.context.visual_selection.is_some() {
            return Err(invalid("clear the Visual selection before framing a beat"));
        }
        let selected = self.context.selected_child.clone().ok_or_else(|| {
            EditError::new(
                EditErrorCode::SelectionUnavailable,
                "select a beat before framing it",
            )
        })?;
        if !self.child_indices.contains_key(&selected) {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "the framed beat must be a direct child of the current Sequence",
            ));
        }
        self.charge_step(false)?;
        let SemanticAllocation::ParameterEdit { new_revision } =
            (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                step_index: self.steps.len(),
            })?
        else {
            return Err(invalid("macro framing requires a parameter allocation"));
        };
        self.reserve_revision(&new_revision)?;
        let edit = LeafEdit::new(
            new_revision,
            Command::SetFraming {
                node: selected.clone(),
                framing,
            },
        )?;
        self.commit_leaf(edit)?;
        self.trace[trace_index].resolved_parent = Some(self.context.parent.clone());
        self.trace[trace_index].resolved_selection =
            Some(SliceCaptureSelection::Child { node: selected });
        Ok(())
    }

    /// Apply one leaf to the staged document within the byte limits.
    fn commit_leaf(&mut self, edit: LeafEdit) -> Result<(), EditError> {
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
        self.refresh_children()
    }

    /// Whether `node` is the current Sequence or lies inside it.
    fn within_scope(&self, node: &NodeId) -> bool {
        let mut current = node.clone();
        for _ in 0..=crate::MAX_DOCUMENT_DEPTH {
            if current == self.context.parent {
                return true;
            }
            match self.current.parent_of(&current) {
                Some(parent) => current = parent,
                None => return false,
            }
        }
        false
    }
}
