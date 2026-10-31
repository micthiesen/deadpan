//! Guarded collection for the actual Root and physical-Domain processing products.
use super::*;
use crate::audio_allocated::admission::array;
use crate::plan::audio_bound::query::GuardedBound;
use crate::plan::audio_owners::admission::{self, Budget as QueryBudget, Memory, Owned, Result};
use crate::{AudioProcessingQuery, AudioSoundContextAdmission};

struct ProcessingValue<'plan, I, L> {
    query: AudioProcessingQuery<'plan>,
    _capacity: Memory<I, L>,
}

/// Actual processing storage and its independent Core/Plan leases remain owned.
pub struct AdmittedAudioProcessingQuery<'plan, I, L> {
    value: GuardedBound<ProcessingValue<'plan, I, L>, I, L>,
    canonical: usize,
    additional: usize,
    forwarded: bool,
}
impl<'plan, I: Copy + Eq, L> AdmittedAudioProcessingQuery<'plan, I, L> {
    pub fn query(&self) -> &AudioProcessingQuery<'plan> {
        &self.value.query
    }
    pub fn work(&self) -> usize {
        self.value.query.work
    }
    pub fn canonical_units(&self) -> usize {
        self.canonical
    }
    pub fn additional_units(&self) -> usize {
        self.additional
    }
    pub fn was_forwarded(&self) -> bool {
        self.forwarded
    }
}
impl<'plan> AdmittedAudioProcessingQuery<'plan, (), ()> {
    pub(in crate::plan) fn unmetered(self) -> AudioProcessingQuery<'plan> {
        self.value.unmetered().query
    }
}

pub(in crate::plan) struct Accumulator<S, I, L> {
    pub(in crate::plan) spans: Vec<S>,
    pub(in crate::plan) capacity: Memory<I, L>,
}
pub(in crate::plan) type Collected<S, A> = GuardedBound<
    Accumulator<
        S,
        <A as AudioSoundContextAdmission>::Issuer,
        <A as AudioSoundContextAdmission>::Lease,
    >,
    <A as AudioSoundContextAdmission>::Issuer,
    <A as AudioSoundContextAdmission>::Lease,
>;
pub(in crate::plan) fn empty<S, A: AudioSoundContextAdmission>(
    budget: &QueryBudget<'_, A>,
) -> Collected<S, A> {
    GuardedBound::plain(
        Accumulator {
            spans: Vec::new(),
            capacity: budget.memory(),
        },
        budget.issuer,
    )
}

/// Keep the previous buffer, remaining values and current span behind all lanes.
pub(in crate::plan) fn push<S, A: AudioSoundContextAdmission>(
    accumulator: Collected<S, A>,
    span: GuardedBound<S, A::Issuer, A::Lease>,
    budget: &mut QueryBudget<'_, A>,
) -> Result<Collected<S, A>, A> {
    let needed = accumulator
        .spans
        .len()
        .checked_add(1)
        .ok_or(TimeError::Overflow)?;
    budget.check_memory(&accumulator.capacity)?;
    if !A::METERED || needed <= accumulator.spans.capacity() {
        return accumulator
            .zip(span, budget)?
            .map(budget, |(mut accumulator, span)| {
                accumulator.spans.push(span);
                accumulator
            });
    }
    let count = needed
        .max(
            accumulator
                .spans
                .capacity()
                .checked_mul(2)
                .ok_or(TimeError::Overflow)?,
        )
        .max(1);
    let mut replacement = Owned {
        value: Vec::new(),
        memory: budget.memory(),
    };
    budget.reserve(&mut replacement.memory, array::<S>(count)?)?;
    replacement.value = Vec::with_capacity(count);
    assert_eq!(replacement.value.capacity(), count);
    struct Growth<S, I, L> {
        old: std::vec::IntoIter<S>,
        replacement: Vec<S>,
        pending: S,
        old_capacity: Memory<I, L>,
        replacement_capacity: Memory<I, L>,
    }
    let mut growth = accumulator
        .zip(span, budget)?
        .map(budget, move |(accumulator, span)| Growth {
            old: accumulator.spans.into_iter(),
            replacement: replacement.value,
            pending: span,
            old_capacity: accumulator.capacity,
            replacement_capacity: replacement.memory,
        })?;
    while !growth.old.as_slice().is_empty() {
        budget.additional(1)?;
        growth = growth.map(budget, |mut state| {
            state
                .replacement
                .push(state.old.next().expect("observed old span"));
            state
        })?;
    }
    growth.map(budget, |state| {
        let Growth {
            old,
            mut replacement,
            pending,
            old_capacity,
            replacement_capacity,
        } = state;
        drop(old);
        drop(old_capacity);
        replacement.push(pending);
        Accumulator {
            spans: replacement,
            capacity: replacement_capacity,
        }
    })
}

