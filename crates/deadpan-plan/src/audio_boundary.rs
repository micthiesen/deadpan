//! Exact provenance of the intersections that form a root audio allocation.
use std::ops::Range;

pub use deadpan_core::AudioBoundaryKind;
use deadpan_core::{
    AudioEdgePolicies, AudioEdgePolicy, ExactRatio, InstancePath, IterationId, NodeId,
    RepeatInstance,
};
use serde::Serialize;

use super::audio::Budget;
use crate::PlanError;

/// One constraint at an exact project boundary. Ancestor owners retain their
/// own occurrence path, excluding any descendant Repeat plays traversed later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioBoundaryOrigin {
    pub instance: InstancePath,
    pub gap_after: Option<IterationId>,
    pub kind: AudioBoundaryKind,
    /// The owning node's choice for this original constraint in this revision.
    pub policy: AudioEdgePolicy,
}

/// Owners of `AudioSpan::project_extent` and its rounded `allocated_samples`.
/// Coincident exact constraints are retained in outer-to-inner descent order;
/// equal rounded samples alone never make different constraints coincident.
/// Any explicit Hard among coincident origins suppresses this edge's fade.
/// Interior boundaries do not inherit policies from noncoincident ancestors.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct AudioBoundaries {
    pub start: Vec<AudioBoundaryOrigin>,
    pub end: Vec<AudioBoundaryOrigin>,
}

#[derive(Clone, Copy)]
pub(super) struct BoundaryOwner<'a> {
    pub node: &'a NodeId,
    pub repeats: &'a [RepeatInstance],
    pub gap_after: Option<&'a IterationId>,
    pub policies: AudioEdgePolicies,
}

impl BoundaryOwner<'_> {
    fn capture(
        self,
        kind: AudioBoundaryKind,
        budget: &mut Budget,
    ) -> Result<AudioBoundaryOrigin, PlanError> {
        // Charge before cloning. Coincident ancestor paths can otherwise add
        // quadratic storage to a deeply nested query's returned metadata.
        budget.spend(self.repeats.len() + 1)?;
        Ok(AudioBoundaryOrigin {
            instance: InstancePath {
                node: self.node.clone(),
                repeats: self.repeats.to_vec(),
            },
            gap_after: self.gap_after.cloned(),
            kind,
            policy: self.policies.get(kind),
        })
    }
}

pub(super) struct AudioExtent {
    pub range: Range<ExactRatio>,
    pub boundaries: AudioBoundaries,
}

impl AudioExtent {
    pub fn new(range: Range<ExactRatio>) -> Self {
        Self {
            range,
            boundaries: AudioBoundaries::default(),
        }
    }

    pub fn intersect(
        &mut self,
        range: Range<ExactRatio>,
        owner: BoundaryOwner<'_>,
        kinds: (AudioBoundaryKind, AudioBoundaryKind),
        budget: &mut Budget,
    ) -> Result<(), PlanError> {
        self.clip_start(range.start, owner, kinds.0, budget)?;
        self.clip_end(range.end, owner, kinds.1, budget)
    }

    pub fn clip_start(
        &mut self,
        value: ExactRatio,
        owner: BoundaryOwner<'_>,
        kind: AudioBoundaryKind,
        budget: &mut Budget,
    ) -> Result<(), PlanError> {
        let comparison = self.range.start.checked_sub(value)?.compare_integer(0);
        if comparison.is_le() {
            let origin = owner.capture(kind, budget)?;
            if comparison.is_lt() {
                self.boundaries.start.clear();
                self.range.start = value;
            }
            self.boundaries.start.push(origin);
        }
        Ok(())
    }

    pub fn clip_end(
        &mut self,
        value: ExactRatio,
        owner: BoundaryOwner<'_>,
        kind: AudioBoundaryKind,
        budget: &mut Budget,
    ) -> Result<(), PlanError> {
        let comparison = self.range.end.checked_sub(value)?.compare_integer(0);
        if comparison.is_ge() {
            let origin = owner.capture(kind, budget)?;
            if comparison.is_gt() {
                self.boundaries.end.clear();
                self.range.end = value;
            }
            self.boundaries.end.push(origin);
        }
        Ok(())
    }
}
