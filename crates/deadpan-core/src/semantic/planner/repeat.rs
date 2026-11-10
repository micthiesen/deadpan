//! Repeat resolves its selector once on the staged document and authors one
//! leaf. It neither reads nor writes a copied-content register.

use super::*;

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: crate::SemanticSpeechProvider,
    P: FnMut(&ProjectDocument, super::PauseSite) -> Result<super::PauseProvider, EditError>,
{
    pub(super) fn set_repeat_plays(
        &mut self,
        trace_index: usize,
        plays: u32,
    ) -> Result<(), EditError> {
        if self.context.visual_selection.is_some() {
            return Err(invalid(
                "clear the Visual selection before setting Repeat total plays",
            ));
        }
        let selected = self.context.selected_child.clone().ok_or_else(|| {
            EditError::new(
                EditErrorCode::SelectionUnavailable,
                "select an existing Repeat before setting total plays",
            )
        })?;
        let slot = self.child_indices.get(&selected).copied().ok_or_else(|| {
            EditError::new(
                EditErrorCode::SelectionUnavailable,
                "the selected Repeat must be a direct child of the current Sequence",
            )
        })?;
        if !matches!(
            self.current.nodes()[&selected].kind,
            NodeKind::Repeat { .. }
        ) {
            return Err(EditError::new(
                EditErrorCode::WrongNodeKind,
                "set-repeat-plays requires an existing Repeat",
            ));
        }
        let start = slot
            .checked_sub(1)
            .map_or(self.bounds.0, |previous| self.child_ends[previous].1);
        let range =
            FrameRange::new(start, self.child_ends[slot].1).map_err(crate::DocumentError::from)?;
        self.charge_step(false)?;
        let allocation = (self.allocate)(SemanticAllocationRequest::SetRepeatPlays {
            step_index: self.steps.len(),
        })?;
        let SemanticAllocation::SetRepeatPlays { new_revision } = allocation else {
            return Err(invalid(
                "macro count setting requires a SetRepeatPlays allocation",
            ));
        };
        self.reserve_revision(&new_revision)?;
        let timing = AudioTimingId {
            allocation: new_revision.clone(),
            ordinal: 0,
        };
        // Even an unchanged count authors the supplied fresh revision, matching
        // the ordinary setter. Existing Repeat identities and clocks survive.
        let edit = LeafEdit::new(
            new_revision,
            Command::SetRepeatPlays {
                node: selected.clone(),
                plays,
                timing,
            },
        )?;
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
        self.context.cursor = start;
        self.context.selected_child = Some(selected.clone());
        self.refresh_children()?;
        self.trace[trace_index].resolved_selection =
            Some(SliceCaptureSelection::Child { node: selected });
        self.trace[trace_index].resolved_range = Some(range);
        self.trace[trace_index].resolved_parent = Some(self.context.parent.clone());
        Ok(())
    }

    pub(super) fn repeat(
        &mut self,
        trace_index: usize,
        selector: &SemanticSelector,
        plays: u32,
        escalation: Option<crate::RepeatEscalation>,
    ) -> Result<(), EditError> {
        if let Some(escalation) = &escalation {
            if escalation.speed.is_some() {
                return Err(invalid(
                    "a speed step needs the Repeat's plays; give it with :repeat N speed-step=",
                ));
            }
            escalation
                .validate(plays)
                .map_err(|error| invalid(&error.to_string()))?;
        }
        self.charge_step(false)?;
        let target = self.resolve_selector(selector)?;
        let selection = target.selection()?.clone();
        // `rib` repeats only the beat's content; its attachments stay with the
        // first play. Refuse unsupported owners before authoring anything.
        let first_play_only = !target.attachments.is_owned();
        if first_play_only && let SliceCaptureSelection::Child { node } = &selection {
            self.check_first_play_attachments(node)?;
        }
        let plan = self
            .current
            .repeat_selection(&target.parent, &selection, plays)?;
        let allocation = (self.allocate)(SemanticAllocationRequest::Repeat {
            step_index: self.steps.len(),
            required_split_ids: plan.required_split_ids,
            needs_group: plan.needs_group,
        })?;
        let SemanticAllocation::Repeat {
            new_revision,
            identities,
        } = allocation
        else {
            return Err(invalid("macro repeat requires a Repeat allocation"));
        };
        if identities.group.is_some() != plan.needs_group
            || identities.split.nodes.len() != plan.required_split_ids
        {
            return Err(invalid(
                "macro Repeat requires its exact group and Split identities",
            ));
        }
        self.reserve_revision(&new_revision)?;
        for node in std::iter::once(&identities.repeat)
            .chain(identities.group.iter())
            .chain(&identities.split.nodes)
        {
            self.reserve_node(node)?;
        }
        let selected = identities.repeat.clone();
        let timing = AudioTimingId {
            allocation: new_revision.clone(),
            ordinal: 0,
        };
        let edit = LeafEdit::new(
            new_revision,
            Command::RepeatSelection {
                parent: target.parent.clone(),
                selection: selection.clone(),
                plays,
                identities,
                timing,
            },
        )?;
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
        if first_play_only {
            self.keep_attachments_on_first_play(&selected)?;
        }
        if let Some(escalation) = escalation {
            self.charge_step(false)?;
            let SemanticAllocation::ParameterEdit { new_revision } =
                (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                    step_index: self.steps.len(),
                })?
            else {
                return Err(invalid("Repeat escalation requires a parameter allocation"));
            };
            self.reserve_revision(&new_revision)?;
            let edit = LeafEdit::new(
                new_revision,
                Command::SetRepeatEscalation {
                    node: selected.clone(),
                    escalation: Some(escalation),
                },
            )?;
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
        }
        self.continue_target(&target, plan.range.start(), Some(selected))?;
        self.trace[trace_index].resolved_parent = Some(target.parent);
        self.trace[trace_index].resolved_selection = Some(selection);
        self.trace[trace_index].resolved_range = Some(plan.range);
        Ok(())
    }

    /// `SetRepeat`: wrap a plain selected beat, or change the selected Repeat,
    /// then set its gaps and escalation, each as its own leaf.
    pub(super) fn set_repeat(
        &mut self,
        trace_index: usize,
        plays: Option<std::num::NonZeroU32>,
        gaps: Option<&[crate::PauseLength]>,
        escalation: Option<crate::RepeatEscalation>,
    ) -> Result<(), EditError> {
        if self.context.visual_selection.is_some() {
            return Err(invalid(
                "clear the Visual selection before changing a Repeat",
            ));
        }
        let selected = self.context.selected_child.clone().ok_or_else(|| {
            EditError::new(
                EditErrorCode::SelectionUnavailable,
                "select a beat or Repeat before changing its plays, gaps or escalation",
            )
        })?;
        if !self.child_indices.contains_key(&selected) {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "the selected beat must be a direct child of the current Sequence",
            ));
        }
        match &self.current.nodes()[&selected].kind {
            NodeKind::Repeat { iterations, .. } => {
                if let Some(plays) = plays.filter(|plays| plays.get() != iterations.len()) {
                    self.set_repeat_plays(trace_index, plays.get())?;
                }
            }
            _ => {
                let plays = plays.ok_or_else(|| {
                    EditError::new(
                        EditErrorCode::WrongNodeKind,
                        "select a Repeat, or give a total play count to wrap this beat",
                    )
                })?;
                self.repeat(
                    trace_index,
                    &SemanticSelector::SelectedBeat,
                    plays.get(),
                    None,
                )?;
            }
        }
        if let Some(gaps) = gaps {
            self.set_repeat_gaps(trace_index, gaps)?;
        }
        if let Some(escalation) = escalation {
            self.set_escalation(escalation)?;
        }
        // Report the Repeat's final extent, independently of which parts ran.
        let repeat = self
            .context
            .selected_child
            .clone()
            .ok_or_else(|| invalid("the changed Repeat is no longer selected"))?;
        let slot = self.child_indices[&repeat];
        let start = slot
            .checked_sub(1)
            .map_or(self.bounds.0, |previous| self.child_ends[previous].1);
        self.context.cursor = start;
        self.trace[trace_index].resolved_selection =
            Some(SliceCaptureSelection::Child { node: repeat });
        self.trace[trace_index].resolved_range = Some(
            FrameRange::new(start, self.child_ends[slot].1).map_err(crate::DocumentError::from)?,
        );
        self.trace[trace_index].resolved_parent = Some(self.context.parent.clone());
        Ok(())
    }

    /// Replace the selected Repeat's gain and zoom steps; a step-free value
    /// removes them. The speed recipe belongs to [`Self::speed_plays`] and is
    /// kept.
    fn set_escalation(&mut self, escalation: crate::RepeatEscalation) -> Result<(), EditError> {
        if escalation.speed.is_some() {
            return Err(invalid("a speed step is set with speed-step="));
        }
        let selected = self
            .context
            .selected_child
            .clone()
            .ok_or_else(|| invalid("select a Repeat before escalating it"))?;
        let NodeKind::Repeat {
            iterations,
            escalation: current,
            ..
        } = &self.current.nodes()[&selected].kind
        else {
            return Err(EditError::new(
                EditErrorCode::WrongNodeKind,
                "escalation requires a Repeat",
            ));
        };
        let escalation = crate::RepeatEscalation {
            speed: current.as_ref().and_then(|current| current.speed),
            ..escalation
        };
        self.replace_escalation(&selected, iterations.len(), escalation)
    }

    /// Author `escalation` on `repeat` unless it is already current; an
    /// escalation with no step is removed.
    fn replace_escalation(
        &mut self,
        repeat: &NodeId,
        plays: u32,
        escalation: crate::RepeatEscalation,
    ) -> Result<(), EditError> {
        let NodeKind::Repeat {
            escalation: current,
            ..
        } = &self.current.nodes()[repeat].kind
        else {
            return Err(invalid("escalation requires a Repeat"));
        };
        let escalation =
            (escalation.derives_plays() || escalation.speed.is_some()).then_some(escalation);
        if let Some(escalation) = &escalation {
            escalation
                .validate(plays)
                .map_err(|error| invalid(&error.to_string()))?;
        }
        if escalation == *current {
            return Ok(());
        }
        self.charge_step(false)?;
        let SemanticAllocation::ParameterEdit { new_revision } =
            (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                step_index: self.steps.len(),
            })?
        else {
            return Err(invalid("Repeat escalation requires a parameter allocation"));
        };
        self.reserve_revision(&new_revision)?;
        let edit = LeafEdit::new(
            new_revision,
            Command::SetRepeatEscalation {
                node: repeat.clone(),
                escalation,
            },
        )?;
        self.commit_leaf(edit)
    }

    /// Silent freeze gaps on the selected Repeat, holding the last picture of
    /// its first play.
    pub(super) fn set_repeat_gaps(
        &mut self,
        trace_index: usize,
        gaps: &[crate::PauseLength],
    ) -> Result<(), EditError> {
        if self.context.visual_selection.is_some() {
            return Err(invalid(
                "clear the Visual selection before setting Repeat gaps",
            ));
        }
        let selected = self.context.selected_child.clone().ok_or_else(|| {
            EditError::new(
                EditErrorCode::SelectionUnavailable,
                "select an existing Repeat before setting its gaps",
            )
        })?;
        let slot = self.child_indices.get(&selected).copied().ok_or_else(|| {
            EditError::new(
                EditErrorCode::SelectionUnavailable,
                "the selected Repeat must be a direct child of the current Sequence",
            )
        })?;
        let NodeKind::Repeat {
            child,
            iterations,
            gap,
            ..
        } = &self.current.nodes()[&selected].kind
        else {
            return Err(EditError::new(
                EditErrorCode::WrongNodeKind,
                "Repeat gaps require an existing Repeat",
            ));
        };
        let rendered = iterations.len().saturating_sub(1).max(1);
        if u32::try_from(gaps.len()).map_or(true, |count| count > rendered) {
            return Err(invalid(&format!(
                "this Repeat has {} gap{} between its plays",
                rendered,
                if rendered == 1 { "" } else { "s" }
            )));
        }
        let start = slot
            .checked_sub(1)
            .map_or(self.bounds.0, |previous| self.child_ends[previous].1);
        let rate = self.current.presentation_basis().frame_rate;
        let durations = gaps
            .iter()
            .map(|length| length.resolve(rate))
            .collect::<Result<Vec<_>, _>>()?;
        let provider = if durations.is_empty() {
            None
        } else {
            let lengths = self.current.durations()?;
            let layout = crate::RepeatLayout::compile_with_gap_overrides(
                iterations,
                child,
                self.current.overrides().get(&selected),
                gap.as_ref()
                    .map_or(crate::FrameDuration::ZERO, |gap| gap.duration),
                self.current.gap_overrides().get(&selected),
                &lengths,
            )?;
            let first = iterations.at(0).expect("positive Repeat plays");
            let play = layout.play(&first).expect("validated Repeat layout");
            if play.duration == crate::FrameDuration::ZERO {
                return Err(EditError::new(
                    EditErrorCode::SelectionUnavailable,
                    "the Repeat's first play is empty, so a gap has no picture to hold",
                ));
            }
            let frame = start
                .0
                .checked_add(play.start)
                .and_then(|value| value.checked_add(play.duration.frames() - 1))
                .ok_or_else(|| {
                    EditError::new(EditErrorCode::TimingOverflow, "Repeat play overflows")
                })?;
            Some((self.resolve_pause)(
                &self.current,
                super::PauseSite::RepeatGap {
                    repeat: selected.clone(),
                    frame: ProjectFrame(frame),
                },
            )?)
        };
        let iterations = iterations.clone();
        let branches = durations.len().saturating_sub(1);
        let recipe = |duration| {
            let provider = provider.clone().expect("gaps resolve a picture");
            crate::HoldRecipe {
                duration,
                video: provider.video,
                audio: crate::HoldAudio::Silence,
                picture_context: provider.picture_context,
            }
        };
        let requested: Vec<_> = durations
            .iter()
            .skip(1)
            .enumerate()
            .map(|(index, duration)| {
                (
                    iterations.at(index as u32 + 1).expect("bounded gap"),
                    recipe(*duration),
                )
            })
            .collect();
        if self.current.repeat_gaps_unchanged(
            &selected,
            durations.first().copied().map(recipe).as_ref(),
            &requested,
        ) {
            // Nothing changes: no revision, identity or history is spent.
            self.context.cursor = start;
            self.context.selected_child = Some(selected);
            return Ok(());
        }
        self.charge_step(false)?;
        let SemanticAllocation::SetRepeatGaps {
            new_revision,
            nodes,
        } = (self.allocate)(SemanticAllocationRequest::SetRepeatGaps {
            step_index: self.steps.len(),
            branches,
        })?
        else {
            return Err(invalid("Repeat gaps require a gap allocation"));
        };
        if nodes.len() != branches {
            return Err(invalid("Repeat gaps require one identity per gap Hold"));
        }
        self.reserve_revision(&new_revision)?;
        for node in &nodes {
            self.reserve_node(node)?;
        }
        let hold = recipe;
        let edit = LeafEdit::new(
            new_revision.clone(),
            Command::SetRepeatGaps {
                node: selected.clone(),
                gap: durations.first().copied().map(hold),
                branches: nodes
                    .into_iter()
                    .zip(durations.iter().skip(1))
                    .enumerate()
                    .map(|(index, (id, duration))| crate::RepeatGapHold {
                        after: iterations.at(index as u32 + 1).expect("bounded gap"),
                        id,
                        hold: hold(*duration),
                    })
                    .collect(),
                timing: AudioTimingId {
                    allocation: new_revision,
                    ordinal: 0,
                },
            },
        )?;
        self.commit_leaf(edit)?;
        self.context.cursor = start;
        self.context.selected_child = Some(selected.clone());
        let end = self.child_ends[slot].1;
        self.trace[trace_index].resolved_selection =
            Some(SliceCaptureSelection::Child { node: selected });
        self.trace[trace_index].resolved_range =
            Some(FrameRange::new(start, end).map_err(crate::DocumentError::from)?);
        self.trace[trace_index].resolved_parent = Some(self.context.parent.clone());
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
    /// Give each later play of the selected Repeat its own speed change, as a
    /// sparse one-play Retime override, so every play stays editable, and
    /// record the recipe on the Repeat. A zero step removes the recipe and
    /// returns its plays to normal speed.
    ///
    /// A play belongs to the previous recipe (normal speed when there is
    /// none) when it has no override, or when its override's outermost node is
    /// an Edit Retime at that recipe's rate and pitch for its own input; only
    /// that Retime's duration and pitch change, so edits inside the play
    /// survive. Any other play has been changed by hand: it refuses a new
    /// recipe, and `continuing` (after a count change) leaves it alone.
    pub(super) fn speed_plays(
        &mut self,
        trace_index: usize,
        speed: Option<crate::SpeedStep>,
        continuing: bool,
    ) -> Result<(), EditError> {
        let repeat = self
            .context
            .selected_child
            .clone()
            .ok_or_else(|| invalid("select a Repeat before giving its plays speeds"))?;
        // One Repeat step addresses a play only below ordinary groups.
        let mut ancestor = Some(self.context.parent.clone());
        while let Some(node) = ancestor {
            if !matches!(self.current.nodes()[&node].kind, NodeKind::Sequence { .. }) {
                return Err(invalid(
                    "per-play speeds need ordinary groups above the Repeat; leave the contents first",
                ));
            }
            ancestor = self.current.parent_of(&node);
        }
        let NodeKind::Repeat {
            child,
            iterations,
            escalation,
            ..
        } = &self.current.nodes()[&repeat].kind
        else {
            return Err(EditError::new(
                EditErrorCode::WrongNodeKind,
                "per-play speeds require a Repeat",
            ));
        };
        let (child, plays) = (child.clone(), iterations.len());
        let escalation = escalation.clone().unwrap_or_default();
        let old = escalation.speed;
        let speed = speed.filter(|speed| speed.step != crate::ExactRatio::ZERO);
        if speed.is_some() && plays < 2 {
            return Err(invalid("one play has no later plays to speed up"));
        }
        if speed.is_none() && old.is_none() {
            return Err(invalid("the Repeat's plays have no speed step to remove"));
        }
        // Record the recipe first so its validation names every play.
        self.replace_escalation(
            &repeat,
            plays,
            crate::RepeatEscalation {
                speed,
                ..escalation
            },
        )?;
        let rate = |step: Option<crate::SpeedStep>, play: u32| {
            step.map_or(Ok(crate::ExactRatio::ONE), |step| step.rate(play))
                .map_err(|error| invalid(&error.to_string()))
        };
        let pitch = |step: Option<crate::SpeedStep>| {
            step.map_or(crate::PitchPolicy::FollowSpeed, |step| step.pitch())
        };
        let default_input = self.current.durations_shared()?[&child];
        let mut subtree = 0usize;
        let mut pending = vec![child.clone()];
        while let Some(node) = pending.pop() {
            subtree += 1;
            pending.extend(self.current.children(&node).cloned());
        }
        for ordinal in 1..plays {
            let NodeKind::Repeat { iterations, .. } = &self.current.nodes()[&repeat].kind else {
                return Err(invalid("the Repeat changed while its plays were sped up"));
            };
            let iteration = iterations
                .at(ordinal)
                .ok_or_else(|| invalid("a play disappeared while its speed was set"))?;
            let root = self
                .current
                .overrides()
                .get(&repeat)
                .and_then(|plays| plays.get(&iteration))
                .cloned();
            let Some(root) = root else {
                let Some(step) = speed else {
                    continue;
                };
                let duration = step
                    .duration(ordinal, default_input)
                    .map_err(|error| invalid(&error.to_string()))?;
                self.wrap_play(&repeat, &child, iteration, subtree, duration, step.pitch())?;
                continue;
            };
            let refused = || {
                invalid(&format!(
                    "play {} has been changed by hand; open it with :scope play {} to change its speed",
                    ordinal + 1,
                    ordinal + 1
                ))
            };
            let matching = match &self.current.nodes()[&root].kind {
                NodeKind::Retime {
                    duration,
                    mapping,
                    pitch: current_pitch,
                    purpose,
                    ..
                } if purpose.is_edit() => {
                    let previous = crate::output_duration(rate(old, ordinal)?, mapping.duration())
                        .map_err(|error| invalid(&error.to_string()))?;
                    // Pitch is inaudible at normal speed, so a unity play
                    // matches either policy.
                    (*duration == previous && (old.is_none() || *current_pitch == pitch(old)))
                        .then_some((*duration, mapping.duration(), *current_pitch))
                }
                _ => None,
            };
            let Some((duration, input, current_pitch)) = matching else {
                if continuing {
                    continue;
                }
                return Err(refused());
            };
            let wanted = crate::output_duration(rate(speed, ordinal)?, input)
                .map_err(|error| invalid(&error.to_string()))?;
            let wanted_pitch = if speed.is_some() {
                pitch(speed)
            } else {
                current_pitch
            };
            if wanted == duration && wanted_pitch == current_pitch {
                continue;
            }
            self.charge_step(false)?;
            let SemanticAllocation::ParameterEdit { new_revision } =
                (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                    step_index: self.steps.len(),
                })?
            else {
                return Err(invalid(
                    "a play's speed change requires a parameter allocation",
                ));
            };
            self.reserve_revision(&new_revision)?;
            self.commit_leaf(LeafEdit::new(
                new_revision,
                Command::SetRetime {
                    node: root,
                    duration: wanted,
                    pitch: wanted_pitch,
                },
            )?)?;
        }
        self.context.selected_child = Some(repeat.clone());
        let slot = self.child_indices[&repeat];
        let start = slot
            .checked_sub(1)
            .map_or(self.bounds.0, |previous| self.child_ends[previous].1);
        self.trace[trace_index].resolved_range = Some(
            FrameRange::new(start, self.child_ends[slot].1).map_err(crate::DocumentError::from)?,
        );
        Ok(())
    }

    /// Continue the selected Repeat's recorded speed recipe onto plays that
    /// have none yet, after its play count changed.
    pub(super) fn continue_speed_plays(&mut self, trace_index: usize) -> Result<(), EditError> {
        let Some(repeat) = self.context.selected_child.clone() else {
            return Ok(());
        };
        let Some(NodeKind::Repeat {
            escalation:
                Some(crate::RepeatEscalation {
                    speed: Some(speed), ..
                }),
            ..
        }) = self.current.nodes().get(&repeat).map(|node| &node.kind)
        else {
            return Ok(());
        };
        let speed = *speed;
        self.speed_plays(trace_index, Some(speed), true)
    }

    /// One play's own Retime override, through occurrence isolation.
    fn wrap_play(
        &mut self,
        repeat: &NodeId,
        child: &NodeId,
        iteration: crate::IterationId,
        subtree: usize,
        duration: crate::FrameDuration,
        pitch: crate::PitchPolicy,
    ) -> Result<(), EditError> {
        self.charge_step(false)?;
        let SemanticAllocation::Isolation {
            new_revision,
            identities,
        } = (self.allocate)(SemanticAllocationRequest::Isolation {
            step_index: self.steps.len(),
            nodes: subtree + 1,
            marks: 0,
        })?
        else {
            return Err(invalid("a per-play speed requires an Isolation allocation"));
        };
        self.reserve_revision(&new_revision)?;
        for node in &identities.nodes {
            self.reserve_node(node)?;
        }
        let mut nodes = identities.nodes;
        let wrapper = nodes
            .pop()
            .ok_or_else(|| invalid("a per-play speed needs its wrapper identity"))?;
        self.commit_leaf(LeafEdit::new(
            new_revision,
            Command::EditOccurrence {
                instance: crate::InstancePath {
                    node: child.clone(),
                    repeats: vec![crate::RepeatInstance {
                        node: repeat.clone(),
                        iteration,
                    }],
                },
                edit: crate::OccurrenceEdit::WrapRetime {
                    id: wrapper,
                    duration,
                    pitch,
                },
                identities: crate::OccurrenceIdentities {
                    nodes,
                    marks: identities.marks,
                },
            },
        )?)
    }
}
