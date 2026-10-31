//! Exact provenance of the intersections that form a root audio allocation.
use std::ops::Range;

pub use deadpan_core::AudioBoundaryKind;
use deadpan_core::{
    AudioEdgePolicies, AudioEdgePolicy, ExactRatio, InstancePath, IterationId, NodeId,
    RepeatInstance,
};
use serde::Serialize;

use super::audio::Budget;
use super::audio_owners::admission::{
    Budget as QueryBudget, Memory, Owned, Result as QueryResult, Unmetered, outer,
};
use crate::{AudioSoundContextAdmission, PlanError};

/// One constraint at an exact project boundary. Ancestor owners retain their
/// own occurrence path, excluding any descendant Repeat plays traversed later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioBoundaryOrigin {
    pub instance: InstancePath,
    pub gap_after: Option<IterationId>,
    pub kind: AudioBoundaryKind,
    /// An explicit evaluation-support edge, not an authored node boundary.
    /// Its NodeStart/NodeEnd kind gives direction and its policy is Automatic.
    /// Real exactly coincident boundaries retain their independent policies.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub placement_support: bool,
    /// The owning node's choice for this original constraint in this revision.
    pub policy: AudioEdgePolicy,
}

/// Owners of `AudioSpan::envelope_extent` and its rounded `envelope_samples`.
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
    pub placement_support: bool,
}

impl BoundaryOwner<'_> {
    pub(super) fn capture(
        self,
        kind: AudioBoundaryKind,
        budget: &mut Budget,
    ) -> Result<AudioBoundaryOrigin, PlanError> {
        let mut admission = Unmetered;
        outer::<_, Unmetered>(
            self.capture_admitted(kind, &mut QueryBudget::new(budget, &mut admission)),
        )
        .map(Owned::unmetered)
    }

    pub(in crate::plan) fn capture_admitted<A: AudioSoundContextAdmission>(
        self,
        kind: AudioBoundaryKind,
        budget: &mut QueryBudget<'_, A>,
    ) -> QueryResult<Owned<AudioBoundaryOrigin, A::Issuer, A::Lease>, A> {
        // This canonical debit already prepays the path copy. Helpers add
        // only actual metered layout/identity inspections before allocation.
        budget.canonical(self.repeats.len() + 1)?;
        let mut memory = budget.memory();
        let mut capacity = budget.memory();
        let node = budget.node(&mut memory, self.node)?;
        let repeats = budget.repeats(&mut memory, &mut capacity, self.repeats)?;
        let gap_after = self
            .gap_after
            .map(|gap| budget.iteration(&mut memory, gap))
            .transpose()?;
        let value = AudioBoundaryOrigin {
            instance: InstancePath { node, repeats },
            gap_after,
            kind,
            placement_support: self.placement_support,
            policy: self.policies.get(kind),
        };
        budget.check_memory(&memory)?;
        budget.check_memory(&capacity)?;
        budget.merge_checked(&mut memory, &mut capacity)?;
        Ok(Owned { value, memory })
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

    #[cfg(test)]
    pub fn clip_start(
        &mut self,
        value: ExactRatio,
        owner: BoundaryOwner<'_>,
        kind: AudioBoundaryKind,
        budget: &mut Budget,
    ) -> Result<(), PlanError> {
        let mut admission = Unmetered;
        let range = self.range.clone();
        let mut extent = AdmittedExtent::from_value(std::mem::replace(self, Self::new(range)), ());
        let result = extent.clip_start(
            value,
            owner,
            kind,
            &mut QueryBudget::new(budget, &mut admission),
        );
        // Preserve the actual caller's state even if the shared body fails.
        *self = extent.into_unmetered();
        outer::<_, Unmetered>(result)
    }

    #[cfg(test)]
    pub fn clip_end(
        &mut self,
        value: ExactRatio,
        owner: BoundaryOwner<'_>,
        kind: AudioBoundaryKind,
        budget: &mut Budget,
    ) -> Result<(), PlanError> {
        let mut admission = Unmetered;
        let range = self.range.clone();
        let mut extent = AdmittedExtent::from_value(std::mem::replace(self, Self::new(range)), ());
        let result = extent.clip_end(
            value,
            owner,
            kind,
            &mut QueryBudget::new(budget, &mut admission),
        );
        *self = extent.into_unmetered();
        outer::<_, Unmetered>(result)
    }
}

