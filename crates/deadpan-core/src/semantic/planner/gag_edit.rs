//! `:gag-set`: change an inserted gag's exposed parameters after insertion
//! (specification §8.4). The group's pinned label names its recipe; each
//! changed parameter edits the ordinary part the recipe made, through the same
//! leaves as the native edits, and the label is rewritten to pin the new
//! parameters. A group whose parts no longer have the recipe's shape refuses,
//! so a hand-edited gag is never silently rebuilt.

use super::*;
use crate::{CutawayFit, Framing, FramingCurve, FramingPose, GagRecipe, HoldAudio, TailEffect};

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: FnMut(&ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError>,
    P: FnMut(&ProjectDocument, super::PauseSite) -> Result<super::PauseProvider, EditError>,
{
    pub(super) fn set_gag(
        &mut self,
        trace_index: usize,
        changed: &GagRecipe,
        parameters: &[crate::GagParameter],
    ) -> Result<(), EditError> {
        if self.context.visual_selection.is_some() {
            return Err(invalid(
                "clear the Visual selection before changing a gag's parameters",
            ));
        }
        let group = self.context.selected_child.clone().ok_or_else(|| {
            EditError::new(
                EditErrorCode::SelectionUnavailable,
                "select an inserted gag group before changing its parameters",
            )
        })?;
        let Some(&slot) = self.child_indices.get(&group) else {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "the gag group must be a direct child of the current Sequence",
            ));
        };
        let node = &self.current.nodes()[&group];
        let NodeKind::Sequence { children } = &node.kind else {
            return Err(EditError::new(
                EditErrorCode::WrongNodeKind,
                "select an inserted gag's group",
            ));
        };
        let current = GagRecipe::from_label(&node.label).ok_or_else(|| {
            EditError::new(
                EditErrorCode::WrongNodeKind,
                "this group's label does not pin a gag recipe; edit its parts directly",
            )
        })?;
        if !current.same_recipe(changed) {
            return Err(invalid(&format!(
                "this group is {}; give that recipe's parameters",
                current.name()
            )));
        }
        // Only the named parameters change; the rest keep this gag's values.
        let target = if parameters.is_empty() {
            *changed
        } else {
            current.with_parameters(changed, parameters)?
        };
        let recipe = &target;
        if &current == recipe {
            return Err(invalid(
                "the gag already has those parameters; no edit was made",
            ));
        }
        // Validate the new parameters and version as the recipe would.
        let rate = self.current.presentation_basis().frame_rate;
        recipe.expand(false, rate)?;
        let children = children.clone();
        self.verify_gag_parts(&current, &children)?;
        let start = slot
            .checked_sub(1)
            .map_or(self.bounds.0, |previous| self.child_ends[previous].1);
        let outer = self.context.parent.clone();
        self.context.parent = group.clone();
        self.refresh_children()?;
        let parts = self.set_gag_parts(trace_index, &current, recipe, &children);
        self.context.parent = outer;
        self.refresh_children()?;
        parts?;
        self.charge_step(false)?;
        let SemanticAllocation::ParameterEdit { new_revision } =
            (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                step_index: self.steps.len(),
            })?
        else {
            return Err(invalid("a gag relabel requires a parameter allocation"));
        };
        self.reserve_revision(&new_revision)?;
        self.commit_leaf(LeafEdit::new(
            new_revision,
            Command::Rename {
                node: group.clone(),
                label: recipe.label(),
            },
        )?)?;
        let slot = self.child_indices[&group];
        let end = self.child_ends[slot].1;
        self.context.cursor = start;
        self.context.selected_child = Some(group.clone());
        self.trace[trace_index].resolved_parent = Some(self.context.parent.clone());
        self.trace[trace_index].resolved_selection =
            Some(SliceCaptureSelection::Child { node: group });
        self.trace[trace_index].resolved_range =
            Some(FrameRange::new(start, end).map_err(crate::DocumentError::from)?);
        Ok(())
    }

    /// Every part still exactly as `recipe` made it: kinds, durations, plays,
    /// gaps, escalation, sound, framing and the reaction cutaway. A gag whose
    /// parts were changed by hand refuses instead of being rebuilt over the
    /// changes.
    fn verify_gag_parts(&self, recipe: &GagRecipe, children: &[NodeId]) -> Result<(), EditError> {
        let reshaped = || {
            EditError::new(
                EditErrorCode::WrongNodeKind,
                "this gag's parts were changed after it was inserted; edit them directly",
            )
        };
        let rate = self.current.presentation_basis().frame_rate;
        let frames = |length: crate::PauseLength| length.resolve(rate);
        let nodes = self.current.nodes();
        let hold = |index: usize| match children.get(index).map(|child| &nodes[child]) {
            Some(node) => match &node.kind {
                NodeKind::Hold { recipe } => Some((node, recipe)),
                _ => None,
            },
            None => None,
        };
        let plain = |node: &crate::BeatNode| {
            node.framing.is_none()
                && node.cutaways.is_empty()
                && node.captions.is_empty()
                && node.audio_treatments.is_empty()
        };
        let check = |ok: bool| if ok { Ok(()) } else { Err(reshaped()) };
        match recipe {
            GagRecipe::LongAnswer { pause, scale, .. } => {
                let start = FramingPose::identity();
                let end = FramingPose::new(start.center_x, start.center_y, *scale)
                    .and_then(|pose| pose.quantized())
                    .map_err(|error| invalid(&error.to_string()))?;
                let creep = Framing::creep(start, end, FramingCurve::Smoothstep)
                    .map_err(|error| invalid(&error.to_string()))?;
                let Some((node, held)) = hold(0) else {
                    return Err(reshaped());
                };
                check(
                    children.len() == 1
                        && held.duration == frames(*pause)?
                        && held.audio == HoldAudio::Silence
                        && node.framing.as_ref() == Some(&creep)
                        && node.cutaways.is_empty()
                        && node.captions.is_empty(),
                )
            }
            GagRecipe::Escalator {
                plays,
                gain_step,
                zoom_step,
                ..
            } => {
                let escalation = crate::RepeatEscalation {
                    gain_step: *gain_step,
                    zoom: (*zoom_step != crate::ExactRatio::ZERO).then_some(crate::ZoomStep {
                        step: *zoom_step,
                        progression: crate::ZoomProgression::Add,
                    }),
                };
                let repeat = children.first().filter(|_| children.len() == 1);
                check(repeat.is_some_and(|repeat| {
                    matches!(
                        &nodes[repeat].kind,
                        NodeKind::Repeat { iterations, gap: None, escalation: Some(current), .. }
                            if iterations.len() == plays.get() && *current == escalation
                    ) && !self.current.overrides().contains_key(repeat)
                        && !self.current.gap_overrides().contains_key(repeat)
                        && plain(&nodes[repeat])
                }))
            }
            GagRecipe::OneMoreTime {
                plays,
                gap,
                shorten,
                variation,
                ..
            } => {
                let gaps = GagRecipe::one_more_time_gaps(*plays, *gap, *shorten, *variation)?
                    .into_iter()
                    .map(frames)
                    .collect::<Result<Vec<_>, _>>()?;
                let Some(repeat) = children.first().filter(|_| children.len() == 1) else {
                    return Err(reshaped());
                };
                let NodeKind::Repeat {
                    iterations,
                    gap: Some(default),
                    escalation: None,
                    ..
                } = &nodes[repeat].kind
                else {
                    return Err(reshaped());
                };
                if iterations.len() != plays.get()
                    || default.duration != gaps[0]
                    || default.audio != HoldAudio::Silence
                    || self.current.overrides().contains_key(repeat)
                    || !plain(&nodes[repeat])
                {
                    return Err(reshaped());
                }
                // Each later gap is its own silent Hold of exactly its length.
                let branches = self.current.gap_overrides().get(repeat);
                if branches.map_or(0, |branches| branches.len()) != gaps.len() - 1 {
                    return Err(reshaped());
                }
                for (position, length) in gaps.iter().enumerate().skip(1) {
                    let branch = iterations
                        .at(u32::try_from(position).map_err(|_| reshaped())?)
                        .and_then(|play| branches?.get(&play));
                    check(branch.is_some_and(|branch| {
                        matches!(
                            &nodes[branch].kind,
                            NodeKind::Hold { recipe } if recipe.duration == *length
                                && recipe.audio == HoldAudio::Silence
                        ) && plain(&nodes[branch])
                    }))?;
                }
                Ok(())
            }
            GagRecipe::NothingHappens { tone, silence, .. } => {
                let (Some((tone_node, tone_hold)), Some((silence_node, silence_hold))) =
                    (hold(0), hold(1))
                else {
                    return Err(reshaped());
                };
                check(
                    children.len() == 2
                        && tone_hold.duration == frames(*tone)?
                        && matches!(tone_hold.audio, HoldAudio::RoomTone { .. })
                        && silence_hold.duration == frames(*silence)?
                        && silence_hold.audio == HoldAudio::Silence
                        && plain(tone_node)
                        && plain(silence_node),
                )
            }
            GagRecipe::AreWeDone { pause, .. } => {
                let Some((node, held)) = hold(0) else {
                    return Err(reshaped());
                };
                let length = frames(*pause)?;
                check(
                    children.len() == 1
                        && held.duration == length
                        && held.audio
                            == (HoldAudio::Tail {
                                maximum: length,
                                effect: TailEffect::Reverb,
                            })
                        && node.cutaways.len() == 1
                        && node.cutaways[0].range
                            == FrameRange::new(ProjectFrame(0), ProjectFrame(length.frames()))
                                .map_err(crate::DocumentError::from)?
                        && node.cutaways[0].fit == CutawayFit::Hold
                        && !node.cutaways[0].removed
                        && node.framing.is_none()
                        && node.captions.is_empty(),
                )
            }
            GagRecipe::NonSequitur { .. } => Ok(()),
        }
    }

    /// The part edits inside the entered gag group.
    fn set_gag_parts(
        &mut self,
        trace_index: usize,
        current: &GagRecipe,
        recipe: &GagRecipe,
        children: &[NodeId],
    ) -> Result<(), EditError> {
        let reshaped = || {
            EditError::new(
                EditErrorCode::WrongNodeKind,
                "this gag's parts were changed after it was inserted; edit them directly",
            )
        };
        // The parts' kinds, as the recipe made them.
        let shape: Vec<&str> = children
            .iter()
            .map(|child| match &self.current.nodes()[child].kind {
                NodeKind::Hold { recipe } if matches!(recipe.audio, HoldAudio::Tail { .. }) => {
                    "tail"
                }
                NodeKind::Hold { .. } => "hold",
                NodeKind::Repeat { .. } => "repeat",
                _ => "other",
            })
            .collect();
        match (current, recipe) {
            (
                GagRecipe::LongAnswer {
                    pause: old_pause,
                    scale: old_scale,
                    ..
                },
                GagRecipe::LongAnswer { pause, scale, .. },
            ) => {
                if shape != ["hold"] {
                    return Err(reshaped());
                }
                self.context.selected_child = Some(children[0].clone());
                if pause != old_pause {
                    self.set_hold_duration(trace_index, *pause)?;
                }
                if scale != old_scale {
                    let start = FramingPose::identity();
                    let end = FramingPose::new(start.center_x, start.center_y, *scale)
                        .and_then(|pose| pose.quantized())
                        .map_err(|error| invalid(&error.to_string()))?;
                    let creep = Framing::creep(start, end, FramingCurve::Smoothstep)
                        .map_err(|error| invalid(&error.to_string()))?;
                    self.set_framing(trace_index, Some(creep))?;
                }
            }
            (
                GagRecipe::Escalator {
                    plays: old_plays,
                    gain_step: old_gain,
                    zoom_step: old_zoom,
                    ..
                },
                GagRecipe::Escalator {
                    plays,
                    gain_step,
                    zoom_step,
                    ..
                },
            ) => {
                if shape != ["repeat"] {
                    return Err(reshaped());
                }
                self.context.selected_child = Some(children[0].clone());
                let escalation = (gain_step != old_gain
                    || zoom_step != old_zoom
                    || plays != old_plays)
                    .then(|| crate::RepeatEscalation {
                        gain_step: *gain_step,
                        zoom: (*zoom_step != crate::ExactRatio::ZERO).then_some(crate::ZoomStep {
                            step: *zoom_step,
                            progression: crate::ZoomProgression::Add,
                        }),
                    });
                self.set_repeat(
                    trace_index,
                    (plays != old_plays).then_some(*plays),
                    None,
                    escalation,
                )?;
            }
            (
                GagRecipe::OneMoreTime {
                    plays: old_plays, ..
                },
                GagRecipe::OneMoreTime {
                    plays,
                    gap,
                    shorten,
                    variation,
                    ..
                },
            ) => {
                if shape != ["repeat"] {
                    return Err(reshaped());
                }
                self.context.selected_child = Some(children[0].clone());
                let gaps = GagRecipe::one_more_time_gaps(*plays, *gap, *shorten, *variation)?;
                self.set_repeat(
                    trace_index,
                    (plays != old_plays).then_some(*plays),
                    Some(&gaps),
                    None,
                )?;
            }
            (
                GagRecipe::NothingHappens {
                    tone: old_tone,
                    silence: old_silence,
                    register: old_register,
                    ..
                },
                GagRecipe::NothingHappens {
                    tone,
                    silence,
                    register,
                    ..
                },
            ) => {
                if shape != ["hold", "hold"] {
                    return Err(reshaped());
                }
                if tone != old_tone {
                    self.context.selected_child = Some(children[0].clone());
                    self.set_hold_duration(trace_index, *tone)?;
                }
                if register != old_register {
                    self.context.selected_child = Some(children[0].clone());
                    self.set_room_tone(trace_index, *register)?;
                }
                if silence != old_silence {
                    self.context.selected_child = Some(children[1].clone());
                    self.set_hold_duration(trace_index, *silence)?;
                }
            }
            (
                GagRecipe::AreWeDone {
                    pause: old_pause, ..
                },
                GagRecipe::AreWeDone {
                    pause, register, ..
                },
            ) => {
                if shape != ["tail"] {
                    return Err(reshaped());
                }
                let hold = children[0].clone();
                self.context.selected_child = Some(hold.clone());
                // The reaction covers the whole pause and is, as verified,
                // the pause's only cutaway: clear it, change the length, ring
                // the tail through the new length, then show the (new)
                // register's reaction over all of it again.
                self.clear_cutaways(&hold)?;
                if pause != old_pause {
                    self.set_hold_duration(trace_index, *pause)?;
                    self.tail(trace_index, None, TailEffect::Reverb)?;
                }
                self.set_cutaway(trace_index, *register, CutawayFit::Hold)?;
            }
            (GagRecipe::NonSequitur { .. }, _) => {
                return Err(invalid(
                    "a non-sequitur's register content is already pasted; paste the new register's content instead",
                ));
            }
            _ => return Err(invalid("the recipes differ")),
        }
        Ok(())
    }

    /// Remove every cutaway shown over `node`'s host.
    fn clear_cutaways(&mut self, node: &NodeId) -> Result<(), EditError> {
        let (host, _) = crate::cutaway_host(&self.current, node).ok_or_else(|| {
            EditError::new(
                EditErrorCode::WrongNodeKind,
                "the pause has no cutaway host",
            )
        })?;
        if self.current.nodes()[&host].cutaways.is_empty() {
            return Ok(());
        }
        self.charge_step(false)?;
        let SemanticAllocation::ParameterEdit { new_revision } =
            (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                step_index: self.steps.len(),
            })?
        else {
            return Err(invalid(
                "clearing a cutaway requires a parameter allocation",
            ));
        };
        self.reserve_revision(&new_revision)?;
        self.commit_leaf(LeafEdit::new(
            new_revision,
            Command::SetCutaways {
                node: host,
                cutaways: Vec::new(),
            },
        )?)
    }
}
