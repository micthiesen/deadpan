//! Picture queries in an authored definition's clock, before its outer owners.

use std::sync::Arc;

use deadpan_core::{
    BoundaryQueryLimits, CapturedFraming, ExactFrameRange, ExactRatio, FrameDuration, InstancePath,
    IterationId, MAX_DOCUMENT_DEPTH, NodeId, ProjectId, RepeatEditBranch, RevisionId,
    ScopedNodeTarget,
};
use serde::Serialize;

use super::{CompiledKind, LookupStats, RenderPlan};
use crate::{Picture, PictureCaption, PictureFraming, PlanError};

/// A picture in `definition`'s complete intrinsic output, before outer Repeat,
/// Retime, framing or cutaway owners. `position` is exact local time, not a root
/// project frame. The definition and revision brand every relative path here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DefinitionPictureSample {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub definition: NodeId,
    pub position: ExactRatio,
    /// Relative to the definition root. Outer Repeat steps are absent, so this
    /// must not be validated or used as a full project occurrence on its own.
    pub instance: InstancePath,
    pub gap_after: Option<IterationId>,
    /// Exact coordinate within the resulting provider or implicit gap recipe.
    pub local_position: ExactRatio,
    pub picture: Picture,
    #[serde(serialize_with = "crate::picture::serialize_shared")]
    pub picture_context: Option<Arc<CapturedFraming>>,
    /// Provider to definition root, with definition-relative instance paths.
    pub framing: Vec<PictureFraming>,
    pub captions: Vec<PictureCaption>,
    pub lookup: LookupStats,
}

/// The complete authored Hold and adjacent pictures in its containing ordinary
/// Sequence definition. Outer Repeat/Retime boundaries stop that definition.
/// This grants no root presentation or claim about joins after outer retiming.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScopedHoldBoundaries {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub target: ScopedNodeTarget,
    pub definition: NodeId,
    pub range: ExactFrameRange,
    pub duration: FrameDuration,
    /// Absent at the definition's beginning, even when the Hold freezes a frame.
    pub left: Option<DefinitionPictureSample>,
    /// Absent at the definition's end. No source successor is manufactured.
    pub right: Option<DefinitionPictureSample>,
    /// Combined ancestry and picture-query work. Repeat identity validation
    /// conservatively charges the compact segment count before its lookup.
    pub lookup: LookupStats,
}

/// Shared descent output before the caller brands its clock. Keeping that
/// distinction here avoids adding a definition identity allocation to ordinary
/// project-frame sampling.
pub(super) struct PictureWalk {
    pub(super) instance: InstancePath,
    pub(super) gap_after: Option<IterationId>,
    pub(super) local_position: ExactRatio,
    pub(super) picture: Picture,
    pub(super) picture_context: Option<Arc<CapturedFraming>>,
    pub(super) framing: Vec<PictureFraming>,
    pub(super) captions: Vec<PictureCaption>,
    pub(super) lookup: LookupStats,
}

impl RenderPlan {
    /// Sample an exact position in an existing authored definition. This works
    /// for dormant Repeat defaults and inactive explicitly owned gap branches.
    /// Cutaways and descendant retimes retain the ordinary picture semantics.
    /// Limits cover visited nodes and Sequence/Repeat comparisons; bounded
    /// per-node framing, captions and cutaways are covered by each node visit.
    pub fn definition_picture(
        &self,
        definition: &NodeId,
        position: ExactRatio,
        limits: BoundaryQueryLimits,
    ) -> Result<DefinitionPictureSample, PlanError> {
        let root = *self
            .by_id
            .get(definition)
            .ok_or_else(|| PlanError::InvalidPictureDefinition(definition.clone()))?;
        self.sample_definition_picture(root, position, true, &mut PictureBudget::new(limits))
    }

