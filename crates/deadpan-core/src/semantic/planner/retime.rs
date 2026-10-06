//! Speed, pitch and pause-duration changes of the selected direct child: the
//! semantic forms of `:speed`, `:pitch` and `:hold-duration`, so a macro
//! records them and `.` repeats them on another beat. They author the same
//! `WrapRetime`, `SetRetime` and `SetHoldDuration` commands as the native
//! edits, resolved against the staged document.

use super::*;
use crate::{ExactRatio, FrameDuration, PitchPolicy, RetimePurpose};

/// A speed resolved against one beat's exact input.
struct Resolved {
    duration: FrameDuration,
    /// The beat is an ordinary Retime updated in place.
    update: bool,
}

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: FnMut(&ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError>,
    P: FnMut(&ProjectDocument, super::PauseSite) -> Result<super::PauseProvider, EditError>,
{
    /// The selected direct child and its absolute range.
    fn selected_direct_child(
        &self,
        what: &str,
    ) -> Result<(NodeId, ProjectFrame, ProjectFrame), EditError> {
        if self.context.visual_selection.is_some() {
            return Err(invalid(&format!(
                "clear the Visual selection before changing a beat's {what}"
            )));
        }
        let selected = self.context.selected_child.clone().ok_or_else(|| {
            EditError::new(
                EditErrorCode::SelectionUnavailable,
                format!("select a beat before changing its {what}"),
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
        Ok((selected, start, self.child_ends[slot].1))
    }

    /// `speed` against the selected beat: an ordinary Retime's own input
    /// unless `wrap`, otherwise the beat's whole current duration.
    fn resolve_speed(
        &self,
        node: &NodeId,
        start: ProjectFrame,
        end: ProjectFrame,
        speed: ExactRatio,
        wrap: bool,
    ) -> Result<Resolved, EditError> {
        if !speed.compare_integer(0).is_gt() {
            return Err(invalid("a retime speed must be positive"));
        }
        let (input, update) = match &self.current.nodes()[node].kind {
            NodeKind::Retime {
                mapping,
                purpose: RetimePurpose::Edit,
                ..
            } if !wrap => (mapping.duration(), true),
            _ => (
                FrameDuration::new(end.0 - start.0).map_err(crate::DocumentError::from)?,
                false,
            ),
        };
        if input == FrameDuration::ZERO {
            return Err(invalid("an empty beat cannot be retimed"));
        }
        let overflow = |_| EditError::new(EditErrorCode::TimingOverflow, "retime overflows");
        let frames = ExactRatio::integer(input.frames())
            .checked_div(speed)
            .and_then(ExactRatio::round_even)
            .map_err(overflow)?;
        let frames = i64::try_from(frames).map_err(|_| {
            EditError::new(EditErrorCode::TimingOverflow, "retime duration overflows")
        })?;
        if frames == 0 {
            return Err(EditError::new(
                EditErrorCode::InvalidDuration,
                "this speed resolves to 0 frames; choose a slower speed",
            ));
        }
        Ok(Resolved {
            duration: FrameDuration::new(frames).map_err(crate::DocumentError::from)?,
            update,
        })
    }

    pub(super) fn retime(
        &mut self,
        trace_index: usize,
        speed: ExactRatio,
        pitch: PitchPolicy,
        wrap: bool,
    ) -> Result<(), EditError> {
        let (selected, start, end) = self.selected_direct_child("speed")?;
        let resolved = self.resolve_speed(&selected, start, end, speed, wrap)?;
        self.apply_retime(trace_index, selected, start, resolved, pitch)
    }

    pub(super) fn pitch(&mut self, trace_index: usize, semitones: i8) -> Result<(), EditError> {
        let (selected, start, end) = self.selected_direct_child("pitch")?;
        let pitch = PitchPolicy::shifted(semitones)?;
        let speed = match &self.current.nodes()[&selected].kind {
            NodeKind::Retime {
                mapping,
                duration,
                purpose: RetimePurpose::Edit,
                ..
            } => ExactRatio::new(
                i128::from(mapping.duration().frames()),
                i128::from(duration.frames()),
            )
            .map_err(|_| invalid("the Retime has no speed"))?,
            _ if semitones == 0 => {
                return Err(invalid("this beat has no pitch shift to remove"));
            }
            _ => ExactRatio::ONE,
        };
        let resolved = self.resolve_speed(&selected, start, end, speed, false)?;
        self.apply_retime(trace_index, selected, start, resolved, pitch)
    }

    fn apply_retime(
        &mut self,
        trace_index: usize,
        selected: NodeId,
        start: ProjectFrame,
        resolved: Resolved,
        pitch: PitchPolicy,
    ) -> Result<(), EditError> {
        if resolved.update
            && matches!(&self.current.nodes()[&selected].kind,
                NodeKind::Retime { duration, pitch: current, .. }
                    if *duration == resolved.duration && *current == pitch)
        {
            return Err(invalid(
                "the beat already has that speed and pitch; no edit was made",
            ));
        }
        self.charge_step(false)?;
        let (new_revision, command, result) = if resolved.update {
            let SemanticAllocation::ParameterEdit { new_revision } =
                (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                    step_index: self.steps.len(),
                })?
            else {
                return Err(invalid("a retime update requires a parameter allocation"));
            };
            (
                new_revision,
                Command::SetRetime {
                    node: selected.clone(),
                    duration: resolved.duration,
                    pitch,
                },
                selected,
            )
        } else {
            let SemanticAllocation::WrapRetime { new_revision, id } =
                (self.allocate)(SemanticAllocationRequest::WrapRetime {
                    step_index: self.steps.len(),
                })?
            else {
                return Err(invalid("a retime wrap requires a WrapRetime allocation"));
            };
            self.reserve_node(&id)?;
            (
                new_revision,
                Command::WrapRetime {
                    node: selected,
                    id: id.clone(),
                    duration: resolved.duration,
                    pitch,
                },
                id,
            )
        };
        self.reserve_revision(&new_revision)?;
        self.commit_leaf(LeafEdit::new(new_revision, command)?)?;
        // The cursor stays at the beat's start, which the speed change keeps.
        self.context.cursor = start;
        self.context.selected_child = Some(result.clone());
        let end = ProjectFrame(start.0.checked_add(resolved.duration.frames()).ok_or_else(
            || EditError::new(EditErrorCode::TimingOverflow, "retime end overflows"),
        )?);
        self.trace[trace_index].resolved_parent = Some(self.context.parent.clone());
        self.trace[trace_index].resolved_selection =
            Some(SliceCaptureSelection::Child { node: result });
        self.trace[trace_index].resolved_range =
            Some(FrameRange::new(start, end).map_err(crate::DocumentError::from)?);
        Ok(())
    }

    /// `:audio-lag`: the selected beat's Source keeps its audio mapping and
    /// takes the exact sample offset, as the native edit authors it.
    pub(super) fn set_audio_lag(
        &mut self,
        trace_index: usize,
        offset: crate::AudioSample,
    ) -> Result<(), EditError> {
        let (selected, start, end) = self.selected_direct_child("sound offset")?;
        let (host, _) = crate::cutaway_host(&self.current, &selected).ok_or_else(|| {
            EditError::new(
                EditErrorCode::WrongNodeKind,
                "audio lag belongs to a source beat; open a group with Enter first",
            )
        })?;
        let NodeKind::Source { source } = &self.current.nodes()[&host].kind else {
            return Err(EditError::new(
                EditErrorCode::WrongNodeKind,
                "audio lag needs a beat of the Original; this is a pause",
            ));
        };
        if source.audio.is_none() {
            return Err(invalid("this beat has no sound to offset"));
        }
        if source.audio_offset == offset {
            return Err(invalid(
                "the sound already has that offset; no edit was made",
            ));
        }
        let mapping = source.audio_mapping;
        self.charge_step(false)?;
        let SemanticAllocation::ParameterEdit { new_revision } =
            (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                step_index: self.steps.len(),
            })?
        else {
            return Err(invalid("a sound offset requires a parameter allocation"));
        };
        self.reserve_revision(&new_revision)?;
        self.commit_leaf(LeafEdit::new(
            new_revision,
            Command::SetSourceAudioMapping {
                node: host,
                mapping,
                offset,
            },
        )?)?;
        self.context.selected_child = Some(selected.clone());
        self.trace[trace_index].resolved_parent = Some(self.context.parent.clone());
        self.trace[trace_index].resolved_selection =
            Some(SliceCaptureSelection::Child { node: selected });
        self.trace[trace_index].resolved_range =
            Some(FrameRange::new(start, end).map_err(crate::DocumentError::from)?);
        Ok(())
    }

    /// `:edge`: one leaf per changed boundary policy.
    pub(super) fn set_audio_edges(
        &mut self,
        trace_index: usize,
        side: crate::EdgeSide,
        policy: crate::AudioEdgePolicy,
    ) -> Result<(), EditError> {
        use crate::{AudioBoundaryKind as Kind, EdgeSide};
        let (selected, start, end) = self.selected_direct_child("sound edges")?;
        let repeat_child = match &self.current.nodes()[&selected].kind {
            NodeKind::Repeat { child, .. } => Some(child.clone()),
            _ => None,
        };
        let (owner, edges): (NodeId, &[Kind]) = match side {
            EdgeSide::Start => (selected.clone(), &[Kind::NodeStart]),
            EdgeSide::End => (selected.clone(), &[Kind::NodeEnd]),
            EdgeSide::Both => (selected.clone(), &[Kind::NodeStart, Kind::NodeEnd]),
            EdgeSide::Plays => (
                repeat_child.ok_or_else(|| {
                    EditError::new(
                        EditErrorCode::WrongNodeKind,
                        "play seams belong to a Repeat; select one",
                    )
                })?,
                &[Kind::NodeStart, Kind::NodeEnd],
            ),
            EdgeSide::Gaps => {
                if repeat_child.is_none() {
                    return Err(EditError::new(
                        EditErrorCode::WrongNodeKind,
                        "gap edges belong to a Repeat; select one",
                    ));
                }
                (
                    selected.clone(),
                    &[Kind::RepeatGapStart, Kind::RepeatGapEnd],
                )
            }
        };
        let changed: Vec<Kind> = edges
            .iter()
            .copied()
            .filter(|edge| self.current.nodes()[&owner].audio_edges.get(*edge) != policy)
            .collect();
        // A split fragment's sides add no fade until they are marked as
        // editorial edges; the policy then chooses fade or cut there. Like a
        // Trim, an edge between two pieces marks both incident sides. A side
        // that continues the same fragment's sound uninterrupted (a pure
        // Split) is no cut: a fade there would dip continuous sound.
        let nodes = self.current.nodes();
        let partition = |node: &NodeId| match &nodes[node].kind {
            NodeKind::Retime {
                child,
                mapping,
                purpose: RetimePurpose::Partition,
                ..
            } => Some((child.clone(), *mapping)),
            _ => None,
        };
        let mut wanted: Vec<(NodeId, bool, bool)> = Vec::new();
        if side == EdgeSide::Plays {
            if self.current.overrides().contains_key(&selected) {
                return Err(EditError::new(
                    EditErrorCode::WrongNodeKind,
                    "this Repeat has plays with their own contents; set their seams in each play with :scope play N",
                ));
            }
            if partition(&owner).is_some() {
                wanted.push((owner.clone(), true, true));
            }
        } else if side != EdgeSide::Gaps {
            let slot = self.child_indices[&selected];
            let own = partition(&selected);
            for (start_side, neighbor) in [
                (true, slot.checked_sub(1)),
                (
                    false,
                    Some(slot + 1).filter(|next| *next < self.child_ends.len()),
                ),
            ] {
                if !edges.contains(&if start_side {
                    Kind::NodeStart
                } else {
                    Kind::NodeEnd
                }) {
                    continue;
                }
                let neighbor = neighbor.map(|index| self.child_ends[index].0.clone());
                let theirs = neighbor.as_ref().and_then(partition);
                // Split fragments hold full copies of one context, so the same
                // sound is an equal child recipe, not the same node.
                if let (Some((child, mapping)), Some((other_child, other))) = (&own, &theirs)
                    && nodes[child].kind == nodes[other_child].kind
                    && if start_side {
                        other.end() == mapping.start()
                    } else {
                        mapping.end() == other.start()
                    }
                {
                    return Err(invalid(
                        "this side continues the same sound without a cut (a split, not an edit); a fade would dip it, so there is no edge to set there",
                    ));
                }
                if own.is_some() {
                    wanted.push((selected.clone(), start_side, !start_side));
                }
                if let (Some(neighbor), Some(_)) = (neighbor, theirs) {
                    wanted.push((neighbor, !start_side, start_side));
                }
            }
        }
        let mut marks: Vec<(NodeId, crate::AudioEditorialEdges)> = Vec::new();
        for (node, start_side, end_side) in wanted {
            let current = match marks.iter().find(|(marked, _)| marked == &node) {
                Some((_, edges)) => *edges,
                None => nodes[&node].audio_editorial_edges,
            };
            let marked = crate::AudioEditorialEdges {
                start: current.start || start_side,
                end: current.end || end_side,
            };
            if marked != nodes[&node].audio_editorial_edges {
                marks.retain(|(existing, _)| existing != &node);
                marks.push((node, marked));
            }
        }
        if changed.is_empty() && marks.is_empty() {
            return Err(invalid(
                "those sound edges already have that policy; no edit was made",
            ));
        }
        for (node, edges) in marks {
            self.charge_step(false)?;
            let SemanticAllocation::ParameterEdit { new_revision } =
                (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                    step_index: self.steps.len(),
                })?
            else {
                return Err(invalid("a sound edge requires a parameter allocation"));
            };
            self.reserve_revision(&new_revision)?;
            self.commit_leaf(LeafEdit::new(
                new_revision,
                Command::SetEditorialEdges { node, edges },
            )?)?;
        }
        for edge in changed {
            self.charge_step(false)?;
            let SemanticAllocation::ParameterEdit { new_revision } =
                (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                    step_index: self.steps.len(),
                })?
            else {
                return Err(invalid("a sound edge requires a parameter allocation"));
            };
            self.reserve_revision(&new_revision)?;
            self.commit_leaf(LeafEdit::new(
                new_revision,
                Command::SetAudioEdge {
                    node: owner.clone(),
                    edge,
                    policy,
                },
            )?)?;
        }
        self.context.selected_child = Some(selected.clone());
        self.trace[trace_index].resolved_parent = Some(self.context.parent.clone());
        self.trace[trace_index].resolved_selection =
            Some(SliceCaptureSelection::Child { node: selected });
        self.trace[trace_index].resolved_range =
            Some(FrameRange::new(start, end).map_err(crate::DocumentError::from)?);
        Ok(())
    }

    pub(super) fn set_hold_duration(
        &mut self,
        trace_index: usize,
        length: crate::PauseLength,
    ) -> Result<(), EditError> {
        let (selected, start, _) = self.selected_direct_child("pause duration")?;
        let NodeKind::Hold { recipe } = &self.current.nodes()[&selected].kind else {
            return Err(EditError::new(
                EditErrorCode::WrongNodeKind,
                "a duration change applies to a pause (Hold)",
            ));
        };
        let duration = length.resolve(self.current.presentation_basis().frame_rate)?;
        if recipe.duration == duration {
            return Err(invalid(
                "the pause already has that duration; no edit was made",
            ));
        }
        self.charge_step(false)?;
        let SemanticAllocation::ParameterEdit { new_revision } =
            (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                step_index: self.steps.len(),
            })?
        else {
            return Err(invalid("a pause duration requires a parameter allocation"));
        };
        self.reserve_revision(&new_revision)?;
        self.commit_leaf(LeafEdit::new(
            new_revision,
            Command::SetHoldDuration {
                node: selected.clone(),
                duration,
            },
        )?)?;
        self.context.cursor = start;
        self.context.selected_child = Some(selected.clone());
        let end =
            ProjectFrame(start.0.checked_add(duration.frames()).ok_or_else(|| {
                EditError::new(EditErrorCode::TimingOverflow, "pause end overflows")
            })?);
        self.trace[trace_index].resolved_parent = Some(self.context.parent.clone());
        self.trace[trace_index].resolved_selection =
            Some(SliceCaptureSelection::Child { node: selected });
        self.trace[trace_index].resolved_range =
            Some(FrameRange::new(start, end).map_err(crate::DocumentError::from)?);
        Ok(())
    }
}
