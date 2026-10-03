//! Selected-beat copies and pastes use the same frozen bank and leaf reducers
//! as cuts. Selection is explicit because empty siblings share a time boundary.

use super::*;

#[derive(Clone, Copy)]
enum Destination {
    Seam { index: usize, cursor: ProjectFrame },
    Replacement { range: FrameRange },
}

impl<F, R> Planner<'_, F, R>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
{
    pub(super) fn capture_selector(
        &mut self,
        trace_index: usize,
        register: RegisterName,
        selector: SemanticSelector,
        cut: bool,
    ) -> Result<(), EditError> {
        let selection = self.resolve_selector(selector)?;
        self.capture_instruction(trace_index, register, selection, cut)?;
        if !cut && selector == SemanticSelector::VisualSelection {
            self.finish_selection()?;
        }
        Ok(())
    }

    pub(super) fn capture_instruction(
        &mut self,
        trace_index: usize,
        register: RegisterName,
        selection: SliceCaptureSelection,
        cut: bool,
    ) -> Result<(), EditError> {
        let label = match &selection {
            SliceCaptureSelection::Child { node } => Some(self.current.nodes()[node].label.clone()),
            SliceCaptureSelection::Range { .. } => None,
        };
        let range = if cut {
            self.cut(register, &selection)?
        } else {
            self.capture(register, selection.clone())?
        };
        let trace = &mut self.trace[trace_index];
        trace.resolved_range = Some(range);
        trace.resolved_selection = Some(selection);
        trace.captured_child_label = label;
        Ok(())
    }

    pub(super) fn capture(
        &mut self,
        register: RegisterName,
        selection: SliceCaptureSelection,
    ) -> Result<FrameRange, EditError> {
        self.charge_step(true)?;
        let SemanticAllocation::Yank { capture_revision } =
            (self.allocate)(SemanticAllocationRequest::Yank {
                step_index: self.steps.len(),
            })?
        else {
            return Err(invalid("macro yank requires a Yank allocation"));
        };
        self.reserve_revision(&capture_revision)?;
        let slice = Arc::new(CapturedEditSlice::capture_selection(
            &self.current,
            &self.context.parent,
            &selection,
            AudioTimingId {
                allocation: capture_revision,
                ordinal: 0,
            },
        )?);
        let range = slice.range();
        let value = Arc::new(RegisterValue::Edited { slice });
        self.writes.insert(register, value.clone());
        self.writes.insert(RegisterName::unnamed(), value.clone());
        self.steps.push(ResolvedStep::Yank {
            name: register,
            value,
        });
        Ok(range)
    }

    pub(super) fn paste(
        &mut self,
        register: RegisterName,
        before: bool,
    ) -> Result<FrameRange, EditError> {
        validate_selection(&self.current, &self.context)?;
        let NodeKind::Sequence { children } = &self.current.nodes()[&self.context.parent].kind
        else {
            unreachable!("the macro parent is an ordinary Sequence")
        };
        let index = if children.is_empty() {
            0
        } else {
            let selected = self.context.selected_child.as_ref().ok_or_else(|| {
                EditError::new(
                    EditErrorCode::SelectionUnavailable,
                    "select a beat before pasting",
                )
            })?;
            children
                .iter()
                .position(|child| child == selected)
                .expect("validated direct child")
                + usize::from(!before)
        };
        let cursor = self
            .current
            .source_splice_boundary(&self.context.parent, index)?;
        self.place(register, Destination::Seam { index, cursor })
    }

    pub(super) fn replace(
        &mut self,
        register: RegisterName,
        range: FrameRange,
    ) -> Result<FrameRange, EditError> {
        self.place(register, Destination::Replacement { range })
    }

