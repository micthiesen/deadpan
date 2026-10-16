//! Cursor-anchored annotations as semantic instructions: `m{letter}` saves
//! the same native mark the editor key does, owned by the direct child at the
//! Edit cursor, `'{letter}` moves to its resolved Edit boundary in the current
//! group, and `,s` places a catalog sound at the cursor.

use super::*;
use crate::{
    Anchor, AnchorIndex, AnchorLossPolicy, BoundaryAnchor, BoundarySelector, ExactRatio,
    InsertionBias, InstancePath, MediaRole, NamedMarkTarget, ResolvedSelectionKind,
    SelectionRequest,
};

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: FnMut(&ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError>,
    P: FnMut(&ProjectDocument, super::PauseSite) -> Result<super::PauseProvider, EditError>,
{
    pub(super) fn set_mark(&mut self, trace_index: usize, letter: char) -> Result<(), EditError> {
        let id = crate::semantic::native_mark_id(letter)?;
        if self
            .current
            .marks()
            .get(&id)
            .is_some_and(|mark| mark.label != letter.to_string())
        {
            return Err(invalid(&format!(
                "mark {letter} conflicts with a named mark using its native key address"
            )));
        }
        let at = self.context.cursor;
        // The child containing the cursor, the last child at the group's end,
        // or a selected empty child exactly at the cursor; else the group.
        let mut start = self.bounds.0;
        let mut chosen = None;
        let mut explicit_empty = None;
        for (index, (child, end)) in self.child_ends.iter().enumerate() {
            if start == *end && at == start && self.context.selected_child.as_ref() == Some(child) {
                explicit_empty = Some((child.clone(), start, *end));
            }
            if (start <= at && at < *end)
                || (at == self.bounds.1 && index + 1 == self.child_ends.len())
            {
                chosen = Some((child.clone(), start, *end));
            }
            start = *end;
        }
        let (host, start, end) = explicit_empty.or(chosen).unwrap_or((
            self.context.parent.clone(),
            self.bounds.0,
            self.bounds.1,
        ));
        let position =
            at.0.checked_sub(start.0)
                .ok_or_else(|| invalid("the mark precedes its host"))?;
        self.charge_step(false)?;
        let SemanticAllocation::ParameterEdit { new_revision } =
            (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                step_index: self.steps.len(),
            })?
        else {
            return Err(invalid("a mark requires a parameter allocation"));
        };
        self.reserve_revision(&new_revision)?;
        self.commit_leaf(LeafEdit::new(
            new_revision,
            Command::SetMark {
                id,
                owner: host.clone(),
                label: letter.to_string(),
                boundary: BoundaryAnchor {
                    coordinate: Anchor::Occurrence {
                        instance: InstancePath {
                            node: host,
                            repeats: Vec::new(),
                        },
                        position: ExactRatio::integer(position),
                    },
                    bias: if at == end {
                        InsertionBias::Left
                    } else {
                        InsertionBias::Right
                    },
                },
                loss_policy: AnchorLossPolicy::KeepUnresolved,
            },
        )?)?;
        self.trace[trace_index].resolved_parent = Some(self.context.parent.clone());
        Ok(())
    }

    pub(super) fn jump_mark(&mut self, letter: char) -> Result<(), EditError> {
        let id = crate::semantic::native_mark_id(letter)?;
        let mark = self
            .current
            .marks()
            .get(&id)
            .ok_or_else(|| invalid(&format!("mark {letter} has not been set")))?;
        if mark
            .bindings()
            .any(|binding| matches!(binding.coordinate, Anchor::Source { .. }))
        {
            return Err(invalid(&format!(
                "mark {letter} is on the Original; a macro moves only in Your edit"
            )));
        }
        let unavailable = |error: crate::AnchorError| {
            EditError::new(EditErrorCode::SelectionUnavailable, error.to_string())
        };
        let resolved = AnchorIndex::new(&self.current)
            .map_err(unavailable)?
            .resolve(&SelectionRequest {
                project_id: self.current.project_id().clone(),
                expected_revision: self.current.revision_id().clone(),
                role: MediaRole::Linked,
                selector: BoundarySelector::Mark {
                    target: NamedMarkTarget {
                        id,
                        occurrence: None,
                    },
                },
            })
            .map_err(unavailable)?;
        let ResolvedSelectionKind::Point { point } = resolved.selection else {
            return Err(invalid("a mark must resolve to one boundary"));
        };
        let cursor = point.frame;
        if cursor.0 < self.bounds.0.0 || cursor.0 > self.bounds.1.0 {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                format!("mark {letter} is outside the current group"),
            ));
        }
        self.context.cursor = cursor;
        self.context.selected_child = selected_child(&self.child_ends, cursor, self.bounds);
        if let Some(SemanticVisualSelection::Time {
            head,
            extending: true,
            ..
        }) = &mut self.context.visual_selection
        {
            *head = cursor;
        }
        Ok(())
    }

    pub(super) fn place_sound(
        &mut self,
        trace_index: usize,
        asset: &crate::AssetId,
    ) -> Result<(), EditError> {
        let metadata = self.current.assets().get(asset).ok_or_else(|| {
            EditError::new(
                EditErrorCode::SelectionUnavailable,
                "the catalog sound is no longer registered",
            )
        })?;
        let span = match metadata {
            crate::AssetRecord {
                video: None,
                still_image: false,
                audio: Some(span),
                source_qualification: Some(_),
                ..
            } => *span,
            _ => {
                return Err(invalid(
                    "sound placement requires a qualified audio-only catalog source",
                ));
            }
        };
        let rate = self.current.presentation_basis().frame_rate;
        let event = crate::SoundEvent {
            owner: self.current.root().clone(),
            label: metadata.label.clone(),
            source: crate::SourceAudio {
                asset: asset.clone(),
                span,
            },
            mapping: crate::SourceAudioMapping::natural_rate(span, rate)
                .map_err(crate::DocumentError::from)?,
            offset: rate
                .audio_boundary(self.context.cursor)
                .map_err(crate::DocumentError::from)?,
            treatments: crate::AudioTreatments::default(),
            start_edge: crate::AudioEdgePolicy::Automatic,
            end_edge: crate::AudioEdgePolicy::Automatic,
            overflow: crate::SoundOverflowPolicy::Reject,
        };
        let selected = event
            .mapping
            .selection_frames_with_offset(crate::FrameDuration::ZERO, event.offset, rate)
            .map_err(crate::DocumentError::from)?;
        if selected.start.compare_integer(0).is_lt()
            || selected
                .end
                .compare_integer(self.current.duration()?.frames())
                .is_gt()
        {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "the complete sound extends past the end of the edit; place it earlier",
            ));
        }
        self.charge_step(false)?;
        let SemanticAllocation::Sound { new_revision, id } =
            (self.allocate)(SemanticAllocationRequest::Sound {
                step_index: self.steps.len(),
            })?
        else {
            return Err(invalid("a placed sound requires a sound allocation"));
        };
        if self.current.sounds().contains_key(&id) {
            return Err(EditError::new(
                EditErrorCode::IdentityConflict,
                "the placed sound's identity is already present",
            ));
        }
        self.reserve_revision(&new_revision)?;
        self.commit_leaf(LeafEdit::new(
            new_revision,
            Command::SetSound { id, event },
        )?)?;
        self.trace[trace_index].resolved_parent = Some(self.context.parent.clone());
        Ok(())
    }
}