pub(in crate::plan) fn finish<'plan, A: AudioSoundContextAdmission>(
    plan: &'plan RenderPlan,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
    accumulator: Collected<AudioProcessingSpan<'plan>, A>,
    budget: &mut QueryBudget<'_, A>,
) -> Result<AdmittedAudioProcessingQuery<'plan, A::Issuer, A::Lease>, A> {
    let mut header = Owned {
        value: (None, None),
        memory: budget.memory(),
    };
    header.value.0 = Some(budget.project(&mut header.memory, &plan.metadata.project_id)?);
    header.value.1 = Some(budget.revision(&mut header.memory, &plan.metadata.revision_id)?);
    let header =
        GuardedBound::plain(header.value, budget.issuer).adopt_memory(header.memory, budget)?;
    let canonical = budget.canonical_used;
    let additional = budget.additional_used;
    let work = canonical
        .checked_add(additional)
        .ok_or(TimeError::Overflow)?;
    if limits.maximum_work.checked_sub(budget.remaining) != Some(work) {
        return Err(PlanError::InvalidPlan("processing query work provenance mismatch").into());
    }
    let lookup = budget.lookup;
    let value = accumulator
        .zip(header, budget)?
        .map(budget, |(accumulator, header)| ProcessingValue {
            query: AudioProcessingQuery {
                project_id: header.0.expect("completed processing project header"),
                revision_id: header.1.expect("completed processing revision header"),
                samples,
                spans: accumulator.spans,
                lookup,
                work,
            },
            _capacity: accumulator.capacity,
        })?;
    Ok(AdmittedAudioProcessingQuery {
        value,
        canonical,
        additional,
        forwarded: A::METERED,
    })
}

pub(in crate::plan) fn domain_query<'plan, A: AudioSoundContextAdmission>(
    plan: &'plan RenderPlan,
    seed: &AudioWalkSeed,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
    mut local: Budget,
    stops: (bool, Option<&mut capture::ProcessingStops>),
    admission: &mut A,
) -> std::result::Result<AdmittedAudioProcessingQuery<'plan, A::Issuer, A::Lease>, A::Error> {
    let (stop_at_preserve, mut stops) = stops;
    let mut budget = QueryBudget::new(&mut local, admission);
    admission::outer::<_, A>((|| {
        let mut accumulator = empty(&budget);
        let mut cursor = samples.start;
        while cursor < samples.end {
            if accumulator.spans.len() == limits.maximum_spans {
                return Err(PlanError::AudioQueryLimit("span count").into());
            }
            let walk = plan.audio_walk_inner_admitted(
                cursor,
                &mut budget,
                Some(seed),
                stop_at_preserve,
                true,
                (None, None, None, stops.as_deref_mut()),
            )?;
            let value = walk.value.adopt_memory(walk.memory, &mut budget)?;
            let end = match &*value {
                AudioWalkSpan::Leaf(span) => span.allocated_samples.end,
                AudioWalkSpan::Stage(span) => span.allocated_samples.end,
            }
            .min(samples.end);
            let span = value.map(&mut budget, |value| {
                let mut span = value.into_processing();
                span.samples = cursor..end;
                span
            })?;
            cursor = end;
            accumulator = push(accumulator, span, &mut budget)?;
        }
        finish(plan, samples, limits, accumulator, &mut budget)
    })())
}
pub(in crate::plan) fn ordinary<'plan, A: AudioSoundContextAdmission>(
    query: AudioProcessingQuery<'plan>,
    admission: &A,
) -> Result<AdmittedAudioProcessingQuery<'plan, A::Issuer, A::Lease>, A> {
    if A::METERED {
        return Err(PlanError::InvalidPlan("processing accounting mode mismatch").into());
    }
    let canonical = query.work;
    let issuer = admission.issuer();
    Ok(AdmittedAudioProcessingQuery {
        value: GuardedBound::plain(
            ProcessingValue {
                query,
                _capacity: Memory::empty(issuer),
            },
            issuer,
        ),
        canonical,
        additional: 0,
        forwarded: false,
    })
}

type Converted<'plan, A> = GuardedBound<
    (
        AudioProcessingQuery<'plan>,
        Memory<<A as AudioSoundContextAdmission>::Issuer, <A as AudioSoundContextAdmission>::Lease>,
    ),
    <A as AudioSoundContextAdmission>::Issuer,
    <A as AudioSoundContextAdmission>::Lease,
>;

pub(in crate::plan) fn seal<'plan, A: AudioSoundContextAdmission>(
    query: Converted<'plan, A>,
    limits: AudioQueryLimits,
    budget: &mut QueryBudget<'_, A>,
) -> Result<AdmittedAudioProcessingQuery<'plan, A::Issuer, A::Lease>, A> {
    let canonical = budget.canonical_used;
    let additional = budget.additional_used;
    let work = canonical
        .checked_add(additional)
        .ok_or(TimeError::Overflow)?;
    if query.0.work != work || limits.maximum_work.checked_sub(budget.remaining) != Some(work) {
        return Err(PlanError::InvalidPlan("processing query work provenance mismatch").into());
    }
    budget.check_memory(&query.1)?;
    let value = query.map(budget, |(query, capacity)| ProcessingValue {
        query,
        _capacity: capacity,
    })?;
    Ok(AdmittedAudioProcessingQuery {
        value,
        canonical,
        additional,
        forwarded: A::METERED,
    })
}
