//! Pause insertion and framing replacement: the macro forms of `,h`/`:hold`
//! and `,c`/`,z`. The host resolves a pause's frozen picture; the planner
//! authors the same InsertTime and SetFraming commands as the native edits.

use super::*;
use crate::{AudioTimingId, ExactRatio, FrameDuration, Framing, HoldAudio, HoldRecipe};

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
        self.insert_pause_with_intent(trace_index, length, black, false)
    }

    pub(super) fn insert_ai_pause(
        &mut self,
        trace_index: usize,
        length: crate::PauseLength,
    ) -> Result<(), EditError> {
        self.insert_pause_with_intent(trace_index, length, false, true)
    }

    fn insert_pause_with_intent(
        &mut self,
        trace_index: usize,
        length: crate::PauseLength,
        black: bool,
        request_ai: bool,
    ) -> Result<(), EditError> {
        let duration = length.resolve(self.current.presentation_basis().frame_rate)?;
        let at = self.context.cursor;
        self.insert_hold_with_intent(trace_index, duration, request_ai, |planner| {
            // Black punctuation samples no picture and captures no framing.
            if black {
                Ok(super::PauseProvider {
                    video: crate::HoldVideo::Background,
                    picture_context: None,
                    audio: HoldAudio::Silence,
                })
            } else {
                (planner.resolve_pause)(&planner.current, super::PauseSite::Boundary { at })
            }
        })
    }

    /// `:reverse` / `:ping-pong`: a pause that plays the `length` before the
    /// cursor backwards, resolved by the host from the staged document.
    pub(super) fn insert_reverse(
        &mut self,
        trace_index: usize,
        length: crate::PauseLength,
        bounce: bool,
    ) -> Result<(), EditError> {
        let frames = length.resolve(self.current.presentation_basis().frame_rate)?;
        let duration = if bounce {
            FrameDuration::new(frames.frames() - 1)
                .ok()
                .filter(|duration| *duration != FrameDuration::ZERO)
                .ok_or_else(|| {
                    EditError::new(
                        EditErrorCode::InvalidDuration,
                        "a ping-pong needs at least two frames to bounce over",
                    )
                })?
        } else {
            frames
        };
        if frames.frames() > self.context.cursor.0 {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "there is not that much before the cursor to reverse",
            ));
        }
        let at = self.context.cursor;
        self.insert_hold(trace_index, duration, |planner| {
            (planner.resolve_pause)(
                &planner.current,
                super::PauseSite::Reverse { at, frames, bounce },
            )
        })
    }

    /// `,b` / `:bleep`: resolve the pictures of the Visual time range, cut
    /// it, then refill its time with a pause playing those pictures forward
    /// over a tone.
    pub(super) fn bleep(
        &mut self,
        trace_index: usize,
        register: crate::RegisterName,
        frequency_hz: u32,
        level: crate::GainDb,
    ) -> Result<(), EditError> {
        if self.context.visual_selection.is_none() {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "select the range to bleep with v first",
            ));
        }
        let target = self.resolve_selector(crate::SemanticSelector::VisualSelection)?;
        let SliceCaptureSelection::Range { range } = target.selection()?.clone() else {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "bleep a time range, not a group object",
            ));
        };
        if range.duration() == FrameDuration::ZERO {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "the bleeped range is empty",
            ));
        }
        // The pictures are resolved before the cut removes them.
        let provider = (self.resolve_pause)(
            &self.current,
            super::PauseSite::Bleep {
                at: range.end(),
                frames: range.duration(),
                frequency_hz,
                level,
            },
        )?;
        self.capture_instruction(trace_index, register, target, true)?;
        if self.context.cursor != range.start() {
            return Err(invalid("a bleep refills its time at the cut's join"));
        }
        self.insert_hold(trace_index, range.duration(), |_| Ok(provider))
    }

    /// `:lift`: cut the Visual selection, then fill its time with a silent
    /// black pause of exactly the cut length at the join.
    pub(super) fn lift(
        &mut self,
        trace_index: usize,
        register: crate::RegisterName,
    ) -> Result<(), EditError> {
        if self.context.visual_selection.is_none() {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "select the range to lift with v first",
            ));
        }
        self.capture_selector(
            trace_index,
            register,
            crate::SemanticSelector::VisualSelection,
            true,
        )?;
        let cut = self.trace[trace_index]
            .resolved_range
            .ok_or_else(|| invalid("a lift needs its cut range"))?;
        let duration = cut.duration();
        if duration == FrameDuration::ZERO {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "the lifted range is empty",
            ));
        }
        let at = self.context.cursor;
        if at != cut.start() {
            return Err(invalid("a lift refills its time at the cut's join"));
        }
        self.insert_hold(trace_index, duration, |_| {
            Ok(super::PauseProvider {
                video: crate::HoldVideo::Background,
                picture_context: None,
                audio: HoldAudio::Silence,
            })
        })
    }

    /// `,t` / `:tail`: a hanging tail on the selected Hold, or a new freeze
    /// pause carrying one at the cursor.
    pub(super) fn tail(
        &mut self,
        trace_index: usize,
        length: Option<crate::PauseLength>,
        effect: crate::TailEffect,
    ) -> Result<(), EditError> {
        let rate = self.current.presentation_basis().frame_rate;
        let length = length.map(|length| length.resolve(rate)).transpose()?;
        let selected_hold = self.context.selected_child.as_ref().and_then(|selected| {
            let index = *self.child_indices.get(selected)?;
            match &self.current.nodes()[selected].kind {
                NodeKind::Hold { recipe } => Some((
                    selected.clone(),
                    index
                        .checked_sub(1)
                        .map_or(self.bounds.0, |previous| self.child_ends[previous].1),
                    recipe.duration,
                )),
                _ => None,
            }
        });
        if self.context.visual_selection.is_some() {
            return Err(invalid("clear the Visual selection before adding a tail"));
        }
        let Some((hold, start, hold_duration)) = selected_hold else {
            let duration = length.ok_or_else(|| {
                EditError::new(
                    EditErrorCode::InvalidCommand,
                    "give the new tail pause a length, for example :tail 400ms",
                )
            })?;
            let at = self.context.cursor;
            // The picture is the ordinary freeze; the tail is a live
            // reference to what precedes the pause, so it needs no media.
            return self.insert_hold(trace_index, duration, |planner| {
                let mut provider =
                    (planner.resolve_pause)(&planner.current, super::PauseSite::Boundary { at })?;
                provider.audio = HoldAudio::Tail {
                    maximum: duration,
                    effect,
                };
                Ok(provider)
            });
        };
        if start.0 == 0 {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "nothing plays before this pause to ring into it",
            ));
        }
        let maximum = length.map_or(hold_duration, |length| length.min(hold_duration));
        // Only the sound changes: no picture is resolved, so any provider
        // (accepted, still or black) keeps its picture.
        self.charge_step(false)?;
        let SemanticAllocation::ParameterEdit { new_revision } =
            (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                step_index: self.steps.len(),
            })?
        else {
            return Err(invalid("a tail requires a parameter allocation"));
        };
        self.reserve_revision(&new_revision)?;
        let edit = LeafEdit::new(
            new_revision,
            Command::SetHoldAudio {
                node: hold.clone(),
                audio: HoldAudio::Tail { maximum, effect },
            },
        )?;
        self.commit_leaf(edit)?;
        self.trace[trace_index].resolved_parent = Some(self.context.parent.clone());
        self.trace[trace_index].resolved_selection =
            Some(SliceCaptureSelection::Child { node: hold });
        Ok(())
    }

    /// Insert one pause of `duration` at the cursor whose provider the host
    /// resolves on the staged document.
    fn insert_hold(
        &mut self,
        trace_index: usize,
        duration: FrameDuration,
        provider: impl FnOnce(&mut Self) -> Result<super::PauseProvider, EditError>,
    ) -> Result<(), EditError> {
        self.insert_hold_with_intent(trace_index, duration, false, provider)
    }

    fn insert_hold_with_intent(
        &mut self,
        trace_index: usize,
        duration: FrameDuration,
        request_ai: bool,
        provider: impl FnOnce(&mut Self) -> Result<super::PauseProvider, EditError>,
    ) -> Result<(), EditError> {
        if self.context.visual_selection.is_some() {
            return Err(invalid(
                "clear the Visual selection before inserting a pause",
            ));
        }
        let at = self.context.cursor;
        let target = self.current.insert_time_target(at)?;
        if !self.within_scope(&target.parent) {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "this boundary belongs to an enclosing group; insert the pause from that group",
            ));
        }
        let provider = provider(self)?;
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
        let hold = HoldRecipe {
            duration,
            video: provider.video,
            audio: provider.audio,
            picture_context: provider.picture_context,
        };
        let timing = AudioTimingId {
            allocation: new_revision.clone(),
            ordinal: 0,
        };
        let command = if request_ai {
            Command::InsertAiTime {
                at,
                hold,
                id: id.clone(),
                identities: split,
                timing,
            }
        } else {
            Command::InsertTime {
                at,
                hold,
                id: id.clone(),
                identities: split,
                timing,
            }
        };
        let edit = LeafEdit::new(new_revision, command)?;
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

    /// `SetAudio`: the selected direct child's clip gain or saturation, as
    /// the native `+`/`-`, `:gain` and `:saturate` author it.
    pub(super) fn set_audio(
        &mut self,
        trace_index: usize,
        change: crate::AudioChange,
    ) -> Result<(), EditError> {
        let ranged = self.context.visual_selection.is_some();
        let range_step = match change {
            crate::AudioChange::RangeStep { millidecibels } => Some(millidecibels),
            _ => None,
        };
        if ranged && range_step.is_none() {
            return Err(invalid(
                "clear the Visual selection before changing a beat's gain or saturation",
            ));
        }
        if !ranged && range_step.is_some() {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "select the range to change with v first",
            ));
        }
        let (selected, local) = self.beat_span(
            "select a beat before changing its gain or saturation",
            "a gain range stays inside one beat; select a range inside one beat",
        )?;
        let current = &self.current.nodes()[&selected].audio_treatments;
        let treatments = match range_step {
            Some(millidecibels) => {
                let range = crate::GainRange::new(
                    ExactRatio::integer(local.start),
                    ExactRatio::integer(local.end),
                )
                .map_err(crate::audio_gain::invalid)?;
                let clip = current.clip_gain().cloned().unwrap_or_default();
                current
                    .with_clip_gain_or_none(
                        clip.adjust_range(range, millidecibels)
                            .map_err(crate::audio_gain::invalid)?,
                    )
                    .map_err(crate::audio_gain::invalid)?
            }
            None => change.apply(current).map_err(crate::audio_gain::invalid)?,
        };
        if &treatments == current {
            return Err(invalid(
                "the beat already has that gain and saturation; no edit was made",
            ));
        }
        self.charge_step(false)?;
        let SemanticAllocation::ParameterEdit { new_revision } =
            (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                step_index: self.steps.len(),
            })?
        else {
            return Err(invalid("an audio change requires a parameter allocation"));
        };
        self.reserve_revision(&new_revision)?;
        let edit = LeafEdit::new(
            new_revision,
            Command::SetAudioTreatments {
                node: selected.clone(),
                treatments,
            },
        )?;
        self.commit_leaf(edit)?;
        self.finish_beat_span(trace_index, selected, local)
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

    /// `SetCutaway`: the Original moment in `register` over the whole
    /// selected direct child, as the native `:cutaway register=` places it.
    pub(super) fn set_cutaway(
        &mut self,
        trace_index: usize,
        register: crate::RegisterName,
        fit: crate::CutawayFit,
    ) -> Result<(), EditError> {
        let (selected, local) = self.beat_span(
            "select a beat before placing a cutaway over it",
            "a cutaway stays inside one beat; select a range inside one beat",
        )?;
        let (host, offset) = crate::cutaway_host(&self.current, &selected).ok_or_else(|| {
            EditError::new(
                EditErrorCode::WrongNodeKind,
                "cutaways belong to a source or pause beat",
            )
        })?;
        let value = if let Some(value) = self.writes.get(&register) {
            value.clone()
        } else {
            let value = self.bank.get(&register).cloned();
            self.inputs.insert(register, value.clone());
            value.ok_or_else(|| invalid("the cutaway register is empty"))?
        };
        if !matches!(value.as_ref(), RegisterValue::Original { .. }) {
            return Err(invalid(
                "a cutaway shows a copied Original moment from its register",
            ));
        }
        let source = (self.resolve_original)(&self.current, value.as_ref())?;
        let crate::SourceVideo::Stream { asset, span } = &source.video else {
            return Err(invalid("the copied Original moment has no picture"));
        };
        let selection = source
            .video_mapping
            .selection_in_source(*span, source.duration)
            .map_err(crate::DocumentError::from)?;
        let range = FrameRange::new(
            ProjectFrame(offset + local.start),
            ProjectFrame(offset + local.end),
        )
        .map_err(crate::DocumentError::from)?;
        let mut cutaways = self.current.nodes()[&host].cutaways.clone();
        if cutaways.iter().any(|cutaway| {
            cutaway.range.start() < range.end() && range.start() < cutaway.range.end()
        }) {
            return Err(invalid(
                "this beat already shows a cutaway there; clear it first",
            ));
        }
        let position = cutaways.partition_point(|cutaway| cutaway.range.start() < range.start());
        cutaways.insert(
            position,
            crate::Cutaway {
                range,
                asset: asset.clone(),
                selection,
                fit,
                removed: false,
            },
        );
        self.charge_step(false)?;
        let SemanticAllocation::ParameterEdit { new_revision } =
            (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                step_index: self.steps.len(),
            })?
        else {
            return Err(invalid("a cutaway requires a parameter allocation"));
        };
        self.reserve_revision(&new_revision)?;
        let edit = LeafEdit::new(
            new_revision,
            Command::SetCutaways {
                node: host,
                cutaways,
            },
        )?;
        self.commit_leaf(edit)?;
        self.finish_beat_span(trace_index, selected, local)
    }

    /// `SetCaption`: one line of text over the selected direct child from
    /// `delay` after its start to its end, as the native `:caption` places it.
    pub(super) fn set_caption(
        &mut self,
        trace_index: usize,
        text: &str,
        placement: crate::CaptionPlacement,
        delay: Option<crate::PauseLength>,
        reveal: Option<std::num::NonZeroU32>,
    ) -> Result<(), EditError> {
        let ranged = self.context.visual_selection.is_some();
        if ranged && delay.is_some() {
            return Err(invalid(
                "a Visual range already sets where the caption starts; leave out delay",
            ));
        }
        let (selected, mut local) = self.beat_span(
            "select a beat before captioning it",
            "a caption stays inside one beat; select a range inside one beat",
        )?;
        let delay = delay
            .map(|delay| delay.resolve(self.current.presentation_basis().frame_rate))
            .transpose()?
            .map_or(0, |delay| delay.frames());
        if delay >= local.end - local.start {
            return Err(invalid(
                "the caption delay reaches past the end of the beat",
            ));
        }
        local.start += delay;
        let (host, offset) = crate::cutaway_host(&self.current, &selected).ok_or_else(|| {
            EditError::new(
                EditErrorCode::WrongNodeKind,
                "captions belong to a source or pause beat",
            )
        })?;
        let caption = crate::Caption {
            range: FrameRange::new(
                ProjectFrame(offset + local.start),
                ProjectFrame(offset + local.end),
            )
            .map_err(crate::DocumentError::from)?,
            text: text.to_owned(),
            placement,
            reveal,
        };
        let mut captions = self.current.nodes()[&host].captions.clone();
        let position =
            captions.partition_point(|existing| existing.range.start() <= caption.range.start());
        captions.insert(position, caption);
        crate::caption::validate(&captions).map_err(|_| {
            invalid("this beat already shows a caption there at that placement; clear it first")
        })?;
        self.charge_step(false)?;
        let SemanticAllocation::ParameterEdit { new_revision } =
            (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                step_index: self.steps.len(),
            })?
        else {
            return Err(invalid("a caption requires a parameter allocation"));
        };
        self.reserve_revision(&new_revision)?;
        let edit = LeafEdit::new(
            new_revision,
            Command::SetCaptions {
                node: host,
                captions,
            },
        )?;
        self.commit_leaf(edit)?;
        self.finish_beat_span(trace_index, selected, local)
    }

    /// The selected direct child and its whole local range, or the one
    /// direct child containing the nonempty Visual time range and that range
    /// in the child's own clock.
    fn beat_span(
        &self,
        unselected: &str,
        across: &str,
    ) -> Result<(NodeId, std::ops::Range<i64>), EditError> {
        if self.context.visual_selection.is_some() {
            let (child, start, _, range) = self.range_in_child(across)?;
            if range.duration() == FrameDuration::ZERO {
                return Err(EditError::new(
                    EditErrorCode::SelectionUnavailable,
                    "the Visual range is empty",
                ));
            }
            return Ok((
                child,
                (range.start().0 - start.0)..(range.end().0 - start.0),
            ));
        }
        let selected = self
            .context
            .selected_child
            .clone()
            .ok_or_else(|| EditError::new(EditErrorCode::SelectionUnavailable, unselected))?;
        let Some(&index) = self.child_indices.get(&selected) else {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "the beat must be a direct child of the current Sequence",
            ));
        };
        let start = index
            .checked_sub(1)
            .map_or(self.bounds.0, |previous| self.child_ends[previous].1);
        Ok((selected, 0..self.child_ends[index].1.0 - start.0))
    }

    /// Trace a beat-span edit: a Visual range records its exact range and
    /// stays selected, so `+` can be pressed again over it; a whole beat
    /// records the child.
    fn finish_beat_span(
        &mut self,
        trace_index: usize,
        selected: NodeId,
        local: std::ops::Range<i64>,
    ) -> Result<(), EditError> {
        self.trace[trace_index].resolved_parent = Some(self.context.parent.clone());
        if self.context.visual_selection.is_some() {
            let index = self.child_indices[&selected];
            let start = index
                .checked_sub(1)
                .map_or(self.bounds.0, |previous| self.child_ends[previous].1);
            self.trace[trace_index].resolved_range = Some(
                FrameRange::new(
                    ProjectFrame(start.0 + local.start),
                    ProjectFrame(start.0 + local.end),
                )
                .map_err(crate::DocumentError::from)?,
            );
        } else {
            self.trace[trace_index].resolved_selection =
                Some(SliceCaptureSelection::Child { node: selected });
        }
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