    /// Resolve a validated scoped Hold without selecting a representative play.
    /// Every Repeat ancestor is checked against this immutable compiled revision.
    /// Default means the literal definition even when all plays override it;
    /// Play must own the effective child or an explicitly owned gap branch.
    ///
    /// The definition is the highest ordinary Sequence ancestor below the first
    /// Repeat/Retime owner, or the project root. For Hold interval `[s,e)`, the
    /// neighboring frame centers are `s - 1/2` and `e + 1/2`. A definition edge
    /// yields `None`, including a Hold that is itself the entire definition.
    /// An implicit configured Repeat gap has no Hold NodeId and cannot be named.
    /// One budget covers target validation and both boundary samples.
    pub fn scoped_hold_boundaries(
        &self,
        target: &ScopedNodeTarget,
        limits: BoundaryQueryLimits,
    ) -> Result<ScopedHoldBoundaries, PlanError> {
        if self.audio_context {
            return Err(PlanError::AudioOnlyContext);
        }
        if target.repeats.len() > MAX_DOCUMENT_DEPTH {
            return Err(PlanError::InvalidScopedHold(
                "Repeat ancestry exceeds its bound",
            ));
        }
        let hold = *self
            .by_id
            .get(&target.node)
            .ok_or(PlanError::InvalidScopedHold(
                "the Hold is absent from this revision",
            ))?;
        if !matches!(&self.nodes[hold].kind, CompiledKind::Hold { .. }) {
            return Err(PlanError::InvalidScopedHold(
                "the target is not an authored Hold",
            ));
        }
        let duration = self.nodes[hold].inspection.duration;
        let mut budget = PictureBudget::new(limits);
        let mut current = hold;
        let mut definition = hold;
        let mut start = 0_i64;
        let mut in_definition = true;
        let mut step = target.repeats.len();
        loop {
            budget.visit()?;
            let Some(parent) = self.parents[current] else {
                if current != self.root {
                    return Err(PlanError::InvalidScopedHold("the target is unreachable"));
                }
                break;
            };
            match &self.nodes[parent].kind {
                CompiledKind::Repeat {
                    default_child,
                    layout,
                    ..
                } => {
                    step = step.checked_sub(1).ok_or(PlanError::InvalidScopedHold(
                        "the target omits a Repeat ancestor",
                    ))?;
                    let requested = &target.repeats[step];
                    if requested.repeat != self.nodes[parent].inspection.id {
                        return Err(PlanError::InvalidScopedHold(
                            "the Repeat ancestry is out of order",
                        ));
                    }
                    match &requested.branch {
                        RepeatEditBranch::Default if current == *default_child => {}
                        RepeatEditBranch::Default => {
                            return Err(PlanError::InvalidScopedHold(
                                "Default does not own this branch",
                            ));
                        }
                        RepeatEditBranch::Play { iteration } => {
                            // RepeatLayout::play scans compact segments, not plays.
                            // Reserve that bounded worst case before doing the work.
                            budget.repeat_comparisons(layout.segment_count())?;
                            let play =
                                layout.play(iteration).ok_or(PlanError::InvalidScopedHold(
                                    "the selected Repeat play is absent or retired",
                                ))?;
                            let child = &self.nodes[current].inspection.id;
                            if &play.child != child && play.gap_child.as_ref() != Some(child) {
                                return Err(PlanError::InvalidScopedHold(
                                    "the selected play does not own this branch",
                                ));
                            }
                        }
                    }
                }
                CompiledKind::Sequence { entries } if in_definition => {
                    let mut offset = None;
                    for entry in entries {
                        budget.sequence_comparison()?;
                        if entry.child == current {
                            offset = Some(entry.start);
                            break;
                        }
                    }
                    start =
                        start
                            .checked_add(offset.ok_or(PlanError::InvalidPlan(
                                "Sequence parent omitted its child",
                            ))?)
                            .ok_or(deadpan_core::TimeError::Overflow)?;
                    definition = parent;
                }
                _ => {}
            }
            if !matches!(&self.nodes[parent].kind, CompiledKind::Sequence { .. }) {
                in_definition = false;
            }
            current = parent;
        }
        if step != 0 {
            return Err(PlanError::InvalidScopedHold(
                "the target has extra Repeat ancestors",
            ));
        }
        let end = start
            .checked_add(duration.frames())
            .ok_or(deadpan_core::TimeError::Overflow)?;
        let half = ExactRatio::new(1, 2)?;
        let left = if start > 0 {
            Some(self.sample_definition_picture(
                definition,
                ExactRatio::integer(start).checked_sub(half)?,
                true,
                &mut budget,
            )?)
        } else {
            None
        };
        let right = if end < self.nodes[definition].inspection.duration.frames() {
            Some(self.sample_definition_picture(
                definition,
                ExactRatio::integer(end).checked_add(half)?,
                true,
                &mut budget,
            )?)
        } else {
            None
        };
        Ok(ScopedHoldBoundaries {
            project_id: self.metadata.project_id.clone(),
            revision_id: self.metadata.revision_id.clone(),
            target: target.clone(),
            definition: self.nodes[definition].inspection.id.clone(),
            range: ExactFrameRange::new(ExactRatio::integer(start), ExactRatio::integer(end))?,
            duration,
            left,
            right,
            lookup: budget.lookup,
        })
    }
}

pub(super) struct PictureBudget {
    limits: BoundaryQueryLimits,
    pub(super) lookup: LookupStats,
}

impl PictureBudget {
    fn new(limits: BoundaryQueryLimits) -> Self {
        Self {
            limits,
            lookup: LookupStats::default(),
        }
    }

    pub(super) fn unlimited() -> Self {
        Self::new(BoundaryQueryLimits {
            max_scopes: usize::MAX,
            max_comparisons: usize::MAX,
        })
    }

    pub(super) fn visit(&mut self) -> Result<(), PlanError> {
        if self.lookup.visited_nodes == self.limits.max_scopes {
            return Err(PlanError::PictureQueryLimit("node visits"));
        }
        self.lookup.visited_nodes += 1;
        Ok(())
    }

    pub(super) fn comparisons_left(&self) -> usize {
        self.limits.max_comparisons
            - self.lookup.sequence_comparisons
            - self.lookup.iteration_run_comparisons
    }

    pub(super) fn sequence_comparison(&mut self) -> Result<(), PlanError> {
        if self.comparisons_left() == 0 {
            return Err(PlanError::PictureQueryLimit("comparisons"));
        }
        self.lookup.sequence_comparisons += 1;
        Ok(())
    }

    pub(super) fn repeat_comparisons(&mut self, count: usize) -> Result<(), PlanError> {
        if count > self.comparisons_left() {
            return Err(PlanError::PictureQueryLimit("comparisons"));
        }
        self.lookup.iteration_run_comparisons += count;
        Ok(())
    }

    pub(super) fn since(&self, before: LookupStats) -> LookupStats {
        LookupStats {
            visited_nodes: self.lookup.visited_nodes - before.visited_nodes,
            sequence_comparisons: self.lookup.sequence_comparisons - before.sequence_comparisons,
            iteration_run_comparisons: self.lookup.iteration_run_comparisons
                - before.iteration_run_comparisons,
        }
    }
}
