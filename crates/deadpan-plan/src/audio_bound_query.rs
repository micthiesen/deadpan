//! Query values guarded by the plan memory charged for them.
use crate::AudioSoundContextAdmission;
use crate::PlanError;
use crate::plan::audio_owners::admission::{Budget as QueryBudget, Memory, Owned, Result};

/// A query value kept ahead of the plan memory it was charged to.
pub(in crate::plan) struct GuardedBound<T, I, L> {
    value: T,
    pub(in crate::plan) _memory: Memory<I, L>,
}
impl<T, I: Copy + Eq, L> std::ops::Deref for GuardedBound<T, I, L> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}
impl<T> GuardedBound<T, (), ()> {
    pub(in crate::plan) fn unmetered(self) -> T {
        self.value
    }
}
impl<T, I: Copy + Eq, L> GuardedBound<T, I, L> {
    pub(in crate::plan) fn plain(value: T, issuer: I) -> Self {
        Self {
            value,
            _memory: Memory::empty(issuer),
        }
    }

    pub(in crate::plan) fn adopt_memory<A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        self,
        memory: Memory<I, L>,
        budget: &mut QueryBudget<'_, A>,
    ) -> Result<Self, A> {
        let mut guarded = Owned {
            value: self,
            memory,
        };
        budget.check_memory(&guarded.value._memory)?;
        budget.check_memory(&guarded.memory)?;
        budget.merge_checked(&mut guarded.value._memory, &mut guarded.memory)?;
        budget.check_memory(&guarded.value._memory)?;
        Ok(guarded.value)
    }

    /// Both values stay ahead of both memories throughout collection.
    pub(in crate::plan) fn zip<U, A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        self,
        other: GuardedBound<U, I, L>,
        budget: &mut QueryBudget<'_, A>,
    ) -> Result<GuardedBound<(T, U), I, L>, A> {
        struct Joined<T, I, L> {
            value: T,
            memory: Memory<I, L>,
            other_memory: Memory<I, L>,
        }
        let mut joined = Joined {
            value: (self.value, other.value),
            memory: self._memory,
            other_memory: other._memory,
        };
        budget.check_memory(&joined.memory)?;
        budget.check_memory(&joined.other_memory)?;
        budget.merge_checked(&mut joined.memory, &mut joined.other_memory)?;
        budget.check_memory(&joined.memory)?;
        Ok(GuardedBound {
            value: joined.value,
            _memory: joined.memory,
        })
    }

    /// The callback is allocation-free and all of its actual owning fields
    /// remain guarded by its caller through pre/post-map failure.
    pub(in crate::plan) fn map<U, A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        self,
        budget: &mut QueryBudget<'_, A>,
        f: impl FnOnce(T) -> U,
    ) -> Result<GuardedBound<U, I, L>, A> {
        budget.check_memory(&self._memory)?;
        let result = GuardedBound {
            value: f(self.value),
            _memory: self._memory,
        };
        budget.check_memory(&result._memory)?;
        Ok(result)
    }
}
// This extraction is deliberately limited to the actual flattened walker.
impl<'plan, I: Copy + Eq, L> GuardedBound<crate::plan::audio::AudioWalkSpan<'plan>, I, L> {
    pub(in crate::plan) fn flattened_leaf<A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        self,
        memory: Memory<I, L>,
        budget: &mut QueryBudget<'_, A>,
    ) -> Result<Owned<crate::plan::audio::AudioSpan, I, L>, A> {
        use crate::plan::audio::AudioWalkSpan;
        // Check while the complete value precedes both independent guards.
        let guarded = Owned {
            value: self,
            memory,
        };
        budget.check_memory(&guarded.memory)?;
        budget.check_memory(&guarded.value._memory)?;
        match guarded {
            Owned {
                value:
                    GuardedBound {
                        value: AudioWalkSpan::Leaf(span),
                        _memory: mut leaf_memory,
                    },
                memory,
            } => {
                let mut result = Owned {
                    value: span,
                    memory,
                };
                budget.merge_checked(&mut result.memory, &mut leaf_memory)?;
                budget.check_memory(&result.memory)?;
                Ok(result)
            }
            _ => Err(PlanError::InvalidPlan("flattened audio retained a stage").into()),
        }
    }
}
impl<T, I: Copy + Eq, L> GuardedBound<T, I, L> {
    pub(in crate::plan) fn merge_memory<A: AudioSoundContextAdmission<Issuer = I, Lease = L>>(
        &mut self,
        memory: &mut Memory<I, L>,
        budget: &mut QueryBudget<'_, A>,
    ) -> Result<(), A> {
        budget.check_memory(&self._memory)?;
        budget.check_memory(memory)?;
        budget.merge_checked(&mut self._memory, memory)?;
        Ok(())
    }
}
