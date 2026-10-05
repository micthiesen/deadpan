//! Audio-only and video-only repeats (specification §6.5): a role repeats
//! over a fixed host interval without inserting picture time.
//!
//! Audio: the range's own sound keeps its first play; every later play is a
//! sample-timed root sound event of the same Original audio, through the host
//! Source's exact audio mapping translated to the play's start, and the beat's
//! own sound is muted under those plays. Root sounds follow root-clock edits;
//! capturing content that would leave such a sound behind is refused (see
//! `edit_slice`). Video: a looping cutaway of the
//! range's pictures covers the later plays while the host's sound continues.
//! A repeat that would run past its beat refuses unless `trim` cuts it at the
//! beat's end; nothing is silently discarded.

use super::*;
use crate::{Cutaway, CutawayFit, ExactFrameRange, ExactRatio, SourceAudioMapping, SourceVideo};

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: FnMut(&ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError>,
    P: FnMut(&ProjectDocument, super::PauseSite) -> Result<super::PauseProvider, EditError>,
{
    pub(super) fn role_repeat(
        &mut self,
        trace_index: usize,
        role: crate::MediaRole,
        plays: u32,
        trim: bool,
    ) -> Result<(), EditError> {
        if role == crate::MediaRole::Linked {
            return Err(invalid(
                "a linked repeat inserts time; use r, or choose role=audio or role=video",
            ));
        }
        if plays < 2 {
            return Err(invalid("one play repeats nothing; give 2 or more plays"));
        }
        // Edit frames equal root frames only below ordinary Sequences.
        let mut ancestor = Some(self.context.parent.clone());
        while let Some(node) = ancestor {
            if !matches!(self.current.nodes()[&node].kind, NodeKind::Sequence { .. }) {
                return Err(invalid(
                    "a role repeat needs ordinary groups above it; leave the Repeat or speed change first",
                ));
            }
            ancestor = self.current.parent_of(&node);
        }
        let (child, child_start, child_end, range) = self.range_in_child(
            "a role repeat stays inside one beat; select a range inside one beat",
        )?;
        let length = range.duration().frames();
        let total = length
            .checked_mul(i64::from(plays))
            .ok_or_else(|| EditError::new(EditErrorCode::TimingOverflow, "repeat overflows"))?;
        let wanted =
            range.start().0.checked_add(total).ok_or_else(|| {
                EditError::new(EditErrorCode::TimingOverflow, "repeat end overflows")
            })?;
        let end = if wanted > child_end.0 {
            if !trim {
                return Err(EditError::new(
                    EditErrorCode::SelectionUnavailable,
                    format!(
                        "{plays} plays need {} more frames than this beat has; add overflow=trim to cut the repeats at the beat's end",
                        wanted - child_end.0
                    ),
                ));
            }
            child_end.0
        } else {
            wanted
        };
        let repeats = range.end().0..end;
        if repeats.is_empty() {
            return Err(invalid(
                "the range ends at its beat; nothing is left to repeat over",
            ));
        }
        let (host, offset) = crate::cutaway_host(&self.current, &child).ok_or_else(|| {
            EditError::new(
                EditErrorCode::WrongNodeKind,
                "a role repeat acts inside a source beat; open a group with Enter first",
            )
        })?;
        let NodeKind::Source { source } = &self.current.nodes()[&host].kind else {
            return Err(EditError::new(
                EditErrorCode::WrongNodeKind,
                "a role repeat acts inside a source beat",
            ));
        };
        let source = source.clone();
        // Host-physical frame of an absolute Edit frame inside the child.
        let physical = |frame: i64| frame - child_start.0 + offset;
        let first = physical(range.start().0)..physical(range.end().0);
        match role {
            crate::MediaRole::Video => {
                let rate = self.current.presentation_basis().frame_rate;
                let (asset, selection, natural) =
                    super::split_edit::source_pictures(&source, first, rate)?;
                if !natural {
                    return Err(invalid(
                        "a video-only repeat plays pictures at their natural speed; this beat's picture is retimed",
                    ));
                }
                let selection =
                    super::split_edit::clamp_to_video(&self.current, &asset, selection)?;
                let range = FrameRange::new(
                    ProjectFrame(physical(repeats.start)),
                    ProjectFrame(physical(repeats.end)),
                )
                .map_err(crate::DocumentError::from)?;
                let mut cutaways = self.current.nodes()[&host].cutaways.clone();
                if cutaways.iter().any(|cutaway| {
                    cutaway.range.start() < range.end() && range.start() < cutaway.range.end()
                }) {
                    return Err(invalid(
                        "a cutaway already covers part of these repeats; clear it first",
                    ));
                }
                cutaways.push(Cutaway {
                    range,
                    asset,
                    selection,
                    fit: CutawayFit::Loop,
                    removed: false,
                });
                cutaways.sort_by_key(|cutaway| cutaway.range.start());
                self.parameter_leaf(Command::SetCutaways {
                    node: host,
                    cutaways,
                })?;
            }
            _ => {
                let audio = source
                    .audio
                    .clone()
                    .filter(|_| source.link == crate::LinkRelation::Linked)
                    .ok_or_else(|| invalid("this beat has no linked sound to repeat"))?;
                if !matches!(source.video, SourceVideo::Stream { .. }) {
                    return Err(invalid("a role repeat acts on an Original source beat"));
                }
                let rate = self.current.presentation_basis().frame_rate;
                let shift = source
                    .audio_mapping
                    .start_frames_with_offset(source.audio_offset, rate)
                    .and_then(|with| with.checked_sub(source.audio_mapping.start_frames()))
                    .map_err(crate::DocumentError::from)?;
                let selected = source
                    .audio_mapping
                    .selection_frames(source.duration)
                    .map_err(crate::DocumentError::from)?;
                let (start, frames) = match source.audio_mapping {
                    SourceAudioMapping::FitBeat => {
                        return Err(invalid(
                            "this beat's sound is fitted to its picture; it has no natural-rate placement to repeat",
                        ));
                    }
                    SourceAudioMapping::Duration { frames } => (ExactRatio::ZERO, frames),
                    SourceAudioMapping::Placement { start, frames }
                    | SourceAudioMapping::SelectedPlacement { start, frames, .. } => {
                        (start, frames)
                    }
                };
                let label = self.current.nodes()[&child].label.clone();
                // The later plays' host sound is muted; the first play is the
                // host's own sound.
                self.parameter_leaf(self.muted(
                    &child,
                    (repeats.start - child_start.0)..(repeats.end - child_start.0),
                )?)?;

                let mut play = 1u32;
                let mut onset = range.end().0;
                while onset < end {
                    let heard = length.min(end - onset);
                    // Mapping coordinates are before the audio offset.
                    let wanted_start = ExactRatio::integer(first.start)
                        .checked_sub(shift)
                        .map_err(crate::DocumentError::from)?;
                    let wanted_end = wanted_start
                        .checked_add(ExactRatio::integer(heard))
                        .map_err(crate::DocumentError::from)?;
                    let low = if wanted_start.compare(selected.start).is_lt() {
                        selected.start
                    } else {
                        wanted_start
                    };
                    let high = if wanted_end.compare(selected.end).is_gt() {
                        selected.end
                    } else {
                        wanted_end
                    };
                    if !low.compare(high).is_lt() {
                        return Err(invalid("the selected range has no sound to repeat"));
                    }
                    // Host-physical frame p sounds at root frame p + delta: a
                    // root sound follows root-clock edits; copies that would
                    // leave it behind are refused at capture.
                    let delta = ExactRatio::integer(onset - first.start);
                    let moved = |value: ExactRatio| {
                        value
                            .checked_add(delta)
                            .map_err(|error| EditError::from(crate::DocumentError::from(error)))
                    };
                    let event = crate::SoundEvent {
                        owner: self.current.root().clone(),
                        label: format!("Repeat {} of {plays} · {label}", play + 1),
                        source: audio.clone(),
                        mapping: SourceAudioMapping::SelectedPlacement {
                            start: moved(start)?,
                            frames,
                            selection: ExactFrameRange::new(moved(low)?, moved(high)?)?,
                        },
                        offset: source.audio_offset,
                        gain_millidecibels: 0,
                        start_edge: crate::AudioEdgePolicy::Automatic,
                        end_edge: crate::AudioEdgePolicy::Automatic,
                        overflow: crate::SoundOverflowPolicy::Reject,
                    };
                    self.charge_step(false)?;
                    let SemanticAllocation::Sound { new_revision, id } =
                        (self.allocate)(SemanticAllocationRequest::Sound {
                            step_index: self.steps.len(),
                        })?
                    else {
                        return Err(invalid("a repeated sound requires a sound allocation"));
                    };
                    if self.current.sounds().contains_key(&id) {
                        return Err(EditError::new(
                            EditErrorCode::IdentityConflict,
                            "the repeated sound's identity is already present",
                        ));
                    }
                    self.reserve_revision(&new_revision)?;
                    self.commit_leaf(LeafEdit::new(
                        new_revision,
                        Command::SetSound { id, event },
                    )?)?;
                    play += 1;
                    onset += length;
                }
            }
        }
        self.context.visual_selection = None;
        self.trace[trace_index].resolved_parent = Some(self.context.parent.clone());
        self.trace[trace_index].resolved_range = Some(
            FrameRange::new(range.start(), ProjectFrame(end))
                .map_err(crate::DocumentError::from)?,
        );
        Ok(())
    }

    /// Commit one parameter edit as its own staged leaf.
    fn parameter_leaf(&mut self, command: Command) -> Result<(), EditError> {
        self.charge_step(false)?;
        let SemanticAllocation::ParameterEdit { new_revision } =
            (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                step_index: self.steps.len(),
            })?
        else {
            return Err(invalid("a role repeat requires a parameter allocation"));
        };
        self.reserve_revision(&new_revision)?;
        self.commit_leaf(LeafEdit::new(new_revision, command)?)
    }
}
