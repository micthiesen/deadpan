//! J-cuts and L-cuts (specification §8.3 "Premature sound" and "Lingering
//! sound"): the sound's cut moves away from the picture's cut.
//!
//! A split edit is built from two qualified primitives in one transaction.
//! `RollSources` moves the linked seam between two adjacent source beats by
//! the split length, so the leading beat's real sound (and picture) fills
//! that stretch from its source handle. A picture-only `Cutaway` over exactly
//! that stretch then shows the pictures that were there before, so the
//! picture still changes at the cursor and the edit's duration is unchanged.
//! The cutaway shows the same source pictures the beat showed, at their
//! natural rate, so preview and export sample them identically.

use super::*;
use crate::{
    Cutaway, CutawayFit, ExactRatio, ExactSourceSpan, FrameRange, SourcePoint, SourceVideo,
    SplitEditKind,
};

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: crate::SemanticSpeechProvider,
    P: FnMut(&ProjectDocument, super::PauseSite) -> Result<super::PauseProvider, EditError>,
{
    pub(super) fn split_edit(
        &mut self,
        trace_index: usize,
        kind: SplitEditKind,
        length: crate::PauseLength,
    ) -> Result<(), EditError> {
        if self.context.visual_selection.is_some() {
            return Err(invalid(
                "clear the Visual selection, then put the cursor on the cut between two beats",
            ));
        }
        let rate = self.current.presentation_basis().frame_rate;
        let frames = length.resolve(rate)?;
        let at = self.context.cursor;
        let slot = self
            .child_ends
            .iter()
            .position(|(_, end)| *end == at)
            .filter(|slot| slot + 1 < self.child_ends.len())
            .ok_or_else(|| {
                EditError::new(
                    EditErrorCode::SelectionUnavailable,
                    format!(
                        "a {} needs the cursor on the cut between two beats of this group",
                        kind.name()
                    ),
                )
            })?;
        let left = self.child_ends[slot].0.clone();
        let right = self.child_ends[slot + 1].0.clone();
        let parent = self.context.parent.clone();
        let is_source = |child: &NodeId| {
            crate::cutaway_host(&self.current, child).is_some_and(|(host, _)| {
                matches!(self.current.nodes()[&host].kind, NodeKind::Source { .. })
            })
        };
        if !(is_source(&left) && is_source(&right)) {
            // A pause or a group on one side cannot roll media; the leading
            // sound continues from its own handle as a root sound instead.
            return self.split_edit_by_sound(trace_index, kind, frames, slot);
        }
        let delta = match kind {
            SplitEditKind::J => frames.frames().checked_neg(),
            SplitEditKind::L => Some(frames.frames()),
        }
        .ok_or_else(|| EditError::new(EditErrorCode::TimingOverflow, "split length overflows"))?;
        let resolved = self.current.source_roll(&parent, &left, &right, delta)?;
        if resolved.applied_delta_frames != delta {
            let available = resolved.applied_delta_frames.unsigned_abs();
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                format!(
                    "a {} of {} frames needs more source on both sides of the cut; at most {available} frames are available here",
                    kind.name(),
                    frames.frames()
                ),
            ));
        }
        // The pictures that were shown where the leading sound now plays.
        let (shown, local) = match kind {
            SplitEditKind::J => {
                let length = resolved.left.output_before.duration().frames();
                (&resolved.left, length - frames.frames()..length)
            }
            SplitEditKind::L => (&resolved.right, 0..frames.frames()),
        };
        let (asset, selection) = shown_pictures(shown, local, rate)?;
        let needs_wrapper = resolved.left.needs_wrapper || resolved.right.needs_wrapper;
        self.charge_step(false)?;
        let SemanticAllocation::Roll {
            new_revision,
            wrapper,
        } = (self.allocate)(SemanticAllocationRequest::Roll {
            step_index: self.steps.len(),
            needs_wrapper,
        })?
        else {
            return Err(invalid("a split edit requires a Roll allocation"));
        };
        if wrapper.is_some() != needs_wrapper {
            return Err(invalid("a split edit requires exactly its crop wrapper"));
        }
        self.reserve_revision(&new_revision)?;
        if let Some(wrapper) = &wrapper {
            self.reserve_node(wrapper)?;
        }
        let roll = LeafEdit::new(
            new_revision.clone(),
            Command::RollSources {
                parent: parent.clone(),
                left: left.clone(),
                right: right.clone(),
                delta_frames: delta,
                left_wrapper: wrapper.clone().filter(|_| resolved.left.needs_wrapper),
                right_wrapper: wrapper.filter(|_| resolved.right.needs_wrapper),
                timing: crate::AudioTimingId {
                    allocation: new_revision,
                    ordinal: 0,
                },
            },
        )?;
        self.commit_leaf(roll)?;

        // A contracting side may now be wrapped; find each side by slot.
        let leading = &self.child_ends[match kind {
            SplitEditKind::J => slot + 1,
            SplitEditKind::L => slot,
        }]
        .0;
        let leading = leading.clone();
        let (host, offset) = crate::cutaway_host(&self.current, &leading)
            .ok_or_else(|| invalid("the rolled beat no longer hosts a source for its cutaway"))?;
        let start = match kind {
            SplitEditKind::J => offset,
            SplitEditKind::L => offset
                .checked_add(resolved.left.output_before.duration().frames())
                .ok_or_else(|| {
                    EditError::new(EditErrorCode::TimingOverflow, "cutaway start overflows")
                })?,
        };
        let range = FrameRange::new(
            ProjectFrame(start),
            ProjectFrame(start.checked_add(frames.frames()).ok_or_else(|| {
                EditError::new(EditErrorCode::TimingOverflow, "cutaway end overflows")
            })?),
        )
        .map_err(crate::DocumentError::from)?;
        let mut cutaways = self.current.nodes()[&host].cutaways.clone();
        if cutaways.iter().any(|cutaway| {
            cutaway.range.start() < range.end() && range.start() < cutaway.range.end()
        }) {
            return Err(invalid(
                "a cutaway already covers part of this split; clear it first",
            ));
        }
        cutaways.push(Cutaway {
            range,
            asset,
            selection,
            fit: CutawayFit::Hold,
            removed: false,
        });
        cutaways.sort_by_key(|cutaway| cutaway.range.start());
        self.charge_step(false)?;
        let SemanticAllocation::ParameterEdit { new_revision } =
            (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                step_index: self.steps.len(),
            })?
        else {
            return Err(invalid(
                "a split edit's cutaway requires a parameter allocation",
            ));
        };
        self.reserve_revision(&new_revision)?;
        self.commit_leaf(LeafEdit::new(
            new_revision,
            Command::SetCutaways {
                node: host,
                cutaways,
            },
        )?)?;
        // The picture still cuts at the cursor; the beat after it stays
        // selected as the seam's right-hand object.
        self.context.cursor = at;
        let end =
            ProjectFrame(at.0.checked_add(frames.frames()).ok_or_else(|| {
                EditError::new(EditErrorCode::TimingOverflow, "split end overflows")
            })?);
        let start = ProjectFrame(at.0 - frames.frames());
        self.trace[trace_index].resolved_parent = Some(parent);
        self.trace[trace_index].resolved_range = Some(
            match kind {
                SplitEditKind::J => FrameRange::new(start, at),
                SplitEditKind::L => FrameRange::new(at, end),
            }
            .map_err(crate::DocumentError::from)?,
        );
        Ok(())
    }
}

