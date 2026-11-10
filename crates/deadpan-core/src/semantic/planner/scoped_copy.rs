//! `yy`, `dd` and `p`/`P` on the beat in the invocation's explicit play. A copy
//! captures that occurrence's effective contents without isolating anything;
//! a cut copies first and then removes the beat from that play only; a paste
//! is one `EditOccurrence` leaf beside the beat, as `scoped_structure` builds
//! the other one-play edits.

use super::*;

/// One register value resolved for a one-play paste before allocation.
pub(super) enum ScopedPaste {
    Slice {
        value: Arc<RegisterValue>,
        slice: Arc<CapturedEditSlice>,
        requirements: crate::SliceIdentityRequirements,
    },
    Source {
        value: Arc<RegisterValue>,
        source: Box<SourceNode>,
        label: String,
    },
}

impl ScopedPaste {
    pub(super) fn value(&self) -> &Arc<RegisterValue> {
        match self {
            Self::Slice { value, .. } | Self::Source { value, .. } => value,
        }
    }

    /// Fresh node and mark names beyond the isolation pool.
    pub(super) fn identities(&self) -> (usize, usize) {
        match self {
            Self::Slice { requirements, .. } => (
                requirements.nodes + requirements.aliases,
                requirements.marks,
            ),
            Self::Source { .. } => (1, 0),
        }
    }

    pub(super) fn duration(&self) -> crate::FrameDuration {
        match self {
            Self::Slice { slice, .. } => slice.duration(),
            Self::Source { source, .. } => source.duration,
        }
    }