    fn place(
        &mut self,
        register: RegisterName,
        destination: Destination,
    ) -> Result<FrameRange, EditError> {
        let cursor = match destination {
            Destination::Seam { cursor, .. } => cursor,
            Destination::Replacement { range } => range.start(),
        };
        let value = if let Some(value) = self.writes.get(&register) {
            value.clone()
        } else {
            let value = self.bank.get(&register).cloned();
            self.inputs.insert(register, value.clone());
            value.ok_or_else(|| invalid("the paste register is empty"))?
        };
        if matches!(value.as_ref(), RegisterValue::Macro { .. }) {
            return Err(invalid("a macro cannot be pasted as content"));
        }
        self.charge_step(false)?;
        charge(
            &mut self.document_bytes,
            wire::size(value.as_ref(), MAX_DOCUMENT_JSON_BYTES)?,
            MAX_COMPOUND_DOCUMENT_BYTES,
            "macro pasted content byte limit",
        )?;
        let (edit, selected, duration) = match value.as_ref() {
            RegisterValue::Edited { slice } => {
                let requirements = slice.identity_requirements()?;
                let required_split_ids = match destination {
                    Destination::Seam { .. } => 0,
                    Destination::Replacement { range } => {
                        self.current
                            .slice_replacement(&self.context.parent, range, slice)?
                            .required_ids
                    }
                };
                if self
                    .current
                    .nodes()
                    .len()
                    .checked_add(requirements.nodes)
                    .is_none_or(|count| count > crate::MAX_DOCUMENT_NODES)
                    || self
                        .current
                        .marks()
                        .len()
                        .checked_add(requirements.marks)
                        .is_none_or(|count| count > crate::MAX_DOCUMENT_MARKS)
                {
                    return Err(limit("macro pasted identities exceed document limits"));
                }
                let SemanticAllocation::PasteEdited {
                    new_revision,
                    identities,
                    split_identities,
                } = (self.allocate)(SemanticAllocationRequest::PasteEdited {
                    step_index: self.steps.len(),
                    requirements,
                    required_split_ids,
                })?
                else {
                    return Err(invalid(
                        "macro Edited paste requires a PasteEdited allocation",
                    ));
                };
                if identities.authored.nodes.len() != requirements.nodes
                    || identities.authored.marks.len() != requirements.marks
                    || identities.aliases.len() != requirements.aliases
                {
                    return Err(invalid(
                        "macro Edited paste requires exactly its preflight identity counts",
                    ));
                }
                self.reserve_revision(&new_revision)?;
                self.reserve_splits(&split_identities, required_split_ids)?;
                for node in identities.authored.nodes.iter().chain(&identities.aliases) {
                    self.reserve_node(node)?;
                }
                for mark in &identities.authored.marks {
                    if !self.marks.insert(mark.clone()) {
                        return Err(identity("macro reuses a mark identity"));
                    }
                }
                let selected = identities
                    .authored
                    .nodes
                    .first()
                    .cloned()
                    .ok_or_else(|| invalid("macro pasted content has no imported root"))?;
                let timing = AudioTimingId {
                    allocation: new_revision.clone(),
                    ordinal: 0,
                };
                let command = match destination {
                    Destination::Seam { index, .. } => Command::SpliceSlice {
                        parent: self.context.parent.clone(),
                        index,
                        slice: slice.as_ref().clone(),
                        identities,
                        timing,
                    },
                    Destination::Replacement { range } => Command::ReplaceSlice {
                        parent: self.context.parent.clone(),
                        range,
                        slice: slice.as_ref().clone(),
                        identities,
                        split_identities,
                        timing,
                    },
                };
                (
                    LeafEdit::new(new_revision, command)?,
                    selected,
                    slice.duration(),
                )
            }
            RegisterValue::Original {
                asset, ordinals, ..
            } => {
                let required_split_ids = match destination {
                    Destination::Seam { .. } => 0,
                    Destination::Replacement { range } => {
                        self.current
                            .source_replacement(&self.context.parent, range)?
                            .required_ids
                    }
                };
                if self.current.nodes().len() == crate::MAX_DOCUMENT_NODES {
                    return Err(limit(
                        "macro Original paste exceeds the document node limit",
                    ));
                }
                let source = (self.resolve_original)(&self.current, value.as_ref())?;
                let duration = source.duration;
                let label = self
                    .current
                    .assets()
                    .get(asset)
                    .ok_or_else(|| {
                        invalid("Original paste asset is missing from the staged document")
                    })?
                    .label
                    .clone();
                let SemanticAllocation::PasteOriginal {
                    new_revision,
                    node,
                    split_identities,
                } = (self.allocate)(SemanticAllocationRequest::PasteOriginal {
                    step_index: self.steps.len(),
                    required_split_ids,
                })?
                else {
                    return Err(invalid(
                        "macro Original paste requires a PasteOriginal allocation",
                    ));
                };
                self.reserve_revision(&new_revision)?;
                self.reserve_splits(&split_identities, required_split_ids)?;
                self.reserve_node(&node)?;
                let label = format!("{label} [{}..{})", ordinals.start, ordinals.end);
                let timing = AudioTimingId {
                    allocation: new_revision.clone(),
                    ordinal: 0,
                };
                let command = match destination {
                    Destination::Seam { index, .. } => Command::SpliceSource {
                        parent: self.context.parent.clone(),
                        index,
                        source,
                        id: node.clone(),
                        label,
                        timing,
                    },
                    Destination::Replacement { range } => Command::ReplaceSource {
                        parent: self.context.parent.clone(),
                        range,
                        source,
                        id: node.clone(),
                        label,
                        identities: split_identities,
                        timing,
                    },
                };
                (LeafEdit::new(new_revision, command)?, node, duration)
            }
            RegisterValue::Macro { .. } => unreachable!("macro paste rejected before allocation"),
        };
        crate::compound::validate_paste(&self.current, value.as_ref(), edit.command.as_command())?;
        let end = cursor.0.checked_add(duration.frames()).ok_or_else(|| {
            EditError::new(
                EditErrorCode::TimingOverflow,
                "macro pasted interval overflows",
            )
        })?;
        let range =
            FrameRange::new(cursor, ProjectFrame(end)).map_err(crate::DocumentError::from)?;
        let applied = crate::apply(&self.current, &edit.request(&self.current))?;
        let next = applied.forward.apply(&self.current)?;
        charge(
            &mut self.document_bytes,
            wire::size(&next, MAX_DOCUMENT_JSON_BYTES)?,
            MAX_COMPOUND_DOCUMENT_BYTES,
            "macro staged document byte limit",
        )?;
        self.current = next;
        self.steps.push(ResolvedStep::Paste {
            name: register,
            edit,
        });
        self.context.cursor = cursor;
        self.context.selected_child = Some(selected);
        self.context.visual_selection = None;
        self.refresh_children()?;
        Ok(range)
    }