/// The beat at one edge of a direct child, through ordinary Sequence groups:
/// its node and absolute Edit start and end.
struct EdgeLeaf {
    node: NodeId,
    start: i64,
    end: i64,
}

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: crate::SemanticSpeechProvider,
    P: FnMut(&ProjectDocument, super::PauseSite) -> Result<super::PauseProvider, EditError>,
{
    /// A J- or L-cut at a seam where one side is a pause or a group. Nothing
    /// rolls: every picture, beat and duration stays. The leading beat (the
    /// incoming beat's first beat for a J-cut, the outgoing beat's last for
    /// an L-cut) keeps sounding from its own Original handle across the cut,
    /// as one root sound event through that beat's exact audio mapping, its
    /// seam side made Hard on both voices so the sound is continuous there.
    /// The other side's sound under that stretch is muted, or, for a silent
    /// pause, the event gets an explicit silence allowance for it.
    fn split_edit_by_sound(
        &mut self,
        trace_index: usize,
        kind: SplitEditKind,
        frames: crate::FrameDuration,
        slot: usize,
    ) -> Result<(), EditError> {
        let name = kind.name();
        let length = frames.frames();
        let at = self.context.cursor.0;
        if self.context.scoped.is_some() {
            return Err(invalid(&format!(
                "a {name} next to a pause or group acts on the whole edit; leave the play scope first"
            )));
        }
        // Edit frames are root frames, and so root sound frames, only below
        // ordinary Sequences.
        let mut ancestor = Some(self.context.parent.clone());
        while let Some(node) = ancestor {
            if !matches!(self.current.nodes()[&node].kind, NodeKind::Sequence { .. }) {
                return Err(invalid(&format!(
                    "a {name} next to a pause or group needs ordinary groups above it; leave the Repeat or speed change first"
                )));
            }
            ancestor = self.current.parent_of(&node);
        }
        let left_start = slot
            .checked_sub(1)
            .map_or(self.bounds.0.0, |previous| self.child_ends[previous].1.0);
        let left = (self.child_ends[slot].0.clone(), left_start, at);
        let right = (
            self.child_ends[slot + 1].0.clone(),
            at,
            self.child_ends[slot + 1].1.0,
        );
        let (leading, receiving) = match kind {
            SplitEditKind::J => (right, left),
            SplitEditKind::L => (left, right),
        };
        let leading_at_end = kind == SplitEditKind::L;
        let lead = self.edge_leaf(&leading.0, leading.1, leading.2, leading_at_end, true)?;
        let recv = self.edge_leaf(
            &receiving.0,
            receiving.1,
            receiving.2,
            !leading_at_end,
            false,
        )?;
        let (host, offset) = crate::cutaway_host(&self.current, &lead.node)
            .filter(|(host, _)| {
                matches!(self.current.nodes()[host].kind, NodeKind::Source { .. })
            })
            .ok_or_else(|| {
                EditError::new(
                    EditErrorCode::WrongNodeKind,
                    match self.current.nodes()[&lead.node].kind {
                        NodeKind::Hold { .. } => format!(
                            "a pause has no sound of its own to carry across the cut; a {name} needs a beat of the Original on its {} side",
                            if leading_at_end { "outgoing" } else { "incoming" }
                        ),
                        _ => format!(
                            "a {name} needs a beat of the Original at the cut, not a Repeat or speed change"
                        ),
                    },
                )
            })?;
        let stretch = if leading_at_end {
            at..at + length
        } else {
            at - length..at
        };
        let available = recv.end - recv.start;
        if available < length {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                format!(
                    "a {name} of {length} frames needs that much of one beat or pause on the other side of the cut; at most {available} frames are available here"
                ),
            ));
        }
        // How the other side's own sound gives way under the stretch.
        let pause = match &self.current.nodes()[&recv.node].kind {
            NodeKind::Hold { recipe } if matches!(recipe.audio, crate::HoldAudio::Silence) => {
                Some(recv.node.clone())
            }
            NodeKind::Hold { .. } => {
                return Err(invalid(&format!(
                    "this pause has its own sound; make it silent before a {name} reaches into it"
                )));
            }
            _ => match crate::cutaway_host(&self.current, &recv.node) {
                Some((host, _))
                    if matches!(self.current.nodes()[&host].kind, NodeKind::Source { .. }) =>
                {
                    None
                }
                _ => {
                    return Err(invalid(&format!(
                        "a {name} needs a beat or a pause at the cut, not a Repeat or speed change"
                    )));
                }
            },
        };
        let event = self.handle_sound(&lead, &host, offset, stretch.clone(), kind)?;
        if pause.is_none() {
            let local = (stretch.start - recv.start)..(stretch.end - recv.start);
            self.parameter_edit(self.muted(&recv.node, local)?)?;
        }
        let edge = if leading_at_end {
            crate::AudioBoundaryKind::NodeEnd
        } else {
            crate::AudioBoundaryKind::NodeStart
        };
        let leading_beat = &self.current.nodes()[&lead.node];
        // A Split fragment's side fades only once marked editorial; an
        // unmarked side already joins its handle sound without a fade.
        let fades = match leading_beat.kind {
            NodeKind::Retime {
                purpose: crate::RetimePurpose::Partition,
                ..
            } => {
                if leading_at_end {
                    leading_beat.audio_editorial_edges.end
                } else {
                    leading_beat.audio_editorial_edges.start
                }
            }
            _ => true,
        };
        if fades && leading_beat.audio_edges.get(edge) != crate::AudioEdgePolicy::Hard {
            self.parameter_edit(Command::SetAudioEdge {
                node: lead.node.clone(),
                edge,
                policy: crate::AudioEdgePolicy::Hard,
            })?;
        }
        self.charge_step(false)?;
        let SemanticAllocation::Sound { new_revision, id } =
            (self.allocate)(SemanticAllocationRequest::Sound {
                step_index: self.steps.len(),
            })?
        else {
            return Err(invalid("a split edit's sound requires a sound allocation"));
        };
        if self.current.sounds().contains_key(&id) {
            return Err(EditError::new(
                EditErrorCode::IdentityConflict,
                "the split edit's sound identity is already present",
            ));
        }
        self.reserve_revision(&new_revision)?;
        self.commit_leaf(LeafEdit::new(
            new_revision,
            Command::SetSound {
                id: id.clone(),
                event,
            },
        )?)?;
        if let Some(pause) = pause {
            self.parameter_edit(Command::SetSoundAllowance {
                sound: id,
                issuer: crate::SoundHoldIssuer::Node {
                    instance: crate::InstancePath {
                        node: pause,
                        repeats: Vec::new(),
                    },
                },
                allowed: true,
            })?;
        }
        self.context.cursor = ProjectFrame(at);
        self.trace[trace_index].resolved_parent = Some(self.context.parent.clone());
        self.trace[trace_index].resolved_range = Some(
            FrameRange::new(ProjectFrame(stretch.start), ProjectFrame(stretch.end))
                .map_err(crate::DocumentError::from)?,
        );
        Ok(())
    }

    /// The beat at `child`'s end (or start), descending through ordinary
    /// Sequence groups and skipping empty children. Groups on the leading
    /// side must carry no audio treatment, which the sound event would not
    /// receive.
    fn edge_leaf(
        &self,
        child: &NodeId,
        start: i64,
        end: i64,
        at_end: bool,
        leading: bool,
    ) -> Result<EdgeLeaf, EditError> {
        let mut leaf = EdgeLeaf {
            node: child.clone(),
            start,
            end,
        };
        for _ in 0..=crate::MAX_DOCUMENT_DEPTH {
            let node = &self.current.nodes()[&leaf.node];
            let NodeKind::Sequence { children } = &node.kind else {
                if leading && !node.audio_treatments.is_empty() {
                    return Err(invalid(
                        "the beat whose sound crosses the cut has gain or effects, which its handle would not keep; clear them first",
                    ));
                }
                return Ok(leaf);
            };
            if leading && !node.audio_treatments.is_empty() {
                return Err(invalid(
                    "the group whose sound crosses the cut has gain or effects, which its handle would not keep; clear them first",
                ));
            }
            let mut next = None;
            let ordered: Vec<&NodeId> = if at_end {
                children.iter().rev().collect()
            } else {
                children.iter().collect()
            };
            for candidate in ordered {
                let duration = self.current.node_duration(candidate)?.frames();
                if duration > 0 {
                    next = Some(if at_end {
                        EdgeLeaf {
                            node: candidate.clone(),
                            start: leaf.end - duration,
                            end: leaf.end,
                        }
                    } else {
                        EdgeLeaf {
                            node: candidate.clone(),
                            start: leaf.start,
                            end: leaf.start + duration,
                        }
                    });
                    break;
                }
            }
            leaf = next.ok_or_else(|| invalid("an empty group sits at the cut"))?;
        }
        Err(invalid("the group at the cut is nested too deeply"))
    }

    /// A root sound event playing `lead`'s own Original sound over the
    /// absolute Edit frames `heard` beyond its edge, from its handle.
    fn handle_sound(
        &self,
        lead: &EdgeLeaf,
        host: &NodeId,
        offset: i64,
        heard: std::ops::Range<i64>,
        kind: SplitEditKind,
    ) -> Result<crate::SoundEvent, EditError> {
        let name = kind.name();
        let NodeKind::Source { source } = &self.current.nodes()[host].kind else {
            return Err(invalid("the leading beat is not a Source"));
        };
        let audio = source
            .audio
            .clone()
            .filter(|_| source.link == crate::LinkRelation::Linked)
            .ok_or_else(|| {
                invalid(&format!(
                    "the beat at the cut has no linked sound for a {name}"
                ))
            })?;
        if !matches!(source.video, SourceVideo::Stream { .. }) {
            return Err(invalid(&format!("a {name} needs a beat of the Original")));
        }
        let rate = self.current.presentation_basis().frame_rate;
        let (start, frames) = match source.audio_mapping {
            crate::SourceAudioMapping::FitBeat => {
                return Err(invalid(&format!(
                    "the beat's sound is fitted to its picture; it has no natural-rate handle for a {name}"
                )));
            }
            crate::SourceAudioMapping::Duration { frames } => (ExactRatio::ZERO, frames),
            crate::SourceAudioMapping::Placement { start, frames }
            | crate::SourceAudioMapping::SelectedPlacement { start, frames, .. } => (start, frames),
        };
        let shift = source
            .audio_mapping
            .start_frames_with_offset(source.audio_offset, rate)
            .and_then(|with| with.checked_sub(start))
            .map_err(crate::DocumentError::from)?;
        // Host-physical frame of an absolute Edit frame of the leaf.
        let delta = lead.start - offset;
        let wanted_start = ExactRatio::integer(heard.start - delta)
            .checked_sub(shift)
            .map_err(crate::DocumentError::from)?;
        let wanted_end = wanted_start
            .checked_add(ExactRatio::integer(heard.end - heard.start))
            .map_err(crate::DocumentError::from)?;
        let support_end = start
            .checked_add(frames)
            .map_err(crate::DocumentError::from)?;
        if wanted_start.compare(start).is_lt() || wanted_end.compare(support_end).is_gt() {
            let seam = ExactRatio::integer(
                if kind == SplitEditKind::L {
                    heard.start
                } else {
                    heard.end
                } - delta,
            )
            .checked_sub(shift)
            .map_err(crate::DocumentError::from)?;
            let room = if kind == SplitEditKind::L {
                support_end.checked_sub(seam)
            } else {
                seam.checked_sub(start)
            }
            .map_err(crate::DocumentError::from)?
            .floor()
            .max(0);
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                format!(
                    "a {name} of {} frames needs more of the Original's sound {} this beat; at most {room} frames are available here",
                    heard.end - heard.start,
                    if kind == SplitEditKind::L {
                        "after"
                    } else {
                        "before"
                    }
                ),
            ));
        }
        let moved = |value: ExactRatio| {
            value
                .checked_add(ExactRatio::integer(delta))
                .map_err(|error| EditError::from(crate::DocumentError::from(error)))
        };
        let label = self.current.nodes()[&lead.node].label.clone();
        let hard_start = kind == SplitEditKind::L;
        Ok(crate::SoundEvent {
            owner: self.current.root().clone(),
            label: format!("{name} · {label}"),
            source: audio,
            mapping: crate::SourceAudioMapping::SelectedPlacement {
                start: moved(start)?,
                frames,
                selection: crate::ExactFrameRange::new(moved(wanted_start)?, moved(wanted_end)?)?,
            },
            offset: source.audio_offset,
            treatments: crate::AudioTreatments::default(),
            start_edge: if hard_start {
                crate::AudioEdgePolicy::Hard
            } else {
                crate::AudioEdgePolicy::Automatic
            },
            end_edge: if hard_start {
                crate::AudioEdgePolicy::Automatic
            } else {
                crate::AudioEdgePolicy::Hard
            },
            overflow: crate::SoundOverflowPolicy::Reject,
        })
    }
}

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: crate::SemanticSpeechProvider,
    P: FnMut(&ProjectDocument, super::PauseSite) -> Result<super::PauseProvider, EditError>,
{
    /// `:delete role=audio|video` over the Visual time range inside one
    /// direct child: a mute range of that beat, or a removed-picture cutaway
    /// on its Source. Time, the other role and later content are unchanged.
    pub(super) fn delete_role(
        &mut self,
        trace_index: usize,
        role: crate::MediaRole,
    ) -> Result<(), EditError> {
        if role == crate::MediaRole::Linked {
            return Err(invalid(
                "a linked delete removes time; use d, or choose role=audio or role=video",
            ));
        }
        let range = self.visual_range()?;
        // A range across cuts deletes the role in each beat it meets, as one
        // transaction; no beat moves.
        for (child, local) in self.range_segments(range)? {
            let command = match role {
                crate::MediaRole::Audio => self.muted(&child, local.clone())?,
                crate::MediaRole::Video | crate::MediaRole::Linked => {
                    let (host, offset) =
                        crate::cutaway_host(&self.current, &child).ok_or_else(|| {
                            EditError::new(
                                EditErrorCode::WrongNodeKind,
                                "delete the picture inside a source beat; open a group with Enter first",
                            )
                        })?;
                    let NodeKind::Source { source } = &self.current.nodes()[&host].kind else {
                        return Err(EditError::new(
                            EditErrorCode::WrongNodeKind,
                            "a pause's picture is its whole content; use :lift to blank it with its sound",
                        ));
                    };
                    let start = local.start + offset;
                    let end = local.end + offset;
                    let rate = self.current.presentation_basis().frame_rate;
                    let cutaways = remove_top_pictures(
                        &self.current,
                        source,
                        &self.current.nodes()[&host].cutaways,
                        start..end,
                        rate,
                    )?;
                    Command::SetCutaways {
                        node: host,
                        cutaways,
                    }
                }
            };
            self.charge_step(false)?;
            let SemanticAllocation::ParameterEdit { new_revision } =
                (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                    step_index: self.steps.len(),
                })?
            else {
                return Err(invalid(
                    "a role-only delete requires a parameter allocation",
                ));
            };
            self.reserve_revision(&new_revision)?;
            self.commit_leaf(LeafEdit::new(new_revision, command)?)?;
        }
        self.context.visual_selection = None;
        self.trace[trace_index].resolved_parent = Some(self.context.parent.clone());
        self.trace[trace_index].resolved_range = Some(range);
        Ok(())
    }
}

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: crate::SemanticSpeechProvider,
    P: FnMut(&ProjectDocument, super::PauseSite) -> Result<super::PauseProvider, EditError>,
{
    /// The nonempty Visual time range.
    pub(super) fn visual_range(&self) -> Result<FrameRange, EditError> {
        let target = self.resolve_selector(&crate::SemanticSelector::VisualSelection)?;
        let SliceCaptureSelection::Range { range } = target.selection()?.clone() else {
            return Err(invalid(
                "a role edit acts on a Visual time range, not a group object",
            ));
        };
        Ok(range)
    }

    /// Each direct child `range` meets, with the part it covers in that
    /// child's own frames.
    pub(super) fn range_segments(
        &self,
        range: FrameRange,
    ) -> Result<Vec<(NodeId, std::ops::Range<i64>)>, EditError> {
        let mut segments = Vec::new();
        let mut start = self.bounds.0.0;
        for (child, end) in &self.child_ends {
            let (low, high) = (range.start().0.max(start), range.end().0.min(end.0));
            if low < high {
                segments.push((child.clone(), (low - start)..(high - start)));
            }
            start = end.0;
        }
        if segments.is_empty() {
            return Err(invalid("the range lies outside this group"));
        }
        Ok(segments)
    }

    /// The nonempty Visual time range and the one direct child containing
    /// it, with the child's absolute start and end.
    pub(super) fn range_in_child(
        &self,
        across: &str,
    ) -> Result<(NodeId, ProjectFrame, ProjectFrame, FrameRange), EditError> {
        let target = self.resolve_selector(&crate::SemanticSelector::VisualSelection)?;
        let SliceCaptureSelection::Range { range } = target.selection()?.clone() else {
            return Err(invalid(
                "a role edit acts on a Visual time range, not a group object",
            ));
        };
        let slot = self
            .child_ends
            .iter()
            .position(|(_, end)| *end > range.start())
            .ok_or_else(|| invalid("the range lies outside this group"))?;
        let child_start = slot
            .checked_sub(1)
            .map_or(self.bounds.0, |previous| self.child_ends[previous].1);
        let child_end = self.child_ends[slot].1;
        if range.end() > child_end {
            return Err(EditError::new(EditErrorCode::SelectionUnavailable, across));
        }
        Ok((
            self.child_ends[slot].0.clone(),
            child_start,
            child_end,
            range,
        ))
    }

    /// `child`'s treatments with one more mute range over its own output
    /// frames `local`.
    pub(super) fn muted(
        &self,
        child: &NodeId,
        local: std::ops::Range<i64>,
    ) -> Result<Command, EditError> {
        let current = &self.current.nodes()[child].audio_treatments;
        let clip = current.clip_gain().cloned().unwrap_or_default();
        let mut ranges = clip.mute_ranges().to_vec();
        ranges.push(
            crate::GainRange::new(
                ExactRatio::integer(local.start),
                ExactRatio::integer(local.end),
            )
            .map_err(crate::audio_gain::invalid)?,
        );
        let clip =
            crate::ClipGain::new(clip.trim(), clip.muted(), clip.envelopes().to_vec(), ranges)
                .map_err(crate::audio_gain::invalid)?;
        Ok(Command::SetAudioTreatments {
            node: child.clone(),
            treatments: current
                .with_clip_gain(clip)
                .map_err(crate::audio_gain::invalid)?,
        })
    }
}

