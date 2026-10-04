//! Visual ownership, exact selector targets and motions on the staged scope.

use super::*;

pub(super) struct ResolvedTarget {
    pub parent: NodeId,
    pub selection: Option<SliceCaptureSelection>,
    pub range: FrameRange,
    navigation_parent: NodeId,
    retained_group: Option<NodeId>,
}

impl ResolvedTarget {
    pub(super) fn local(
        parent: &NodeId,
        selection: SliceCaptureSelection,
        range: FrameRange,
    ) -> Self {
        Self {
            parent: parent.clone(),
            selection: Some(selection),
            range,
            navigation_parent: parent.clone(),
            retained_group: None,
        }
    }

    pub(super) fn selection(&self) -> Result<&SliceCaptureSelection, EditError> {
        self.selection
            .as_ref()
            .ok_or_else(|| unavailable("the selected group has no child contents"))
    }
}

pub(super) fn validate_context(
    document: &ProjectDocument,
    context: &SemanticContext,
    bounds: (ProjectFrame, ProjectFrame),
) -> Result<(), EditError> {
    let inside = |at| bounds.0 <= at && at <= bounds.1;
    if !inside(context.cursor) {
        return Err(unavailable(
            "the macro cursor is outside its current Sequence",
        ));
    }
    match &context.visual_selection {
        Some(SemanticVisualSelection::Time {
            anchor,
            head,
            extending,
        }) => {
            if !inside(*anchor) || !inside(*head) {
                return Err(unavailable(
                    "the macro Visual endpoints are outside their current Sequence",
                ));
            }
            if *extending && *head != context.cursor {
                return Err(unavailable(
                    "an extending macro Visual selection must end at its cursor",
                ));
            }
        }
        Some(SemanticVisualSelection::Object {
            selection,
            extending,
        }) => {
            let target = document.resolve_object_selection(&context.parent, selection)?;
            if *extending && target.range.end() != context.cursor {
                return Err(unavailable(
                    "an extending group object must end at its cursor",
                ));
            }
        }
        None => {}
    }
    Ok(())
}

impl<F, R, S> Planner<'_, F, R, S>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: FnMut(&ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError>,
{
    pub(super) fn finish_selection(&mut self) -> Result<(), EditError> {
        match self
            .context
            .visual_selection
            .as_mut()
            .ok_or_else(|| unavailable("there is no macro Visual selection to finish"))?
        {
            SemanticVisualSelection::Time { extending, .. }
            | SemanticVisualSelection::Object { extending, .. } => *extending = false,
        }
        Ok(())
    }

    pub(super) fn resolve_selector(
        &self,
        selector: SemanticSelector,
    ) -> Result<ResolvedTarget, EditError> {
        let selection = match selector {
            SemanticSelector::SelectedBeat => {
                let node =
                    self.context.selected_child.clone().ok_or_else(|| {
                        unavailable("select a beat before using this macro selector")
                    })?;
                validate_selection(&self.current, &self.context)?;
                SliceCaptureSelection::Child { node }
            }
            SemanticSelector::TextObject { object } => {
                let object = self.current.resolve_group_object(&self.context, object)?;
                return self.object_target(&object);
            }
            SemanticSelector::VisualSelection => {
                match self.context.visual_selection.as_ref().ok_or_else(|| {
                    unavailable("select a Visual range before using this macro instruction")
                })? {
                    SemanticVisualSelection::Object { selection, .. } => {
                        return self.object_target(selection);
                    }
                    SemanticVisualSelection::Time { anchor, head, .. } => {
                        if anchor == head {
                            return Err(unavailable("the macro Visual selection is empty"));
                        }
                        SliceCaptureSelection::Range {
                            range: FrameRange::new((*anchor).min(*head), (*anchor).max(*head))
                                .map_err(crate::DocumentError::from)?,
                        }
                    }
                }
            }
            SemanticSelector::Speech { object } => SliceCaptureSelection::Range {
                range: self.speech()?.object_range(
                    self.context.cursor,
                    self.bounds,
                    object,
                    self.current.presentation_basis().frame_rate,
                )?,
            },
            SemanticSelector::Motion { motion } => {
                let (destination, _) = self.motion_target(motion)?;
                let cursor = self.context.cursor;
                if destination == cursor {
                    return Err(unavailable("the macro motion selection is empty"));
                }
                SliceCaptureSelection::Range {
                    range: FrameRange::new(cursor.min(destination), cursor.max(destination))
                        .map_err(crate::DocumentError::from)?,
                }
            }
        };
        let range = match &selection {
            SliceCaptureSelection::Child { node } => {
                self.current
                    .sequence_children(&self.context.parent, node, node)?
                    .range
            }
            SliceCaptureSelection::Range { range } => *range,
            SliceCaptureSelection::Children { .. } => {
                unreachable!("children selectors are resolved as objects")
            }
        };
        Ok(ResolvedTarget::local(
            &self.context.parent,
            selection,
            range,
        ))
    }

