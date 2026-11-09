//! `s`, `dd`, `rr`, `:repeat N` and `:repeat [N] gap=` on the beat in the
//! invocation's explicit play. Each is one `EditOccurrence` leaf built exactly
//! as the native edit builds it; the saved instruction names no node or play,
//! so a replay or dot resolves the beat and play chain chosen at invocation.

use super::*;

/// The structural edit a scoped instruction resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ScopedStructure {
    Split,
    Delete,
    Repeat {
        plays: u32,
    },
    SetPlays {
        plays: u32,
    },
    SetGaps {
        plays: Option<u32>,
        gaps: Vec<crate::PauseLength>,
    },
}

/// Gap recipes resolved once from the pre-edit document; independent of the
/// identities and revision allocated later.
struct Gaps {
    plays: u32,
    iterations: crate::IterationOrder,
    durations: Vec<crate::FrameDuration>,
    provider: Option<super::PauseProvider>,
}

impl Gaps {
    fn recipe(&self, duration: crate::FrameDuration) -> crate::HoldRecipe {
        let provider = self.provider.clone().expect("gaps resolve a picture");
        crate::HoldRecipe {
            duration,
            video: provider.video,
            audio: crate::HoldAudio::Silence,
            picture_context: provider.picture_context,
        }
    }

    fn edit(
        &self,
        revision: &RevisionId,
        timing: AudioTimingId,
        ids: Vec<NodeId>,
    ) -> Result<crate::OccurrenceEdit, EditError> {
        // Play identities after the count, exactly as the core reducer names them.
        let order = self.iterations.resized(self.plays, revision.clone())?;
        let branches = self
            .durations
            .iter()
            .skip(1)
            .zip(ids)
            .enumerate()
            .map(|(index, (duration, id))| {
                Ok(crate::RepeatGapHold {
                    after: u32::try_from(index + 1)
                        .ok()
                        .and_then(|ordinal| order.at(ordinal))
                        .ok_or_else(|| invalid("a gap follows a missing play"))?,
                    id,
                    hold: self.recipe(*duration),
                })
            })
            .collect::<Result<Vec<_>, EditError>>()?;
        Ok(crate::OccurrenceEdit::SetRepeatGaps {
            plays: self.plays,
            gap: self
                .durations
                .first()
                .map(|duration| self.recipe(*duration)),
            branches,
            timing,
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
    /// `SetRepeat` without escalation or speed steps, inside one play: gaps
    /// (and the count first) on a Repeat, else a count change or a wrap.
    pub(super) fn scoped_set_repeat(
        &mut self,
        trace_index: usize,
        plays: Option<std::num::NonZeroU32>,
        gaps: Option<&[crate::PauseLength]>,
    ) -> Result<(), EditError> {
        let plays = plays.map(std::num::NonZeroU32::get);
        let edit = match (gaps, plays) {
            (Some(gaps), plays) => ScopedStructure::SetGaps {
                plays,
                gaps: gaps.to_vec(),
            },
            (None, Some(plays)) => {
                let node = &self
                    .context
                    .scoped
                    .as_ref()
                    .ok_or_else(|| invalid("a one-play Repeat change needs scoped contents"))?
                    .presentation
                    .node;
                if matches!(
                    self.current.nodes().get(node).map(|node| &node.kind),
                    Some(NodeKind::Repeat { .. })
                ) {
                    ScopedStructure::SetPlays { plays }
                } else {
                    ScopedStructure::Repeat { plays }
                }
            }
            (None, None) => {
                return Err(invalid(
                    "a one-play Repeat change sets its plays or gaps; escalation and speed steps are set from the parent",
                ));
            }
        };
        self.scoped_structure(trace_index, edit)
    }

    pub(super) fn scoped_structure(
        &mut self,
        trace_index: usize,
        kind: ScopedStructure,
    ) -> Result<(), EditError> {
        let scoped = self
            .context
            .scoped
            .clone()
            .ok_or_else(|| invalid("a one-play structural edit needs scoped contents"))?;
        super::framing::validate_scoped_context(&self.current, &self.context)?;
        let instance = scoped.presentation.clone();
        if !scoped.target.matches_instance(&self.current, &instance)? {
            return Err(invalid(
                "the one-play target differs from its shown play; choose the play again",
            ));
        }
        let (start, end) = self.current.occurrence_structure_interval(&instance)?;
        let cursor = self.context.cursor;
        let extra = match &kind {
            ScopedStructure::Split => {
                if cursor.0 <= start.0 || cursor.0 >= end.0 {
                    return Err(EditError::new(
                        EditErrorCode::SelectionUnavailable,
                        "move inside the selected beat to split it; existing boundaries need no split",
                    ));
                }
                // The split pool covers the beat's subtree, both Partitions, a
                // root context and a container; unused names are dropped.
                let mut count = 3_usize;
                let mut pending = vec![instance.node.clone()];
                while let Some(id) = pending.pop() {
                    count = count
                        .checked_add(1)
                        .filter(|count| *count <= crate::MAX_DOCUMENT_NODES)
                        .ok_or_else(|| limit("split exceeds the document node limit"))?;
                    pending.extend(self.current.children(&id).cloned());
                }
                count
            }
            ScopedStructure::Repeat { .. } => 1,
            ScopedStructure::Delete | ScopedStructure::SetPlays { .. } => 0,
            ScopedStructure::SetGaps { .. } => 0,
        };
        let gaps = match &kind {
            ScopedStructure::SetGaps { plays, gaps } => {
                Some(self.scoped_gaps(&instance, start, *plays, gaps)?)
            }
            _ => None,
        };
        let extra = extra
            + gaps
                .as_ref()
                .map_or(0, |gaps| gaps.durations.len().saturating_sub(1));
        let build = |revision: &RevisionId, mut ids: Vec<NodeId>| {
            let timing = AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            };
            Ok::<_, EditError>(match &kind {
                ScopedStructure::Split => crate::OccurrenceEdit::Split {
                    at: crate::FrameDuration::new(cursor.0 - start.0)
                        .map_err(crate::DocumentError::from)?,
                    identities: SplitIdentities { nodes: ids },
                },
                ScopedStructure::Delete => crate::OccurrenceEdit::DeleteRipple { timing },
                ScopedStructure::Repeat { plays } => crate::OccurrenceEdit::RepeatChild {
                    id: ids
                        .pop()
                        .ok_or_else(|| invalid("a one-play Repeat needs its wrapper identity"))?,
                    plays: *plays,
                    timing,
                },
                ScopedStructure::SetPlays { plays } => crate::OccurrenceEdit::SetRepeatPlays {
                    plays: *plays,
                    timing,
                },
                ScopedStructure::SetGaps { .. } => gaps
                    .as_ref()
                    .expect("gap edits resolve their recipes")
                    .edit(revision, timing, ids)?,
            })
        };
        // Isolation sizing and the no-op refusals depend on neither the
        // allocated names nor the revision, which only names new plays.
        let probe = build(
            self.current.revision_id(),
            vec![instance.node.clone(); extra],
        )?;
        let needs = self
            .current
            .occurrence_structure_requirements(&instance, &probe)?;
        self.charge_step(false)?;
        let SemanticAllocation::Isolation {
            new_revision,
            identities,
        } = (self.allocate)(SemanticAllocationRequest::Isolation {
            step_index: self.steps.len(),
            nodes: needs
                .nodes
                .checked_add(extra)
                .ok_or_else(|| limit("one-play edit identities overflow"))?,
            marks: needs.marks,
        })?
        else {
            return Err(invalid(
                "a one-play structural edit requires an Isolation allocation",
            ));
        };
        if identities.nodes.len() != needs.nodes + extra || identities.marks.len() != needs.marks {
            return Err(invalid(
                "a one-play structural edit requires its exact isolation identity pools",
            ));
        }
        self.reserve_revision(&new_revision)?;
        for node in &identities.nodes {
            self.reserve_node(node)?;
        }
        for mark in &identities.marks {
            if !self.marks.insert(mark.clone()) {
                return Err(identity(
                    "macro reuses an authored or allocated mark identity",
                ));
            }
        }
        let mut nodes = identities.nodes;
        let fresh = nodes.split_off(needs.nodes);
        let edit = build(&new_revision, fresh)?;
        let identities = crate::OccurrenceIdentities {
            nodes,
            marks: identities.marks,
        };
        let (target, presentation) =
            self.current
                .occurrence_structure_selection(&instance, &edit, &identities)?;
        self.commit_leaf(LeafEdit::new(
            new_revision,
            Command::EditOccurrence {
                instance,
                edit,
                identities,
            },
        )?)?;
        let node = target.node.clone();
        self.context.scoped = Some(SemanticScopedContext {
            root: scoped.root,
            target,
            presentation,
        });
        // Split keeps the cursor on its new boundary; the other edits continue
        // at the changed beat's start, as the native edit does.
        if kind != ScopedStructure::Split {
            self.context.cursor = start;
        }
        super::framing::validate_scoped_context(&self.current, &self.context)?;
        self.trace[trace_index].resolved_parent = self.current.parent_of(&node);
        self.trace[trace_index].resolved_selection = Some(SliceCaptureSelection::Child { node });
        Ok(())
    }

    /// The gap Holds of the shown Repeat: each holds the last picture of the
    /// Repeat's first play in this occurrence, captured below the Repeat.
    fn scoped_gaps(
        &mut self,
        instance: &crate::InstancePath,
        start: ProjectFrame,
        plays: Option<u32>,
        gaps: &[crate::PauseLength],
    ) -> Result<Gaps, EditError> {
        let repeat = &instance.node;
        let Some(NodeKind::Repeat {
            child,
            iterations,
            gap: current,
            ..
        }) = self.current.nodes().get(repeat).map(|node| &node.kind)
        else {
            return Err(EditError::new(
                EditErrorCode::WrongNodeKind,
                "select a Repeat to set its gaps in this play; wrap a beat first with :repeat N, then set its gaps",
            ));
        };
        let plays = plays.unwrap_or(iterations.len());
        if plays == 0 {
            return Err(invalid("a Repeat needs at least one total play"));
        }
        let rate = self.current.presentation_basis().frame_rate;
        let durations = gaps
            .iter()
            .map(|length| length.resolve(rate))
            .collect::<Result<Vec<_>, _>>()?;
        let rendered = plays.saturating_sub(1).max(1);
        if u32::try_from(durations.len()).map_or(true, |count| count > rendered) {
            return Err(invalid(&format!(
                "this Repeat has {rendered} gap{} between its plays",
                if rendered == 1 { "" } else { "s" }
            )));
        }
        let provider = if durations.is_empty() {
            None
        } else {
            let lengths = self.current.durations()?;
            let layout = crate::RepeatLayout::compile_with_gap_overrides(
                iterations,
                child,
                self.current.overrides().get(repeat),
                current
                    .as_ref()
                    .map_or(crate::FrameDuration::ZERO, |gap| gap.duration),
                self.current.gap_overrides().get(repeat),
                &lengths,
            )?;
            let first = iterations
                .at(0)
                .ok_or_else(|| invalid("the Repeat has no plays"))?;
            let play = layout
                .play(&first)
                .ok_or_else(|| invalid("the Repeat's first play is missing"))?;
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
                    repeat: repeat.clone(),
                    frame: ProjectFrame(frame),
                },
            )?)
        };
        Ok(Gaps {
            plays,
            iterations: iterations.clone(),
            durations,
            provider,
        })
    }
}
