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
    #[serde(skip)]
    pub(super) hold_provider: Option<DefinitionHoldWitness>,
}

/// Private evidence from the canonical walker. Public sample fields remain
/// inspectable, but changing them cannot grant a fallback-provider lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DefinitionHoldWitness {
    identity: Arc<()>,
    node: usize,
    definition: usize,
    position: ExactRatio,
    instance: InstancePath,
    local_position: ExactRatio,
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
    /// Work for this target and both samples. In a batch this is the target's
    /// delta against one shared budget, so summing results gives the total.
    /// The single-query path charges its full ancestry and compact run scans.
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
    /// Enumerate each authored Hold once, in node-ID order, including dormant
    /// defaults and owned final gaps. Every Play step owns its branch; shared
    /// defaults use Default even when that template has no active occurrence.
    /// The compiled index is linear in authored nodes/edges. This query visits
    /// only Holds and their linked Repeat scopes, never rendered occurrences.
    pub fn authored_hold_targets(
        &self,
        limits: BoundaryQueryLimits,
    ) -> Result<Vec<ScopedNodeTarget>, PlanError> {
        let index = self
            .definition_index
            .as_ref()
            .ok_or(PlanError::AudioOnlyContext)?;
        if index.holds.len() > limits.max_scopes {
            return Err(PlanError::PictureQueryLimit("node visits"));
        }
        let mut budget = PictureBudget::new(limits);
        index
            .holds
            .iter()
            .map(|&node| index.target(node, &self.nodes, &mut budget))
            .collect()
    }

    /// Resolve canonical authored Hold targets with one aggregate work budget.
    /// Every Play step must already own its override or gap branch, including
    /// outer ancestors. Shared defaults must be addressed with Default. This
    /// ownership contract supports automatic provider edits without isolation;
    /// use scoped_hold_boundaries for read-only effective Play conditioning.
    /// Input order and duplicates are preserved; every entry consumes budget.
    /// No partial result escapes on invalid ownership or budget exhaustion.
    ///
    /// Compilation caches parent ownership and Sequence offsets once. Each
    /// query validates only its explicit Repeat steps, then uses the ordinary
    /// indexed picture walker for both neighboring centers. Map lookup and
    /// immutable-plan compilation are outside BoundaryQueryLimits, as for the
    /// single query. Callers batching many Holds must supply aggregate limits.
    pub fn scoped_hold_boundaries_batch(
        &self,
        targets: &[ScopedNodeTarget],
        limits: BoundaryQueryLimits,
    ) -> Result<Vec<ScopedHoldBoundaries>, PlanError> {
        let index = self
            .definition_index
            .as_ref()
            .ok_or(PlanError::AudioOnlyContext)?;
        if targets.len() > limits.max_scopes || targets.len() > deadpan_core::MAX_DOCUMENT_NODES {
            return Err(PlanError::PictureQueryLimit("node visits"));
        }
        let mut budget = PictureBudget::new(limits);
        let mut results = Vec::with_capacity(targets.len());
        for target in targets {
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
            let before = budget.lookup;
            let address = index.validate(hold, target, &mut budget)?;
            results.push(self.hold_boundaries_in(
                target,
                hold,
                address.definition,
                address.start,
                before,
                &mut budget,
            )?);
        }
        Ok(results)
    }

    /// The captured deterministic fallback for a genuine Generated Hold sample
    /// from this exact compiled snapshot (or its clone). Cutaways, other
    /// providers and implicit Repeat gaps return None. Public identity/provider
    /// edits to a sample are rejected; this is not an arbitrary-node evaluator.
    /// The result still needs the host's admitted measured source-frame lookup.
    pub fn definition_hold_fallback_picture(
        &self,
        sample: &DefinitionPictureSample,
    ) -> Result<Option<Picture>, PlanError> {
        let index = self
            .definition_index
            .as_ref()
            .ok_or(PlanError::AudioOnlyContext)?;
        if sample.project_id != self.metadata.project_id
            || sample.revision_id != self.metadata.revision_id
        {
            return Err(PlanError::InvalidPlan(
                "definition sample belongs to another revision",
            ));
        }
        let Some(proof) = &sample.hold_provider else {
            return Ok(None);
        };
        if !Arc::ptr_eq(&proof.identity, &index.identity)
            || sample.definition != self.nodes[proof.definition].inspection.id
            || sample.position != proof.position
            || sample.instance != proof.instance
            || sample.gap_after.is_some()
            || sample.local_position != proof.local_position
        {
            return Err(PlanError::InvalidPlan(
                "definition sample provider evidence does not match",
            ));
        }
        let CompiledKind::Hold { video, .. } = &self.nodes[proof.node].kind else {
            return Err(PlanError::InvalidPlan(
                "definition sample provider is not a Hold",
            ));
        };
        if sample.picture != video.picture(proof.local_position)? {
            return Err(PlanError::InvalidPlan(
                "definition sample picture does not match its provider",
            ));
        }
        index
            .fallbacks
            .get(&proof.node)
            .map(|fallback| fallback.picture(proof.local_position))
            .transpose()
    }

    pub(super) fn definition_hold_witness(
        &self,
        definition: usize,
        position: ExactRatio,
        sample: &PictureWalk,
    ) -> Option<DefinitionHoldWitness> {
        let index = self.definition_index.as_ref()?;
        if sample.gap_after.is_some()
            || !matches!(
                sample.picture,
                Picture::Accepted {
                    generated: Some(_),
                    ..
                }
            )
        {
            return None;
        }
        let node = *self.by_id.get(&sample.instance.node)?;
        index.fallbacks.get(&node)?;
        Some(DefinitionHoldWitness {
            identity: Arc::clone(&index.identity),
            node,
            definition,
            position,
            instance: sample.instance.clone(),
            local_position: sample.local_position,
        })
    }

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
    /// Play resolves its effective child, including a shared default, or its
    /// explicitly owned gap. This is a read-only conditioning address, not
    /// proof that the provider can be changed without occurrence isolation.
    /// The batch query requires canonical, already-owned authored addresses.
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
                                    "the selected play does not select this branch",
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
        self.hold_boundaries_in(
            target,
            hold,
            definition,
            start,
            LookupStats::default(),
            &mut budget,
        )
    }

    fn hold_boundaries_in(
        &self,
        target: &ScopedNodeTarget,
        hold: usize,
        definition: usize,
        start: i64,
        before: LookupStats,
        budget: &mut PictureBudget,
    ) -> Result<ScopedHoldBoundaries, PlanError> {
        let duration = self.nodes[hold].inspection.duration;
        let end = start
            .checked_add(duration.frames())
            .ok_or(deadpan_core::TimeError::Overflow)?;
        let half = ExactRatio::new(1, 2)?;
        let left = if start > 0 {
            Some(self.sample_definition_picture(
                definition,
                ExactRatio::integer(start).checked_sub(half)?,
                true,
                budget,
            )?)
        } else {
            None
        };
        let right = if end < self.nodes[definition].inspection.duration.frames() {
            Some(self.sample_definition_picture(
                definition,
                ExactRatio::integer(end).checked_add(half)?,
                true,
                budget,
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
            lookup: budget.since(before),
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
