//! Checked structural accounting for heap retained by audio query handles.
//!
//! This measures an existing graph. It does not reserve or bound construction's
//! transient allocations. Borrowed RenderPlan storage is deliberately excluded;
//! the owner pinning that plan must account for it separately. Shared Arcs are
//! charged at every owning edge, avoiding an allocating visited-pointer set.

#[path = "audio_metadata_admission.rs"]
pub(crate) mod admission;

use std::{alloc::Layout, mem::size_of, sync::Arc};

use deadpan_core::{
    AssetId, InstancePath, IterationId, NodeId, ProjectId, RepeatInstance, RevisionId,
};

use crate::{
    AudioContent, AudioDefinitionSelector, AudioProcessingQuery, AudioRoutedRoot,
    AudioSourceOccurrence, AudioSourceOccurrences, PlanError,
};

pub(crate) trait OwnedHeap {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError>;
}

pub(crate) struct HeapBudget {
    remaining: usize,
    bytes: usize,
    depth: usize,
}

impl HeapBudget {
    pub(crate) fn spend(&mut self, work: usize) -> Result<(), PlanError> {
        self.remaining = self
            .remaining
            .checked_sub(work)
            .ok_or(PlanError::AudioQueryLimit("retained audio heap traversal"))?;
        Ok(())
    }

    pub(crate) fn add(&mut self, bytes: usize) -> Result<(), PlanError> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .ok_or(PlanError::AudioQueryLimit("retained audio heap bytes"))?;
        Ok(())
    }

    pub(crate) fn visit<T: OwnedHeap + ?Sized>(&mut self, value: &T) -> Result<(), PlanError> {
        self.spend(1)?;
        // The work limit bounds repeated DAG visits; this separate cap also
        // bounds stack use when the caller supplies an enormous work limit.
        if self.depth == deadpan_core::MAX_DOCUMENT_DEPTH {
            return Err(PlanError::AudioQueryLimit("retained audio heap depth"));
        }
        self.depth += 1;
        let result = value.visit_heap(self);
        self.depth -= 1;
        result
    }

    pub(crate) fn storage<T>(&mut self, values: &Vec<T>) -> Result<(), PlanError> {
        // Charge the loop before visiting any element, including scalar-only
        // vectors. Spare capacity consumes bytes but never traversal work.
        self.spend(values.len())?;
        self.add(
            values
                .capacity()
                .checked_mul(size_of::<T>())
                .ok_or(PlanError::AudioQueryLimit("retained audio heap bytes"))?,
        )
    }

    pub(crate) fn core_route(&mut self, route: &deadpan_core::SoundRoute) -> Result<(), PlanError> {
        let (bytes, work) = route
            .allocated_bytes_bound(self.remaining)
            .map_err(|_| PlanError::AudioQueryLimit("retained sound route heap"))?;
        self.spend(work)?;
        self.add(bytes)
    }

    fn arc_allocation(&mut self, value: Layout) -> Result<(), PlanError> {
        // ArcInner holds strong/weak AtomicUsize counters followed by a payload
        // with its own alignment. Include both inter-field and tail padding.
        let (layout, _) = Layout::new::<[usize; 2]>()
            .extend(value)
            .map_err(|_| PlanError::AudioQueryLimit("retained audio heap bytes"))?;
        self.add(layout.pad_to_align().size())
    }

    fn identity(&mut self, text: &str) -> Result<(), PlanError> {
        self.arc_allocation(Layout::for_value(text))
    }
}

pub(crate) fn measure<T: OwnedHeap + ?Sized>(
    value: &T,
    maximum_work: usize,
) -> Result<(usize, usize), PlanError> {
    let mut budget = HeapBudget {
        remaining: maximum_work,
        bytes: 0,
        depth: 0,
    };
    budget.visit(value)?;
    Ok((budget.bytes, maximum_work - budget.remaining))
}

impl<T: OwnedHeap> OwnedHeap for Vec<T> {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
        budget.storage(self)?;
        for value in self {
            budget.visit(value)?;
        }
        Ok(())
    }
}
impl<T: OwnedHeap> OwnedHeap for Option<T> {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
        if let Some(value) = self {
            budget.visit(value)?;
        }
        Ok(())
    }
}
impl<T: OwnedHeap> OwnedHeap for Box<T> {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
        budget.add(size_of::<T>())?;
        budget.visit(self.as_ref())
    }
}
impl<T: OwnedHeap> OwnedHeap for Arc<T> {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
        budget.arc_allocation(Layout::new::<T>())?;
        budget.visit(self.as_ref())
    }
}

macro_rules! identity_heap {
    ($($kind:ty),+ $(,)?) => { $(impl OwnedHeap for $kind {
        fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
            budget.identity(self.as_str())
        }
    })+ };
}
identity_heap!(AssetId, NodeId, ProjectId, RevisionId);