/// Remove the topmost picture over host-physical frames `range`: a cutaway's
/// part there is trimmed so the beat's own picture shows again, and wherever no
/// cutaway covers the range the beat's picture becomes a removed-picture
/// cutaway showing the background. A later piece of a trimmed cutaway keeps
/// its pictures exactly; a looping or bouncing one cannot start mid-cycle.
fn remove_top_pictures(
    document: &ProjectDocument,
    source: &SourceNode,
    existing: &[Cutaway],
    range: std::ops::Range<i64>,
    rate: crate::FrameRate,
) -> Result<Vec<Cutaway>, EditError> {
    let mut kept = Vec::new();
    let mut covered = Vec::new();
    for cutaway in existing {
        let (low, high) = (cutaway.range.start().0, cutaway.range.end().0);
        if high <= range.start || range.end <= low {
            kept.push(cutaway.clone());
            continue;
        }
        if cutaway.removed {
            return Err(invalid(
                "this picture is already removed over part of the range",
            ));
        }
        covered.push(low.max(range.start)..high.min(range.end));
        if low < range.start {
            let mut before = cutaway.clone();
            before.range = FrameRange::new(ProjectFrame(low), ProjectFrame(range.start))
                .map_err(crate::DocumentError::from)?;
            kept.push(before);
        }
        if range.end < high {
            let skipped = range.end - low;
            let after_range = FrameRange::new(ProjectFrame(range.end), ProjectFrame(high))
                .map_err(crate::DocumentError::from)?;
            match cutaway.fit {
                CutawayFit::Hold | CutawayFit::Gap => {
                    match later_selection(cutaway.selection, skipped, rate)? {
                        Some(selection) => kept.push(Cutaway {
                            range: after_range,
                            selection,
                            ..cutaway.clone()
                        }),
                        // A gap cutaway already shows the beat past its pictures.
                        None if cutaway.fit == CutawayFit::Gap => {}
                        None => {
                            return Err(invalid(
                                "the cutaway holds its last picture after this range; delete to its end instead",
                            ));
                        }
                    }
                }
                CutawayFit::Loop | CutawayFit::Bounce => {
                    return Err(invalid(
                        "a looping cutaway cannot start mid-cycle; delete its picture to its end instead",
                    ));
                }
            }
        }
    }
    // Parts of the range no cutaway covers lose the beat's own picture.
    covered.sort_by_key(|part| part.start);
    let mut cursor = range.start;
    let mut bare = Vec::new();
    for part in covered
        .iter()
        .chain(std::iter::once(&(range.end..range.end)))
    {
        if cursor < part.start {
            bare.push(cursor..part.start);
        }
        cursor = cursor.max(part.end);
    }
    for part in bare {
        let (asset, selection, _) = source_pictures(source, part.clone(), rate)?;
        let selection = clamp_to_video(document, &asset, selection)?;
        kept.push(Cutaway {
            range: FrameRange::new(ProjectFrame(part.start), ProjectFrame(part.end))
                .map_err(crate::DocumentError::from)?,
            asset,
            selection,
            fit: CutawayFit::Hold,
            removed: true,
        });
    }
    kept.sort_by_key(|cutaway| cutaway.range.start());
    Ok(kept)
}

