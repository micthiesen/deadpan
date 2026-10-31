//! Actual owner-query accounting. Values precede their independent RAII charge.
use super::*;
use crate::audio_allocated::admission::{array, identity};
use crate::{AudioSoundContextAdmission, PlanError};
use deadpan_core::{
    AudioBindingAdmission, AudioBindingBuildError, AudioBindingForwardedWork, NodeId, RevisionId,
};
use std::alloc::Layout;

#[derive(Debug)]
pub(in crate::plan) enum Error<E> {
    Plan(PlanError),
    Policy(E),
}
impl<E> From<PlanError> for Error<E> {
    fn from(value: PlanError) -> Self {
        Self::Plan(value)
    }
}
impl<E> From<TimeError> for Error<E> {
    fn from(value: TimeError) -> Self {
        Self::Plan(value.into())
    }
}
impl<E> From<deadpan_core::DocumentError> for Error<E> {
    fn from(value: deadpan_core::DocumentError) -> Self {
        Self::Plan(value.into())
    }
}
pub(in crate::plan) type Result<T, A> =
    std::result::Result<T, Error<<A as AudioSoundContextAdmission>::Error>>;
pub(in crate::plan) fn outer<T, A: AudioSoundContextAdmission>(
    value: Result<T, A>,
) -> std::result::Result<T, A::Error> {
    value.map_err(|e| match e {
        Error::Plan(e) => e.into(),
        Error::Policy(e) => e,
    })
}
pub(in crate::plan) fn core<T, A: AudioSoundContextAdmission>(
    value: std::result::Result<T, AudioBindingBuildError<A::Error>>,
) -> Result<T, A> {
    value.map_err(|e| match e {
        AudioBindingBuildError::Document(e) => Error::Plan(e.into()),
        AudioBindingBuildError::Policy(e) => Error::Policy(e),
    })
}
pub(in crate::plan) struct Unmetered;
impl AudioSoundContextAdmission for Unmetered {
    const METERED: bool = false;
    type Error = PlanError;
    type Issuer = ();
    type Lease = ();
    fn issuer(&self) {}
    fn spend(&mut self, _: usize) -> std::result::Result<(), PlanError> {
        Ok(())
    }
    fn reserve(&mut self, _: Layout) -> std::result::Result<(), PlanError> {
        Ok(())
    }
    fn merge(&mut self, _: &mut (), _: &mut ()) {}
}
pub(in crate::plan) struct CoreAdmission<'a, A> {
    pub(in crate::plan) outer: &'a mut A,
}
impl<A: AudioSoundContextAdmission> AudioBindingAdmission for CoreAdmission<'_, A> {
    const METERED: bool = A::METERED;
    type Error = A::Error;
    type Issuer = A::Issuer;
    type Lease = A::Lease;
    fn issuer(&self) -> Self::Issuer {
        self.outer.issuer()
    }
    fn issuer_mismatch(&self) -> Self::Error {
        PlanError::AudioMetadataIssuerMismatch.into()
    }
    fn spend(&mut self, n: usize) -> std::result::Result<(), Self::Error> {
        self.outer.spend(n)
    }
    fn reserve(&mut self, layout: Layout) -> std::result::Result<Self::Lease, Self::Error> {
        self.outer.reserve(layout)
    }
    fn merge(&mut self, a: &mut Self::Lease, b: &mut Self::Lease) {
        self.outer.merge(a, b)
    }
}
pub(in crate::plan) struct Memory<I, L> {
    pub(in crate::plan) issuer: I,
    pub(in crate::plan) lease: Option<L>,
}
impl<I: Copy, L> Memory<I, L> {
    pub(in crate::plan) fn empty(issuer: I) -> Self {
        Self {
            issuer,
            lease: None,
        }
    }
}
pub(in crate::plan) struct Owned<T, I, L> {
    pub(in crate::plan) value: T,
    pub(in crate::plan) memory: Memory<I, L>,
}
impl<T, I, L> std::ops::Deref for Owned<T, I, L> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}
impl<T, I, L> std::ops::DerefMut for Owned<T, I, L> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}
impl<T, I: Copy + Eq, L> Owned<T, I, L> {
    pub(in crate::plan) fn try_map<U, A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        mut self,
        budget: &mut Budget<'_, A>,
        map: impl FnOnce(T, &mut Memory<I, L>, &mut Budget<'_, A>) -> Result<U, A>,
    ) -> Result<Owned<U, I, L>, A> {
        budget.check_memory(&self.memory)?;
        let value = map(self.value, &mut self.memory, budget)?;
        let result = Owned {
            value,
            memory: self.memory,
        };
        budget.check_memory(&result.memory)?;
        Ok(result)
    }
}
impl<T> Owned<T, (), ()> {
    pub(in crate::plan) fn unmetered(self) -> T {
        self.value
    }
}
#[derive(Clone, Copy)]
pub(in crate::plan) struct QueryCheckpoint {
    remaining: usize,
    canonical: usize,
    additional: usize,
    lookup: LookupStats,
}
impl QueryCheckpoint {
    pub(in crate::plan) fn remaining(self) -> usize {
        self.remaining
    }
}