    fn reserve_splits(
        &mut self,
        identities: &SplitIdentities,
        required: usize,
    ) -> Result<(), EditError> {
        if identities.nodes.len() != required {
            return Err(invalid(
                "macro paste requires exactly its preflight Split identities",
            ));
        }
        for node in &identities.nodes {
            self.reserve_node(node)?;
        }
        Ok(())
    }

    fn charge_step(&mut self, capture: bool) -> Result<(), EditError> {
        if self.steps.len() == MAX_COMPOUND_STEPS {
            return Err(limit("macro exceeds 1024 resolved editing steps"));
        }
        let size = wire::size(&self.current, MAX_DOCUMENT_JSON_BYTES)?;
        charge(
            &mut self.document_bytes,
            size,
            MAX_COMPOUND_DOCUMENT_BYTES,
            "macro staged document byte limit",
        )?;
        if capture {
            charge(
                &mut self.captured_bytes,
                size,
                MAX_COMPOUND_CAPTURE_BYTES,
                "macro captured document byte limit",
            )?;
        }
        Ok(())
    }

    fn reserve_revision(&mut self, revision: &RevisionId) -> Result<(), EditError> {
        if !self.revisions.insert(revision.clone()) {
            return Err(identity("macro reuses a revision or capture allocation"));
        }
        Ok(())
    }

    fn reserve_node(&mut self, node: &NodeId) -> Result<(), EditError> {
        if !self.nodes.insert(node.clone()) {
            return Err(identity(
                "macro reuses an authored or historical node identity",
            ));
        }
        Ok(())
    }
}