/// `selection` starting `frames` project frames later at its natural rate,
/// or `None` when that passes its end.
fn later_selection(
    selection: ExactSourceSpan,
    frames: i64,
    rate: crate::FrameRate,
) -> Result<Option<ExactSourceSpan>, EditError> {
    let base = selection.start().time_base;
    let per_frame = ExactRatio::new(
        i128::from(rate.denominator()) * i128::from(base.denominator()),
        i128::from(rate.numerator()) * i128::from(base.numerator()),
    )
    .map_err(crate::DocumentError::from)?;
    let ticks = ExactRatio::integer(frames)
        .checked_mul(per_frame)
        .and_then(|offset| offset.checked_add(selection.start().ticks))
        .map_err(crate::DocumentError::from)?;
    if !ticks.compare(selection.end().ticks).is_lt() {
        return Ok(None);
    }
    let start = SourcePoint {
        ticks,
        time_base: base,
    };
    Ok(Some(
        ExactSourceSpan::new(start, selection.end()).map_err(crate::DocumentError::from)?,
    ))
}

/// `selection` within its asset's measured video, so a range that reaches
/// endpoint-hold padding still records only real pictures.
pub(super) fn clamp_to_video(
    document: &ProjectDocument,
    asset: &crate::AssetId,
    selection: ExactSourceSpan,
) -> Result<ExactSourceSpan, EditError> {
    let video = document
        .assets()
        .get(asset)
        .and_then(|record| record.video)
        .ok_or_else(|| invalid("the beat's Original has no video"))?;
    let low = ExactRatio::integer(video.start().ticks);
    let high = ExactRatio::integer(video.end().ticks);
    let clamp = |ticks: ExactRatio| {
        if ticks.compare(low).is_lt() {
            low
        } else if ticks.compare(high).is_gt() {
            high
        } else {
            ticks
        }
    };
    let start = selection.start();
    let end = selection.end();
    ExactSourceSpan::new(
        SourcePoint {
            ticks: clamp(start.ticks),
            ..start
        },
        SourcePoint {
            ticks: clamp(end.ticks),
            ..end
        },
    )
    .map_err(|_| invalid("the deleted pictures lie outside the Original's video"))
}