impl OwnedHeap for IterationId {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
        let Self {
            allocation,
            ordinal: _,
        } = self;
        budget.visit(allocation)
    }
}
impl OwnedHeap for RepeatInstance {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
        let Self { node, iteration } = self;
        budget.visit(node)?;
        budget.visit(iteration)
    }
}
impl OwnedHeap for InstancePath {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
        let Self { node, repeats } = self;
        budget.visit(node)?;
        budget.visit(repeats)
    }
}
impl OwnedHeap for AudioDefinitionSelector {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
        match self {
            Self::Node { node } => budget.visit(node),
            Self::RepeatDefault { repeat } | Self::RepeatGap { repeat } => budget.visit(repeat),
        }
    }
}
impl OwnedHeap for deadpan_core::SourceAudio {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
        let Self { asset, span: _ } = self;
        budget.visit(asset)
    }
}
impl OwnedHeap for AudioContent {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
        match self {
            Self::Source {
                source,
                start: _,
                duration: _,
                support: _,
            }
            | Self::RoomTone {
                source,
                duration: _,
            }
            | Self::Reverse {
                source,
                duration: _,
            } => budget.visit(source),
            Self::Silence { reason: _ }
            | Self::Tail {
                maximum: _,
                effect: _,
                input: _,
                duration: _,
            }
            | Self::Tone {
                frequency_hz: _,
                level: _,
                duration: _,
            } => Ok(()),
        }
    }
}

impl OwnedHeap for crate::AudioRetimeStage {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
        let Self {
            node,
            child_start: _,
            child_frames_per_local_frame: _,
            pitch: _,
        } = self;
        budget.visit(node)
    }
}
impl OwnedHeap for crate::AudioStageDescriptor {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
        let Self {
            project_id,
            revision_id,
            definition,
            instance,
            child,
            selection: _,
            duration: _,
            rate: _,
            pitch: _,
            bypass_binding: _,
        } = self;
        budget.visit(project_id)?;
        budget.visit(revision_id)?;
        budget.visit(definition)?;
        budget.visit(instance)?;
        budget.visit(child)
    }
}
impl OwnedHeap for crate::AudioSignalContent<'_> {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
        match self {
            Self::Leaf(content) => budget.visit(content),
            Self::Stage(stage) => budget.visit(stage),
            Self::Bound(bound) => budget.visit(bound),
            Self::ProjectedStage(projection) => budget.visit(projection),
        }
    }
}
impl OwnedHeap for crate::AudioProcessingSpan<'_> {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
        let Self {
            definition,
            samples: _,
            allocated_samples: _,
            project_extent: _,
            instance,
            gap_after,
            transform: _,
            grid: _,
            sampling: _,
            retimes,
            content,
        } = self;
        budget.visit(definition)?;
        budget.visit(instance)?;
        budget.visit(gap_after)?;
        budget.visit(retimes)?;
        budget.visit(content)
    }
}
impl OwnedHeap for AudioProcessingQuery<'_> {
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

impl OwnedHeap for deadpan_core::AudioClockRoot {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
        match self {
            Self::ProjectRootRoundEven => Ok(()),
            Self::PreserveInputPointCeil { stage } => budget.visit(stage),
            Self::DefinitionPointCeil { root } => budget.visit(root),
            Self::GapDefinitionPointCeil { repeat } => budget.visit(repeat),
        }
    }
}
impl OwnedHeap for deadpan_core::ResolvedAudioPlacement {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
        let Self {
            clock,
            grid_rule: _,
            grid_origin: _,
            frames_per_sample: _,
            origin: _,
            frames_per_local_frame: _,
            local_duration: _,
            local_support: _,
            instance,
            gap_after,
            birth: _,
            gap_birth: _,
            work: _,
        } = self;
        budget.visit(clock)?;
        budget.visit(instance)?;
        budget.visit(gap_after)
    }
}
impl OwnedHeap for deadpan_core::ResolvedAudioBinding {
    fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
        let Self {
            lattice,
            resume,
            work: _,
        } = self;
        budget.visit(lattice)?;
        budget.visit(resume)
    }
}
impl OwnedHeap for deadpan_core::ResolvedAudioResume {
    fn visit_heap(&self, _budget: &mut HeapBudget) -> Result<(), PlanError> {
        let Self {
            local_boundary: _,
            reference_local_delta: _,
        } = self;
        Ok(())
    }
}

macro_rules! public_measure {
    ($($kind:ident),+ $(,)?) => { $(impl $kind<'_> {
        /// Checked upper bound of retained heap bytes and charged traversal work,
        /// returned as `(bytes, work)`. Counts actual Vec capacities and owning
        /// Box/Arc payloads, including identifier text. Shared Arc allocations
        /// may be counted repeatedly. Excludes this inline handle and borrowed
        /// RenderPlan storage. Does not bound transient construction allocation.
        pub fn allocated_bytes_bound(&self, maximum_work: usize) -> Result<(usize, usize), PlanError> {
            measure(self, maximum_work)
        }
    })+ };
}
public_measure!(
    AudioRoutedRoot,
    AudioSourceOccurrence,
    AudioSourceOccurrences,
    AudioProcessingQuery
);

#[cfg(test)]
mod tests;