struct ExtentMemory<I, L> {
    origins: Memory<I, L>,
    start: Memory<I, L>,
    end: Memory<I, L>,
}

/// Actual boundary storage and its independent payload/capacity charges.
/// Clearing origins retains vector capacity. Owning-edge charges may remain
/// conservatively held until this extent drops, including cleared origins.
pub(in crate::plan) struct AdmittedExtent<I, L> {
    value: AudioExtent,
    memory: ExtentMemory<I, L>,
}

impl<I: Copy + Eq, L> AdmittedExtent<I, L> {
    pub(in crate::plan) fn new(range: Range<ExactRatio>, issuer: I) -> Self {
        Self::from_value(AudioExtent::new(range), issuer)
    }

    fn from_value(value: AudioExtent, issuer: I) -> Self {
        Self {
            value,
            memory: ExtentMemory {
                origins: Memory::empty(issuer),
                start: Memory::empty(issuer),
                end: Memory::empty(issuer),
            },
        }
    }

    pub(in crate::plan) fn clip_start<A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        &mut self,
        value: ExactRatio,
        owner: BoundaryOwner<'_>,
        kind: AudioBoundaryKind,
        budget: &mut QueryBudget<'_, A>,
    ) -> QueryResult<(), A> {
        let comparison = self
            .value
            .range
            .start
            .checked_sub(value)?
            .compare_integer(0);
        if comparison.is_le() {
            let mut origin = owner.capture_admitted(kind, budget)?;
            if comparison.is_lt() {
                self.value.boundaries.start.clear();
                self.value.range.start = value;
            }
            let needed = self.value.boundaries.start.len() + 1;
            budget.grow(
                &mut self.value.boundaries.start,
                &mut self.memory.start,
                needed,
            )?;
            budget.check_memory(&self.memory.origins)?;
            budget.check_memory(&origin.memory)?;
            budget.merge_checked(&mut self.memory.origins, &mut origin.memory)?;
            self.value.boundaries.start.push(origin.value);
        }
        Ok(())
    }

    pub(in crate::plan) fn clip_end<A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        &mut self,
        value: ExactRatio,
        owner: BoundaryOwner<'_>,
        kind: AudioBoundaryKind,
        budget: &mut QueryBudget<'_, A>,
    ) -> QueryResult<(), A> {
        let comparison = self.value.range.end.checked_sub(value)?.compare_integer(0);
        if comparison.is_ge() {
            let mut origin = owner.capture_admitted(kind, budget)?;
            if comparison.is_gt() {
                self.value.boundaries.end.clear();
                self.value.range.end = value;
            }
            let needed = self.value.boundaries.end.len() + 1;
            budget.grow(&mut self.value.boundaries.end, &mut self.memory.end, needed)?;
            budget.check_memory(&self.memory.origins)?;
            budget.check_memory(&origin.memory)?;
            budget.merge_checked(&mut self.memory.origins, &mut origin.memory)?;
            self.value.boundaries.end.push(origin.value);
        }
        Ok(())
    }

    pub(in crate::plan) fn finish<A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        mut self,
        budget: &mut QueryBudget<'_, A>,
    ) -> QueryResult<Owned<AudioExtent, I, L>, A> {
        budget.check_memory(&self.memory.origins)?;
        budget.check_memory(&self.memory.start)?;
        budget.check_memory(&self.memory.end)?;
        budget.merge_checked(&mut self.memory.origins, &mut self.memory.start)?;
        budget.merge_checked(&mut self.memory.origins, &mut self.memory.end)?;
        Ok(Owned {
            value: self.value,
            memory: self.memory.origins,
        })
    }
}

#[cfg(test)]
impl AdmittedExtent<(), ()> {
    pub(in crate::plan) fn into_unmetered(self) -> AudioExtent {
        self.value
    }
}

impl<I, L> std::ops::Deref for AdmittedExtent<I, L> {
    type Target = AudioExtent;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<I, L> std::ops::DerefMut for AdmittedExtent<I, L> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

#[cfg(test)]
#[path = "audio_capture_constructor_tests.rs"]
mod tests;
