//! `:audio-shift` (specification §6.5): sound from the Edit cursor to the end
//! of its group slides by a whole number of frames, across cuts, while every
//! picture stays put. Each beat's own sound over the affected frames is muted
//! and plays again as a root sound event of the same Original audio through
//! the beat's exact audio mapping, translated by the shift. Root sounds follow
//! later root-clock edits. A forward shift leaves silence at the cursor; a
//! backward shift also mutes the frames it slides over. Sound pushed past the
//! group's end refuses unless `trim` cuts it there or `extend` holds the last
//! picture for exactly the shift; nothing is silently discarded.

use super::*;
use crate::{ExactFrameRange, ExactRatio, InstancePath, SourceAudioMapping, SourceVideo};
use std::num::NonZeroU32;

/// One beat's sounding stretch, in absolute Edit frames.
struct Segment {
    child: NodeId,
    child_start: i64,
    frames: std::ops::Range<i64>,
}

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: crate::SemanticSpeechProvider,
    P: FnMut(&ProjectDocument, super::PauseSite) -> Result<super::PauseProvider, EditError>,
{
    pub(super) fn audio_shift(
        &mut self,
        trace_index: usize,
        shift: i64,
        trim: bool,
        extend: bool,
    ) -> Result<(), EditError> {
        if shift == 0 {
            return Err(invalid("a zero audio shift moves nothing"));
        }
        if self.context.visual_selection.is_some() {
            return Err(invalid(
                ":audio-shift slides sound from the Edit cursor; finish or clear the Visual selection first",
            ));
        }
        // Edit frames equal root frames, and so root sound frames, only below
        // ordinary Sequences.
        let mut ancestor = Some(self.context.parent.clone());
        while let Some(node) = ancestor {
            if !matches!(self.current.nodes()[&node].kind, NodeKind::Sequence { .. }) {
                return Err(invalid(
                    "an audio shift needs ordinary groups above it; leave the Repeat or speed change first",
                ));
            }
            ancestor = self.current.parent_of(&node);
        }
        let cursor = self.context.cursor.0;
        let group_start = self.bounds.0.0;
        let group_end = self.child_ends.last().map_or(group_start, |(_, end)| end.0);
        if cursor >= group_end {
            return Err(invalid(
                "the Edit cursor is at the end of its group; nothing follows to shift",
            ));
        }
        let muted_from = cursor.min(cursor + shift);
        if muted_from < group_start {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                format!(
                    "the sound would move {} frames before this group starts",
                    group_start - muted_from
                ),
            ));
        }
        let overflow = shift.max(0);
        if overflow > 0 && !trim && !extend {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                format!(
                    "the last {overflow} frames of sound would pass the group's end; add overflow=trim to cut them or extend=hold to hold its last picture"
                ),
            ));
        }

        // Every beat meeting the muted stretch, and the stretch it sounds.
        let mut muted = Vec::new();
        let mut segments = Vec::new();
        let mut start = group_start;
        for (child, end) in self.child_ends.clone() {
            let end = end.0;
            let low = start.max(muted_from);
            if low < end {
                let sounding = start.max(cursor)..end;
                match &self.current.nodes()[&child].kind {
                    NodeKind::Hold { recipe }
                        if matches!(recipe.audio, crate::HoldAudio::Silence) => {}
                    NodeKind::Hold { .. } => {
                        return Err(invalid(
                            "a pause with its own sound lies in the shifted stretch; make it silent or shift from after it",
                        ));
                    }
                    _ => {
                        muted.push((child.clone(), (low - start)..(end - start)));
                        if sounding.start < sounding.end {
                            segments.push(Segment {
                                child: child.clone(),
                                child_start: start,
                                frames: sounding,
                            });
                        }
                    }
                }
            }
            start = end;
        }
        if segments.is_empty() {
            return Err(invalid("no sound follows the Edit cursor in this group"));
        }

        for (child, local) in muted {
            self.parameter_edit(self.muted(&child, local)?)?;
        }
        let pause = if extend && overflow > 0 {
            let restore = (self.context.cursor, self.context.visual_selection.take());
            self.context.cursor = ProjectFrame(group_end);
            let inserted = self.insert_pause(
                trace_index,
                crate::PauseLength::Frames {
                    frames: u32::try_from(overflow)
                        .ok()
                        .and_then(NonZeroU32::new)
                        .ok_or_else(|| invalid("the shift is too long to hold"))?,
                },
                false,
            );
            (self.context.cursor, self.context.visual_selection) = restore;
            inserted?;
            Some(
                self.context
                    .selected_child
                    .clone()
                    .filter(|node| matches!(self.current.nodes()[node].kind, NodeKind::Hold { .. }))
                    .ok_or_else(|| invalid("the hold after the shifted sound is not selectable"))?,
            )
        } else {
            None
        };
        for segment in segments {
            // Only trim cuts sound that a forward shift pushes past the end.
            let end = if trim {
                segment.frames.end.min(group_end - overflow)
            } else {
                segment.frames.end
            };
            let heard = segment.frames.start..end;
            if heard.start >= heard.end {
                continue;
            }
            let id = self.shifted_sound(&segment, heard.clone(), shift)?;
            if let Some(pause) = &pause
                && heard.end + shift > group_end
            {
                self.parameter_edit(Command::SetSoundAllowance {
                    sound: id,
                    issuer: crate::SoundHoldIssuer::Node {
                        instance: InstancePath {
                            node: pause.clone(),
                            repeats: Vec::new(),
                        },
                    },
                    allowed: true,
                })?;
            }
        }
        self.trace[trace_index].resolved_parent = Some(self.context.parent.clone());
        self.trace[trace_index].resolved_range = Some(
            FrameRange::new(ProjectFrame(muted_from), ProjectFrame(group_end + overflow))
                .map_err(crate::DocumentError::from)?,
        );
        Ok(())
    }

    /// One root sound event playing `segment`'s own sound over `heard`
    /// (absolute Edit frames), `shift` frames later.
    fn shifted_sound(
        &mut self,
        segment: &Segment,
        heard: std::ops::Range<i64>,
        shift: i64,
    ) -> Result<crate::SoundId, EditError> {
        let (host, offset) =
            crate::cutaway_host(&self.current, &segment.child).ok_or_else(|| {
                EditError::new(
                    EditErrorCode::WrongNodeKind,
                    "an audio shift crosses a group; shift inside it, or from after it",
                )
            })?;
        let NodeKind::Source { source } = &self.current.nodes()[&host].kind else {
            return Err(EditError::new(
                EditErrorCode::WrongNodeKind,
                "an audio shift moves the sound of Original source beats",
            ));
        };
        let source = source.clone();
        let audio = source
            .audio
            .clone()
            .filter(|_| source.link == crate::LinkRelation::Linked)
            .ok_or_else(|| invalid("a beat in the shifted stretch has no linked sound"))?;
        if !matches!(source.video, SourceVideo::Stream { .. }) {
            return Err(invalid(
                "an audio shift moves the sound of Original source beats",
            ));
        }
        let rate = self.current.presentation_basis().frame_rate;
        let mapping_shift = source
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
                    "a beat's sound is fitted to its picture; it has no natural-rate placement to shift",
                ));
            }
            SourceAudioMapping::Duration { frames } => (ExactRatio::ZERO, frames),
            SourceAudioMapping::Placement { start, frames }
            | SourceAudioMapping::SelectedPlacement { start, frames, .. } => (start, frames),
        };
        // Host-physical frame of an absolute Edit frame inside the beat.
        let physical = |frame: i64| frame - segment.child_start + offset;
        let first = physical(heard.start);
        // Mapping coordinates are before the audio offset.
        let wanted_start = ExactRatio::integer(first)
            .checked_sub(mapping_shift)
            .map_err(crate::DocumentError::from)?;
        let wanted_end = wanted_start
            .checked_add(ExactRatio::integer(heard.end - heard.start))
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
            return Err(invalid("a beat in the shifted stretch has no sound there"));
        }
        // Host-physical frame p sounds at root frame p + delta.
        let delta = ExactRatio::integer(heard.start + shift - first);
        let moved = |value: ExactRatio| {
            value
                .checked_add(delta)
                .map_err(|error| EditError::from(crate::DocumentError::from(error)))
        };
        let label = self.current.nodes()[&segment.child].label.clone();
        let event = crate::SoundEvent {
            owner: self.current.root().clone(),
            label: format!("Shifted {shift:+}f · {label}"),
            source: audio,
            mapping: SourceAudioMapping::SelectedPlacement {
                start: moved(start)?,
                frames,
                selection: ExactFrameRange::new(moved(low)?, moved(high)?)?,
            },
            offset: source.audio_offset,
            treatments: crate::AudioTreatments::default(),
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
            return Err(invalid("a shifted sound requires a sound allocation"));
        };
        if self.current.sounds().contains_key(&id) {
            return Err(EditError::new(
                EditErrorCode::IdentityConflict,
                "the shifted sound's identity is already present",
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
        Ok(id)
    }

    fn parameter_edit(&mut self, command: Command) -> Result<(), EditError> {
        self.charge_step(false)?;
        let SemanticAllocation::ParameterEdit { new_revision } =
            (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                step_index: self.steps.len(),
            })?
        else {
            return Err(invalid("an audio shift requires a parameter allocation"));
        };
        self.reserve_revision(&new_revision)?;
        self.commit_leaf(LeafEdit::new(new_revision, command)?)
    }
}