    fn object_target(&self, object: &SemanticObjectSelection) -> Result<ResolvedTarget, EditError> {
        let SemanticObjectTarget {
            parent,
            selection,
            range,
        } = self
            .current
            .resolve_object_selection(&self.context.parent, object)?;
        let outside_inner =
            object.kind == SemanticTextObject::InnerGroup && object.group != self.context.parent;
        Ok(ResolvedTarget {
            navigation_parent: if outside_inner {
                self.context.parent.clone()
            } else {
                parent.clone()
            },
            retained_group: outside_inner.then(|| object.group.clone()),
            parent,
            selection,
            range,
        })
    }

    pub(super) fn continue_target(
        &mut self,
        target: &ResolvedTarget,
        cursor: ProjectFrame,
        selected: Option<NodeId>,
    ) -> Result<(), EditError> {
        self.context.parent = target.navigation_parent.clone();
        self.context.cursor = cursor;
        self.context.selected_child = target.retained_group.clone().or(selected);
        self.context.visual_selection = None;
        self.refresh_children()?;
        validate_context(&self.current, &self.context, self.bounds)?;
        validate_selection(&self.current, &self.context)
    }

    pub(super) fn move_context(&mut self, motion: SemanticMotion) -> Result<(), EditError> {
        let object_anchor = match &self.context.visual_selection {
            Some(SemanticVisualSelection::Object {
                selection,
                extending: true,
            }) => Some(
                self.current
                    .resolve_object_selection(&self.context.parent, selection)?
                    .range
                    .start(),
            ),
            _ => None,
        };
        let (cursor, child) = self.motion_target(motion)?;
        self.context.cursor = cursor;
        self.context.selected_child = child;
        if let Some(anchor) = object_anchor {
            self.context.visual_selection = Some(SemanticVisualSelection::Time {
                anchor,
                head: cursor,
                extending: true,
            });
        } else if let Some(SemanticVisualSelection::Time {
            head,
            extending: true,
            ..
        }) = &mut self.context.visual_selection
        {
            *head = cursor;
        }
        Ok(())
    }

    pub(super) fn charge_resolution(&mut self) -> Result<(), EditError> {
        charge(
            &mut self.document_bytes,
            wire::size(&self.current, MAX_DOCUMENT_JSON_BYTES)?,
            MAX_COMPOUND_DOCUMENT_BYTES,
            "macro object resolution byte limit",
        )
    }

    pub(super) fn capture_scope(
        &self,
        parent: &NodeId,
    ) -> Result<(FrameRange, Vec<NodeId>, Vec<String>), EditError> {
        let (start, end) = scope_bounds(&self.current, parent)?;
        let bounds = FrameRange::new(start, end).map_err(crate::DocumentError::from)?;
        let mut scope = Vec::new();
        let mut current = parent.clone();
        while current != *self.current.root() {
            if scope.len() == crate::MAX_DOCUMENT_DEPTH {
                return Err(limit("macro capture scope depth limit"));
            }
            scope.push(current.clone());
            current = self
                .current
                .parent_of(&current)
                .ok_or_else(|| unavailable("macro capture parent is detached"))?;
        }
        scope.reverse();
        let labels = scope
            .iter()
            .map(|node| self.current.nodes()[node].label.clone())
            .collect();
        Ok((bounds, scope, labels))
    }

    fn motion_target(
        &self,
        motion: SemanticMotion,
    ) -> Result<(ProjectFrame, Option<NodeId>), EditError> {
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
                return Ok(self.beat_target(forward, count.get()));
            }
            SemanticMotion::Scope { end } => {
                if end {
                    self.bounds.1
                } else {
                    self.bounds.0
                }
            }
            SemanticMotion::Words {
                forward,
                count,
                end,
            } => self.words()?.motion_target(
                self.context.cursor,
                self.bounds,
                SpeechMotion {
                    forward,
                    count: count.get(),
                    sentence: false,
                    end,
                },
            ),
            SemanticMotion::Sentences { forward, count } => self.words()?.motion_target(
                self.context.cursor,
                self.bounds,
                SpeechMotion {
                    forward,
                    count: count.get(),
                    sentence: true,
                    end: false,
                },
            ),
            SemanticMotion::Pauses { forward, count } => self.speech()?.pause_target(
                self.context.cursor,
                self.bounds,
                forward,
                count.get(),
            )?,
            SemanticMotion::Shots { forward, count } => self.speech()?.shot_target(
                self.context.cursor,
                self.bounds,
                forward,
                count.get(),
            )?,
        };
        Ok((
            cursor,
            selected_child(&self.child_ends, cursor, self.bounds),
        ))
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
