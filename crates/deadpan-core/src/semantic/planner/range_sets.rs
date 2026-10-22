//! One frozen interval union followed by reverse-order ordinary range cuts.

use super::*;

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: FnMut(&ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError>,
    P: FnMut(&ProjectDocument, super::PauseSite) -> Result<super::PauseProvider, EditError>,
{
    pub(super) fn capture_ranges(
        &mut self,
        trace_index: usize,
        register: RegisterName,
        ranges: &[FrameRange],
        cut: bool,
    ) -> Result<(), EditError> {
        let target =
            crate::semantic::range_set::validate_geometry(&self.current, &self.context, ranges)?;
        if self
            .steps
            .len()
            .checked_add(1 + if cut { target.ranges.len() } else { 0 })
            .is_none_or(|count| count > MAX_COMPOUND_STEPS)
        {
            return Err(limit("range set exceeds resolved editing step limit"));
        }
        let parent = self.context.parent.clone();
        let (bounds, scope, scope_labels) = self.capture_scope(&parent)?;
        self.charge_step(true)?;
        let SemanticAllocation::Yank { capture_revision } =
            (self.allocate)(SemanticAllocationRequest::Yank {
                step_index: self.steps.len(),
            })?
        else {
            return Err(invalid("a range set capture requires a Yank allocation"));
        };
        self.reserve_revision(&capture_revision)?;
        let timing = AudioTimingId {
            allocation: capture_revision,
            ordinal: 0,
        };
        let set = Arc::new(crate::CapturedEditSet::capture_ranges(
            &self.current,
            &parent,
            &target.ranges,
            timing.clone(),
        )?);
        charge(
            &mut self.captured_bytes,
            wire::size(set.as_ref(), MAX_COMPOUND_CAPTURE_BYTES)?,
            MAX_COMPOUND_CAPTURE_BYTES,
            "macro union capture byte limit",
        )?;
        let value = Arc::new(RegisterValue::EditedSet { set });
        self.writes.insert(register, value.clone());
        self.writes.insert(RegisterName::unnamed(), value.clone());
        self.steps.push(ResolvedStep::Yank {
            name: register,
            value,
        });
        if cut {
            self.context.visual_selection = None;
            for range in target.ranges.iter().rev() {
                // Preflight each leaf against its actual staged predecessor:
                // earlier cuts may have replaced the same physical Source.
                let required_nodes = self.current.range_deletion(&parent, *range)?.required_ids;
                self.charge_step(false)?;
                let SemanticAllocation::DeleteRange {
                    new_revision,
                    identities,
                } = (self.allocate)(SemanticAllocationRequest::DeleteRange {
                    step_index: self.steps.len(),
                    required_nodes,
                })?
                else {
                    return Err(invalid(
                        "range set deletion requires a DeleteRange allocation",
                    ));
                };
                if identities.nodes.len() != required_nodes {
                    return Err(invalid(
                        "range set deletion requires exactly its staged Split identities",
                    ));
                }
                self.reserve_revision(&new_revision)?;
                for node in &identities.nodes {
                    self.reserve_node(node)?;
                }
                let timing = AudioTimingId {
                    allocation: new_revision.clone(),
                    ordinal: 0,
                };
                self.commit_leaf(LeafEdit::new(
                    new_revision,
                    Command::DeleteRange {
                        parent: parent.clone(),
                        range: *range,
                        identities,
                        timing,
                    },
                )?)?;
            }
            self.context.cursor = ProjectFrame(target.ranges[0].start().0.min(self.bounds.1.0));
            self.context.selected_child =
                selected_child(&self.child_ends, self.context.cursor, self.bounds);
            validate_context(&self.current, &self.context)?;
            validate_selection(&self.current, &self.context)?;
        }
        let trace = &mut self.trace[trace_index];
        trace.capture_set = Some(SemanticCaptureProvenance {
            timing,
            parent,
            bounds,
            scope,
            scope_labels,
        });
        trace.resolved_ranges = target.ranges;
        Ok(())
    }
}
