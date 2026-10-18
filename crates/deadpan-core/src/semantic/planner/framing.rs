//! Framing intent resolves its owner and range on every staged document.

use super::*;
use crate::{SemanticVisualSelection, SemanticZoomCenter, framing_zoom};

fn unavailable(message: &str) -> EditError {
    EditError::new(EditErrorCode::SelectionUnavailable, message)
}

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: FnMut(&ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError>,
    P: FnMut(&ProjectDocument, PauseSite) -> Result<PauseProvider, EditError>,
{
    pub(super) fn zoom(
        &mut self,
        trace_index: usize,
        kind: framing_zoom::ZoomKind,
        center: SemanticZoomCenter,
    ) -> Result<(), EditError> {
        let selected = self
            .context
            .selected_child
            .clone()
            .ok_or_else(|| unavailable("select a beat before framing it"))?;
        let slot = *self.child_indices.get(&selected).ok_or_else(|| {
            unavailable("the framed beat must be a direct child of the current Sequence")
        })?;
        let start = slot
            .checked_sub(1)
            .map_or(self.bounds.0, |previous| self.child_ends[previous].1);
        let end = self.child_ends[slot].1;
        let frames = u64::try_from(
            end.0
                .checked_sub(start.0)
                .ok_or_else(|| invalid("framing owner duration overflows"))?,
        )
        .map_err(|_| invalid("framing owner has a negative duration"))?;
        if frames == 0 {
            return Err(unavailable("an empty beat has no picture to frame"));
        }
        let range = match &self.context.visual_selection {
            Some(SemanticVisualSelection::Time { anchor, head, .. }) => Some(
                FrameRange::new((*anchor).min(*head), (*anchor).max(*head))
                    .map_err(crate::DocumentError::from)?,
            ),
            Some(SemanticVisualSelection::Object { selection, .. }) => Some(
                self.current
                    .resolve_object_selection(&self.context.parent, selection)?
                    .range,
            ),
            None => None,
        };
        let local = range.map(|range| {
            if range.start() < start || range.end() > end || range.start() >= range.end() {
                return Err(unavailable("select a nonempty range inside the selected beat; framing belongs to that beat"));
            }
            Ok((u64::try_from(range.start().0 - start.0).map_err(|_| invalid("negative local framing In"))?,
                u64::try_from(range.end().0 - start.0).map_err(|_| invalid("negative local framing Out"))?))
        }).transpose()?;
        let terminal = self
            .current
            .duration()?
            .frames()
            .checked_sub(1)
            .ok_or_else(|| unavailable("the edit has no picture to frame"))?;
        let site = SemanticFramingSite {
            node: selected.clone(),
            frame: ProjectFrame(self.context.cursor.0.min(terminal)),
            original_center: center == SemanticZoomCenter::OriginalCenter
                && kind != framing_zoom::ZoomKind::Off,
        };
        let state = (self.resolve_framing)(&self.current, &site)?;
        state
            .current
            .validate()
            .map_err(|error| invalid(&error.to_string()))?;
        let resolved_center = if site.original_center {
            framing_zoom::Center::Point(state.original_center.ok_or_else(|| {
                unavailable("the Original picture center is clipped before the framed owner")
            })?)
        } else {
            framing_zoom::Center::Keep
        };
        let entry = self.current.nodes()[&selected].framing.as_ref();
        let framing = framing_zoom::build(
            kind,
            &resolved_center,
            &framing_zoom::Shot {
                entry,
                current: state.current,
                frames,
                range: local,
                explicit: center == SemanticZoomCenter::OriginalCenter,
            },
        )
        .map_err(|message| EditError::new(EditErrorCode::InvalidCommand, message))?;
        self.trace[trace_index].resolved_parent = Some(self.context.parent.clone());
        self.trace[trace_index].resolved_selection = Some(SliceCaptureSelection::Child {
            node: selected.clone(),
        });
        self.trace[trace_index].resolved_range =
            Some(range.unwrap_or(FrameRange::new(start, end).map_err(crate::DocumentError::from)?));
        if framing.as_ref() == entry {
            return Ok(());
        }
        self.charge_step(false)?;
        let SemanticAllocation::ParameterEdit { new_revision } =
            (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                step_index: self.steps.len(),
            })?
        else {
            return Err(invalid("macro zoom requires a parameter allocation"));
        };
        self.reserve_revision(&new_revision)?;
        self.commit_leaf(LeafEdit::new(
            new_revision,
            Command::SetFraming {
                node: selected,
                framing,
            },
        )?)
    }
}
