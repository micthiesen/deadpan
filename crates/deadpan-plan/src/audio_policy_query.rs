//! Actual policy construction under one work and allocation admission.
#[cfg(test)]
#[path = "audio_policy_point_trace.rs"]
pub(in crate::plan) mod point_trace;

use super::*;
use crate::AudioSoundContextAdmission;
use crate::audio_allocated::admission::array;
use crate::plan::audio::Budget as LocalBudget;
use crate::plan::audio::processing_query::{self as collection, Accumulator};
use crate::plan::audio_bound::query::GuardedBound;
use crate::plan::audio_owners::admission::{
    Budget as QueryBudget, Memory, Owned, Result as QueryResult, outer,
};
use crate::plan::audio_signal::processing_query::{SignalBuffer, SignalValue};
use crate::plan::{
    AudioDefinitionSelector, AudioProcessingSpan, AudioSignalQuery, AudioSignalSpan,
};
use deadpan_core::{ProjectId, RevisionId};

struct PolicyBuffer<I, L> {
    query: AudioPolicyQuery<AudioSample>,
    _contents_capacity: Memory<I, L>,
}

/// Immutable actual policy metadata and its independent construction leases.
pub struct AdmittedAudioPolicyQuery<I, L> {
    value: GuardedBound<PolicyBuffer<I, L>, I, L>,
    canonical: usize,
    additional: usize,
    forwarded: bool,
}
impl<I: Copy + Eq, L> AdmittedAudioPolicyQuery<I, L> {
    pub fn query(&self) -> &AudioPolicyQuery<AudioSample> {
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
type Inventory<A> = collection::Collected<AudioContent, A>;
type Ranges<A> = Owned<
    Vec<Range<i64>>,
    <A as AudioSoundContextAdmission>::Issuer,
    <A as AudioSoundContextAdmission>::Lease,
>;
struct State<A: AudioSoundContextAdmission> {
    maximum_spans: usize,
    inventory: Option<Inventory<A>>,
}
impl<A: AudioSoundContextAdmission> State<A> {
    fn new(maximum_spans: usize, budget: &QueryBudget<'_, A>) -> Self {
        Self {
            maximum_spans,
            inventory: Some(collection::empty(budget)),
        }
    }
}

fn ordinary<A: AudioSoundContextAdmission>(
    query: AudioPolicyQuery<AudioSample>,
    admission: &A,
) -> QueryResult<AdmittedAudioPolicyQuery<A::Issuer, A::Lease>, A> {
    if A::METERED {
        return Err(PlanError::InvalidPlan("policy accounting mode mismatch").into());
    }
    let canonical = query.work;
    let issuer = admission.issuer();
    Ok(AdmittedAudioPolicyQuery {
        value: GuardedBound::plain(
            PolicyBuffer {
                query,
                _contents_capacity: Memory::empty(issuer),
            },
            issuer,
        ),
        canonical,
        additional: 0,
        forwarded: false,
    })
}

impl RenderPlan {
    pub fn audio_policy_admitted<A: AudioSoundContextAdmission>(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> Result<AdmittedAudioPolicyQuery<A::Issuer, A::Lease>, A::Error> {
        if !A::METERED {
            let query = self.audio_policy(samples, limits).map_err(A::Error::from)?;
            return outer::<_, A>(ordinary(query, admission));
        }
        limits.validate()?;
        let mut local = LocalBudget {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        let mut budget = QueryBudget::new(&mut local, admission);
        outer::<_, A>((|| {
            let mut state = State::new(limits.maximum_spans, &budget);
            let ranges = signal_policy(
                &self.audio_signal(),
                samples.start.0..samples.end.0,
                AudioBoundaryRule::RoundEven,
                &mut state,
                &mut budget,
                0,
                true,
            )?;
            finish(state, ranges, limits, &mut budget)
        })())
    }
}

impl AudioDomain<'_> {
    pub fn policy_admitted<A: AudioSoundContextAdmission>(
        &self,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
        admission: &mut A,
    ) -> Result<AdmittedAudioPolicyQuery<A::Issuer, A::Lease>, A::Error> {
        if !A::METERED {
            let query = self.policy(samples, limits).map_err(A::Error::from)?;
            return outer::<_, A>(ordinary(query, admission));
        }
        limits.validate()?;
        let mut local = LocalBudget {
            remaining: limits.maximum_work,
            lookup: LookupStats::default(),
        };
        let mut budget = QueryBudget::new(&mut local, admission);
        outer::<_, A>(domain_query_with_processing(
            self,
            samples,
            limits,
            None,
            &mut budget,
        ))
    }
}

/// Only the sealed first product supplies a borrowed tape to this fresh policy budget.
pub(in crate::plan) fn domain_query_with_processing<'plan, A: AudioSoundContextAdmission>(
    domain: &AudioDomain<'plan>,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
    bindings: Option<&[AudioProcessingSpan<'plan>]>,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<AdmittedAudioPolicyQuery<A::Issuer, A::Lease>, A> {
    limits.validate()?;
    if !A::METERED {
        let query = match bindings {
            Some(bindings) => {
                super::domain_policy_with_processing(domain, samples, limits, bindings)?
            }
            None => domain.policy(samples, limits)?,
        };
        return ordinary(query, budget.admission);
    }
    if limits.maximum_work != budget.remaining
        || budget.canonical_used != 0
        || budget.additional_used != 0
        || budget.lookup != LookupStats::default()
    {
        return Err(PlanError::InvalidPlan("policy query work provenance mismatch").into());
    }
    let mut state = State::new(limits.maximum_spans, budget);
    let ranges = domain_policy_inner(
        domain,
        samples.start.0..samples.end.0,
        &mut state,
        budget,
        0,
        true,
        bindings,
    )?;
    finish(state, ranges, limits, budget)
}

fn spend<A: AudioSoundContextAdmission>(
    budget: &mut QueryBudget<'_, A>,
    units: usize,
) -> QueryResult<(), A> {
    if units > budget.remaining {
        return Err(PlanError::AudioQueryLimit("recursive policy work").into());
    }
    budget.canonical(units)
}
fn enter<A: AudioSoundContextAdmission>(
    budget: &mut QueryBudget<'_, A>,
    depth: usize,
) -> QueryResult<(), A> {
    if depth > MAX_DOCUMENT_DEPTH {
        return Err(PlanError::AudioQueryLimit("policy depth").into());
    }
    spend(budget, 1)
}
fn child_limits<A: AudioSoundContextAdmission>(
    state: &State<A>,
    budget: &QueryBudget<'_, A>,
) -> QueryResult<AudioQueryLimits, A> {
    if budget.remaining == 0 {
        return Err(PlanError::AudioQueryLimit("recursive policy work").into());
    }
    Ok(AudioQueryLimits {
        maximum_spans: state.maximum_spans,
        maximum_work: budget.remaining,
    })
}
fn endpoint_grid<A: AudioSoundContextAdmission>(
    enabled: bool,
    retimes: &[AudioRetimeStage],
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<bool, A> {
    if !enabled {
        return Ok(false);
    }
    spend(budget, retimes.len())?;
    Ok(!retimes.iter().any(|retime| {
        retime
            .pitch
            .processes(retime.child_frames_per_local_frame == ExactRatio::ONE)
    }))
}

fn empty_ranges<A: AudioSoundContextAdmission>(budget: &QueryBudget<'_, A>) -> Ranges<A> {
    Owned {
        value: Vec::new(),
        memory: budget.memory(),
    }
}
fn push_range<A: AudioSoundContextAdmission>(
    out: &mut Ranges<A>,
    range: Range<i64>,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<(), A> {
    let needed = out.value.len().checked_add(1).ok_or(TimeError::Overflow)?;
    budget.grow(&mut out.value, &mut out.memory, needed)?;
    budget.additional(1)?;
    out.value.push(range);
    Ok(())
}
fn extend_ranges<A: AudioSoundContextAdmission>(
    out: &mut Ranges<A>,
    input: Ranges<A>,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<(), A> {
    let mut input = Owned {
        value: input.value.into_iter(),
        memory: input.memory,
    };
    for range in input.value.by_ref() {
        push_range(out, range, budget)?;
    }
    Ok(())
}
fn add_outside<A: AudioSoundContextAdmission>(
    out: &mut Ranges<A>,
    samples: Range<i64>,
    support: Range<i64>,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<(), A> {
    if samples.start < support.start {
        push_range(out, samples.start..samples.end.min(support.start), budget)?;
    }
    if samples.end > support.end {
        push_range(out, samples.start.max(support.end)..samples.end, budget)?;
    }
    Ok(())
}

struct Current<Q, I, L> {
    query: Q,
    inventory: Accumulator<AudioContent, I, L>,
}
type CurrentValue<Q, A> = GuardedBound<
    Current<Q, <A as AudioSoundContextAdmission>::Issuer, <A as AudioSoundContextAdmission>::Lease>,
    <A as AudioSoundContextAdmission>::Issuer,
    <A as AudioSoundContextAdmission>::Lease,
>;

fn grow_inventory<Q, A: AudioSoundContextAdmission>(
    current: CurrentValue<Q, A>,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<CurrentValue<Q, A>, A> {
    let needed = current
        .inventory
        .spans
        .len()
        .checked_add(1)
        .ok_or(TimeError::Overflow)?;
    budget.check_memory(&current.inventory.capacity)?;
    if needed <= current.inventory.spans.capacity() {
        return Ok(current);
    }
    let count = needed
        .max(
            current
                .inventory
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
    budget.reserve(&mut replacement.memory, array::<AudioContent>(count)?)?;
    replacement.value = Vec::with_capacity(count);
    assert_eq!(replacement.value.capacity(), count);
    struct Growth<Q, I, L> {
        query: Q,
        old: std::vec::IntoIter<AudioContent>,
        replacement: Vec<AudioContent>,
        old_capacity: Memory<I, L>,
        new_capacity: Memory<I, L>,
    }
    let mut growth = current.map(budget, move |current| Growth {
        query: current.query,
        old: current.inventory.spans.into_iter(),
        replacement: replacement.value,
        old_capacity: current.inventory.capacity,
        new_capacity: replacement.memory,
    })?;
    while !growth.old.as_slice().is_empty() {
        budget.additional(1)?;
        growth = growth.map(budget, |mut growth| {
            growth
                .replacement
                .push(growth.old.next().expect("observed inventory element"));
            growth
        })?;
    }
    growth.map(budget, |growth| {
        let next = Current {
            query: growth.query,
            inventory: Accumulator {
                spans: growth.replacement,
                capacity: growth.new_capacity,
            },
        };
        drop(Owned {
            value: growth.old,
            memory: growth.old_capacity,
        });
        next
    })
}

struct SignalDrain<'plan, I, L> {
    _headers: (ProjectId, RevisionId, Option<AudioDefinitionSelector>),
    spans: std::vec::IntoIter<AudioSignalSpan<'plan>>,
    _capacity: Memory<I, L>,
}

fn signal_current<'plan, A: AudioSoundContextAdmission>(
    current: SignalValue<'plan, A>,
    bindings: bool,
    state: &mut State<A>,
    budget: &mut QueryBudget<'_, A>,
    endpoints: bool,
) -> QueryResult<Ranges<A>, A> {
    let current = current.map(budget, |buffer| {
        let SignalBuffer { query, capacity } = buffer;
        let AudioSignalQuery {
            project_id,
            revision_id,
            definition,
            samples: _,
            spans,
            lookup: _,
            work: _,
        } = query;
        SignalDrain {
            _headers: (project_id, revision_id, definition),
            spans: spans.into_iter(),
            _capacity: capacity,
        }
    })?;
    let inventory = state
        .inventory
        .take()
        .expect("policy inventory available before current query");
    let mut current = current
        .zip(inventory, budget)?
        .map(budget, |(query, inventory)| Current { query, inventory })?;
    let mut suppressed = empty_ranges(budget);
    while let Some(span) = current.query.spans.as_slice().first() {
        let AudioSignalContent::Leaf(content) = &span.content else {
            return Err(PlanError::InvalidPlan("current policy retained processing").into());
        };
        if is_suppressed(content) {
            push_range(
                &mut suppressed,
                span.samples.start.0..span.samples.end.0,
                budget,
            )?;
        }
        if !bindings
            && is_selected_endpoint(content)
            && endpoint_grid(endpoints, &span.retimes, budget)?
        {
            push_range(
                &mut suppressed,
                span.samples.start.0..span.samples.end.0,
                budget,
            )?;
        }
        spend(budget, 1)?;
        current = grow_inventory(current, budget)?;
        budget.additional(1)?;
        current = current.map(budget, |mut current| {
            let span = current
                .query
                .spans
                .next()
                .expect("observed current signal span");
            let AudioSignalContent::Leaf(content) = span.content else {
                unreachable!("checked current Leaf")
            };
            current.inventory.spans.push(content);
            current
        })?;
    }
    state.inventory = Some(current.map(budget, |current| current.inventory)?);
    Ok(suppressed)
}

struct FlatDrain<I, L> {
    _headers: (ProjectId, RevisionId),
    spans: std::vec::IntoIter<crate::AudioSpan>,
    _capacity: Memory<I, L>,
}

fn domain_current<A: AudioSoundContextAdmission>(
    current: crate::plan::audio::flattened_query::FlatValue<A>,
    bindings: bool,
    state: &mut State<A>,
    budget: &mut QueryBudget<'_, A>,
    endpoints: bool,
) -> QueryResult<Ranges<A>, A> {
    let Owned { value, memory } = current;
    let current = GuardedBound::plain(value, budget.issuer).adopt_memory(memory, budget)?;
    let current = current.map(budget, |buffer| {
        let crate::plan::audio::flattened_query::FlatBuffer { query, capacity } = buffer;
        let crate::AudioQuery {
            project_id,
            revision_id,
            samples: _,
            spans,
            lookup: _,
            work: _,
        } = query;
        FlatDrain {
            _headers: (project_id, revision_id),
            spans: spans.into_iter(),
            _capacity: capacity,
        }
    })?;
    let inventory = state
        .inventory
        .take()
        .expect("policy inventory available before current query");
    let mut current = current
        .zip(inventory, budget)?
        .map(budget, |(query, inventory)| Current { query, inventory })?;
    let mut suppressed = empty_ranges(budget);
    while let Some(span) = current.query.spans.as_slice().first() {
        if is_suppressed(&span.content) {
            push_range(
                &mut suppressed,
                span.samples.start.0..span.samples.end.0,
                budget,
            )?;
        }
        if !bindings
            && is_selected_endpoint(&span.content)
            && endpoint_grid(endpoints, &span.retimes, budget)?
        {
            push_range(
                &mut suppressed,
                span.samples.start.0..span.samples.end.0,
                budget,
            )?;
        }
        if endpoint_grid(endpoints, &span.retimes, budget)? {
            add_outside(
                &mut suppressed,
                span.samples.start.0..span.samples.end.0,
                span.envelope_samples.start.0..span.envelope_samples.end.0,
                budget,
            )?;
        }
        spend(budget, 1)?;
        current = grow_inventory(current, budget)?;
        budget.additional(1)?;
        current = current.map(budget, |mut current| {
            let span = current
                .query
                .spans
                .next()
                .expect("observed current physical span");
            current.inventory.spans.push(span.content);
            current
        })?;
    }
    state.inventory = Some(current.map(budget, |current| current.inventory)?);
    Ok(suppressed)
}

fn signal_policy<A: AudioSoundContextAdmission>(
    signal: &AudioSignal<'_>,
    samples: Range<i64>,
    rule: AudioBoundaryRule,
    state: &mut State<A>,
    budget: &mut QueryBudget<'_, A>,
    depth: usize,
    endpoints: bool,
) -> QueryResult<Ranges<A>, A> {
    enter(budget, depth)?;
    let range = SignalSample(samples.start)..SignalSample(samples.end);
    let current = signal.structural_child_query(
        range.clone(),
        child_limits(state, budget)?,
        rule,
        (false, false),
        budget,
    )?;
    let bindings = signal.has_audio_bindings();
    let mut suppressed = signal_current(current, bindings, state, budget, endpoints)?;
    // The closed Root/raw-Point factories are Structural; arbitrary voice providers keep ordinary policy.
    if !bindings || samples.is_empty() {
        return Ok(suppressed);
    }
    let bindings = signal.structural_child_query(
        range,
        child_limits(state, budget)?,
        rule,
        (false, true),
        budget,
    )?;
    for span in &bindings.query.spans {
        match &span.content {
            AudioSignalContent::Bound(bound) => {
                let endpoints = endpoint_grid(endpoints, &span.retimes, budget)?;
                let retained = bound_policy(
                    bound,
                    span.samples.start.0..span.samples.end.0,
                    span.allocated_samples.start.0,
                    state,
                    budget,
                    depth + 1,
                    endpoints,
                )?;
                extend_ranges(&mut suppressed, retained, budget)?;
            }
            AudioSignalContent::Leaf(content)
                if is_selected_endpoint(content)
                    && endpoint_grid(endpoints, &span.retimes, budget)? =>
            {
                push_range(
                    &mut suppressed,
                    span.samples.start.0..span.samples.end.0,
                    budget,
                )?;
            }
            _ => {}
        }
    }
    Ok(suppressed)
}

fn binding_policy<A: AudioSoundContextAdmission>(
    bindings: &[AudioProcessingSpan<'_>],
    suppressed: &mut Ranges<A>,
    state: &mut State<A>,
    budget: &mut QueryBudget<'_, A>,
    depth: usize,
    endpoints: bool,
) -> QueryResult<(), A> {
    for span in bindings {
        match &span.content {
            AudioSignalContent::Bound(bound) => {
                let endpoints = endpoint_grid(endpoints, &span.retimes, budget)?;
                let retained = bound_policy(
                    bound,
                    span.samples.start.0..span.samples.end.0,
                    span.allocated_samples.start.0,
                    state,
                    budget,
                    depth + 1,
                    endpoints,
                )?;
                extend_ranges(suppressed, retained, budget)?;
            }
            AudioSignalContent::Leaf(content)
                if is_selected_endpoint(content)
                    && endpoint_grid(endpoints, &span.retimes, budget)? =>
            {
                push_range(suppressed, span.samples.start.0..span.samples.end.0, budget)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn domain_policy_inner<A: AudioSoundContextAdmission>(
    domain: &AudioDomain<'_>,
    samples: Range<i64>,
    state: &mut State<A>,
    budget: &mut QueryBudget<'_, A>,
    depth: usize,
    endpoints: bool,
    processing_bindings: Option<&[AudioProcessingSpan<'_>]>,
) -> QueryResult<Ranges<A>, A> {
    enter(budget, depth)?;
    let range = AudioSample(samples.start)..AudioSample(samples.end);
    let current = domain.audio_child_query(range.clone(), child_limits(state, budget)?, budget)?;
    let bindings = domain.has_audio_bindings();
    let mut suppressed = domain_current(current, bindings, state, budget, endpoints)?;
    if !bindings || samples.is_empty() {
        return Ok(suppressed);
    }
    if let Some(bindings) = processing_bindings.filter(|spans| spans.len() <= state.maximum_spans) {
        binding_policy(bindings, &mut suppressed, state, budget, depth, endpoints)?;
        return Ok(suppressed);
    }
    let bindings =
        domain.processing_child_query(range, child_limits(state, budget)?, false, budget)?;
    binding_policy(
        &bindings.query.spans,
        &mut suppressed,
        state,
        budget,
        depth,
        endpoints,
    )?;
    Ok(suppressed)
}

fn offset_ranges<A: AudioSoundContextAdmission>(
    ranges: Ranges<A>,
    origin: i64,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<Ranges<A>, A> {
    let mut input = Owned {
        value: ranges.value.into_iter(),
        memory: ranges.memory,
    };
    let mut output = empty_ranges(budget);
    while let Some(range) = input.value.as_slice().first() {
        let mapped = range.start.checked_add(origin).ok_or(TimeError::Overflow)?
            ..range.end.checked_add(origin).ok_or(TimeError::Overflow)?;
        push_range(&mut output, mapped, budget)?;
        input.value.next().expect("observed Point offset range");
    }
    Ok(output)
}

fn bound_policy<A: AudioSoundContextAdmission>(
    bound: &AudioBound<'_>,
    samples: Range<i64>,
    anchor: i64,
    state: &mut State<A>,
    budget: &mut QueryBudget<'_, A>,
    depth: usize,
    endpoints: bool,
) -> QueryResult<Ranges<A>, A> {
    enter(budget, depth)?;
    let raw = bound.raw_domain_admitted(budget)?;
    let support = match &raw.value {
        AudioBoundDomain::Root(domain) => {
            let range = domain.root_samples();
            range.start.0..range.end.0
        }
        AudioBoundDomain::Point(domain) => {
            let range = domain.reference_samples();
            range.start.0..range.end.0
        }
        AudioBoundDomain::Empty => {
            let mut ranges = empty_ranges(budget);
            if endpoints {
                push_range(&mut ranges, samples, budget)?;
            }
            return Ok(ranges);
        }
    };
    if support.is_empty() {
        let mut ranges = empty_ranges(budget);
        if endpoints {
            push_range(&mut ranges, samples, budget)?;
        }
        return Ok(ranges);
    }
    let first = bound.reference_at_wide_offset(i128::from(samples.start) - i128::from(anchor))?;
    let step = bound.reference_samples_per_output_sample();
    let last = first.checked_add(
        ExactRatio::new(i128::from(samples.end) - i128::from(samples.start) - 1, 1)?
            .checked_mul(step)?,
    )?;
    let clipped_start = first.floor().max(i128::from(support.start));
    let clipped_end = last
        .floor()
        .checked_add(1)
        .ok_or(TimeError::Overflow)?
        .min(i128::from(support.end));
    let mut suppressed = empty_ranges(budget);
    let inverse = |at: i64| -> Result<i64, PlanError> {
        let offset = ExactRatio::integer(at)
            .checked_sub(first)?
            .checked_div(step)?
            .ceil()?;
        let value = i128::from(samples.start)
            .checked_add(offset)
            .ok_or(TimeError::Overflow)?;
        Ok(
            i64::try_from(value.clamp(i128::from(samples.start), i128::from(samples.end)))
                .map_err(|_| TimeError::Overflow)?,
        )
    };
    if endpoints {
        add_outside(
            &mut suppressed,
            samples.clone(),
            inverse(support.start)?..inverse(support.end)?,
            budget,
        )?;
    }
    if clipped_start >= clipped_end {
        return Ok(suppressed);
    }
    let selection = i64::try_from(clipped_start).map_err(|_| TimeError::Overflow)?
        ..i64::try_from(clipped_end).map_err(|_| TimeError::Overflow)?;
    let retained = match &raw.value {
        AudioBoundDomain::Root(domain) => {
            domain_policy_inner(domain, selection, state, budget, depth + 1, endpoints, None)?
        }
        AudioBoundDomain::Point(domain) => {
            let origin = domain.reference_samples().start.0;
            let range = selection
                .start
                .checked_sub(origin)
                .ok_or(TimeError::Overflow)?
                ..selection
                    .end
                    .checked_sub(origin)
                    .ok_or(TimeError::Overflow)?;
            #[cfg(test)]
            let point_context = point_trace::Context {
                origin,
                reference_support: [support.start, support.end],
                selection: [selection.start, selection.end],
                signal_range: [range.start, range.end],
            };
            #[cfg(test)]
            point_trace::record(
                point_trace::Phase::BeforeClone,
                point_context,
                &[],
                (budget.canonical_used, budget.additional_used),
            );
            let signal = domain.signal_admitted(budget)?;
            #[cfg(test)]
            point_trace::record(
                point_trace::Phase::AfterClone,
                point_context,
                &[],
                (budget.canonical_used, budget.additional_used),
            );
            let ranges = signal_policy(
                &signal.value,
                range,
                AudioBoundaryRule::PointCeil,
                state,
                budget,
                depth + 1,
                endpoints,
            )?;
            #[cfg(test)]
            point_trace::record(
                point_trace::Phase::BeforeOffset,
                point_context,
                &ranges.value,
                (budget.canonical_used, budget.additional_used),
            );
            let offset = offset_ranges(ranges, origin, budget)?;
            #[cfg(test)]
            point_trace::record(
                point_trace::Phase::AfterOffset,
                point_context,
                &offset.value,
                (budget.canonical_used, budget.additional_used),
            );
            offset
        }
        AudioBoundDomain::Empty => unreachable!("empty operand returned before selection"),
    };
    let mut retained = Owned {
        value: retained.value.into_iter(),
        memory: retained.memory,
    };
    while let Some(range) = retained.value.as_slice().first() {
        spend(budget, 1)?;
        let mapped = inverse(range.start)?..inverse(range.end)?;
        if !mapped.is_empty() {
            push_range(&mut suppressed, mapped, budget)?;
        }
        retained
            .value
            .next()
            .expect("observed retained policy range");
    }
    Ok(suppressed)
}

fn finish<A: AudioSoundContextAdmission>(
    mut state: State<A>,
    mut ranges: Ranges<A>,
    limits: AudioQueryLimits,
    budget: &mut QueryBudget<'_, A>,
) -> QueryResult<AdmittedAudioPolicyQuery<A::Issuer, A::Lease>, A> {
    ranges.value.sort_unstable_by_key(|range| range.start);
    let mut input = Owned {
        value: ranges.value.into_iter(),
        memory: ranges.memory,
    };
    let mut merged = empty_ranges(budget);
    for range in input.value.by_ref() {
        if range.start >= range.end {
            continue;
        }
        if let Some(last) = merged.value.last_mut()
            && range.start <= last.end
        {
            last.end = last.end.max(range.end);
        } else {
            push_range(&mut merged, range, budget)?;
        }
    }
    drop(input);
    let count = merged.value.len();
    let mut output = Owned {
        value: Vec::new(),
        memory: budget.memory(),
    };
    budget.reserve(&mut output.memory, array::<Range<AudioSample>>(count)?)?;
    output.value = Vec::with_capacity(count);
    assert_eq!(output.value.capacity(), count);
    let mut input = Owned {
        value: merged.value.into_iter(),
        memory: merged.memory,
    };
    for range in input.value.by_ref() {
        budget.additional(1)?;
        output
            .value
            .push(AudioSample(range.start)..AudioSample(range.end));
    }
    drop(input);
    let canonical = budget.canonical_used;
    let additional = budget.additional_used;
    let work = canonical
        .checked_add(additional)
        .ok_or(TimeError::Overflow)?;
    if limits.maximum_work.checked_sub(budget.remaining) != Some(work) {
        return Err(PlanError::InvalidPlan("policy query work provenance mismatch").into());
    }
    let lookup = budget.lookup;
    let Owned { value, memory } = output;
    let output = GuardedBound::plain(value, budget.issuer).adopt_memory(memory, budget)?;
    let inventory = state
        .inventory
        .take()
        .expect("completed global policy inventory");
    let value = inventory
        .zip(output, budget)?
        .map(budget, |(inventory, suppressed)| PolicyBuffer {
            query: AudioPolicyQuery {
                suppressed,
                contents: inventory.spans,
                lookup,
                work,
            },
            _contents_capacity: inventory.capacity,
        })?;
    Ok(AdmittedAudioPolicyQuery {
        value,
        canonical,
        additional,
        forwarded: A::METERED,
    })
}
