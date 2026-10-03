//! Oriented Visual selection and direct-child motions on the staged scope.

use super::*;

pub(super) fn validate_context(
    context: &SemanticContext,
    bounds: (ProjectFrame, ProjectFrame),
) -> Result<(), EditError> {
    let inside = |at| bounds.0 <= at && at <= bounds.1;
    if !inside(context.cursor) {
        return Err(unavailable(
            "the macro cursor is outside its current Sequence",
        ));
    }
    if let Some(selection) = &context.visual_selection {
        if !inside(selection.anchor) || !inside(selection.head) {
            return Err(unavailable(
                "the macro Visual endpoints are outside their current Sequence",
            ));
        }
        if selection.extending && selection.head != context.cursor {
            return Err(unavailable(
                "an extending macro Visual selection must end at its cursor",
            ));
        }
    }
    Ok(())
}

impl<F, R> Planner<'_, F, R>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
{
    pub(super) fn visual_range(&self) -> Result<FrameRange, EditError> {
        let selection = self.context.visual_selection.as_ref().ok_or_else(|| {
            unavailable("select a Visual range before using this macro instruction")
        })?;
        if selection.anchor == selection.head {
            return Err(unavailable("the macro Visual selection is empty"));
        }
        FrameRange::new(
            selection.anchor.min(selection.head),
            selection.anchor.max(selection.head),
        )
        .map_err(crate::DocumentError::from)
        .map_err(Into::into)
    }

    pub(super) fn finish_selection(&mut self) -> Result<(), EditError> {
        self.context
            .visual_selection
            .as_mut()
            .ok_or_else(|| unavailable("there is no macro Visual selection to finish"))?
            .extending = false;
        Ok(())
    }

    pub(super) fn extend_selection(&mut self) {
        if let Some(selection) = &mut self.context.visual_selection
            && selection.extending
        {
            selection.head = self.context.cursor;
        }
    }

    pub(super) fn resolve_selector(
        &self,
        selector: SemanticSelector,
    ) -> Result<SliceCaptureSelection, EditError> {
        match selector {
            SemanticSelector::SelectedBeat => {
                let node =
                    self.context.selected_child.clone().ok_or_else(|| {
                        unavailable("select a beat before using this macro selector")
                    })?;
                validate_selection(&self.current, &self.context)?;
                Ok(SliceCaptureSelection::Child { node })
            }
            SemanticSelector::VisualSelection => Ok(SliceCaptureSelection::Range {
                range: self.visual_range()?,
            }),
            SemanticSelector::Motion { motion } => {
                let (destination, _) = self.motion_target(motion);
                let cursor = self.context.cursor;
                if destination == cursor {
                    return Err(unavailable("the macro motion selection is empty"));
                }
                let range = FrameRange::new(cursor.min(destination), cursor.max(destination))
                    .map_err(crate::DocumentError::from)?;
                Ok(SliceCaptureSelection::Range { range })
            }
        }
    }

    pub(super) fn move_context(&mut self, motion: SemanticMotion) {
        let (cursor, child) = self.motion_target(motion);
        self.context.cursor = cursor;
        self.context.selected_child = child;
        self.extend_selection();
    }

    fn motion_target(&self, motion: SemanticMotion) -> (ProjectFrame, Option<NodeId>) {
        let cursor = match motion {
            SemanticMotion::Frames { forward, count } => {
                // Clamp the distance before adding so large counts cannot
                // overflow the absolute Edit boundary.
                let cursor = self.context.cursor.0;
                let distance = if forward {
                    self.bounds.1.0 - cursor
                } else {
                    cursor - self.bounds.0.0
                };
                let amount = distance.min(i64::from(count.get()));
                ProjectFrame(if forward {
                    cursor + amount
                } else {
                    cursor - amount
                })
            }
            SemanticMotion::Beats { forward, count } => {
                return self.beat_target(forward, count.get());
            }
            SemanticMotion::Scope { end } => {
                if end {
                    self.bounds.1
                } else {
                    self.bounds.0
                }
            }
        };
        (
            cursor,
            selected_child(&self.child_ends, cursor, self.bounds),
        )
    }

    fn beat_target(&self, forward: bool, count: u32) -> (ProjectFrame, Option<NodeId>) {
        let current = self.context.selected_child.as_ref().or_else(|| {
            let index = if self.context.cursor == self.bounds.1 {
                self.child_ends.len().checked_sub(1)?
            } else {
                self.child_ends
                    .partition_point(|(_, end)| *end <= self.context.cursor)
            };
            self.child_ends.get(index).map(|(node, _)| node)
        });
        let Some(index) = current
            .and_then(|node| self.child_indices.get(node))
            .copied()
        else {
            return (self.context.cursor, self.context.selected_child.clone());
        };
        let amount = usize::try_from(count).unwrap_or(usize::MAX);
        let next = if forward {
            index.saturating_add(amount).min(self.child_ends.len() - 1)
        } else {
            index.saturating_sub(amount)
        };
        let cursor = next
            .checked_sub(1)
            .map_or(self.bounds.0, |previous| self.child_ends[previous].1);
        (cursor, Some(self.child_ends[next].0.clone()))
    }

    pub(super) fn refresh_children(&mut self) -> Result<(), EditError> {
        self.bounds = scope_bounds(&self.current, &self.context.parent)?;
        self.child_ends = child_ends(&self.current, &self.context.parent, self.bounds)?;
        self.child_indices = child_indices(&self.child_ends);
        Ok(())
    }
}

fn unavailable(message: &str) -> EditError {
    EditError::new(EditErrorCode::SelectionUnavailable, message)
}
