//! Complete owning edges of flattened, policy and fade query products.

use deadpan_core::AudioSample;

use super::{AudioStorageMeasurement, HeapBudget, OwnedHeap, measure_storage};
use crate::{
    AudioBoundaries, AudioBoundaryOrigin, AudioFadeEdge, AudioFadeQuery, AudioFadeSpan,
    AudioPolicyQuery, AudioProcessingQuery, AudioQuery, AudioSpan, PlanError, SignalSample,
};

impl OwnedHeap for AudioBoundaryOrigin {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
        let Self {
            instance,
            gap_after,
            kind: _,
            placement_support: _,
            policy: _,
        } = self;
        budget.visit(instance)?;
        budget.visit(gap_after)
    }
}

impl OwnedHeap for AudioBoundaries {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
        let Self { start, end } = self;
        budget.visit(start)?;
        budget.visit(end)
    }
}

impl OwnedHeap for AudioSpan {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
        let Self {
            definition,
            samples: _,
            allocated_samples: _,
            project_extent: _,
            envelope_extent: _,
            envelope_samples: _,
            envelope: _,
            boundaries,
            instance,
            gap_after,
            transform: _,
            grid: _,
            sampling: _,
            retimes,
            content,
        } = self;
        budget.visit(definition)?;
        budget.visit(boundaries)?;
        budget.visit(instance)?;
        budget.visit(gap_after)?;
        budget.visit(retimes)?;
        budget.visit(content)
    }
}

impl OwnedHeap for AudioQuery {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
        let Self {
            project_id,
            revision_id,
            samples: _,
            spans,
            lookup: _,
            work: _,
        } = self;
        budget.visit(project_id)?;
        budget.visit(revision_id)?;
        budget.visit(spans)
    }
}

impl OwnedHeap for AudioFadeEdge {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
        let Self {
            at: _,
            length: _,
            distance_at_start: _,
            origins,
            editorial: _,
        } = self;
        budget.visit(origins)
    }
}

impl OwnedHeap for AudioFadeSpan {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
        let Self {
            samples: _,
            start,
            end,
        } = self;
        budget.visit(start)?;
        budget.visit(end)
    }
}

impl OwnedHeap for AudioFadeQuery {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
        let Self {
            spans,
            lookup: _,
            work: _,
        } = self;
        budget.visit(spans)
    }
}

// Only the two scalar sample labels used by policy queries have this closure.
// An arbitrary S could own heap, so its inline Range storage is insufficient.
macro_rules! scalar_policy_heap {
    ($($sample:ty),+ $(,)?) => { $(
        impl OwnedHeap for AudioPolicyQuery<$sample> {
            fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
                let Self { suppressed, contents, lookup: _, work: _ } = self;
                budget.storage(suppressed)?;
                budget.visit(contents)
            }
        }

        impl AudioPolicyQuery<$sample> {
            /// Inspect retained heap storage without reserving it. The result
            /// retains actual traversal work even when byte measurement fails.
            /// Excludes this inline query and borrowed immutable plan storage.
            pub fn measure_allocated_storage(&self, maximum_work: usize) -> AudioStorageMeasurement {
                measure_storage(self, maximum_work)
            }
        }
    )+ };
}
scalar_policy_heap!(AudioSample, SignalSample);

macro_rules! query_measure {
    ($($kind:ty),+ $(,)?) => { $(impl $kind {
        /// Inspect retained heap storage without reserving it. Counts actual
        /// Vec capacities and every owning Box/Arc, including identity text;
        /// shared Arc storage may be counted at each owning edge. Excludes the
        /// inline query and borrowed plan. Does not bound construction peak.
        /// The result retains actual traversal work even on a failed inspection.
        pub fn measure_allocated_storage(&self, maximum_work: usize) -> AudioStorageMeasurement {
            measure_storage(self, maximum_work)
        }
    })+ };
}
query_measure!(AudioQuery, AudioFadeQuery, AudioProcessingQuery<'_>);