    pub(super) fn edit(
        &self,
        before: bool,
        mut nodes: Vec<NodeId>,
        marks: Vec<crate::MarkId>,
        timing: AudioTimingId,
    ) -> Result<crate::OccurrenceEdit, EditError> {
        Ok(match self {
            Self::Slice {
                slice,
                requirements,
                ..
            } => {
                let aliases = nodes.split_off(requirements.nodes);
                crate::OccurrenceEdit::PasteSlice {
                    before,
                    slice: Box::new(slice.as_ref().clone()),
                    identities: crate::SlicePasteIdentities {
                        authored: crate::OccurrenceIdentities { nodes, marks },
                        aliases,
                    },
                    timing,
                }
            }
            Self::Source { source, label, .. } => crate::OccurrenceEdit::PasteSource {
                before,
                source: source.clone(),
                id: nodes
                    .pop()
                    .ok_or_else(|| invalid("a one-play paste needs its beat identity"))?,
                label: label.clone(),
                timing,
            },
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
    /// Copy (and for a cut, then delete in this play only) the selected beat
    /// of the shown play.
    pub(super) fn scoped_capture(
        &mut self,
        trace_index: usize,
        register: RegisterName,
        selector: &SemanticSelector,
        cut: bool,
    ) -> Result<(), EditError> {
        if selector != &SemanticSelector::SelectedBeat {
            return Err(unavailable(
                "inside one play, yy copies and dd cuts the selected beat; copy ranges and other motions from the parent",
            ));
        }
        let scoped = self
            .context
            .scoped
            .clone()
            .ok_or_else(|| invalid("a one-play copy needs scoped contents"))?;
        super::framing::validate_scoped_context(&self.current, &self.context)?;
        let instance = scoped.presentation.clone();
        if !scoped.target.matches_instance(&self.current, &instance)? {
            return Err(invalid(
                "the one-play target differs from its shown play; choose the play again",
            ));
        }
        if instance.repeats.is_empty() {
            return Err(unavailable(
                "a one-play copy needs This play chosen at every enclosing Repeat",
            ));
        }
        let parent = self
            .current
            .parent_of(&instance.node)
            .ok_or_else(|| unavailable("the selected beat has no group in this play"))?;
        let label = self.current.nodes()[&instance.node].label.clone();
        self.charge_step(true)?;
        let SemanticAllocation::Yank { capture_revision } =
            (self.allocate)(SemanticAllocationRequest::Yank {
                step_index: self.steps.len(),
            })?
        else {
            return Err(invalid("macro yank requires a Yank allocation"));
        };
        self.reserve_revision(&capture_revision)?;
        let slice = Arc::new(CapturedEditSlice::capture_occurrence(
            &self.current,
            &instance,
            AudioTimingId {
                allocation: capture_revision,
                ordinal: 0,
            },
        )?);
        let value = Arc::new(RegisterValue::Edited {
            slice: slice.clone(),
        });
        self.writes.insert(register, value.clone());
        self.writes.insert(RegisterName::unnamed(), value.clone());
        self.steps.push(ResolvedStep::Yank {
            name: register,
            value,
        });
        // The copy's path names every ancestor, including the enclosing
        // Repeats; its bounds are the copied occurrence itself.
        let mut scope = Vec::new();
        let mut current = parent.clone();
        while current != *self.current.root() {
            if scope.len() == crate::MAX_DOCUMENT_DEPTH {
                return Err(limit("macro capture scope depth limit"));
            }
            scope.push(current.clone());
            current = self
                .current
                .parent_of(&current)
                .ok_or_else(|| unavailable("macro capture parent is detached"))?;
        }
        scope.reverse();
        let scope_labels = scope
            .iter()
            .map(|node| self.current.nodes()[node].label.clone())
            .collect();
        let selection = SliceCaptureSelection::Child {
            node: instance.node.clone(),
        };
        let trace = &mut self.trace[trace_index];
        trace.resolved_parent = Some(parent.clone());
        trace.resolved_range = Some(slice.range());
        trace.resolved_selection = Some(selection);
        trace.captured_child_label = Some(label);
        trace.capture = Some(SemanticCaptureProvenance {
            timing: slice.capture_timing().clone(),
            parent,
            bounds: slice.range(),
            scope,
            scope_labels,
        });
        if cut {
            // The cut's removal is the ordinary one-play deletion, staged on
            // the document the copy captured, in the same transaction.
            let capture = self.trace[trace_index].clone();
            self.scoped_structure(
                trace_index,
                super::scoped_structure::ScopedStructure::Delete,
            )?;
            let trace = &mut self.trace[trace_index];
            trace.resolved_parent = capture.resolved_parent;
            trace.resolved_range = capture.resolved_range;
            trace.resolved_selection = capture.resolved_selection;
        }
        Ok(())
    }

    /// The register value a one-play paste places, resolved on the staged
    /// document before any name is allocated.
    pub(super) fn scoped_paste_content(
        &mut self,
        register: RegisterName,
    ) -> Result<ScopedPaste, EditError> {
        let value = if let Some(value) = self.writes.get(&register) {
            value.clone()
        } else {
            let value = self.bank.get(&register).cloned();
            self.inputs.insert(register, value.clone());
            value.ok_or_else(|| unavailable("the paste register is empty"))?
        };
        charge(
            &mut self.document_bytes,
            wire::size(value.as_ref(), MAX_DOCUMENT_JSON_BYTES)?,
            MAX_COMPOUND_DOCUMENT_BYTES,
            "macro pasted content byte limit",
        )?;
        match value.as_ref() {
            RegisterValue::Edited { slice } => {
                slice.check_destination(&self.current)?;
                if slice.duration() == crate::FrameDuration::ZERO {
                    return Err(unavailable(
                        "an empty copied group adds no time to a play; paste it from the parent",
                    ));
                }
                let requirements = slice.identity_requirements()?;
                if self
                    .current
                    .marks()
                    .len()
                    .checked_add(requirements.marks)
                    .is_none_or(|count| count > crate::MAX_DOCUMENT_MARKS)
                {
                    return Err(limit("macro pasted identities exceed document limits"));
                }
                Ok(ScopedPaste::Slice {
                    slice: slice.clone(),
                    requirements,
                    value,
                })
            }
            RegisterValue::Original {
                asset, ordinals, ..
            } => {
                let source = (self.resolve_original)(&self.current, value.as_ref())?;
                let label = self
                    .current
                    .assets()
                    .get(asset)
                    .ok_or_else(|| {
                        invalid("Original paste asset is missing from the staged document")
                    })?
                    .label
                    .clone();
                let label = format!("{label} [{}..{})", ordinals.start, ordinals.end);
                Ok(ScopedPaste::Source {
                    source: Box::new(source),
                    label,
                    value,
                })
            }
            RegisterValue::EditedSet { .. } => Err(unavailable(
                "a copied set of beats pastes from the parent; inside one play, paste one copied beat or Original moment",
            )),
            RegisterValue::Macro { .. } => Err(invalid("a macro cannot be pasted as content")),
        }
    }
}

fn unavailable(message: &str) -> EditError {
    EditError::new(EditErrorCode::SelectionUnavailable, message)
}
