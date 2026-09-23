//! Exact provenance of the intersections that form a root audio allocation.
use std::ops::Range;

use deadpan_core::{ExactRatio, InstancePath, IterationId, NodeId, RepeatInstance};
use serde::Serialize;

use super::audio::Budget;
use crate::PlanError;

/// Which original constraint meets an allocated edge. This names the side of
/// the constraint, which can differ from the side of a silent output span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioBoundaryKind {
    NodeStart,
    NodeEnd,
    SourcePlacementStart,
    SourcePlacementEnd,
    RepeatGapStart,
    RepeatGapEnd,
}

/// One constraint at an exact project boundary. Ancestor owners retain their
/// own occurrence path, excluding any descendant Repeat plays traversed later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioBoundaryOrigin {
    pub instance: InstancePath,
    pub gap_after: Option<IterationId>,
    pub kind: AudioBoundaryKind,
}

/// Owners of `AudioSpan::project_extent` and its rounded `allocated_samples`.
/// Coincident exact constraints are retained in outer-to-inner descent order;
/// equal rounded samples alone never make different constraints coincident.
/// These are facts about one immutable plan, not a fade or precedence policy.
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
