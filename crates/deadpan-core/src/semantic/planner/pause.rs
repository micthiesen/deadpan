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
    P: FnMut(&ProjectDocument, super::PauseSite) -> Result<super::PauseProvider, EditError>,
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
            (self.resolve_pause)(&self.current, super::PauseSite::Boundary { at })?
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

    /// `SetRoomTone`: the selected Hold loops the exact audio of an Original
    /// moment from a register, as the native room-tone sheet authors it.
    pub(super) fn set_room_tone(
        &mut self,
        trace_index: usize,
        register: crate::RegisterName,
    ) -> Result<(), EditError> {
        if self.context.visual_selection.is_some() {
            return Err(invalid(
                "clear the Visual selection before choosing room tone",
            ));
        }
        let selected = self.context.selected_child.clone().ok_or_else(|| {
            EditError::new(
                EditErrorCode::SelectionUnavailable,
                "select a pause before choosing its room tone",
            )
        })?;
        if !self.child_indices.contains_key(&selected) {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "the pause must be a direct child of the current Sequence",
            ));
        }
        if !matches!(self.current.nodes()[&selected].kind, NodeKind::Hold { .. }) {
            return Err(EditError::new(
                EditErrorCode::WrongNodeKind,
                "room tone applies to a pause (Hold)",
            ));
        }
        let value = if let Some(value) = self.writes.get(&register) {
            value.clone()
        } else {
            let value = self.bank.get(&register).cloned();
            self.inputs.insert(register, value.clone());
            value.ok_or_else(|| invalid("the room-tone register is empty"))?
        };
        if !matches!(value.as_ref(), RegisterValue::Original { .. }) {
            return Err(invalid(
                "room tone needs a copied Original moment in its register",
            ));
        }
        let source = (self.resolve_original)(&self.current, value.as_ref())?;
        let audio = copied_moment_audio(&source)?;
        self.charge_step(false)?;
        let SemanticAllocation::ParameterEdit { new_revision } =
            (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                step_index: self.steps.len(),
            })?
        else {
            return Err(invalid("room tone requires a parameter allocation"));
        };
        self.reserve_revision(&new_revision)?;
        let edit = LeafEdit::new(
            new_revision,
            Command::SetHoldAudio {
                node: selected.clone(),
                audio: HoldAudio::RoomTone { source: audio },
            },
        )?;
        self.commit_leaf(edit)?;
        self.trace[trace_index].resolved_parent = Some(self.context.parent.clone());
        self.trace[trace_index].resolved_selection =
            Some(SliceCaptureSelection::Child { node: selected });
        Ok(())
    }

    /// Apply one leaf to the staged document within the byte limits.
    pub(super) fn commit_leaf(&mut self, edit: LeafEdit) -> Result<(), EditError> {
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

/// The exact source samples heard inside a copied Original moment: its picture
/// selection mapped through the exact audio placement, In rounded up and Out
/// rounded down, intersected with the measured audio. This equals converting
/// the measured picture PTS straight to samples; no project frame is rounded.
pub fn copied_moment_audio(source: &SourceNode) -> Result<crate::SourceAudio, EditError> {
    let unavailable = || {
        EditError::new(
            EditErrorCode::SelectionUnavailable,
            "this Original moment has no measured audio for room tone",
        )
    };
    let audio = source.audio.as_ref().ok_or_else(unavailable)?;
    // Copied moments carry their exact placement in the mapping, never in
    // the independent mix offset.
    if source.audio_offset.0 != 0 {
        return Err(unavailable());
    }
    // Room tone endpoints are whole source samples: the span must count
    // samples (a 1/rate time base), not some coarser or finer tick.
    if audio.span.start().time_base.numerator() != 1
        || audio.span.end().time_base != audio.span.start().time_base
    {
        return Err(unavailable());
    }
    let window = source.edit_window.ok_or_else(unavailable)?;
    // The heard part is the editorial window, further limited by an explicit
    // audible selection on the complete measured mapping. Both are local
    // frames; with no mix offset they share one clock.
    let (start, frames, (window_start, window_end)) = match source.audio_mapping {
        crate::SourceAudioMapping::Placement { start, frames } => {
            (start, frames, (window.start(), window.end()))
        }
        crate::SourceAudioMapping::SelectedPlacement {
            start,
            frames,
            selection,
        } => (
            start,
            frames,
            (
                if selection.start.compare(window.start()).is_gt() {
                    selection.start
                } else {
                    window.start()
                },
                if selection.end.compare(window.end()).is_lt() {
                    selection.end
                } else {
                    window.end()
                },
            ),
        ),
        _ => return Err(unavailable()),
    };
    let first = audio.span.start().ticks;
    let last = audio.span.end().ticks;
    let length =
        crate::ExactRatio::integer(last.checked_sub(first).ok_or_else(|| {
            EditError::new(EditErrorCode::TimingOverflow, "audio span overflows")
        })?);
    let overflow = |_| EditError::new(EditErrorCode::TimingOverflow, "room tone range overflows");
    let sample = |local: crate::ExactRatio| -> Result<crate::ExactRatio, EditError> {
        local
            .checked_sub(start)
            .and_then(|offset| offset.checked_mul(length))
            .and_then(|scaled| scaled.checked_div(frames))
            .and_then(|scaled| scaled.checked_add(crate::ExactRatio::integer(first)))
            .map_err(overflow)
    };
    let begin = i64::try_from(sample(window_start)?.ceil().map_err(overflow)?)
        .map_err(|_| unavailable())?
        .max(first);
    let end = i64::try_from(sample(window_end)?.floor())
        .map_err(|_| unavailable())?
        .min(last);
    if begin >= end {
        return Err(unavailable());
    }
    let time_base = audio.span.start().time_base;
    Ok(crate::SourceAudio {
        asset: audio.asset.clone(),
        span: crate::SourceSpan::new(
            crate::SourceTimestamp {
                ticks: begin,
                time_base,
            },
            crate::SourceTimestamp {
                ticks: end,
                time_base,
            },
        )
        .map_err(EditError::from)?,
    })
}