/// The exact source pictures a rolled side showed over its output-local
/// frames `local`, which must play at the project's natural rate so that a
/// cutaway samples them identically.
fn shown_pictures(
    side: &crate::SourceRollSideResolution,
    local: std::ops::Range<i64>,
    rate: crate::FrameRate,
) -> Result<(crate::AssetId, ExactSourceSpan), EditError> {
    let offset = side.allocation_before.start().0;
    let physical = local
        .start
        .checked_add(offset)
        .zip(local.end.checked_add(offset));
    let (start, end) =
        physical.ok_or_else(|| EditError::new(EditErrorCode::TimingOverflow, "overflow"))?;
    let (asset, selection, natural) = source_pictures(&side.before, start..end, rate)?;
    if !natural {
        return Err(invalid(
            "a split edit keeps the pictures at their natural speed; this beat's picture is retimed",
        ));
    }
    Ok((asset, selection))
}

/// The exact Original pictures a Source shows over its physical-local frames
/// `physical`, through its full-span affine picture map, and whether that map
/// plays them at the project's natural rate.
pub(super) fn source_pictures(
    source: &SourceNode,
    physical: std::ops::Range<i64>,
    rate: crate::FrameRate,
) -> Result<(crate::AssetId, ExactSourceSpan, bool), EditError> {
    let SourceVideo::Stream { asset, span } = &source.video else {
        return Err(invalid("this beat shows no Original pictures"));
    };
    let mapping = source.video_mapping;
    let start = mapping.start_frames();
    let length = mapping
        .duration_frames(source.duration)
        .map_err(crate::DocumentError::from)?;
    let base = span.start().time_base;
    let ticks = ExactRatio::integer(span.end().ticks - span.start().ticks);
    let natural = ExactRatio::new(
        i128::from(rate.denominator()) * i128::from(base.denominator()),
        i128::from(rate.numerator()) * i128::from(base.numerator()),
    )
    .map_err(crate::DocumentError::from)?;
    let per_frame = ticks
        .checked_div(length)
        .map_err(crate::DocumentError::from)?;
    let point = |frame: i64| -> Result<SourcePoint, EditError> {
        let ticks = ExactRatio::integer(frame)
            .checked_sub(start)
            .and_then(|offset| offset.checked_mul(per_frame))
            .and_then(|offset| offset.checked_add(ExactRatio::integer(span.start().ticks)))
            .map_err(crate::DocumentError::from)?;
        Ok(SourcePoint {
            ticks,
            time_base: base,
        })
    };
    let selection = ExactSourceSpan::new(point(physical.start)?, point(physical.end)?)
        .map_err(crate::DocumentError::from)?;
    Ok((asset.clone(), selection, per_frame == natural))
}
