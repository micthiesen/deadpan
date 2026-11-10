//! `s`, `dd`, `rr`, `p`/`P`, `:ungroup`, `:repeat N` and `:repeat [N] gap=`
//! on the beat in the invocation's explicit play. Each is one `EditOccurrence` leaf built exactly
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
    /// Paste a register after (or before) the beat in this play only.
    Paste {
        register: RegisterName,
        before: bool,
    },
    /// Ungroup the selected Sequence in this play only.
    Ungroup,
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
            ScopedStructure::Ungroup => self.ungroup_target(&instance.node)?,
            ScopedStructure::Delete | ScopedStructure::SetPlays { .. } => 0,
            ScopedStructure::SetGaps { .. } | ScopedStructure::Paste { .. } => 0,
        };
        let paste = match &kind {
            ScopedStructure::Paste { register, .. } => Some(self.scoped_paste_content(*register)?),
            _ => None,
        };
        let (paste_nodes, paste_marks) = paste
            .as_ref()
            .map_or((0, 0), super::scoped_copy::ScopedPaste::identities);
        let gaps = match &kind {
            ScopedStructure::SetGaps { plays, gaps } => {
                Some(self.scoped_gaps(&instance, start, *plays, gaps)?)
            }
            _ => None,
        };
        let extra = extra
            + paste_nodes
            + gaps
                .as_ref()
                .map_or(0, |gaps| gaps.durations.len().saturating_sub(1));
        let build = |revision: &RevisionId, mut ids: Vec<NodeId>, marks: Vec<crate::MarkId>| {
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
                ScopedStructure::Paste { before, .. } => paste
                    .as_ref()
                    .expect("pastes resolve their register")
                    .edit(*before, ids, marks, timing)?,
                ScopedStructure::Ungroup => crate::OccurrenceEdit::Ungroup { wrappers: ids },
            })
        };
        // Isolation sizing and the no-op refusals depend on neither the
        // allocated names nor the revision, which only names new plays.
        let probe = build(
            self.current.revision_id(),
            vec![instance.node.clone(); extra],
            Vec::new(),
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
            marks: needs
                .marks
                .checked_add(paste_marks)
                .ok_or_else(|| limit("one-play edit identities overflow"))?,
        })?
        else {
            return Err(invalid(
                "a one-play structural edit requires an Isolation allocation",
            ));
        };
        if identities.nodes.len() != needs.nodes + extra
            || identities.marks.len() != needs.marks + paste_marks
        {
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
        let mut marks = identities.marks;
        let fresh_marks = marks.split_off(needs.marks);
        let edit = build(&new_revision, fresh, fresh_marks)?;
        let identities = crate::OccurrenceIdentities { nodes, marks };
        let (target, presentation) =
            self.current
                .occurrence_structure_selection(&instance, &edit, &identities)?;
        let leaf = LeafEdit::new(
            new_revision,
            Command::EditOccurrence {
                instance,
                edit,
                identities,
            },
        )?;
        let pasted = match (&kind, &paste) {
            (ScopedStructure::Paste { register, before }, Some(paste)) => {
                // As the root paste: a Paste step names its register value, so
                // admission revalidates the copied content or Original moment.
                crate::compound::validate_paste(
                    &self.current,
                    paste.value().as_ref(),
                    leaf.command.as_command(),
                )?;
                let applied = crate::apply(&self.current, &leaf.request(&self.current))?;
                let next = applied.forward.apply(&self.current)?;
                charge(
                    &mut self.document_bytes,
                    wire::size(&next, MAX_DOCUMENT_JSON_BYTES)?,
                    MAX_COMPOUND_DOCUMENT_BYTES,
                    "macro staged document byte limit",
                )?;
                self.current = next;
                self.steps.push(ResolvedStep::Paste {
                    name: *register,
                    edit: leaf,
                });
                self.refresh_children()?;
                let cursor = if *before { start } else { end };
                Some((cursor, paste.duration()))
            }
            _ => {
                self.commit_leaf(leaf)?;
                None
            }
        };
        let node = target.node.clone();
        self.context.scoped = Some(SemanticScopedContext {
            root: scoped.root,
            target,
            presentation,
        });
        // Split keeps the cursor on its new boundary; a paste continues at the
        // pasted beat's start; the other edits continue at the changed beat's
        // start, as the native edit does.
        if let Some((cursor, _)) = pasted {
            self.context.cursor = cursor;
        } else if kind != ScopedStructure::Split {
            self.context.cursor = start;
        }
        super::framing::validate_scoped_context(&self.current, &self.context)?;
        self.trace[trace_index].resolved_parent = self.current.parent_of(&node);
        self.trace[trace_index].resolved_selection = Some(SliceCaptureSelection::Child { node });
        if let Some((cursor, duration)) = pasted {
            self.trace[trace_index].resolved_range = Some(
                FrameRange::new(
                    cursor,
                    ProjectFrame(cursor.0.checked_add(duration.frames()).ok_or_else(|| {
                        EditError::new(EditErrorCode::TimingOverflow, "pasted interval overflows")
                    })?),
                )
                .map_err(crate::DocumentError::from)?,
            );
        }
        Ok(())
    }

    /// `Ungroup` inside Repeat/Retime contents: in one explicit play at every
    /// enclosing Repeat it isolates that occurrence first; with All plays at
    /// every level it ungroups the shared definition, which every play that
    /// follows it shows. Both distribute the group's effects exactly.
    pub(super) fn scoped_ungroup(&mut self, trace_index: usize) -> Result<(), EditError> {
        if self.context.visual_selection.is_some() {
            return Err(invalid(
                "clear the Visual selection before ungrouping a selected Sequence",
            ));
        }
        let scoped = self
            .context
            .scoped
            .clone()
            .ok_or_else(|| invalid("ungrouping inside a Repeat needs scoped contents"))?;
        let branches = |play: bool| {
            scoped
                .target
                .repeats
                .iter()
                .all(|step| matches!(step.branch, crate::RepeatEditBranch::Play { .. }) == play)
        };
        if !scoped.target.repeats.is_empty() && branches(true) {
            return self.scoped_structure(trace_index, ScopedStructure::Ungroup);
        }
        if !branches(false) {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "ungroup in This play at every enclosing Repeat, or in All plays at every level; a mix of the two is not supported",
            ));
        }
        super::framing::validate_scoped_context(&self.current, &self.context)?;
        let instance = scoped.presentation.clone();
        if !scoped.target.matches_instance(&self.current, &instance)? {
            return Err(invalid(
                "the scoped target differs from its shown play; choose the play again",
            ));
        }
        let node = scoped.target.node.clone();
        let (start, _) = self.current.occurrence_structure_interval(&instance)?;
        let required = self.ungroup_target(&node)?;
        let parent = self
            .current
            .parent_of(&node)
            .ok_or_else(|| invalid("the group has no parent"))?;
        self.charge_step(false)?;
        let SemanticAllocation::Ungroup {
            new_revision,
            wrappers,
        } = (self.allocate)(SemanticAllocationRequest::Ungroup {
            step_index: self.steps.len(),
            wrappers: required,
        })?
        else {
            return Err(invalid("macro ungrouping requires an Ungroup allocation"));
        };
        if wrappers.len() != required {
            return Err(invalid(
                "macro Ungroup requires its exact wrapper identities",
            ));
        }
        self.reserve_revision(&new_revision)?;
        for id in &wrappers {
            self.reserve_node(id)?;
        }
        // The first promoted child (or its wrapper), else the group's neighbor.
        let selected = match self.current.children(&node).next() {
            Some(first) => {
                let plan = crate::ungroup::plan(&self.current, &node)?;
                plan.wrapped()
                    .position(|child| child == first)
                    .map_or_else(|| first.clone(), |index| wrappers[index].clone())
            }
            None => self.current.neighbor(&node)?,
        };
        let edit = LeafEdit::new(
            new_revision,
            Command::Ungroup {
                node: node.clone(),
                wrappers,
            },
        )?;
        self.commit_leaf(edit)?;
        self.context.scoped = Some(SemanticScopedContext {
            root: scoped.root,
            target: crate::ScopedNodeTarget {
                node: selected.clone(),
                repeats: scoped.target.repeats,
            },
            presentation: crate::InstancePath {
                node: selected,
                repeats: instance.repeats,
            },
        });
        self.context.cursor = start;
        super::framing::validate_scoped_context(&self.current, &self.context)?;
        self.trace[trace_index].resolved_parent = Some(parent);
        self.trace[trace_index].resolved_selection = Some(SliceCaptureSelection::Child { node });
        Ok(())
    }

    /// The selected Sequence's wrapper count; other kinds refuse.
    fn ungroup_target(&self, node: &NodeId) -> Result<usize, EditError> {
        if !matches!(
            self.current.nodes().get(node).map(|node| &node.kind),
            Some(NodeKind::Sequence { .. })
        ) {
            return Err(EditError::new(
                EditErrorCode::WrongNodeKind,
                "ungroup requires a Sequence",
            ));
        }
        self.current.ungroup_requirements(node)
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
