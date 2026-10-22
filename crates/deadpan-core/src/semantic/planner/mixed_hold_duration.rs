//! Duration-only semantic authority retaining mixed Repeat branch choices.

use super::*;
use crate::{
    Anchor, AnchorIndex, AnchorTarget, BoundaryAnchor, ExactFrameRange, ExactRatio, InsertionBias,
    InstancePath, ScopedNodeTarget,
};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticMixedHoldDurationContext {
    pub root: NodeId,
    pub target: ScopedNodeTarget,
    pub presentation: Option<InstancePath>,
}

fn unavailable(message: &str) -> EditError {
    EditError::new(EditErrorCode::SelectionUnavailable, message)
}

impl ProjectDocument {
    /// Prove mixed body authority and its stable representative, or positively
    /// prove a dormant definition. Work never expands Repeat plays.
    pub fn mixed_hold_duration_visibility(
        &self,
        parent: &NodeId,
        authority: &SemanticMixedHoldDurationContext,
    ) -> Result<Option<HoldDurationVisibility>, EditError> {
        self.validate()?;
        scope_bounds(self, parent)?;
        let Some(NodeKind::Sequence { children }) = self.nodes().get(parent).map(|node| &node.kind)
        else {
            return Err(unavailable(
                "mixed Hold duration requires an ordinary Sequence parent",
            ));
        };
        if !children.contains(&authority.root)
            || !matches!(
                self.nodes().get(&authority.root).map(|node| &node.kind),
                Some(NodeKind::Repeat { .. })
            )
            || authority.target.repeats.first().map(|step| &step.repeat) != Some(&authority.root)
        {
            return Err(unavailable(
                "mixed Hold duration inspector root must be its outermost direct Repeat child",
            ));
        }
        let dormant = crate::scoped_edit::mixed_body_dormant(self, &authority.target)?;
        let Some(NodeKind::Hold { recipe }) = self
            .nodes()
            .get(&authority.target.node)
            .map(|node| &node.kind)
        else {
            return Err(unavailable("mixed Hold duration requires a captured Hold"));
        };
        let instance = match (&authority.presentation, dormant) {
            (None, true) => return Ok(None),
            (Some(instance), false) if authority.target.matches_instance(self, instance)? => {
                instance
            }
            _ => {
                return Err(unavailable(
                    "mixed Hold duration requires its live presentation or a proven dormant definition",
                ));
            }
        };
        if crate::scoped_edit::fixed_mixed_preflight(self, &authority.target)?.is_some() {
            return self
                .hold_duration_visibility_for_instance(&authority.root, instance)
                .map(Some);
        }
        let boundary = crate::derived_clock::with_sealed(|| {
            AnchorIndex::new(self)
                .and_then(|index| {
                    index.resolve_target(&AnchorTarget {
                        boundary: BoundaryAnchor {
                            coordinate: Anchor::Occurrence {
                                instance: instance.clone(),
                                position: ExactRatio::ZERO,
                            },
                            bias: InsertionBias::Right,
                        },
                        occurrence: None,
                    })
                })
                .map_err(|error| unavailable(&error.to_string()))
        })?;
        if boundary.exact_frame.denominator() != 1 {
            return Err(unavailable(
                "mixed Hold duration requires integral Repeat clocks",
            ));
        }
        let end = boundary
            .frame
            .0
            .checked_add(recipe.duration.frames())
            .ok_or_else(|| invalid("mixed Hold duration end overflows"))?;
        Ok(Some(HoldDurationVisibility {
            exact: ExactFrameRange::new(boundary.exact_frame, ExactRatio::integer(end))?,
            frames: FrameRange::new(boundary.frame, ProjectFrame(end))
                .map_err(crate::DocumentError::from)?,
        }))
    }
}

pub(super) fn validate_context(
    document: &ProjectDocument,
    context: &SemanticContext,
) -> Result<(), EditError> {
    let Some(authority) = &context.mixed_hold_duration else {
        return Ok(());
    };
    if context.original.is_some()
        || context.scoped.is_some()
        || context.hold_duration.is_some()
        || context.shared_hold_duration.is_some()
        || context.selected_hold_duration.is_some()
        || context.visual_selection.is_some()
        || context.selected_child.as_ref() != Some(&authority.root)
    {
        return Err(unavailable(
            "mixed Hold duration requires Your edit, its selected inspector root and no other authority or Visual selection",
        ));
    }
    document.mixed_hold_duration_visibility(&context.parent, authority)?;
    Ok(())
}

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: FnMut(&ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError>,
    P: FnMut(&ProjectDocument, PauseSite) -> Result<PauseProvider, EditError>,
{
    pub(super) fn set_mixed_hold_duration(
        &mut self,
        trace_index: usize,
        length: crate::PauseLength,
        authority: SemanticMixedHoldDurationContext,
    ) -> Result<(), EditError> {
        validate_context(&self.current, &self.context)?;
        let duration = length.resolve(self.current.presentation_basis().frame_rate)?;
        let requirements = self
            .current
            .scoped_hold_duration_requirements(&authority.target, duration)?;
        self.charge_step(false)?;
        let SemanticAllocation::Isolation {
            new_revision,
            identities,
        } = (self.allocate)(SemanticAllocationRequest::Isolation {
            step_index: self.steps.len(),
            nodes: requirements.nodes,
            marks: requirements.marks,
        })?
        else {
            return Err(invalid(
                "mixed Hold duration requires an Isolation allocation",
            ));
        };
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
        let leaf = LeafEdit::new(
            new_revision,
            Command::SetScopedHoldDuration {
                target: authority.target,
                duration,
                identities,
            },
        )?;
        let prepared = crate::prepare_scoped_hold_duration(
            &self.current,
            &leaf.request(&self.current),
            authority.presentation.as_ref(),
        )?;
        let start = prepared
            .fixed_visibility()
            .map(|visible| visible.frames.start())
            .or_else(|| prepared.boundary.as_ref().map(|boundary| boundary.frame));
        let mapped = SemanticMixedHoldDurationContext {
            root: authority.root,
            target: prepared.target,
            presentation: prepared.presentation,
        };
        self.commit_leaf(leaf)?;
        self.context.mixed_hold_duration = Some(Box::new(mapped.clone()));
        if let Some(start) = start {
            self.context.cursor = start;
        }
        super::validate_context(&self.current, &self.context)?;
        self.trace[trace_index].resolved_parent = self.current.parent_of(&mapped.target.node);
        self.trace[trace_index].resolved_selection = Some(SliceCaptureSelection::Child {
            node: mapped.target.node.clone(),
        });
        self.trace[trace_index].resolved_range = self
            .current
            .mixed_hold_duration_visibility(&self.context.parent, &mapped)?
            .map(|visible| visible.frames);
        Ok(())
    }
}
