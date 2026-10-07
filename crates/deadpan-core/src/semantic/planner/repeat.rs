//! Repeat resolves its selector once on the staged document and authors one
//! leaf. It neither reads nor writes a copied-content register.

use super::*;

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: FnMut(&ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError>,
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
        selector: SemanticSelector,
        plays: u32,
        escalation: Option<crate::RepeatEscalation>,
    ) -> Result<(), EditError> {
        if let Some(escalation) = escalation {
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
                    SemanticSelector::SelectedBeat,
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

    /// Replace the selected Repeat's escalation; a step-free value removes it.
    fn set_escalation(&mut self, escalation: crate::RepeatEscalation) -> Result<(), EditError> {
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
        let escalation = (escalation.gain_step != crate::GainDb::UNITY
            || escalation.zoom.is_some())
        .then_some(escalation);
        if let Some(escalation) = &escalation {
            escalation
                .validate(iterations.len())
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
                node: selected,
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