pub(in crate::plan) struct QueryUsage {
    pub(in crate::plan) work: usize,
    pub(in crate::plan) canonical: usize,
    pub(in crate::plan) additional: usize,
    pub(in crate::plan) lookup: LookupStats,
}

pub(in crate::plan) struct Budget<'a, A: AudioSoundContextAdmission> {
    local: &'a mut super::Budget,
    pub(in crate::plan) admission: &'a mut A,
    pub(in crate::plan) issuer: A::Issuer,
    pub(in crate::plan) canonical_used: usize,
    pub(in crate::plan) additional_used: usize,
}
impl<A: AudioSoundContextAdmission> std::ops::Deref for Budget<'_, A> {
    type Target = super::Budget;
    fn deref(&self) -> &Self::Target {
        self.local
    }
}
impl<A: AudioSoundContextAdmission> std::ops::DerefMut for Budget<'_, A> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.local
    }
}
impl<'a, A: AudioSoundContextAdmission> Budget<'a, A> {
    pub(in crate::plan) fn new(local: &'a mut super::Budget, admission: &'a mut A) -> Self {
        Self {
            local,
            issuer: admission.issuer(),
            admission,
            canonical_used: 0,
            additional_used: 0,
        }
    }
    pub(in crate::plan) fn check(&self) -> Result<(), A> {
        if self.issuer != self.admission.issuer() {
            return Err(PlanError::AudioMetadataIssuerMismatch.into());
        }
        Ok(())
    }
    pub(in crate::plan) fn checkpoint(&self) -> QueryCheckpoint {
        QueryCheckpoint {
            remaining: self.remaining,
            canonical: self.canonical_used,
            additional: self.additional_used,
            lookup: self.lookup,
        }
    }
    /// Describe a completed child without forwarding or folding its work again.
    pub(in crate::plan) fn usage_since(&self, start: QueryCheckpoint) -> Result<QueryUsage, A> {
        self.check()?;
        let mismatch = || PlanError::InvalidPlan("audio child query work provenance mismatch");
        let canonical = self
            .canonical_used
            .checked_sub(start.canonical)
            .ok_or_else(mismatch)?;
        let additional = self
            .additional_used
            .checked_sub(start.additional)
            .ok_or_else(mismatch)?;
        let work = canonical
            .checked_add(additional)
            .ok_or(TimeError::Overflow)?;
        if start.remaining.checked_sub(self.remaining) != Some(work) {
            return Err(mismatch().into());
        }
        let lookup = LookupStats {
            visited_nodes: self
                .lookup
                .visited_nodes
                .checked_sub(start.lookup.visited_nodes)
                .ok_or_else(mismatch)?,
            sequence_comparisons: self
                .lookup
                .sequence_comparisons
                .checked_sub(start.lookup.sequence_comparisons)
                .ok_or_else(mismatch)?,
            iteration_run_comparisons: self
                .lookup
                .iteration_run_comparisons
                .checked_sub(start.lookup.iteration_run_comparisons)
                .ok_or_else(mismatch)?,
        };
        Ok(QueryUsage {
            work,
            canonical,
            additional,
            lookup,
        })
    }
    pub(in crate::plan) fn memory(&self) -> Memory<A::Issuer, A::Lease> {
        Memory::empty(self.issuer)
    }
    pub(in crate::plan) fn canonical(&mut self, n: usize) -> Result<(), A> {
        self.check()?;
        self.local.spend(n)?;
        self.canonical_used = self
            .canonical_used
            .checked_add(n)
            .ok_or(TimeError::Overflow)?;
        if A::METERED {
            self.admission.spend(n).map_err(Error::Policy)?;
        }
        self.check()
    }
    pub(in crate::plan) fn additional(&mut self, n: usize) -> Result<(), A> {
        if !A::METERED {
            return Ok(());
        }
        self.check()?;
        self.local.spend(n)?;
        self.additional_used = self
            .additional_used
            .checked_add(n)
            .ok_or(TimeError::Overflow)?;
        self.admission.spend(n).map_err(Error::Policy)?;
        self.check()
    }
    pub(in crate::plan) fn account_core(
        &mut self,
        work: AudioBindingForwardedWork<A::Issuer>,
    ) -> Result<(), A> {
        self.check()?;
        if work.issuer() != self.issuer || work.was_forwarded() != A::METERED {
            return Err(PlanError::AudioMetadataIssuerMismatch.into());
        }
        self.local.spend(work.units())?;
        self.canonical_used = self
            .canonical_used
            .checked_add(work.canonical_units())
            .ok_or(TimeError::Overflow)?;
        self.additional_used = self
            .additional_used
            .checked_add(work.additional_units())
            .ok_or(TimeError::Overflow)?;
        Ok(())
    }
    pub(in crate::plan) fn reserve(
        &mut self,
        memory: &mut Memory<A::Issuer, A::Lease>,
        layout: Layout,
    ) -> Result<(), A> {
        self.check_memory(memory)?;
        if !A::METERED || layout.size() == 0 {
            return Ok(());
        }
        self.additional(1)?;
        let mut issued = self.admission.reserve(layout).map_err(Error::Policy)?;
        self.check()?;
        if let Some(target) = memory.lease.as_mut() {
            self.admission.merge(target, &mut issued);
            self.check()?;
        } else {
            memory.lease = Some(issued);
        }
        Ok(())
    }
    pub(in crate::plan) fn check_memory(
        &self,
        memory: &Memory<A::Issuer, A::Lease>,
    ) -> Result<(), A> {
        self.check()?;
        if self.issuer != memory.issuer {
            return Err(PlanError::AudioMetadataIssuerMismatch.into());
        }
        Ok(())
    }
    /// Call only after checking both issuers and completing all fallible work.
    pub(in crate::plan) fn merge_checked(
        &mut self,
        a: &mut Memory<A::Issuer, A::Lease>,
        b: &mut Memory<A::Issuer, A::Lease>,
    ) {
        if let Some(mut issued) = b.lease.take() {
            if let Some(target) = a.lease.as_mut() {
                self.admission.merge(target, &mut issued);
            } else {
                a.lease = Some(issued);
            }
        }
    }
    pub(in crate::plan) fn node(
        &mut self,
        m: &mut Memory<A::Issuer, A::Lease>,
        id: &NodeId,
    ) -> Result<NodeId, A> {
        self.check_memory(m)?;
        if !A::METERED {
            return Ok(id.clone());
        }
        self.additional(1)?;
        self.reserve(m, identity(id.as_str())?)?;
        Ok(id.clone())
    }
    pub(in crate::plan) fn revision(
        &mut self,
        m: &mut Memory<A::Issuer, A::Lease>,
        id: &RevisionId,
    ) -> Result<RevisionId, A> {
        self.check_memory(m)?;
        if !A::METERED {
            return Ok(id.clone());
        }
        self.additional(1)?;
        self.reserve(m, identity(id.as_str())?)?;
        Ok(id.clone())
    }
    pub(in crate::plan) fn project(
        &mut self,
        m: &mut Memory<A::Issuer, A::Lease>,
        id: &deadpan_core::ProjectId,
    ) -> Result<deadpan_core::ProjectId, A> {
        self.check_memory(m)?;
        if A::METERED {
            self.additional(1)?;
            self.reserve(m, identity(id.as_str())?)?;
        }
        Ok(id.clone())
    }
    pub(in crate::plan) fn source(
        &mut self,
        m: &mut Memory<A::Issuer, A::Lease>,
        source: &deadpan_core::SourceAudio,
    ) -> Result<deadpan_core::SourceAudio, A> {
        self.check_memory(m)?;
        let deadpan_core::SourceAudio { asset, span } = source;
        if A::METERED {
            self.additional(1)?;
            self.reserve(m, identity(asset.as_str())?)?;
        }
        Ok(deadpan_core::SourceAudio {
            asset: asset.clone(),
            span: *span,
        })
    }
    pub(in crate::plan) fn instance_fixed(
        &mut self,
        m: &mut Memory<A::Issuer, A::Lease>,
        value: &InstancePath,
    ) -> Result<InstancePath, A> {
        let mut cap = self.memory();
        let result = self.instance(m, &mut cap, value)?;
        self.check_memory(m)?;
        self.check_memory(&cap)?;
        self.merge_checked(m, &mut cap);
        Ok(result)
    }
    pub(in crate::plan) fn owner<'plan>(
        &mut self,
        m: &mut Memory<A::Issuer, A::Lease>,
        value: &AudioOwnerClock<'plan>,
    ) -> Result<AudioOwnerClock<'plan>, A> {
        self.check_memory(m)?;
        if !A::METERED {
            return Ok(value.clone());
        }
        self.additional(1)?;
        Ok(AudioOwnerClock {
            plan: value.plan,
            definition: self.definition(m, &value.definition)?,
            instance: self.instance_fixed(m, &value.instance)?,
            kind: value.kind,
            gap_after: value
                .gap_after
                .as_ref()
                .map(|v| self.iteration(m, v))
                .transpose()?,
            play: value.play,
            sampling: value.sampling,
            origin: self.origin(m, &value.origin)?,
        })
    }
    pub(in crate::plan) fn iteration(
        &mut self,
        m: &mut Memory<A::Issuer, A::Lease>,
        v: &IterationId,
    ) -> Result<IterationId, A> {
        self.check_memory(m)?;
        if !A::METERED {
            return Ok(v.clone());
        }
        Ok(IterationId {
            allocation: self.revision(m, &v.allocation)?,
            ordinal: v.ordinal,
        })
    }
    pub(in crate::plan) fn own_iteration(
        &mut self,
        m: &mut Memory<A::Issuer, A::Lease>,
        v: &IterationId,
    ) -> Result<(), A> {
        if !A::METERED {
            return Ok(());
        }
        self.additional(1)?;
        self.reserve(m, identity(v.allocation.as_str())?)
    }
    pub(in crate::plan) fn definition_ref(
        &mut self,
        m: &mut Memory<A::Issuer, A::Lease>,
        v: Option<&AudioDefinitionSelector>,
    ) -> Result<Option<AudioDefinitionSelector>, A> {
        self.check_memory(m)?;
        if !A::METERED {
            return Ok(v.cloned());
        }
        self.additional(1)?;
        v.map(|v| {
            Ok(match v {
                AudioDefinitionSelector::Node { node } => AudioDefinitionSelector::Node {
                    node: self.node(m, node)?,
                },
                AudioDefinitionSelector::RepeatDefault { repeat } => {
                    AudioDefinitionSelector::RepeatDefault {
                        repeat: self.node(m, repeat)?,
                    }
                }
                AudioDefinitionSelector::RepeatGap { repeat } => {
                    AudioDefinitionSelector::RepeatGap {
                        repeat: self.node(m, repeat)?,
                    }
                }
            })
        })
        .transpose()
    }
    pub(in crate::plan) fn gap_ref(
        &mut self,
        m: &mut Memory<A::Issuer, A::Lease>,
        v: Option<&DomainGap>,
    ) -> Result<Option<DomainGap>, A> {
        self.check_memory(m)?;
        if !A::METERED {
            return Ok(v.cloned());
        }
        self.additional(1)?;
        v.map(|v| {
            Ok(DomainGap {
                after: v.after.as_ref().map(|i| self.iteration(m, i)).transpose()?,
                duration: v.duration,
            })
        })
        .transpose()
    }
    pub(in crate::plan) fn definition(
        &mut self,
        m: &mut Memory<A::Issuer, A::Lease>,
        v: &Option<AudioDefinitionSelector>,
    ) -> Result<Option<AudioDefinitionSelector>, A> {
        self.check_memory(m)?;
        if !A::METERED {
            return Ok(v.clone());
        }
        self.additional(1)?;
        v.as_ref()
            .map(|v| {
                Ok(match v {
                    AudioDefinitionSelector::Node { node } => AudioDefinitionSelector::Node {
                        node: self.node(m, node)?,
                    },
                    AudioDefinitionSelector::RepeatDefault { repeat } => {
                        AudioDefinitionSelector::RepeatDefault {
                            repeat: self.node(m, repeat)?,
                        }
                    }
                    AudioDefinitionSelector::RepeatGap { repeat } => {
                        AudioDefinitionSelector::RepeatGap {
                            repeat: self.node(m, repeat)?,
                        }
                    }
                })
            })
            .transpose()
    }
    pub(in crate::plan) fn gap(
        &mut self,
        m: &mut Memory<A::Issuer, A::Lease>,
        v: &Option<DomainGap>,
    ) -> Result<Option<DomainGap>, A> {
        self.check_memory(m)?;
        if !A::METERED {
            return Ok(v.clone());
        }
        self.additional(1)?;
        v.as_ref()
            .map(|v| {
                Ok(DomainGap {
                    after: v.after.as_ref().map(|i| self.iteration(m, i)).transpose()?,
                    duration: v.duration,
                })
            })
            .transpose()
    }
    pub(in crate::plan) fn repeat(
        &mut self,
        m: &mut Memory<A::Issuer, A::Lease>,
        v: &RepeatInstance,
    ) -> Result<RepeatInstance, A> {
        self.check_memory(m)?;
        if !A::METERED {
            return Ok(v.clone());
        }
        Ok(RepeatInstance {
            node: self.node(m, &v.node)?,
            iteration: self.iteration(m, &v.iteration)?,
        })
    }
    pub(in crate::plan) fn repeats(
        &mut self,
        m: &mut Memory<A::Issuer, A::Lease>,
        cap: &mut Memory<A::Issuer, A::Lease>,
        v: &[RepeatInstance],
    ) -> Result<Vec<RepeatInstance>, A> {
        self.check_memory(m)?;
        if !A::METERED {
            return Ok(v.to_vec());
        }
        self.reserve(cap, array::<RepeatInstance>(v.len())?)?;
        let mut result = Vec::with_capacity(v.len());
        assert_eq!(result.capacity(), v.len());
        for v in v {
            self.additional(1)?;
            result.push(self.repeat(m, v)?);
        }
        Ok(result)
    }
    pub(in crate::plan) fn instance(
        &mut self,
        m: &mut Memory<A::Issuer, A::Lease>,
        cap: &mut Memory<A::Issuer, A::Lease>,
        v: &InstancePath,
    ) -> Result<InstancePath, A> {
        self.check_memory(m)?;
        if !A::METERED {
            return Ok(v.clone());
        }
        Ok(InstancePath {
            node: self.node(m, &v.node)?,
            repeats: self.repeats(m, cap, &v.repeats)?,
        })
    }
    pub(in crate::plan) fn origin(
        &mut self,
        m: &mut Memory<A::Issuer, A::Lease>,
        v: &AudioOwnerClockOrigin,
    ) -> Result<AudioOwnerClockOrigin, A> {
        self.check_memory(m)?;
        if !A::METERED {
            return Ok(v.clone());
        }
        self.additional(1)?;
        match v {
            AudioOwnerClockOrigin::Current => Ok(AudioOwnerClockOrigin::Current),
            AudioOwnerClockOrigin::Retained {
                definition,
                instance,
                kind,
                gap_after,
            } => {
                // No later growth: capacity can join this fixed origin's payload.
                let mut cap = self.memory();
                let definition = self.definition(m, definition)?;
                let instance = self.instance(m, &mut cap, instance)?;
                self.check_memory(m)?;
                self.check_memory(&cap)?;
                self.merge_checked(m, &mut cap);
                Ok(AudioOwnerClockOrigin::Retained {
                    definition,
                    instance,
                    kind: *kind,
                    gap_after: gap_after
                        .as_ref()
                        .map(|v| self.iteration(m, v))
                        .transpose()?,
                })
            }
        }
    }
    pub(in crate::plan) fn constraint(
        &mut self,
        m: &mut Memory<A::Issuer, A::Lease>,
        v: &super::super::audio::EnvelopeConstraint,
    ) -> Result<super::super::audio::EnvelopeConstraint, A> {
        self.check_memory(m)?;
        if !A::METERED {
            return Ok(v.clone());
        }
        self.additional(1)?;
        Ok(super::super::audio::EnvelopeConstraint {
            placement_support: v.placement_support,
            range: v.range.clone(),
            node: v.node,
            repeat_count: v.repeat_count,
            gap_after: v
                .gap_after
                .as_ref()
                .map(|v| self.iteration(m, v))
                .transpose()?,
            kinds: v.kinds,
        })
    }
    pub(in crate::plan) fn grow<T>(
        &mut self,
        data: &mut Vec<T>,
        cap: &mut Memory<A::Issuer, A::Lease>,
        needed: usize,
    ) -> Result<(), A> {
        // False keeps the original push/extend allocator backend untouched.
        if !A::METERED {
            return Ok(());
        }
        if needed <= data.capacity() {
            return Ok(());
        }
        self.check_memory(cap)?;
        let count = needed
            .max(data.capacity().checked_mul(2).ok_or(TimeError::Overflow)?)
            .max(1);
        let mut next = self.memory();
        self.reserve(&mut next, array::<T>(count)?)?;
        let mut replacement = Vec::with_capacity(count);
        assert_eq!(replacement.capacity(), count);
        for item in data.drain(..) {
            self.additional(1)?;
            replacement.push(item);
        }
        let old = std::mem::replace(data, replacement);
        drop(old);
        *cap = next;
        Ok(())
    }
}
