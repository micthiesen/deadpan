//! Duration intent for literal Repeat defaults, including unused definitions.

use super::*;
use crate::{
    Anchor, AnchorIndex, AnchorTarget, BoundaryAnchor, ExactFrameRange, ExactRatio, InsertionBias,
    InstancePath, RepeatEditBranch, ScopedNodeTarget,
};

/// Invocation-only All plays authority. A dormant definition has no invented
/// representative; a live definition retains the host's concrete presentation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSharedHoldDurationContext {
    pub root: NodeId,
    pub target: ScopedNodeTarget,
    pub presentation: Option<InstancePath>,
}

fn unavailable(message: &str) -> EditError {
    EditError::new(EditErrorCode::SelectionUnavailable, message)
}

impl ProjectDocument {
    /// Prove a literal all-Default Hold address and resolve its captured live
    /// presentation. None proves that at least one enclosing Repeat has every
    /// body play overridden. Gap overrides do not suppress default body uses.
    /// Work follows the compact authored graph, never the expanded play count.
    pub fn shared_hold_duration_visibility(
        &self,
        parent: &NodeId,
        authority: &SemanticSharedHoldDurationContext,
    ) -> Result<Option<HoldDurationVisibility>, EditError> {
        self.validate()?;
        scope_bounds(self, parent)?;
        let Some(NodeKind::Sequence { children }) = self.nodes().get(parent).map(|node| &node.kind)
        else {
            return Err(unavailable(
                "shared Hold duration requires an ordinary Sequence parent",
            ));
        };
        if !children.contains(&authority.root)
            || !matches!(
                self.nodes().get(&authority.root).map(|node| &node.kind),
                Some(NodeKind::Repeat { .. })
            )
        {
            return Err(unavailable(
                "shared Hold duration inspector root must be a direct Repeat child",
            ));
        }
        let target = &authority.target;
        if target.repeats.is_empty()
            || target
                .repeats
                .iter()
                .any(|step| step.branch != RepeatEditBranch::Default)
        {
            return Err(unavailable(
                "shared Hold duration requires All plays at every Repeat ancestor",
            ));
        }
        target.validate(self)?;
        let Some(NodeKind::Hold { recipe }) = self.nodes().get(&target.node).map(|node| &node.kind)
        else {
            return Err(unavailable("shared Hold duration requires a captured Hold"));
        };
        let mut parents = BTreeMap::new();
        for id in self.nodes().keys() {
            for child in self.children(id) {
                parents.insert(child, id);
            }
        }
        let mut current = &target.node;
        let mut saw_root = false;
        let mut dormant = false;
        let mut reached_root = false;
        for _ in 0..=crate::MAX_DOCUMENT_DEPTH {
            saw_root |= current == &authority.root;
            if current == self.root() {
                reached_root = true;
                break;
            }
            let ancestor = parents.get(current).copied().ok_or_else(|| {
                unavailable("shared Hold duration target has no project ancestry")
            })?;
            match &self.nodes()[ancestor].kind {
                NodeKind::Sequence { .. } => {}
                NodeKind::Repeat {
                    child, iterations, ..
                } if current == child => {
                    // Validation proves all keys are distinct current body plays.
                    // Equal counts prove this entire definition is unused.
                    let overrides = self
                        .overrides()
                        .get(ancestor)
                        .map_or(0, |entries| entries.len());
                    dormant |= u64::try_from(overrides)
                        .is_ok_and(|count| count == u64::from(iterations.len()));
                }
                _ => {
                    return Err(unavailable(
                        "shared Hold duration requires literal Repeat bodies and ordinary Sequences, without Retime ancestry",
                    ));
                }
            }
            current = ancestor;
        }
        if !reached_root || !saw_root {
            return Err(unavailable(
                "shared Hold duration target lies outside its captured inspector root",
            ));
        }
        match (&authority.presentation, dormant) {
            (None, true) => return Ok(None),
            (None, false) => {
                return Err(unavailable(
                    "a live All plays Hold needs its captured concrete presentation",
                ));
            }
            (Some(_), true) => {
                return Err(unavailable(
                    "a dormant All plays Hold has no concrete presentation",
                ));
            }
            (Some(_), false) => {}
        }
        let instance = authority
            .presentation
            .as_ref()
            .expect("live presentation checked");
        if !target.matches_instance(self, instance)? {
            return Err(unavailable(
                "shared Hold duration presentation differs from its literal default target",
            ));
        }
        // Unrelated clocked Retimes remain sealed. This target's independently
        // checked Sequence/Repeat path uses only exact integral owner clocks.
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
                "shared Hold duration needs an integral Repeat clock",
            ));
        }
        let end = boundary
            .frame
            .0
            .checked_add(recipe.duration.frames())
            .ok_or_else(|| {
                EditError::new(
                    EditErrorCode::TimingOverflow,
                    "shared Hold duration end overflows",
                )
            })?;
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
    let Some(authority) = &context.shared_hold_duration else {
        return Ok(());
    };
    if context.scoped.is_some()
        || context.hold_duration.is_some()
        || context.original.is_some()
        || context.visual_selection.is_some()
        || context.selected_child.as_ref() != Some(&authority.root)
    {
        return Err(unavailable(
            "shared Hold duration requires Your edit, its selected inspector root and no other editing authority or Visual selection",
        ));
    }
    document.shared_hold_duration_visibility(&context.parent, authority)?;
    Ok(())
}

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: FnMut(&ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError>,
    P: FnMut(&ProjectDocument, PauseSite) -> Result<PauseProvider, EditError>,
{
    pub(super) fn set_shared_hold_duration(
        &mut self,
        trace_index: usize,
        length: crate::PauseLength,
        authority: SemanticSharedHoldDurationContext,
    ) -> Result<(), EditError> {
        validate_context(&self.current, &self.context)?;
        let NodeKind::Hold { recipe } = &self.current.nodes()[&authority.target.node].kind else {
            unreachable!("shared duration authority validated a Hold")
        };
        let duration = length.resolve(self.current.presentation_basis().frame_rate)?;
        if recipe.duration == duration {
            return Err(invalid(
                "the pause already has that duration; no edit was made",
            ));
        }
        let before_duration = self.current.structural_duration()?;
        self.charge_step(false)?;
        let SemanticAllocation::ParameterEdit { new_revision } =
            (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                step_index: self.steps.len(),
            })?
        else {
            return Err(invalid(
                "a shared pause duration requires a parameter allocation",
            ));
        };
        self.reserve_revision(&new_revision)?;
        self.commit_leaf(LeafEdit::new(
            new_revision,
            Command::SetHoldDuration {
                node: authority.target.node.clone(),
                duration,
            },
        )?)?;
        let visible = self
            .current
            .shared_hold_duration_visibility(&self.context.parent, &authority)?;
        if let Some(visible) = visible {
            self.context.cursor = visible.frames.start();
        } else if self.current.structural_duration()? != before_duration {
            return Err(invalid(
                "a dormant Hold duration changed rendered project time",
            ));
        }
        super::validate_context(&self.current, &self.context)?;
        self.trace[trace_index].resolved_parent = self.current.parent_of(&authority.target.node);
        self.trace[trace_index].resolved_selection = Some(SliceCaptureSelection::Child {
            node: authority.target.node,
        });
        self.trace[trace_index].resolved_range = visible.map(|visible| visible.frames);
        Ok(())
    }
}
